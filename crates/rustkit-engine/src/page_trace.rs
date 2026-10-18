//! One load's trace, in the schema `tools/replay/diff_trace.py` compares with
//! Chrome. Off unless `HIWAVE_TRACE_JSON` is a path or `HIWAVE_CONSOLE_LOG=1`.
//!
//! Times are milliseconds from the start of the navigation. `scripts` records
//! each `<script>` as `execute` when it runs and `load` or `error` when the
//! element is told; a fetch that never ran is `error` only. `fetched_scripts`
//! lists every parser script and module dependency the load fetched, with
//! `executed` and, when it did not run, `reason` (`blocked`, `module graph
//! error`, `waiting on a dependency`, or `never reached`). `shield` is the
//! startup choice (`HIWAVE_SHIELD`, or off by default under `HIWAVE_REPLAY`).
//! `mutations` are subtree inserts and removes of more than 50 nodes (the
//! same threshold as the Chrome probe). `max_js_stack` is the deepest JS call
//! stack during the load, not the native host-call depth.
//!
//! `parser` is the time the load spent blocked on a stylesheet fetch, a
//! script fetch, or a script execution. `render` is the latest
//! `HIWAVE_DUMP_RENDER` snapshot of `main#main-content`, when that dump is on.
//! A yielded load keeps this trace open after `load_url` returns, until the
//! page is quiet or `HIWAVE_DUMP_RENDER_AT_MS` has passed.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

use crate::{EngineFailure, FetchFailure, ScriptOutcome, ScriptRecord};

const MAX_SCRIPTS: usize = 4_000;
const MAX_CONSOLE: usize = 500;
const MAX_MUTATIONS: usize = 500;
const MAX_PARSER: usize = 2_000;
const MAX_TEXT: usize = 400;
/// No script, mutation, or console note for this long, after the load
/// function has returned, counts as quiet.
const QUIET_AFTER_MS: u64 = 2_000;

#[derive(Clone)]
enum Mode {
    Off,
    File(PathBuf),
    Console,
    Both(PathBuf),
}

impl Mode {
    fn file(&self) -> Option<&Path> {
        match self {
            Mode::File(path) | Mode::Both(path) => Some(path),
            Mode::Off | Mode::Console => None,
        }
    }

    fn console(&self) -> bool {
        matches!(self, Mode::Console | Mode::Both(_))
    }
}

struct Catalog {
    labels: HashMap<usize, String>,
    next_inline: usize,
    next_type: usize,
}

impl Catalog {
    fn clear(&mut self) {
        self.labels.clear();
        self.next_inline = 0;
        self.next_type = 0;
    }
}

thread_local! {
    static LOCAL_MODE: RefCell<Option<Mode>> = const { RefCell::new(None) };
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
    static PARKED: Cell<u64> = const { Cell::new(0) };
    static CATALOG: RefCell<Catalog> = RefCell::new(Catalog {
        labels: HashMap::new(),
        next_inline: 0,
        next_type: 0,
    });
    /// The render dump held the mutation hook. Released when the trace finishes.
    static DUMP_HELD: Cell<bool> = const { Cell::new(false) };
}

static ENV_MODE: Mutex<Mode> = Mutex::new(Mode::Off);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static HOOKS: AtomicUsize = AtomicUsize::new(0);
static SAMPLES: AtomicUsize = AtomicUsize::new(0);

fn read_env() -> Mode {
    let path = std::env::var("HIWAVE_TRACE_JSON")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let console = std::env::var("HIWAVE_CONSOLE_LOG").ok().as_deref() == Some("1");
    match (path, console) {
        (Some(path), true) => Mode::Both(PathBuf::from(path)),
        (Some(path), false) => Mode::File(PathBuf::from(path)),
        (None, true) => Mode::Console,
        (None, false) => Mode::Off,
    }
}

fn mode() -> Mode {
    LOCAL_MODE.with(|local| {
        local.borrow().clone().unwrap_or_else(|| {
            ENV_MODE
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .clone()
        })
    })
}

pub(crate) fn enabled() -> bool {
    !matches!(mode(), Mode::Off)
}

/// Read `HIWAVE_TRACE_JSON` and `HIWAVE_CONSOLE_LOG`. Called when an engine
/// is built. A process that never builds an engine does not trace.
pub(crate) fn arm_from_env() {
    *ENV_MODE.lock().unwrap_or_else(|poison| poison.into_inner()) = read_env();
}

/// `HIWAVE_REPLAY` is handled on the builder. This guard forces a file trace
/// on the calling thread so a test can load a page without exporting the
/// variable for the whole process.
pub struct ForceFile {
    _private: (),
}

impl Drop for ForceFile {
    fn drop(&mut self) {
        LOCAL_MODE.with(|local| *local.borrow_mut() = None);
    }
}

pub fn force_file_for_test(path: &Path) -> ForceFile {
    LOCAL_MODE.with(|local| *local.borrow_mut() = Some(Mode::File(path.to_path_buf())));
    ForceFile { _private: () }
}

struct ScriptEv {
    url: String,
    order: u32,
    event: &'static str,
    t_ms: u64,
}

struct Pending {
    url: String,
    event: &'static str,
}

struct FetchedNote {
    url: String,
    status: Option<u16>,
    fetched: bool,
    executed: bool,
    /// A module graph dependency, not a parser `<script>`.
    dependency: bool,
    reason: Option<&'static str>,
    /// Why a parser script was recorded when the four `reason`s do not say
    /// which kind of miss it was (`shadow tree`, `not a javascript type`,
    /// `inserted after the parser snapshot`).
    detail: Option<&'static str>,
}

struct ParserEv {
    blocked_on: &'static str,
    ms: u64,
    target: String,
}

