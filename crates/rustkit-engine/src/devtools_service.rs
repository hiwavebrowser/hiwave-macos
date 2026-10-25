//! The devtools service thread (dev-tools lane T6a/T6b, `T6-design.md`).
//!
//! One `devtools` thread per process answers the Inspector panel's reads
//! of what a view has already recorded, from the view's
//! [`Inspector`](crate::inspector::Inspector) ring. The page thread is not
//! asked: before T6a every console read was a [`PageCmd::Devtools`]
//! (`crate::PageCmd`) the page answered between its turns, about four a
//! second while a panel was open on an idle page (#785's `CONSOLE_POLL`).
//!
//! Nothing polls (T6b). While a panel is open its view is watched: the
//! Inspector signals this thread after a record is pushed (and its lock is
//! released), and the thread wakes the window once, not again until the
//! panel has read. An idle page with the panel open costs no wakes at all.
//!
//! Records are redacted when they are recorded (`inspector.rs`), so the
//! service serves them as they are. The window never takes the Inspector
//! lock: sinks run under it, and a slow sink must not hold the UI.
//!
//! Live DOM and computed-style questions still go to the page.
//!
//! [`DevtoolsCounters`] counts what devtools costs the page thread: page
//! commands handled for devtools, and window wakes posted for a devtools
//! answer. `HIWAVE_DEVTOOLS_COUNTERS=1` logs the process totals once a
//! second from the service thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use crate::devtools::devtools_console_entries;
use crate::inspector::Inspector;
use crate::page_protocol::devtools_wakes_window;
use crate::DevtoolsAnswer;

/// What devtools has cost one page (or, for [`DevtoolsCounters::process`],
/// every page in the process).
#[derive(Debug, Default)]
pub struct DevtoolsCounters {
    page_commands: AtomicU64,
    window_wakes: AtomicU64,
}

/// A reading of [`DevtoolsCounters`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DevtoolsCount {
    /// [`crate::PageCmd::Devtools`] commands the page thread handled.
    pub page_commands: u64,
    /// Window wakes posted because a devtools answer had something new.
    pub window_wakes: u64,
}

impl DevtoolsCounters {
    /// The totals over every page in the process.
    pub fn process() -> &'static DevtoolsCounters {
        static PROCESS: DevtoolsCounters = DevtoolsCounters {
            page_commands: AtomicU64::new(0),
            window_wakes: AtomicU64::new(0),
        };
        &PROCESS
    }

    pub fn read(&self) -> DevtoolsCount {
        DevtoolsCount {
            page_commands: self.page_commands.load(Ordering::Relaxed),
            window_wakes: self.window_wakes.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn page_command(&self) {
        self.page_commands.fetch_add(1, Ordering::Relaxed);
        Self::process().page_commands.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn window_wake(&self) {
        self.window_wakes.fetch_add(1, Ordering::Relaxed);
        Self::process().window_wakes.fetch_add(1, Ordering::Relaxed);
    }
}

/// `HIWAVE_DEVTOOLS_COUNTERS=1` (or true/on/yes).
fn counters_logged() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        matches!(
            std::env::var("HIWAVE_DEVTOOLS_COUNTERS").ok().as_deref(),
            Some("1" | "true" | "on" | "yes")
        )
    })
}

/// A console read for the service.
struct ConsoleRead {
    inspector: Arc<Inspector>,
    since: u64,
    reply: oneshot::Sender<DevtoolsAnswer>,
    /// Wake the window's loop when the answer has something new, as the
    /// content thread's page did. Counted here, not on the page.
    wake_window: Option<Arc<DevtoolsCounters>>,
}

/// An open panel's view: when it records past what the panel has read, the
/// service wakes the window once, and not again until the panel reads.
struct Watch {
    inspector: Arc<Inspector>,
    counters: Arc<DevtoolsCounters>,
    /// The `next_seq` of the panel's last read.
    read_to: u64,
    /// A wake is out that the panel has not answered with a read.
    woken: bool,
}

#[derive(Default)]
struct Inbox {
    reads: Vec<ConsoleRead>,
    /// A watched view recorded since the service last looked.
    records: bool,
    watches: Vec<Watch>,
    started: bool,
}

struct Service {
    inbox: Mutex<Inbox>,
    ready: Condvar,
}

fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(|| Service {
        inbox: Mutex::new(Inbox::default()),
        ready: Condvar::new(),
    })
}

