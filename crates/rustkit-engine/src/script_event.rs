//! One `script_event` record for every page script the engine skipped or
//! stopped: a structured `tracing` line (target `script_event`, INFO) and an
//! entry in the view's list, which the browser shows as a per-tab notice
//! ([`ScriptNotice`]).
//!
//! The engine records outcomes in many places (the load's script loop,
//! inserted scripts, live turns). Each of them calls [`observe`] with the
//! record it logs; the view whose script is running is the one [`enter`]
//! named, as the script budget (`install_script_js`) is per thread too.
//! A record that ran, or threw an ordinary page error, makes no event; it
//! still counts towards the longest time a script held the window.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tracing::info;

use crate::{EngineConfig, FetchFailure, ScriptOutcome, ScriptRecord};

/// The most events a view keeps for one document. Past it they are still
/// logged and counted, not stored.
const MAX_EVENTS: usize = 256;

/// How much of an external script's source an event quotes.
const HEAD_CHARS: usize = 200;

/// Where a script came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptKind {
    /// A parser-inserted external classic script.
    Classic,
    /// A `type=module` script, inline or external.
    Module,
    /// An external script the page's own script inserted.
    Inserted,
    /// A parser-inserted inline classic script.
    Inline,
    /// A script whose source is a `data:` URL.
    #[serde(rename = "data:")]
    Data,
    /// A listener, timer or network callback (`event:click`, `timers`).
    Callback,
}

impl ScriptKind {
    /// The kind a log label implies, when the caller does not know better.
    pub(crate) fn of(source: &str) -> Self {
        if source.starts_with("data:") {
            ScriptKind::Data
        } else if source.starts_with("inline#") {
            ScriptKind::Inline
        } else if source.starts_with("event:") || matches!(source, "timers" | "network") {
            ScriptKind::Callback
        } else {
            ScriptKind::Classic
        }
    }
}

/// Why a script was skipped or stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptEventReason {
    /// Larger than the script ceiling: never started.
    SkippedSize,
    /// Never started: the load's script budget was already spent.
    SkippedBudget,
    /// Never started: the JS engine panicked earlier on this page.
    SkippedEngine,
    /// Stopped at a host call past the script budget (the deadline).
    StoppedBudget,
    /// Stopped by the watchdog thread, with no host call to stop it at.
    StoppedWatchdog,
    /// Boa's loop-iteration or job-queue limit ended it.
    IterationLimit,
    /// Boa's recursion or stack-size limit ended it.
    StackLimit,
    /// Its fetch failed: an HTTP error status or a network failure.
    FetchError,
    /// A `type` Chromium runs as script and the engine does not.
    UnsupportedType,
    /// Its fetch did not finish within the network cap.
    Timeout,
}

/// One skipped or stopped script.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScriptEvent {
    /// The script URL, `inline#n`, or the callback (`event:click`, `timers`).
    pub source: String,
    /// The document it belongs to.
    pub document: String,
    pub kind: ScriptKind,
    /// Source length in bytes (0 when nothing was fetched).
    pub bytes: usize,
    pub reason: ScriptEventReason,
    /// The HTTP status of a failed fetch.
    pub http_status: Option<u16>,
    /// The engine's own words, for a transport failure or a limit's error.
    pub detail: Option<String>,
    /// The limit that triggered it (`script_budget_ms`, ...) and its value.
    pub limit: Option<&'static str>,
    pub limit_value: Option<u64>,
    /// JavaScript milliseconds it ran before the stop.
    pub js_ms: u64,
    /// Boa parses and compiles inside the one call that runs a script, so
    /// these are not known separately yet; they stay `None` until it can
    /// report them.
    pub parse_ms: Option<u64>,
    pub compile_ms: Option<u64>,
    /// The script that inserted it, when known.
    pub initiator: Option<String>,
    /// Milliseconds since the document's load began.
    pub at_ms: u64,
    /// The first 200 characters of an external script's source.
    pub head: Option<String>,
    /// The SHA-256 of an inline script's source, in hex.
    pub sha256: Option<String>,
}