struct Active {
    generation: u64,
    url: String,
    started: Instant,
    ready_state: Vec<(String, u64)>,
    dcl_ms: Option<u64>,
    load_ms: Option<u64>,
    scripts: Vec<ScriptEv>,
    fetched: Vec<FetchedNote>,
    console: Vec<(u64, String)>,
    mutations: Vec<(u64, &'static str, usize, String)>,
    parser: Vec<ParserEv>,
    render: Option<serde_json::Value>,
    next_order: u32,
    pending: Vec<Pending>,
    scope_ended: bool,
    last_activity: u64,
}

impl Active {
    fn elapsed(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

/// Holds the trace open until the load returns, including an early return.
pub struct Session {
    generation: u64,
}

impl Drop for Session {
    fn drop(&mut self) {
        on_scope_end(self.generation);
    }
}

fn on_scope_end(generation: u64) {
    let keep = ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(active) = slot.as_mut() else {
            return false;
        };
        if active.generation != generation {
            return false;
        }
        active.scope_ended = true;
        should_keep(active)
    });
    if keep {
        PARKED.with(|parked| parked.set(generation));
        flush_file();
        return;
    }
    crate::render_dump::write_quiet_if_due();
    finish(generation);
}

/// The load function has returned and the trace is still open: either
/// `readyState` is not `complete` yet (a yielded load) or a render dump was
/// asked for at a later timestamp.
fn should_keep(active: &Active) -> bool {
    if let Some(at_ms) = crate::render_dump::dump_at_ms() {
        if active.elapsed() < at_ms {
            return true;
        }
    }
    let complete = active.ready_state.iter().any(|(state, _)| state == "complete");
    !complete && active.dcl_ms.is_some()
}

pub(crate) fn is_parked() -> bool {
    PARKED.with(Cell::get) != 0
}

/// Hold the DOM mutation hook for the render dump, whether or not a trace
/// file is open. Paired with [`release_hook`] from the dump's watch, or from
/// [`finish`] when the watch was parked.
pub(crate) fn hold_hook() {
    if HOOKS.fetch_add(1, Ordering::Relaxed) == 0 {
        rustkit_dom::set_large_mutation_hook(Some(on_dom_mutation));
    }
}

pub(crate) fn release_hook() {
    if HOOKS.fetch_sub(1, Ordering::Relaxed) == 1 {
        rustkit_dom::set_large_mutation_hook(None);
    }
}

pub(crate) fn set_dump_held(held: bool) {
    DUMP_HELD.with(|cell| cell.set(held));
}

pub(crate) fn dump_held() -> bool {
    DUMP_HELD.with(Cell::get)
}

/// Milliseconds since navigation start, when a trace is open.
pub(crate) fn elapsed_ms() -> Option<u64> {
    ACTIVE.with(|slot| slot.borrow().as_ref().map(Active::elapsed))
}

/// Test hook: finish a trace that [`on_scope_end`] left open.
pub(crate) fn testing_finish_parked() {
    let generation = PARKED.with(Cell::get);
    if generation == 0 {
        return;
    }
    PARKED.with(|parked| parked.set(0));
    if dump_held() {
        release_hook();
        set_dump_held(false);
    }
    finish(generation);
}

/// A yielded load, or a dump time still in the future, left the trace open.
/// Finish it once the page is quiet or the dump time has passed.
pub(crate) fn poll_settle() {
    let generation = PARKED.with(Cell::get);
    if generation == 0 {
        return;
    }
    let ready = ACTIVE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|active| active.generation == generation && settle_ready(active))
    });
    if !ready {
        return;
    }
    crate::render_dump::write_quiet_if_due();
    if dump_held() {
        release_hook();
        set_dump_held(false);
    }
    PARKED.with(|parked| parked.set(0));
    finish(generation);
}

fn settle_ready(active: &Active) -> bool {
    if !active.scope_ended {
        return false;
    }
    if let Some(at_ms) = crate::render_dump::dump_at_ms() {
        return active.elapsed() >= at_ms;
    }
    let idle = active.elapsed().saturating_sub(active.last_activity) >= QUIET_AFTER_MS;
    let complete = active.ready_state.iter().any(|(state, _)| state == "complete");
    idle && (complete || active.dcl_ms.is_some())
}

fn touch(active: &mut Active) {
    active.last_activity = active.elapsed();
}

/// Start a trace for `url` when one of the env switches is on.
pub(crate) fn begin(url: &str) -> Option<Session> {
    if !enabled() {
        return None;
    }
    let parked = PARKED.with(Cell::get);
    if parked != 0 {
        PARKED.with(|parked_cell| parked_cell.set(0));
        if dump_held() {
            release_hook();
            set_dump_held(false);
        }
        finish(parked);
    }
    CATALOG.with(|catalog| catalog.borrow_mut().clear());
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    if SAMPLES.fetch_add(1, Ordering::Relaxed) == 0 {
        rustkit_js::set_js_stack_sample(true);
    }
    hold_hook();
    ACTIVE.with(|slot| {
        *slot.borrow_mut() = Some(Active {
            generation,
            url: url.to_string(),
            started: Instant::now(),
            ready_state: Vec::new(),
            dcl_ms: None,
            load_ms: None,
            scripts: Vec::new(),
            fetched: Vec::new(),
            console: Vec::new(),
            mutations: Vec::new(),
            parser: Vec::new(),
            render: None,
            next_order: 0,
            pending: Vec::new(),
            scope_ended: false,
            last_activity: 0,
        });
    });
    flush_file();
    Some(Session { generation })
}

fn finish(generation: u64) {
    let active = ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some(active) if active.generation == generation => slot.take(),
            _ => None,
        }
    });
    let Some(active) = active else {
        return;
    };
    let max_js_stack = rustkit_js::js_stack_sample();
    let active = {
        let mut active = active;
        fill_unexecuted(&mut active);
        active
    };
    if SAMPLES.fetch_sub(1, Ordering::Relaxed) == 1 {
        rustkit_js::set_js_stack_sample(false);
    }
    release_hook();
    CATALOG.with(|catalog| catalog.borrow_mut().clear());
    emit(&active, max_js_stack, true);
}

fn on_dom_mutation(kind: &str, nodes: usize, target: &str) {
    let kind = match kind {
        "remove" => "remove",
        _ => "add",
    };
    if kind == "add" && target.eq_ignore_ascii_case("MAIN") {
        crate::render_dump::note_main_insert();
    }
    note_mutation(kind, nodes, target);
}

pub(crate) fn note_ready(state: &str) {
    let flush = with_active(|active| {
        if active
            .ready_state
            .last()
            .is_some_and(|(seen, _)| seen == state)
        {
            return false;
        }
        let t_ms = active.elapsed();
        active.ready_state.push((state.to_string(), t_ms));
        if state == "interactive" {
            active.dcl_ms.get_or_insert(t_ms);
        }
        if state == "complete" {
            active.load_ms.get_or_insert(t_ms);
        }
        touch(active);
        true
    });
    if flush {
        flush_file();
    }
    // A yielded load parked at DOMContentLoaded finishes here once
    // `readyState` is `complete` and the page has gone quiet.
    poll_settle();
}