impl Service {
    fn inbox(&self) -> MutexGuard<'_, Inbox> {
        self.inbox.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Start the thread on first use. False when it could not be started.
    fn start(&'static self, inbox: &mut Inbox) -> bool {
        if !inbox.started {
            inbox.started = std::thread::Builder::new()
                .name("devtools".into())
                .spawn(move || self.run())
                .is_ok();
        }
        inbox.started
    }

    fn run(&self) {
        let logged = counters_logged();
        let mut next_log = Instant::now() + Duration::from_secs(1);
        loop {
            let (reads, records) = {
                let mut inbox = self.inbox();
                while inbox.reads.is_empty() && !inbox.records {
                    if !logged {
                        inbox = self.ready.wait(inbox).unwrap_or_else(|e| e.into_inner());
                        continue;
                    }
                    let wait = next_log.saturating_duration_since(Instant::now());
                    inbox = self.ready.wait_timeout(inbox, wait).unwrap_or_else(|e| e.into_inner()).0;
                    if Instant::now() >= next_log {
                        break;
                    }
                }
                (std::mem::take(&mut inbox.reads), std::mem::take(&mut inbox.records))
            };
            for read in reads {
                let inspector = Arc::clone(&read.inspector);
                let next_seq = answer(read);
                let mut inbox = self.inbox();
                if let Some(watch) = inbox.watches.iter_mut().find(|w| Arc::ptr_eq(&w.inspector, &inspector)) {
                    watch.read_to = watch.read_to.max(next_seq);
                    watch.woken = false;
                }
            }
            if records {
                let mut inbox = self.inbox();
                for watch in inbox.watches.iter_mut().filter(|w| !w.woken) {
                    if watch.inspector.watermark() > watch.read_to {
                        watch.woken = true;
                        watch.counters.window_wake();
                        crate::script_net::poke_live_turn();
                    }
                }
            }
            if logged && Instant::now() >= next_log {
                let c = DevtoolsCounters::process().read();
                tracing::info!(
                    target: "devtools",
                    page_commands = c.page_commands,
                    window_wakes = c.window_wakes,
                    "devtools counters"
                );
                next_log = Instant::now() + Duration::from_secs(1);
            }
        }
    }
}

/// A watched view recorded. The Inspector calls this after its lock is
/// released, and only while a panel watches the view.
pub(crate) fn records_arrived() {
    let service = service();
    service.inbox().records = true;
    service.ready.notify_one();
}

/// A panel opened (`on`) or closed on the view `inspector` records. While
/// it is open, the window is woken when the view records something the
/// panel has not read (counted on `counters`).
pub fn watch(inspector: &Arc<Inspector>, counters: &Arc<DevtoolsCounters>, on: bool) {
    let service = service();
    let mut inbox = service.inbox();
    inbox.watches.retain(|w| !Arc::ptr_eq(&w.inspector, inspector));
    let watched = on && service.start(&mut inbox);
    if watched {
        inbox.watches.push(Watch {
            inspector: Arc::clone(inspector),
            counters: Arc::clone(counters),
            read_to: 0,
            woken: false,
        });
        // What the view already recorded is news to a panel that just opened.
        inbox.records = true;
        service.ready.notify_one();
    }
    inspector.set_watched(watched);
}

/// Answer `read`; returns the `next_seq` it answered up to.
fn answer(read: ConsoleRead) -> u64 {
    let console = devtools_console_entries(&read.inspector.snapshot_since(read.since));
    let next_seq = console.next_seq;
    let answer = DevtoolsAnswer::Console(console);
    let wake = devtools_wakes_window(&answer, false);
    let _ = read.reply.send(answer);
    if let (true, Some(counters)) = (wake, read.wake_window) {
        counters.window_wake();
        crate::script_net::poke_live_turn();
    }
    next_seq
}

/// The console lines `inspector` recorded since `since`, answered on the
/// devtools thread. With `wake_window`, the window's live wake is posted
/// when the answer has something new (and counted there).
pub fn ask_console(
    inspector: Arc<Inspector>,
    since: u64,
    wake_window: Option<Arc<DevtoolsCounters>>,
) -> oneshot::Receiver<DevtoolsAnswer> {
    let (reply, rx) = oneshot::channel();
    let read = ConsoleRead {
        inspector,
        since,
        reply,
        wake_window,
    };
    let service = service();
    let mut inbox = service.inbox();
    if service.start(&mut inbox) {
        inbox.reads.push(read);
        service.ready.notify_one();
    } else {
        // No thread could be started: answer on the caller's thread, still
        // without the page.
        drop(inbox);
        answer(read);
    }
    rx
}