/// What the browser shows for a view's current document: the
/// `script_notice` payload the chrome's badge reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScriptNotice {
    /// Changes whenever anything below does: the chrome redraws on a change.
    pub generation: u64,
    pub document: String,
    /// Skipped and stopped scripts, including any past the stored list.
    pub count: usize,
    /// The longest a single script held the window (it runs on the
    /// window's thread), in milliseconds, and which script it was.
    pub longest_hold_ms: u64,
    pub longest_hold_source: Option<String>,
    pub events: Vec<ScriptEvent>,
}

/// The limits an event names, read from the engine's configuration.
#[derive(Debug, Clone, Copy, Default)]
struct Limits {
    /// The budget a stop is charged to: the load's or a live turn's.
    budget: (&'static str, u64),
    loop_iteration_limit: u64,
    network_cap_ms: u64,
}

#[derive(Debug)]
struct Inner {
    document: String,
    started: Instant,
    limits: Limits,
    events: Vec<ScriptEvent>,
    count: usize,
    longest_hold: Option<(u64, String)>,
    generation: u64,
}

/// A view's script events for its current document.
#[derive(Debug, Clone)]
pub(crate) struct ScriptEvents(Arc<Mutex<Inner>>);

impl Default for ScriptEvents {
    fn default() -> Self {
        ScriptEvents(Arc::new(Mutex::new(Inner {
            document: String::new(),
            started: Instant::now(),
            limits: Limits::default(),
            events: Vec::new(),
            count: 0,
            longest_hold: None,
            generation: 0,
        })))
    }
}

impl ScriptEvents {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A new document: nothing recorded, its load clock starts now.
    pub(crate) fn reset(&self) {
        let mut inner = self.lock();
        inner.document.clear();
        inner.started = Instant::now();
        inner.events.clear();
        inner.count = 0;
        inner.longest_hold = None;
        inner.generation += 1;
    }