pub(crate) fn note_record(record: &ScriptRecord) {
    if !enabled() {
        return;
    }
    let url = record.source.as_str();
    let callback = url.starts_with("event:") || matches!(url, "timers" | "network" | "importmap");
    let ran = apply_outcome(url, record, callback);
    match &record.outcome {
        ScriptOutcome::Nomodule => drop_pending(url),
        ScriptOutcome::FetchFailed(_) | ScriptOutcome::Skipped(_) | ScriptOutcome::OverBudget => {
            drop_pending(url);
            if !callback {
                push_script(url, "error");
            }
        }
        ScriptOutcome::EngineFailed(failure) => {
            note_console(&engine_failure_text(url, failure));
        }
        ScriptOutcome::Threw(message) => {
            if ran {
                push_script(url, "execute");
                flush_pending(url);
            }
            note_console(message);
        }
        ScriptOutcome::Ran => {
            if ran {
                push_script(url, "execute");
                flush_pending(url);
            }
        }
    }
}

/// A parser script whose body is already in hand (`status` 200) or an inline
/// script (`status` none). Recorded before the runner so a load that returns
/// early still explains the ones it never started.
pub(crate) fn note_parser_script(url: &str, status: Option<u16>) {
    remember(url, true, status, false, false, None);
}

/// Record a script that already failed, without emitting an `execute` event.
/// The runner calls [`note_record`] again if it reaches the script.
pub(crate) fn note_observed(url: &str, outcome: &ScriptOutcome) {
    let record = ScriptRecord {
        source: url.to_string(),
        bytes: 0,
        elapsed_ms: 0,
        outcome: outcome.clone(),
    };
    let _ = apply_outcome(url, &record, false);
}

/// A module dependency fetched for `import()`. Not executed until the graph
/// that asked for it is evaluated.
pub(crate) fn note_module_dependency(url: &str, status: Option<u16>) {
    remember(url, true, status, true, false, None);
}

/// The shield (or another blocker) refused this script.
pub(crate) fn note_blocked(url: &str) {
    remember(url, false, None, false, false, Some("blocked"));
}

/// Set a reason before [`note_record`] when the outcome text does not carry
/// one (a module that failed to parse). A later execute clears it.
pub(crate) fn note_script_reason(url: &str, reason: &'static str) {
    remember(url, true, None, false, false, Some(reason));
}

/// The module dependencies fetched since the previous resolution either ran
/// with the graph (`executed`) or all failed for `reason`.
pub(crate) fn resolve_module_dependencies(executed: bool, reason: Option<&'static str>) {
    if !enabled() {
        return;
    }
    with_active(|active| {
        for note in &mut active.fetched {
            if !note.dependency || note.executed || note.reason.is_some() {
                continue;
            }
            if executed {
                note.executed = true;
                note.reason = None;
            } else if let Some(reason) = reason {
                note.reason = Some(reason);
            }
        }
    });
}

fn apply_outcome(url: &str, record: &ScriptRecord, callback: bool) -> bool {
    if callback {
        return false;
    }
    match &record.outcome {
        ScriptOutcome::Nomodule => false,
        ScriptOutcome::Ran => {
            remember(url, true, status_of(url, record), false, true, None);
            true
        }
        ScriptOutcome::Threw(message) if is_module_graph(message) => {
            remember(url, true, status_of(url, record), false, false, Some("module graph error"));
            false
        }
        ScriptOutcome::Threw(message) if is_waiting(message) => {
            remember(url, true, status_of(url, record), false, false, Some("waiting on a dependency"));
            false
        }
        ScriptOutcome::Threw(_) => {
            if reason_of(url).is_some() {
                false
            } else {
                remember(url, true, status_of(url, record), false, true, None);
                true
            }
        }
        ScriptOutcome::FetchFailed(failure) => {
            if is_blocked_failure(failure) {
                remember(url, false, status_of_failure(failure), false, false, Some("blocked"));
            } else {
                remember(url, false, status_of_failure(failure), false, false, None);
            }
            false
        }
        ScriptOutcome::Skipped(_) | ScriptOutcome::OverBudget => {
            remember(url, true, status_of(url, record), false, false, Some("never reached"));
            false
        }
        ScriptOutcome::EngineFailed(_) => false,
    }
}

fn status_of(url: &str, record: &ScriptRecord) -> Option<u16> {
    if url.starts_with("inline#") || record.bytes == 0 {
        None
    } else {
        Some(200)
    }
}

fn status_of_failure(failure: &FetchFailure) -> Option<u16> {
    match failure {
        FetchFailure::Status(status) => Some(*status),
        FetchFailure::Transport(_) => None,
    }
}

fn is_module_graph(message: &str) -> bool {
    message.starts_with("module graph failed")
}

fn is_waiting(message: &str) -> bool {
    message.contains("module imports were not loaded")
}

fn is_blocked_failure(failure: &FetchFailure) -> bool {
    match failure {
        FetchFailure::Transport(message) => {
            let message = message.to_ascii_lowercase();
            message.contains("blocked") || message.contains("shield")
        }
        FetchFailure::Status(_) => false,
    }
}

fn reason_of(url: &str) -> Option<&'static str> {
    with_active(|active| {
        active
            .fetched
            .iter()
            .find(|note| note.url == url)
            .and_then(|note| note.reason)
    })
}

fn remember(
    url: &str,
    fetched: bool,
    status: Option<u16>,
    dependency: bool,
    executed: bool,
    reason: Option<&'static str>,
) {
    if !enabled() || url.is_empty() {
        return;
    }
    with_active(|active| {
        if let Some(note) = active.fetched.iter_mut().find(|note| note.url == url) {
            if fetched {
                note.fetched = true;
            }
            if status.is_some() {
                note.status = status;
            }
            if dependency {
                note.dependency = true;
            }
            if executed {
                note.executed = true;
                note.reason = None;
            } else if let Some(reason) = reason {
                if !note.executed {
                    note.reason = Some(reason);
                }
            }
            return;
        }
        if active.fetched.len() >= MAX_SCRIPTS {
            return;
        }
        active.fetched.push(FetchedNote {
            url: url.to_string(),
            status,
            fetched,
            executed,
            dependency,
            reason: if executed { None } else { reason },
            detail: None,
        });
    });
}

