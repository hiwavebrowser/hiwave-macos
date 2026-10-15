//! Stopping a runaway script from another thread (threads plan M1,
//! invariant 6; the design is `THREAD_OWNERSHIP_2026-10.md` section 3,
//! "Interrupt word: lifecycle").
//!
//! Two control blocks, atomics only, so they are `Send + Sync`:
//!
//! - [`ThreadStamp`], one per page thread and written only by it, says which
//!   turn of script that thread is running, since when, for which view and
//!   with what budget.
//! - [`ViewControl`], one per view, holds the interrupt word the vendored
//!   Boa reads at every call and loop back-edge
//!   (`third_party/boa_engine/PATCHES.md`), and the host-call check reads
//!   too. A script stops when the word equals its turn id, or [`ALL`].
//!
//! The [`Watchdog`] is its own thread: before M4 the thread that would
//! otherwise notice an overrun is the one spinning. It sleeps while no turn
//! is open, wakes when one is published, and sets the word of a turn that
//! has run past its budget.
//!
//! Turn ids come from one counter, only grow and are never reused, so a
//! word naming a turn that has ended cannot stop the next one, and writers
//! never need to clear a turn-targeted word: every writer uses `fetch_max`.

use std::collections::HashMap;
use std::sync::atomic::{fence, AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

/// The interrupt word's "every turn": this view runs no more script until
/// a new document clears it ([`ViewControl::clear_all`]).
pub const ALL: u64 = u64::MAX;

/// What is left of a turn after its script was stopped may run this long
/// (`JsRuntime`'s grace turn): the promise jobs the turn's finished
/// callbacks queued, and the embedder's own script.
pub const STOP_GRACE: Duration = Duration::from_millis(50);

/// The longest the watchdog sleeps while a turn is open. It wakes sooner
/// when an open turn's budget ends first.
const TICK: Duration = Duration::from_millis(250);

/// The next turn id. Starts at 1: 0 is "no turn" and "no interrupt".
static NEXT_TURN: AtomicU64 = AtomicU64::new(1);
static NEXT_VIEW: AtomicU64 = AtomicU64::new(1);

/// Microseconds since the first call, the clock turn start times are on.
/// Finer than the budgets so a turn is never judged by a rounded clock.
fn now_us() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_micros() as u64
}

/// One view's stop state.
///
/// Created with the view and dropped with it. View ids are never reused,
/// so a new view never inherits an old view's [`ALL`].
#[derive(Debug)]
pub struct ViewControl {
    id: u64,
    /// A turn id to stop, or [`ALL`]; 0 = none. An `Arc` of its own
    /// because Boa holds it ([`boa_engine::Context::set_interrupt_word`]).
    interrupt: Arc<AtomicU64>,
    /// Cancel every load whose navigation id is at or below this; 0 =
    /// none. Not read yet: it is the load cancel's (M3b).
    pub cancel_nav: AtomicU64,
    /// Bumped on Stop and Close, never lowered. Not read yet: navigations a
    /// stopped page raised are dropped by it from M4.
    pub nav_epoch: AtomicU64,
}

impl ViewControl {
    fn new() -> Self {
        Self {
            id: NEXT_VIEW.fetch_add(1, Ordering::Relaxed),
            interrupt: Arc::new(AtomicU64::new(0)),
            cancel_nav: AtomicU64::new(0),
            nav_epoch: AtomicU64::new(0),
        }
    }

    /// This view's id in [`ThreadStamp`] (`turn_view`).
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Stop `turn` if it is this view's running turn. Never lowers the
    /// word: a later turn's id, or [`ALL`], stays.
    pub fn interrupt_turn(&self, turn: u64) {
        self.interrupt.fetch_max(turn, Ordering::Relaxed);
    }

    /// Stop every turn of this view until [`ViewControl::clear_all`].
    pub fn stop_all(&self) {
        self.interrupt.store(ALL, Ordering::Relaxed);
    }

