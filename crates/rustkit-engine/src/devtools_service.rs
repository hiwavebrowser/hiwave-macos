//! The devtools service thread (dev-tools lane T6a, `T6-design.md`).
//!
//! One `devtools` thread per process answers the Inspector panel's reads
//! of what a view has already recorded, from the view's
//! [`Inspector`](crate::inspector::Inspector) ring. The page thread is not
//! asked: before T6a every console read was a [`PageCmd::Devtools`]
//! (`crate::PageCmd`) the page answered between its turns, about four a
//! second while a panel was open on an idle page (#785's `CONSOLE_POLL`).
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
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
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

fn service() -> Option<Sender<ConsoleRead>> {
    static SERVICE: OnceLock<Mutex<Option<Sender<ConsoleRead>>>> = OnceLock::new();
    let slot = SERVICE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ConsoleRead>();
        let spawned = std::thread::Builder::new().name("devtools".into()).spawn(move || {
            let logged = counters_logged();
            let mut next_log = Instant::now() + Duration::from_secs(1);
            loop {
                let read = if logged {
                    match rx.recv_timeout(next_log.saturating_duration_since(Instant::now())) {
                        Ok(read) => Some(read),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                } else {
                    match rx.recv() {
                        Ok(read) => Some(read),
                        Err(_) => return,
                    }
                };
                if let Some(read) = read {
                    answer(read);
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
        });
        Mutex::new(spawned.ok().map(|_| tx))
    });
    slot.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn answer(read: ConsoleRead) {
    let answer = DevtoolsAnswer::Console(devtools_console_entries(&read.inspector.snapshot_since(read.since)));
    let wake = devtools_wakes_window(&answer, false);
    let _ = read.reply.send(answer);
    if let (true, Some(counters)) = (wake, read.wake_window) {
        counters.window_wake();
        crate::script_net::poke_live_turn();
    }
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
    match service() {
        Some(tx) => {
            if let Err(mpsc::SendError(read)) = tx.send(read) {
                answer(read);
            }
        }
        // No thread could be started: answer on the caller's thread, still
        // without the page.
        None => answer(read),
    }
    rx
}