fn fill_unexecuted(active: &mut Active) {
    for note in &mut active.fetched {
        if note.executed || note.reason.is_some() || !note.fetched {
            continue;
        }
        note.reason = Some(if note.dependency {
            "waiting on a dependency"
        } else {
            "never reached"
        });
    }
}

/// Same decision as `hiwave_shield::shield_choice_label`. The engine does not
/// depend on the shield crate; the strings are covered by tests on both sides.
fn shield_choice_label(shield: Option<&str>, shield_off_set: bool, replay: Option<&str>) -> &'static str {
    let blocks = matches!(
        replay.map(str::trim),
        Some(raw) if raw.starts_with("http://") || raw.starts_with("https://")
    );
    match shield
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("off" | "0" | "false") => return "off (HIWAVE_SHIELD=off)",
        Some("on" | "1" | "true") => {
            return if blocks {
                "on (filter lists stay cached under HIWAVE_REPLAY)"
            } else {
                "on"
            };
        }
        _ => {}
    }
    if shield_off_set {
        return "off (HIWAVE_SHIELD_OFF)";
    }
    if blocks {
        return "off (HIWAVE_REPLAY defaults HIWAVE_SHIELD=off)";
    }
    "on"
}

fn shield_label() -> &'static str {
    let shield = std::env::var("HIWAVE_SHIELD").ok();
    let off = std::env::var("HIWAVE_SHIELD_OFF").is_ok();
    let replay = std::env::var("HIWAVE_REPLAY").ok();
    shield_choice_label(shield.as_deref(), off, replay.as_deref())
}

fn engine_failure_text(url: &str, failure: &EngineFailure) -> String {
    match failure {
        EngineFailure::Panic => format!("{url} JS engine panic"),
        EngineFailure::ImportMap(_) => format!("{url} import map rejected"),
    }
}

/// `load` or `error` fired at a script element. A matching `execute` that has
/// not been logged yet keeps this until [`note_record`] flushes it, so the
/// trace order is execute then load even when the element hears `load` first.
pub(crate) fn note_element_event(url: &str, event: &str) {
    let event = match event {
        "load" => "load",
        "error" => "error",
        _ => return,
    };
    if !enabled() {
        return;
    }
    let mut flush = false;
    with_active(|active| {
        if last_event_is(active, url, event) {
            return;
        }
        let executed = active
            .scripts
            .iter()
            .any(|script| script.url == url && script.event == "execute");
        // A script that will not run (module graph error, blocked, …) still
        // records the element event. Waiting for `execute` would drop it.
        let explained = active
            .fetched
            .iter()
            .any(|note| note.url == url && note.reason.is_some() && !note.executed);
        if executed || explained {
            push_script_on(active, url, event);
            flush = true;
        } else {
            active.pending.push(Pending {
                url: url.to_string(),
                event,
            });
        }
    });
    if flush {
        flush_file();
    }
    poll_settle();
}

pub(crate) fn note_console(text: &str) {
    let flush = with_active(|active| {
        if active.console.len() >= MAX_CONSOLE {
            return false;
        }
        let text = clamp(text);
        if text.is_empty() {
            return false;
        }
        active.console.push((active.elapsed(), text));
        touch(active);
        true
    });
    if flush {
        flush_file();
    }
}

pub(crate) fn note_mutation(kind: &'static str, nodes: usize, target: &str) {
    if nodes <= 50 {
        return;
    }
    let flush = with_active(|active| {
        if active.mutations.len() >= MAX_MUTATIONS {
            return false;
        }
        active
            .mutations
            .push((active.elapsed(), kind, nodes, target.to_string()));
        touch(active);
        true
    });
    if flush {
        flush_file();
    }
}

/// The latest render dump. Replaced as later commits dump again.
pub(crate) fn set_render(value: serde_json::Value) {
    let flush = with_active(|active| {
        active.render = Some(value);
        true
    });
    if flush {
        flush_file();
    }
}

/// One stretch where the load could not parse because it was waiting on
/// `stylesheet`, `script fetch`, or `script execute`.
pub(crate) fn note_parser_pause(blocked_on: &'static str, ms: u64, target: &str) {
    if !enabled() {
        return;
    }
    let target = clamp(target);
    let flush = with_active(|active| {
        if active.parser.len() >= MAX_PARSER {
            return false;
        }
        active.parser.push(ParserEv {
            blocked_on,
            ms,
            target,
        });
        touch(active);
        true
    });
    if flush {
        flush_file();
    }
}

/// The label already given to this script element, if the catalog saw it.
pub(crate) fn script_label(node: usize) -> Option<String> {
    CATALOG.with(|catalog| catalog.borrow().labels.get(&node).cloned())
}

/// Remember `label` for `node`. `false` when the node was already labeled.
pub(crate) fn remember_script_label(node: usize, label: String) -> bool {
    CATALOG.with(|catalog| {
        let mut catalog = catalog.borrow_mut();
        if catalog.labels.contains_key(&node) {
            return false;
        }
        catalog.labels.insert(node, label);
        true
    })
}

/// The next `inline#N`, counting every javascript script in discovery order.
pub(crate) fn next_inline_label() -> String {
    CATALOG.with(|catalog| {
        let mut catalog = catalog.borrow_mut();
        catalog.next_inline += 1;
        format!("inline#{}", catalog.next_inline)
    })
}

/// A `<script>` whose type is not javascript. It does not take an `inline#N`.
pub(crate) fn next_type_label(essence: &str) -> String {
    CATALOG.with(|catalog| {
        let mut catalog = catalog.borrow_mut();
        catalog.next_type += 1;
        let essence = if essence.is_empty() { "empty" } else { essence };
        format!("type:{essence}#{}", catalog.next_type)
    })
}

pub(crate) fn note_detail(url: &str, detail: &'static str) {
    if !enabled() {
        return;
    }
    with_active(|active| {
        if let Some(note) = active.fetched.iter_mut().find(|note| note.url == url) {
            if note.detail.is_none() {
                note.detail = Some(detail);
            }
        }
    });
}

