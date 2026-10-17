//! One load's trace, in the schema `tools/replay/diff_trace.py` compares with
//! Chrome. Off unless `HIWAVE_TRACE_JSON` is a path or `HIWAVE_CONSOLE_LOG=1`.
//!
//! Times are milliseconds from the start of the navigation. `scripts` records
//! each `<script>` as `execute` when it runs and `load` or `error` when the
//! element is told; a fetch that never ran is `error` only. `mutations` are
//! subtree inserts and removes of more than 50 nodes (the same threshold as
//! the Chrome probe). `max_js_stack` is the deepest JS call stack during the
//! load, not the native host-call depth.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

use crate::{EngineFailure, FetchFailure, ScriptOutcome, ScriptRecord};

const MAX_SCRIPTS: usize = 4_000;
const MAX_CONSOLE: usize = 500;
const MAX_MUTATIONS: usize = 500;
const MAX_TEXT: usize = 400;

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

thread_local! {
    static LOCAL_MODE: RefCell<Option<Mode>> = const { RefCell::new(None) };
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
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

struct Active {
    generation: u64,
    url: String,
    started: Instant,
    ready_state: Vec<(String, u64)>,
    dcl_ms: Option<u64>,
    load_ms: Option<u64>,
    scripts: Vec<ScriptEv>,
    console: Vec<(u64, String)>,
    mutations: Vec<(u64, &'static str, usize, String)>,
    next_order: u32,
    pending: Vec<Pending>,
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
        finish(self.generation);
    }
}

/// Start a trace for `url` when one of the env switches is on.
pub(crate) fn begin(url: &str) -> Option<Session> {
    if !enabled() {
        return None;
    }
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    if SAMPLES.fetch_add(1, Ordering::Relaxed) == 0 {
        rustkit_js::set_js_stack_sample(true);
    }
    if HOOKS.fetch_add(1, Ordering::Relaxed) == 0 {
        rustkit_dom::set_large_mutation_hook(Some(on_dom_mutation));
    }
    ACTIVE.with(|slot| {
        *slot.borrow_mut() = Some(Active {
            generation,
            url: url.to_string(),
            started: Instant::now(),
            ready_state: Vec::new(),
            dcl_ms: None,
            load_ms: None,
            scripts: Vec::new(),
            console: Vec::new(),
            mutations: Vec::new(),
            next_order: 0,
            pending: Vec::new(),
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
    if SAMPLES.fetch_sub(1, Ordering::Relaxed) == 1 {
        rustkit_js::set_js_stack_sample(false);
    }
    if HOOKS.fetch_sub(1, Ordering::Relaxed) == 1 {
        rustkit_dom::set_large_mutation_hook(None);
    }
    emit(&active, max_js_stack, true);
}

fn on_dom_mutation(kind: &str, nodes: usize, target: &str) {
    let kind = match kind {
        "remove" => "remove",
        _ => "add",
    };
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
        true
    });
    if flush {
        flush_file();
    }
}

pub(crate) fn note_record(record: &ScriptRecord) {
    if !enabled() {
        return;
    }
    let url = record.source.as_str();
    let callback = url.starts_with("event:") || matches!(url, "timers" | "network" | "importmap");
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
            if !callback {
                push_script(url, "execute");
                flush_pending(url);
            }
            note_console(message);
        }
        ScriptOutcome::Ran => {
            if !callback {
                push_script(url, "execute");
                flush_pending(url);
            }
        }
    }
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
        if executed {
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
        true
    });
    if flush {
        flush_file();
    }
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
        console: active.console.clone(),
        mutations: active
            .mutations
            .iter()
            .map(|(t_ms, kind, nodes, target)| (*t_ms, *kind, *nodes, target.clone()))
            .collect(),
    }
}

struct Snapshot {
    url: String,
    ready_state: Vec<(String, u64)>,
    dcl_ms: Option<u64>,
    load_ms: Option<u64>,
    scripts: Vec<(String, u32, &'static str, u64)>,
    console: Vec<(u64, String)>,
    mutations: Vec<(u64, &'static str, usize, String)>,
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
    console: Vec<ConsoleJson<'a>>,
    mutations: Vec<MutationJson<'a>>,
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
}