    /// Lift [`ALL`]; done by the page thread only, when a new document
    /// commits in the view. A turn id in the word is left: it names a turn
    /// that has ended. Whether `ALL` was set.
    pub fn clear_all(&self) -> bool {
        self.interrupt
            .compare_exchange(ALL, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Whether the word stops `turn`.
    pub fn stops(&self, turn: u64) -> bool {
        let word = self.interrupt.load(Ordering::Relaxed);
        word != 0 && (word == ALL || word == turn)
    }

    /// The word itself, for Boa.
    #[cfg_attr(not(feature = "boa"), allow(dead_code))]
    pub(crate) fn word(&self) -> &Arc<AtomicU64> {
        &self.interrupt
    }
}

/// What one page thread is running.
///
/// Written only by its thread; read by the watchdog. A turn is published
/// seqlock-style: the start time, view and budget first, then `turn`; the
/// watchdog reads `turn` on both sides of them and uses them only if it
/// did not change.
#[derive(Debug)]
pub struct ThreadStamp {
    /// The open turn's id; 0 = idle.
    turn: AtomicU64,
    turn_started_us: AtomicU64,
    turn_view: AtomicU64,
    turn_budget_us: AtomicU64,
    /// Open turns, counting nested ones; only the outermost publishes.
    depth: AtomicU32,
    watchdog: Weak<Shared>,
}

/// A turn as the watchdog read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenTurn {
    turn: u64,
    started_us: u64,
    view: u64,
    budget_us: u64,
}

impl ThreadStamp {
    /// Open a turn of script for `view` that may run for `budget`, and
    /// return its id; the watchdog stops it past that. A turn opened while
    /// another is open on this thread joins it and returns its id: the
    /// outer turn's budget governs.
    pub fn begin_turn(&self, view: &ViewControl, budget: Duration) -> u64 {
        if self.depth.fetch_add(1, Ordering::Relaxed) > 0 {
            return self.turn.load(Ordering::Relaxed);
        }
        let id = NEXT_TURN.fetch_add(1, Ordering::Relaxed);
        // The previous turn's `turn = 0` is ordered before these: a watchdog
        // that reads them sees `turn` change and skips the tick.
        fence(Ordering::Release);
        let started = now_us();
        let budget_us = budget.as_micros().min(u64::MAX as u128) as u64;
        self.turn_view.store(view.id, Ordering::Relaxed);
        self.turn_started_us.store(started, Ordering::Relaxed);
        self.turn_budget_us.store(budget_us, Ordering::Relaxed);
        self.turn.store(id, Ordering::Release);
        // Pairs with the watchdog's fence between publishing when it will
        // next wake and its last look: either it sees this turn or this sees
        // when it wakes, and wakes it if that is after this turn's budget
        // (it may be sleeping a whole tick after stopping another turn).
        fence(Ordering::SeqCst);
        if let Some(watchdog) = self.watchdog.upgrade() {
            let parked = watchdog.parked.swap(false, Ordering::Relaxed);
            let wakes = watchdog.wake_at_us.load(Ordering::Relaxed);
            if parked || started.saturating_add(budget_us) < wakes {
                if let Some(thread) = watchdog.thread.get() {
                    thread.unpark();
                }
            }
        }
        id
    }

    /// Close the turn [`ThreadStamp::begin_turn`] opened.
    pub fn end_turn(&self) {
        let depth = self.depth.load(Ordering::Relaxed);
        debug_assert!(depth > 0, "end_turn without begin_turn");
        if depth == 0 {
            return;
        }
        self.depth.store(depth - 1, Ordering::Relaxed);
        if depth == 1 {
            self.turn.store(0, Ordering::Release);
        }
    }

    /// The open turn's id; 0 = idle.
    pub fn current_turn(&self) -> u64 {
        self.turn.load(Ordering::Relaxed)
    }