/// A script-inserted inline script ran (`indirectEval`). Record it under its
/// catalog label, or the next `inline#N` if the parser snapshot missed it.
pub(crate) fn mark_node_executed(node: usize) {
    if !enabled() {
        return;
    }
    let already = CATALOG.with(|catalog| catalog.borrow().labels.get(&node).cloned());
    let label = match already {
        Some(label) => label,
        None => {
            let label = next_inline_label();
            remember_script_label(node, label.clone());
            label
        }
    };
    let executed = with_active(|active| {
        active
            .fetched
            .iter()
            .any(|note| note.url == label && note.executed)
    });
    if executed {
        return;
    }
    note_parser_script(&label, None);
    note_detail(&label, "script-inserted inline");
    note_record(&ScriptRecord {
        source: label,
        bytes: 0,
        elapsed_ms: 0,
        outcome: ScriptOutcome::Ran,
    });
}

fn drop_pending(url: &str) {
    with_active(|active| {
        active.pending.retain(|pending| pending.url != url);
    });
}

fn flush_pending(url: &str) {
    with_active(|active| {
        let pending: Vec<Pending> = active.pending.drain(..).collect();
        let mut keep = Vec::new();
        for item in pending {
            if item.url == url {
                if !last_event_is(active, url, item.event) {
                    push_script_on(active, url, item.event);
                }
            } else {
                keep.push(item);
            }
        }
        active.pending = keep;
    });
}

fn push_script(url: &str, event: &'static str) {
    let flush = with_active(|active| {
        push_script_on(active, url, event);
        true
    });
    if flush {
        flush_file();
    }
}

fn push_script_on(active: &mut Active, url: &str, event: &str) {
    if active.scripts.len() >= MAX_SCRIPTS {
        return;
    }
    let event: &'static str = match event {
        "load" => "load",
        "error" => "error",
        _ => "execute",
    };
    let order = if event == "execute" {
        let order = active.next_order;
        active.next_order = active.next_order.saturating_add(1);
        order
    } else {
        active
            .scripts
            .iter()
            .rev()
            .find(|script| script.url == url && script.event == "execute")
            .map(|script| script.order)
            .unwrap_or_else(|| {
                let order = active.next_order;
                active.next_order = active.next_order.saturating_add(1);
                order
            })
    };
    let t_ms = active.elapsed();
    touch(active);
    active.scripts.push(ScriptEv {
        url: url.to_string(),
        order,
        event,
        t_ms,
    });
}

fn last_event_is(active: &Active, url: &str, event: &str) -> bool {
    active
        .scripts
        .iter()
        .rev()
        .find(|script| script.url == url)
        .is_some_and(|script| script.event == event)
}

fn with_active<T>(body: impl FnOnce(&mut Active) -> T) -> T
where
    T: Default,
{
    ACTIVE.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_mut() {
            Some(active) => body(active),
            None => T::default(),
        }
    })
}

fn clamp(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().take(MAX_TEXT) {
        if ch == '\n' || ch == '\r' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn flush_file() {
    let Some(active) = ACTIVE.with(|slot| slot.borrow().as_ref().map(|active| snapshot(active)))
    else {
        return;
    };
    // max_js_stack is filled in at finish; intermediate files say 0 so a
    // hang still leaves the events gathered so far.
    emit_snapshot(&active, 0, false);
}

fn snapshot(active: &Active) -> Snapshot {
    Snapshot {
        url: active.url.clone(),
        ready_state: active.ready_state.clone(),
        dcl_ms: active.dcl_ms,
        load_ms: active.load_ms,
        scripts: active
            .scripts
            .iter()
            .map(|script| (script.url.clone(), script.order, script.event, script.t_ms))
            .collect(),
        fetched: active
            .fetched
            .iter()
            .map(|note| {
                (
                    note.url.clone(),
                    note.status,
                    note.fetched,
                    note.executed,
                    note.reason,
                    note.detail,
                )
            })
            .collect(),
        console: active.console.clone(),
        mutations: active
            .mutations
            .iter()
            .map(|(t_ms, kind, nodes, target)| (*t_ms, *kind, *nodes, target.clone()))
            .collect(),
        parser: active
            .parser
            .iter()
            .map(|pause| (pause.blocked_on, pause.ms, pause.target.clone()))
            .collect(),
        render: active.render.clone(),
    }
}

struct Snapshot {
    url: String,
    ready_state: Vec<(String, u64)>,
    dcl_ms: Option<u64>,
    load_ms: Option<u64>,
    scripts: Vec<(String, u32, &'static str, u64)>,
    fetched: Vec<(String, Option<u16>, bool, bool, Option<&'static str>, Option<&'static str>)>,
    console: Vec<(u64, String)>,
    mutations: Vec<(u64, &'static str, usize, String)>,
    parser: Vec<(&'static str, u64, String)>,
    render: Option<serde_json::Value>,
}

fn emit(active: &Active, max_js_stack: usize, console: bool) {
    emit_snapshot(&snapshot(active), max_js_stack, console);
}

fn emit_snapshot(active: &Snapshot, max_js_stack: usize, console: bool) {
    let trace = TraceJson {
        browser: "hiwave",
        url: &active.url,
        ready_state: active
            .ready_state
            .iter()
            .map(|(state, t_ms)| ReadyJson { state, t_ms: *t_ms })
            .collect(),
        dcl_ms: active.dcl_ms,
        load_ms: active.load_ms,
        scripts: active
            .scripts
            .iter()
            .map(|(url, order, event, t_ms)| ScriptJson {
                url,
                order: *order,
                event,
                t_ms: *t_ms,
            })
            .collect(),
        fetched_scripts: active
            .fetched
            .iter()
            .map(|(url, status, fetched, executed, reason, detail)| FetchedJson {
                url,
                status: *status,
                fetched: *fetched,
                executed: *executed,
                reason: *reason,
                detail: *detail,
            })
            .collect(),
        shield: shield_label(),
        console: active
            .console
            .iter()
            .map(|(t_ms, text)| ConsoleJson {
                level: "error",
                text,
                t_ms: *t_ms,
            })
            .collect(),
        mutations: active
            .mutations
            .iter()
            .map(|(t_ms, kind, nodes, target)| MutationJson {
                t_ms: *t_ms,
                kind,
                nodes: *nodes,
                target,
            })
            .collect(),
        parser: active
            .parser
            .iter()
            .map(|(blocked_on, ms, target)| ParserJson {
                blocked_on,
                ms: *ms,
                target,
            })
            .collect(),
        render: active.render.as_ref(),
        max_js_stack,
    };
    let mode = mode();
    if let Some(path) = mode.file() {
        if let Ok(body) = serde_json::to_string_pretty(&trace) {
            write_atomic(path, &body);
        }
    }
    if console && mode.console() {
        if let Ok(body) = serde_json::to_string(&trace) {
            eprintln!("PAGE_TRACE {body}");
        }
    }
}

fn write_atomic(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[derive(Serialize)]
struct TraceJson<'a> {
    browser: &'static str,
    url: &'a str,
    ready_state: Vec<ReadyJson<'a>>,
    dcl_ms: Option<u64>,
    load_ms: Option<u64>,
    scripts: Vec<ScriptJson<'a>>,
    fetched_scripts: Vec<FetchedJson<'a>>,
    shield: &'a str,
    console: Vec<ConsoleJson<'a>>,
    mutations: Vec<MutationJson<'a>>,
    parser: Vec<ParserJson<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    render: Option<&'a serde_json::Value>,
    max_js_stack: usize,
}

#[derive(Serialize)]
struct ReadyJson<'a> {
    state: &'a str,
    t_ms: u64,
}

#[derive(Serialize)]
struct ScriptJson<'a> {
    url: &'a str,
    order: u32,
    event: &'a str,
    t_ms: u64,
}

#[derive(Serialize)]
struct FetchedJson<'a> {
    url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
    fetched: bool,
    executed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

#[derive(Serialize)]
struct ParserJson<'a> {
    blocked_on: &'a str,
    ms: u64,
    target: &'a str,
}

#[derive(Serialize)]
struct ConsoleJson<'a> {
    level: &'static str,
    text: &'a str,
    t_ms: u64,
}

#[derive(Serialize)]
struct MutationJson<'a> {
    t_ms: u64,
    #[serde(rename = "type")]
    kind: &'a str,
    nodes: usize,
    target: &'a str,
}