    pub(crate) fn set_document(&self, url: &str) {
        let mut inner = self.lock();
        if inner.document != url {
            inner.document = url.to_string();
            inner.generation += 1;
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.lock().generation
    }

    pub(crate) fn events(&self) -> Vec<ScriptEvent> {
        self.lock().events.clone()
    }

    pub(crate) fn notice(&self) -> ScriptNotice {
        let inner = self.lock();
        ScriptNotice {
            generation: inner.generation,
            document: inner.document.clone(),
            count: inner.count,
            longest_hold_ms: inner.longest_hold.as_ref().map_or(0, |(ms, _)| *ms),
            longest_hold_source: inner
                .longest_hold
                .as_ref()
                .map(|(_, source)| source.clone()),
            events: inner.events.clone(),
        }
    }
}

thread_local! {
    /// The view whose script this thread is running ([`enter`]).
    static CURRENT: RefCell<Option<ScriptEvents>> = const { RefCell::new(None) };
    /// How the last stopped execution ended, set by `log_script_stopped`:
    /// `true` at a host call past the deadline, `false` by the watchdog.
    static LAST_STOP: RefCell<Option<bool>> = const { RefCell::new(None) };
}

/// Until dropped, [`observe`] records into the view [`enter`] named.
pub(crate) struct Scope(Option<ScriptEvents>);

/// Record this thread's script events into `events` until the scope drops.
/// `live` says which budget a stop in it is charged to: a live turn's
/// (`live_turn_budget_ms`) or the load's (`script_budget_ms`).
pub(crate) fn enter(events: &ScriptEvents, config: &EngineConfig, live: bool) -> Scope {
    events.lock().limits = Limits {
        budget: if live {
            ("live_turn_budget_ms", config.live_turn_budget_ms)
        } else {
            ("script_budget_ms", config.script_budget_ms)
        },
        loop_iteration_limit: config.script_loop_iteration_limit,
        network_cap_ms: crate::SCRIPT_NETWORK_CAP.as_millis() as u64,
    };
    LAST_STOP.with(|stop| stop.borrow_mut().take());
    Scope(CURRENT.with(|current| current.borrow_mut().replace(events.clone())))
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.0.take();
        CURRENT.with(|current| *current.borrow_mut() = previous);
    }
}

/// A script was just stopped; `by_deadline` says how (see [`LAST_STOP`]).
pub(crate) fn note_stop(by_deadline: bool) {
    LAST_STOP.with(|stop| *stop.borrow_mut() = Some(by_deadline));
}

/// Record `record`, just logged: an event when it was skipped or stopped,
/// and its time against the longest hold. `kind` is what the caller knows
/// better than the label (a module, an inserted script); `text` is its
/// source, when there is one.
pub(crate) fn observe(record: &ScriptRecord, kind: Option<ScriptKind>, text: Option<&str>) {
    let stop = LAST_STOP.with(|stop| stop.borrow_mut().take());
    let Some(events) = CURRENT.with(|current| current.borrow().clone()) else {
        // No view named: still one line in the log.
        if let Some(event) = build(record, kind, text, stop, &Limits::default(), "", 0) {
            emit(&event);
        }
        return;
    };
    let mut inner = events.lock();
    if record.elapsed_ms > inner.longest_hold.as_ref().map_or(0, |(ms, _)| *ms) {
        inner.longest_hold = Some((record.elapsed_ms, record.source.clone()));
        inner.generation += 1;
    }
    let at_ms = inner.started.elapsed().as_millis() as u64;
    let Some(event) = build(
        record,
        kind,
        text,
        stop,
        &inner.limits,
        &inner.document,
        at_ms,
    ) else {
        return;
    };
    emit(&event);
    push(&mut inner, event);
}

/// A `<script type>` the engine does not run but Chromium does, so it is
/// not a data block: legacy JavaScript MIME types, speculation rules and
/// web bundles. `None` for a type the engine runs or a real data block.
pub(crate) fn unsupported_script_type(essence: &str) -> Option<&str> {
    let unsupported = matches!(
        essence,
        "application/x-ecmascript"
            | "text/javascript1.0"
            | "text/javascript1.1"
            | "text/javascript1.2"
            | "text/javascript1.3"
            | "text/javascript1.4"
            | "text/javascript1.5"
            | "text/livescript"
            | "text/x-ecmascript"
            | "text/x-javascript"
            | "speculationrules"
            | "webbundle"
    );
    unsupported.then_some(essence)
}

/// Record a `<script>` whose `type` the engine does not run.
pub(crate) fn observe_unsupported(source: &str, script_type: &str, text: Option<&str>) {
    let record = ScriptRecord {
        source: source.to_string(),
        bytes: text.map_or(0, str::len),
        elapsed_ms: 0,
        outcome: ScriptOutcome::Skipped(UNSUPPORTED_TYPE),
    };
    let kind = ScriptKind::of(source);
    LAST_STOP.with(|stop| stop.borrow_mut().take());
    let Some(events) = CURRENT.with(|current| current.borrow().clone()) else {
        return;
    };
    let mut inner = events.lock();
    let at_ms = inner.started.elapsed().as_millis() as u64;
    if let Some(mut event) = build(
        &record,
        Some(kind),
        text,
        None,
        &inner.limits,
        &inner.document,
        at_ms,
    ) {
        event.detail = Some(format!("type={script_type}"));
        emit(&event);
        push(&mut inner, event);
    }
}

const UNSUPPORTED_TYPE: &str = "unsupported script type";

fn push(inner: &mut Inner, event: ScriptEvent) {
    inner.count += 1;
    inner.generation += 1;
    if inner.events.len() < MAX_EVENTS {
        inner.events.push(event);
    }
}

fn emit(event: &ScriptEvent) {
    let record = serde_json::to_string(event).unwrap_or_default();
    info!(
        target: "script_event",
        reason = ?event.reason,
        source = %event.source,
        document = %event.document,
        %record,
        "script_event"
    );
}

/// The event for `record`, or `None` when it was neither skipped nor
/// stopped.
fn build(
    record: &ScriptRecord,
    kind: Option<ScriptKind>,
    text: Option<&str>,
    stop: Option<bool>,
    limits: &Limits,
    document: &str,
    at_ms: u64,
) -> Option<ScriptEvent> {
    let kind = match (kind, ScriptKind::of(&record.source)) {
        // A `data:` source says the most about where it came from.
        (_, ScriptKind::Data) => ScriptKind::Data,
        (Some(kind), _) => kind,
        (None, kind) => kind,
    };
    let budget = limits.budget;
    let mut http_status = None;
    let mut detail = None;
    let (reason, limit) = match &record.outcome {
        ScriptOutcome::Skipped(why) if *why == crate::SCRIPT_TOO_LARGE => (
            ScriptEventReason::SkippedSize,
            Some(("max_page_script_bytes", crate::MAX_PAGE_SCRIPT_BYTES as u64)),
        ),
        ScriptOutcome::Skipped(why) if *why == UNSUPPORTED_TYPE => {
            (ScriptEventReason::UnsupportedType, None)
        }
        ScriptOutcome::Skipped(why) => {
            detail = Some(why.to_string());
            (ScriptEventReason::SkippedEngine, None)
        }
        ScriptOutcome::OverBudget => match stop {
            Some(true) => (ScriptEventReason::StoppedBudget, Some(budget)),
            Some(false) => (ScriptEventReason::StoppedWatchdog, Some(budget)),
            None => (ScriptEventReason::SkippedBudget, Some(budget)),
        },
        ScriptOutcome::FetchFailed(FetchFailure::Status(status)) => {
            http_status = Some(*status);
            (ScriptEventReason::FetchError, None)
        }
        ScriptOutcome::FetchFailed(FetchFailure::Transport(message)) => {
            detail = Some(message.clone());
            if message == "timed out" {
                (
                    ScriptEventReason::Timeout,
                    Some(("script_network_cap_ms", limits.network_cap_ms)),
                )
            } else {
                (ScriptEventReason::FetchError, None)
            }
        }
        ScriptOutcome::Threw(message) => {
            let reason = if message.contains("maximum number of iteration loops")
                || message.contains("Job queue iteration limit")
            {
                ScriptEventReason::IterationLimit
            } else if message.contains("maximum number of recursive calls")
                || message.contains("maximum stack size")
            {
                ScriptEventReason::StackLimit
            } else {
                return None;
            };
            detail = Some(message.chars().take(HEAD_CHARS).collect());
            let limit = (reason == ScriptEventReason::IterationLimit)
                .then_some(("script_loop_iteration_limit", limits.loop_iteration_limit));
            (reason, limit)
        }
        ScriptOutcome::Ran | ScriptOutcome::Nomodule | ScriptOutcome::EngineFailed(_) => {
            return None
        }
    };
    let inline = matches!(kind, ScriptKind::Inline) || record.source.starts_with("inline#");
    let head = text
        .filter(|_| !inline)
        .map(|text| text.chars().take(HEAD_CHARS).collect());
    let sha256 = text.filter(|_| inline).map(|text| {
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    });
    Some(ScriptEvent {
        source: record.source.clone(),
        document: document.to_string(),
        kind,
        bytes: record.bytes.max(text.map_or(0, str::len)),
        reason,
        http_status,
        detail,
        limit: limit.map(|(name, _)| name),
        limit_value: limit.map(|(_, value)| value),
        js_ms: record.elapsed_ms,
        parse_ms: None,
        compile_ms: None,
        initiator: None,
        at_ms,
        head,
        sha256,
    })
}