    /// The open turn, if the read was not torn by a turn ending or
    /// starting in between.
    fn read(&self) -> Option<OpenTurn> {
        let first = self.turn.load(Ordering::Acquire);
        if first == 0 {
            return None;
        }
        let started_us = self.turn_started_us.load(Ordering::Relaxed);
        let view = self.turn_view.load(Ordering::Relaxed);
        let budget_us = self.turn_budget_us.load(Ordering::Relaxed);
        // Keeps the three loads above from moving below the second load of
        // `turn`, which two Acquire loads alone do not.
        fence(Ordering::Acquire);
        let second = self.turn.load(Ordering::Relaxed);
        (first == second).then_some(OpenTurn {
            turn: first,
            started_us,
            view,
            budget_us,
        })
    }
}

#[derive(Default)]
struct Shared {
    stamps: Mutex<Vec<Weak<ThreadStamp>>>,
    views: Mutex<HashMap<u64, Weak<ViewControl>>>,
    /// Set by the watchdog before it parks with no turn open.
    parked: AtomicBool,
    /// When the watchdog next wakes on its own (`now_us`); `u64::MAX`
    /// while parked with no turn open.
    wake_at_us: AtomicU64,
    thread: OnceLock<std::thread::Thread>,
    /// Turns the watchdog has stopped (a diagnostic).
    stopped: AtomicU64,
}

impl Drop for Shared {
    fn drop(&mut self) {
        // The thread holds only a `Weak`: woken, it finds it dead and ends.
        if let Some(thread) = self.thread.get() {
            thread.unpark();
        }
    }
}

impl Shared {
    /// Stops every open turn past its budget. Returns how long until the
    /// next open turn's budget ends, at most [`TICK`]; `None` with no turn
    /// open.
    fn scan(&self) -> Option<Duration> {
        let now = now_us();
        let mut due: Option<Duration> = None;
        let mut over: Vec<OpenTurn> = Vec::new();
        {
            let mut stamps = self.stamps.lock().unwrap_or_else(|e| e.into_inner());
            stamps.retain(|stamp| {
                let Some(stamp) = stamp.upgrade() else {
                    return false;
                };
                if let Some(open) = stamp.read() {
                    let ends = open.started_us.saturating_add(open.budget_us);
                    let wait = if now >= ends {
                        over.push(open);
                        TICK
                    } else {
                        Duration::from_micros(ends - now).min(TICK)
                    };
                    due = Some(due.map_or(wait, |d| d.min(wait)));
                }
                true
            });
        }
        if !over.is_empty() {
            let mut views = self.views.lock().unwrap_or_else(|e| e.into_inner());
            views.retain(|_, view| view.strong_count() > 0);
            for open in over {
                let Some(view) = views.get(&open.view).and_then(Weak::upgrade) else {
                    continue;
                };
                if !view.stops(open.turn) {
                    self.stopped.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        turn = open.turn,
                        ran_ms = now.saturating_sub(open.started_us) / 1000,
                        budget_ms = open.budget_us / 1000,
                        "Watchdog: a script turn ran past its budget; interrupting it"
                    );
                }
                view.interrupt_turn(open.turn);
            }
        }
        due
    }
}

/// The thread that stops script turns that run past their budget.
///
/// Cheap to clone; the thread ends when the last clone is dropped (stamps
/// and controls do not keep it).
#[derive(Clone)]
pub struct Watchdog {
    shared: Arc<Shared>,
}

impl Watchdog {
    /// A watchdog with a thread of its own.
    pub fn spawn() -> Self {
        let shared = Arc::new(Shared::default());
        let weak = Arc::downgrade(&shared);
        let handle = std::thread::Builder::new()
            .name("watchdog".into())
            .spawn(move || run(weak))
            .expect("spawn the watchdog thread");
        let _ = shared.thread.set(handle.thread().clone());
        Self { shared }
    }

    /// The process's watchdog, started on first use.
    pub fn shared() -> &'static Watchdog {
        static SHARED: OnceLock<Watchdog> = OnceLock::new();
        SHARED.get_or_init(Watchdog::spawn)
    }

    /// A stamp for the calling page thread, watched by this watchdog.
    pub fn thread_stamp(&self) -> Arc<ThreadStamp> {
        let stamp = Arc::new(ThreadStamp {
            turn: AtomicU64::new(0),
            turn_started_us: AtomicU64::new(0),
            turn_view: AtomicU64::new(0),
            turn_budget_us: AtomicU64::new(0),
            depth: AtomicU32::new(0),
            watchdog: Arc::downgrade(&self.shared),
        });
        self.shared
            .stamps
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::downgrade(&stamp));
        stamp
    }

    /// The control block for a new view, watched by this watchdog.
    pub fn view_control(&self) -> Arc<ViewControl> {
        let control = Arc::new(ViewControl::new());
        self.shared
            .views
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(control.id, Arc::downgrade(&control));
        control
    }

    /// How many turns this watchdog has stopped.
    pub fn turns_stopped(&self) -> u64 {
        self.shared.stopped.load(Ordering::Relaxed)
    }
}