/// Silence unused-import warnings if a build strips the match arms that name
/// `FetchFailure`. The variant is matched by reference above.
#[allow(dead_code)]
fn _fetch_failure_is_an_error(failure: &FetchFailure) -> bool {
    matches!(
        failure,
        FetchFailure::Status(_) | FetchFailure::Transport(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(source: &str, outcome: ScriptOutcome) -> ScriptRecord {
        ScriptRecord {
            source: source.to_string(),
            bytes: 1,
            elapsed_ms: 0,
            outcome,
        }
    }

    #[test]
    fn a_forced_trace_writes_the_shared_schema_and_ignores_small_mutations() {
        let path =
            std::env::temp_dir().join(format!("hiwave-trace-{}-unit.json", std::process::id()));
        let _force = force_file_for_test(&path);
        let _session = begin("https://example.test/").expect("trace on");
        note_ready("loading");
        note_record(&record("https://example.test/app.js", ScriptOutcome::Ran));
        note_element_event("https://example.test/app.js", "load");
        note_record(&record(
            "https://example.test/missing.js",
            ScriptOutcome::FetchFailed(FetchFailure::Status(404)),
        ));
        note_element_event("https://example.test/missing.js", "error");
        note_record(&record(
            "inline#2",
            ScriptOutcome::Threw("TypeError: x".into()),
        ));
        note_console("synthetic-error");
        note_mutation("add", 50, "DIV");
        note_mutation("add", 51, "BODY");
        note_ready("interactive");
        note_ready("complete");
        drop(_session);

        let body = std::fs::read_to_string(&path).expect("trace file");
        let _ = std::fs::remove_file(&path);
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["browser"], "hiwave");
        assert_eq!(value["url"], "https://example.test/");
        let states: Vec<&str> = value["ready_state"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["loading", "interactive", "complete"]);
        assert!(value["dcl_ms"].is_number());
        assert!(value["load_ms"].is_number());
        let scripts = value["scripts"].as_array().unwrap();
        assert_eq!(scripts[0]["url"], "https://example.test/app.js");
        assert_eq!(scripts[0]["event"], "execute");
        assert_eq!(scripts[0]["order"], 0);
        assert_eq!(scripts[1]["event"], "load");
        assert_eq!(scripts[1]["order"], 0);
        assert_eq!(scripts[2]["url"], "https://example.test/missing.js");
        assert_eq!(scripts[2]["event"], "error");
        assert_eq!(scripts[3]["url"], "inline#2");
        assert_eq!(scripts[3]["event"], "execute");
        let console = value["console"].as_array().unwrap();
        assert_eq!(console[0]["text"], "TypeError: x");
        assert_eq!(console[1]["text"], "synthetic-error");
        let mutations = value["mutations"].as_array().unwrap();
        assert_eq!(mutations.len(), 1);
        assert_eq!(mutations[0]["type"], "add");
        assert_eq!(mutations[0]["nodes"], 51);
        assert_eq!(mutations[0]["target"], "BODY");
        assert!(value["max_js_stack"].is_number());
    }

    #[test]
    fn an_element_event_that_arrives_before_execute_is_logged_after_it() {
        let path =
            std::env::temp_dir().join(format!("hiwave-trace-{}-order.json", std::process::id()));
        let _force = force_file_for_test(&path);
        let _session = begin("https://example.test/ordered").unwrap();
        note_element_event("https://example.test/inserted.js", "load");
        note_record(&record(
            "https://example.test/inserted.js",
            ScriptOutcome::Ran,
        ));
        drop(_session);
        let body = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        let scripts = value["scripts"].as_array().unwrap();
        assert_eq!(scripts[0]["event"], "execute");
        assert_eq!(scripts[1]["event"], "load");
        assert_eq!(scripts[0]["order"], scripts[1]["order"]);
    }

    #[test]
    fn a_script_that_never_runs_records_why() {
        let path = std::env::temp_dir().join(format!("hiwave-trace-{}-why.json", std::process::id()));
        let _force = force_file_for_test(&path);
        let _session = begin("https://example.test/why").unwrap();
        note_parser_script("inline#1", None);
        note_parser_script("https://example.test/ran.js", Some(200));
        note_record(&record("https://example.test/ran.js", ScriptOutcome::Ran));
        note_parser_script("https://example.test/later.js", Some(200));
        note_blocked("https://www.googletagmanager.com/gtag/js");
        note_module_dependency("https://www.redditstatic.com/js/concat?chunks=aaa", Some(200));
        note_script_reason("inline#9", "module graph error");
        note_record(&record(
            "inline#9",
            ScriptOutcome::Threw("Unexpected token".into()),
        ));
        note_record(&record(
            "https://example.test/graph.js",
            ScriptOutcome::Threw("module graph failed: boom".into()),
        ));
        note_record(&record(
            "https://example.test/wait.js",
            ScriptOutcome::Threw("module imports were not loaded".into()),
        ));
        drop(_session);
        let body = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        let fetched = value["fetched_scripts"].as_array().unwrap();
        let reason = |url: &str| {
            fetched
                .iter()
                .find(|item| item["url"] == url)
                .unwrap_or_else(|| panic!("missing {url}"))
        };
        assert_eq!(reason("inline#1")["executed"], false);
        assert_eq!(reason("inline#1")["reason"], "never reached");
        assert_eq!(reason("https://example.test/ran.js")["executed"], true);
        assert!(reason("https://example.test/ran.js").get("reason").is_none());
        assert_eq!(reason("https://example.test/later.js")["reason"], "never reached");
        assert_eq!(reason("https://www.googletagmanager.com/gtag/js")["reason"], "blocked");
        assert_eq!(reason("https://www.googletagmanager.com/gtag/js")["fetched"], false);
        assert_eq!(
            reason("https://www.redditstatic.com/js/concat?chunks=aaa")["reason"],
            "waiting on a dependency"
        );
        assert_eq!(reason("inline#9")["reason"], "module graph error");
        assert_eq!(reason("inline#9")["executed"], false);
        assert_eq!(reason("https://example.test/graph.js")["reason"], "module graph error");
        assert_eq!(reason("https://example.test/wait.js")["reason"], "waiting on a dependency");
        let scripts = value["scripts"].as_array().unwrap();
        assert!(scripts.iter().any(|script| script["url"] == "https://example.test/ran.js" && script["event"] == "execute"));
        assert!(!scripts.iter().any(|script| script["url"] == "inline#9" && script["event"] == "execute"));
        assert!(value["shield"].is_string());
    }

    #[test]
    fn shield_choice_matches_the_replay_default() {
        assert_eq!(
            shield_choice_label(None, false, Some("http://127.0.0.1:9")),
            "off (HIWAVE_REPLAY defaults HIWAVE_SHIELD=off)"
        );
        assert_eq!(
            shield_choice_label(Some("on"), false, Some("https://127.0.0.1:9")),
            "on (filter lists stay cached under HIWAVE_REPLAY)"
        );
        assert_eq!(shield_choice_label(Some("off"), false, None), "off (HIWAVE_SHIELD=off)");
        assert_eq!(shield_choice_label(None, true, None), "off (HIWAVE_SHIELD_OFF)");
        assert_eq!(shield_choice_label(None, false, None), "on");
        assert_eq!(shield_choice_label(None, false, Some("")), "on");
    }

    #[test]
    fn begin_without_a_switch_does_not_open_a_session() {
        if std::env::var("HIWAVE_TRACE_JSON")
            .ok()
            .is_some_and(|v| !v.trim().is_empty())
            || std::env::var("HIWAVE_CONSOLE_LOG").ok().as_deref() == Some("1")
        {
            return;
        }
        LOCAL_MODE.with(|local| *local.borrow_mut() = None);
        assert!(begin("https://example.test/").is_none());
    }

    #[test]
    fn replay_proxy_stays_off_unless_the_env_is_an_http_url() {
        let fresh = crate::EngineBuilder::new();
        assert!(fresh.config().replay_proxy.is_none());
        assert!(crate::EngineBuilder::new()
            .replay_proxy_from_str("")
            .config()
            .replay_proxy
            .is_none());
        assert!(crate::EngineBuilder::new()
            .replay_proxy_from_str("not a url")
            .config()
            .replay_proxy
            .is_none());
        assert!(crate::EngineBuilder::new()
            .replay_proxy_from_str("ftp://127.0.0.1:9")
            .config()
            .replay_proxy
            .is_none());
        let set = crate::EngineBuilder::new().replay_proxy_from_str("http://127.0.0.1:9/replay");
        assert_eq!(
            set.config().replay_proxy.as_ref().map(url::Url::as_str),
            Some("http://127.0.0.1:9/replay")
        );
        let kept = crate::EngineBuilder::new()
            .replay_proxy(Some(url::Url::parse("http://127.0.0.1:1/").unwrap()))
            .replay_proxy_from_str("http://127.0.0.1:2/");
        assert_eq!(
            kept.config().replay_proxy.as_ref().map(url::Url::as_str),
            Some("http://127.0.0.1:1/")
        );
        if std::env::var("HIWAVE_REPLAY").is_err() {
            assert!(crate::EngineBuilder::new()
                .replay_proxy_from_env()
                .config()
                .replay_proxy
                .is_none());
        }
    }

    struct FinishParked;
    impl Drop for FinishParked {
        fn drop(&mut self) {
            testing_finish_parked();
        }
    }

    #[test]
    fn a_yielded_load_keeps_the_trace_open_past_dom_content_loaded() {
        let path = std::env::temp_dir().join(format!(
            "hiwave-trace-{}-parked.json",
            std::process::id()
        ));
        let _force = force_file_for_test(&path);
        let _finish = FinishParked;
        let session = begin("https://example.test/yielded").expect("trace on");
        note_ready("loading");
        note_ready("interactive");
        note_parser_pause("script fetch", 13, "page scripts");
        note_parser_script("inline#7", None);
        note_detail("inline#7", "shadow tree");
        drop(session);
        assert!(is_parked(), "DOMContentLoaded without complete parks the trace");
        note_console("after-park");
        drop(_finish);

        let body = std::fs::read_to_string(&path).expect("trace file");
        let _ = std::fs::remove_file(&path);
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        let states: Vec<&str> = value["ready_state"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["loading", "interactive"]);
        assert!(value["dcl_ms"].is_number());
        assert!(value["load_ms"].is_null());
        let console = value["console"].as_array().unwrap();
        assert!(console.iter().any(|item| item["text"] == "after-park"));
        let parser = value["parser"].as_array().unwrap();
        assert_eq!(parser[0]["blocked_on"], "script fetch");
        assert_eq!(parser[0]["ms"], 13);
        assert_eq!(parser[0]["target"], "page scripts");
        let fetched = value["fetched_scripts"].as_array().unwrap();
        let inline = fetched.iter().find(|item| item["url"] == "inline#7").unwrap();
        assert_eq!(inline["executed"], false);
        assert_eq!(inline["reason"], "never reached");
        assert_eq!(inline["detail"], "shadow tree");
        assert_eq!(value["max_js_stack"], 0);
        assert!(!body.contains("secret-cookie-value"));
    }

    #[cfg(feature = "headless")]
    fn trace_site(
        path: &str,
        _cookie: &str,
    ) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/" => (
                200,
                "text/html",
                vec![],
                "<!doctype html><html><body><script src=\"/app.js\"></script><script>\n\
                 var d = document.createElement('div');\n\
                 for (var i = 0; i < 60; i++) d.appendChild(document.createElement('span'));\n\
                 document.body.appendChild(d);\n\
                 </script></body></html>"
                    .into(),
            ),
            "/app.js" => (
                200,
                "text/javascript",
                vec![],
                "console.error('synthetic-error');".into(),
            ),
            _ => (404, "text/plain", vec![], "missing".into()),
        }
    }

    #[cfg(feature = "headless")]
    #[test]
    fn a_page_load_writes_ready_state_scripts_console_and_a_large_mutation() {
        let path =
            std::env::temp_dir().join(format!("hiwave-trace-{}-load.json", std::process::id()));
        let _force = force_file_for_test(&path);
        let server = crate::error_page_script_tests::serve(trace_site);
        let (mut engine, view, runtime) = crate::error_page_script_tests::engine();
        runtime
            .block_on(engine.load_url(view, server.url("/")))
            .expect("load");
        let body = std::fs::read_to_string(&path).expect("trace file");
        let _ = std::fs::remove_file(&path);
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["browser"], "hiwave");
        let states: Vec<&str> = value["ready_state"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["loading", "interactive", "complete"]);
        assert!(value["dcl_ms"].is_number() && value["load_ms"].is_number());
        let scripts = value["scripts"].as_array().unwrap();
        assert!(scripts.iter().any(|script| {
            script["event"] == "execute" && script["url"].as_str().unwrap().ends_with("/app.js")
        }));
        assert!(scripts
            .iter()
            .any(|script| script["event"] == "execute" && script["url"] == "inline#2"));
        let console = value["console"].as_array().unwrap();
        assert!(console
            .iter()
            .any(|item| item["text"].as_str().unwrap().contains("synthetic-error")));
        let mutations = value["mutations"].as_array().unwrap();
        assert!(mutations.iter().any(|item| {
            item["type"] == "add"
                && item["target"] == "BODY"
                && item["nodes"].as_u64().unwrap() > 50
        }));
        assert!(value["max_js_stack"].as_u64().unwrap() >= 1);
    }

    #[cfg(feature = "headless")]
    fn render_site(
        path: &str,
        _cookie: &str,
    ) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/" => (
                200,
                "text/html",
                vec![],
                "<!doctype html><html><body>\
                 <script type=\"application/json\">{\"k\":1}</script>\
                 <script>window.ranInline = true;</script>\
                 <main id=\"main-content\">\
                   <shreddit-post class=\"post\"><span>a</span></shreddit-post>\
                 </main>\
                 <script>\
                   var holder = document.createElement('div');\
                   for (var i = 0; i < 60; i++) holder.appendChild(document.createElement('i'));\
                   document.getElementById('main-content').appendChild(holder);\
                 </script>\
                 </body></html>"
                    .into(),
            ),
            _ => (404, "text/plain", vec![], "missing".into()),
        }
    }

    #[cfg(feature = "headless")]
    #[test]
    fn a_render_dump_records_main_and_an_unrun_inline_script() {
        let trace_path = std::env::temp_dir().join(format!(
            "hiwave-trace-{}-render.json",
            std::process::id()
        ));
        let dump_path = std::env::temp_dir().join(format!(
            "hiwave-render-{}-dump.json",
            std::process::id()
        ));
        let _trace = force_file_for_test(&trace_path);
        let _dump = crate::render_dump::force_for_test(&dump_path);
        let server = crate::error_page_script_tests::serve(render_site);
        let (mut engine, view, runtime) = crate::error_page_script_tests::engine();
        runtime
            .block_on(engine.load_url(view, server.url("/")))
            .expect("load");
        let trace_body = std::fs::read_to_string(&trace_path).expect("trace file");
        let dump_body = std::fs::read_to_string(&dump_path).expect("render dump");
        let _ = std::fs::remove_file(&trace_path);
        let _ = std::fs::remove_file(&dump_path);
        assert!(!trace_body.contains("secret-cookie-value"));
        assert!(!dump_body.contains("secret-cookie-value"));
        let trace: serde_json::Value = serde_json::from_str(&trace_body).unwrap();
        let dump: serde_json::Value = serde_json::from_str(&dump_body).unwrap();
        let dumps = dump["dumps"].as_array().expect("dumps");
        assert!(dumps.iter().any(|item| item["when"] == "after-main-insert"));
        let quiet = dumps
            .iter()
            .find(|item| item["when"] == "quiet")
            .expect("quiet dump");
        assert_eq!(quiet["main"]["tag"], "main");
        assert_eq!(quiet["main"]["id"], "main-content");
        let post = quiet["main"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|child| child["tag"] == "shreddit-post")
            .expect("shreddit-post");
        assert_eq!(post["custom_element"], "undefined");
        assert_eq!(post["class"], "post");
        assert!(post["hidden"] == false);
        assert!(post["box"].is_object() || post["box"] == "no box");
        assert!(post["in_display_list"].is_boolean());
        assert!(post["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|child| child["tag"] == "span"));
        assert!(quiet["display_list_items_in_main"].is_number());
        let fetched = trace["fetched_scripts"].as_array().unwrap();
        let data = fetched
            .iter()
            .find(|item| item["url"] == "type:application/json#1")
            .expect("non-javascript script");
        assert_eq!(data["executed"], false);
        assert_eq!(data["reason"], "never reached");
        assert_eq!(data["detail"], "not a javascript type");
        let scripts = trace["scripts"].as_array().unwrap();
        assert!(scripts
            .iter()
            .any(|script| script["event"] == "execute" && script["url"] == "inline#1"));
        let parser = trace["parser"].as_array().unwrap();
        assert!(parser
            .iter()
            .any(|pause| pause["blocked_on"] == "script execute"));
        assert!(parser
            .iter()
            .any(|pause| pause["blocked_on"] == "stylesheet" || pause["blocked_on"] == "script fetch"));
        assert_eq!(trace["render"]["main"]["tag"], "main");
    }
}