fn run(shared: Weak<Shared>) {
    loop {
        let Some(live) = shared.upgrade() else {
            return;
        };
        let first = live.scan();
        // Say when this thread wakes next (never, with nothing open), then
        // look once more: a turn published after that look sees it and
        // unparks this thread if its budget ends sooner.
        let wake_at = first.map_or(u64::MAX, |wait| {
            now_us().saturating_add(wait.as_micros() as u64)
        });
        live.wake_at_us.store(wake_at, Ordering::Relaxed);
        live.parked.store(first.is_none(), Ordering::Relaxed);
        fence(Ordering::SeqCst);
        let wait = live.scan();
        if wait.is_none() != first.is_none() {
            // A turn opened or closed between the looks: look again.
            continue;
        }
        drop(live);
        match wait {
            Some(wait) => std::thread::park_timeout(wait),
            None => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writers_never_lower_the_word() {
        let control = ViewControl::new();
        control.interrupt_turn(5);
        control.interrupt_turn(3);
        assert!(control.stops(5));
        assert!(
            !control.stops(3),
            "an older id does not replace a newer one"
        );
        control.stop_all();
        control.interrupt_turn(9);
        assert!(
            control.stops(9) && control.stops(10),
            "a turn id never replaces ALL"
        );
        assert!(control.clear_all());
        assert!(!control.stops(10));
        control.interrupt_turn(11);
        assert!(!control.clear_all(), "clear_all leaves a turn id");
        assert!(control.stops(11));
    }

    #[test]
    fn nested_turns_join_the_outer_one() {
        let watchdog = Watchdog::spawn();
        let stamp = watchdog.thread_stamp();
        let view = watchdog.view_control();
        let outer = stamp.begin_turn(&view, Duration::from_secs(60));
        assert_eq!(stamp.begin_turn(&view, Duration::from_secs(1)), outer);
        stamp.end_turn();
        assert_eq!(
            stamp.current_turn(),
            outer,
            "the inner end leaves the outer turn open"
        );
        stamp.end_turn();
        assert_eq!(stamp.current_turn(), 0);
        assert!(
            stamp.begin_turn(&view, Duration::from_secs(1)) > outer,
            "ids only grow"
        );
        stamp.end_turn();
    }

    #[test]
    fn a_read_torn_by_a_turn_change_is_not_acted_on() {
        // A stepped version of the watchdog's read: the turn ends and the
        // next starts between its two loads of `turn`.
        let watchdog = Watchdog::spawn();
        let stamp = watchdog.thread_stamp();
        let view = watchdog.view_control();
        let first = stamp.begin_turn(&view, Duration::from_millis(0));
        let t1 = stamp.turn.load(Ordering::Acquire);
        stamp.end_turn();
        let second = stamp.begin_turn(&view, Duration::from_secs(60));
        let t2 = stamp.turn.load(Ordering::Relaxed);
        assert_eq!((t1, t2), (first, second));
        assert_ne!(t1, t2, "the reader skips this tick");
        // An untorn read sees the new turn with its own clock and budget.
        let open = stamp.read().expect("a turn is open");
        assert_eq!(
            (open.turn, open.budget_us, open.view),
            (second, 60_000_000, view.id())
        );
        stamp.end_turn();
        assert_eq!(stamp.read(), None);
    }

    #[test]
    fn the_watchdog_stops_a_turn_past_its_budget_and_not_the_next() {
        let watchdog = Watchdog::spawn();
        let stamp = watchdog.thread_stamp();
        let view = watchdog.view_control();
        let started = Instant::now();
        let turn = stamp.begin_turn(&view, Duration::from_millis(100));
        while !view.stops(turn) {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "the watchdog never fired"
            );
            std::thread::yield_now();
        }
        let fired = started.elapsed();
        assert!(
            fired >= Duration::from_millis(100),
            "fired early: {fired:?}"
        );
        assert!(fired < Duration::from_millis(350), "fired late: {fired:?}");
        stamp.end_turn();
        assert_eq!(watchdog.turns_stopped(), 1);

        // The next turn, within its budget, is left alone.
        let next = stamp.begin_turn(&view, Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!view.stops(next));
        stamp.end_turn();
    }

    #[test]
    fn a_short_turn_opened_while_the_watchdog_sleeps_a_tick_is_stopped_on_time() {
        // After stopping a turn the watchdog sleeps a whole tick; a turn
        // whose budget ends sooner (the grace turn after a stop) wakes it.
        let watchdog = Watchdog::spawn();
        let stamp = watchdog.thread_stamp();
        let view = watchdog.view_control();
        let first = stamp.begin_turn(&view, Duration::from_millis(0));
        let started = Instant::now();
        while !view.stops(first) {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "the first stop never came"
            );
            std::thread::yield_now();
        }
        stamp.end_turn();
        let started = Instant::now();
        let short = stamp.begin_turn(&view, Duration::from_millis(30));
        while !view.stops(short) {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "the short turn was never stopped"
            );
            std::thread::yield_now();
        }
        let took = started.elapsed();
        stamp.end_turn();
        assert!(
            took < Duration::from_millis(200),
            "stopped after {took:?}, a tick late"
        );
    }

    #[test]
    fn the_watchdog_ends_with_its_last_owner_and_its_stamps_still_work() {
        let watchdog = Watchdog::spawn();
        let weak = Arc::downgrade(&watchdog.shared);
        let stamp = watchdog.thread_stamp();
        let view = watchdog.view_control();
        drop(watchdog);
        let started = Instant::now();
        // The thread may hold it for the length of one scan.
        while weak.upgrade().is_some() {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "the watchdog outlived its owner"
            );
            std::thread::yield_now();
        }
        stamp.begin_turn(&view, Duration::from_millis(1));
        stamp.end_turn();
    }
}
