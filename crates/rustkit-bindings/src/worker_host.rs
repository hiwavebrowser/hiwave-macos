//! Dedicated workers (STR-4). A worker is one OS thread with a JsRuntime of
//! its own: a Boa context cannot move between threads, and a worker must not
//! share the page's objects anyway. The page and the worker talk through
//! channels carrying strings; the page side (web_worker.js) drains the
//! worker's channel on the timer pump, so every page callback still runs on
//! the page thread.
//!
//! `terminate()` stops a worker that never returns through the view-control
//! interrupt word the page's own watchdog uses: Boa reads it at every call
//! and loop back-edge, so a `while (true) {}` ends.

use rustkit_js::{JsRuntime, JsValue, ViewControl, Watchdog};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const PRELUDE: &str = include_str!("worker_prelude.js");

/// A worker's turn is bounded by the interrupt word, not by a budget.
const TURN_BUDGET: Duration = Duration::from_secs(60 * 60 * 24);

/// At most this many workers a page may have at once.
const MAX_WORKERS: usize = 64;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

enum FromWorker {
    Message(String),
    Error(String),
}

struct Handle {
    to: Sender<String>,
    from: Receiver<FromWorker>,
    control: Arc<ViewControl>,
    thread: JoinHandle<()>,
}

thread_local! {
    /// The page thread's workers.
    static WORKERS: RefCell<HashMap<u64, Handle>> = RefCell::new(HashMap::new());
    /// Terminated workers whose threads have not ended yet.
    static ENDING: RefCell<Vec<JoinHandle<()>>> = const { RefCell::new(Vec::new()) };
}

/// How many worker threads have not yet returned.
pub fn running() -> usize {
    ENDING.with(|e| e.borrow_mut().retain(|t| !t.is_finished()));
    let ending = ENDING.with(|e| e.borrow().len());
    ending + WORKERS.with(|w| w.borrow().values().filter(|h| !h.thread.is_finished()).count())
}

fn arg_str(args: &[JsValue], i: usize) -> String {
    match args.get(i) {
        Some(JsValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn arg_id(args: &[JsValue]) -> u64 {
    match args.first() {
        Some(JsValue::Number(n)) if *n >= 0.0 => *n as u64,
        _ => 0,
    }
}

fn item(m: FromWorker) -> Value {
    match m {
        FromWorker::Message(d) => json!({"k": "m", "d": d}),
        FromWorker::Error(m) => json!({"k": "e", "m": m}),
    }
}

/// Install `__rustkit_worker_{spawn,post,poll,terminate}` on the page runtime.
pub fn install(runtime: &mut JsRuntime) -> Result<(), rustkit_js::JsError> {
    // spawn(source, name, locationJson) -> id, 0 when refused
    runtime.register_host_function(
        "__rustkit_worker_spawn",
        3,
        Box::new(|args| {
            if WORKERS.with(|w| w.borrow().len()) >= MAX_WORKERS {
                return JsValue::Number(0.0);
            }
            let (source, name, loc) = (arg_str(args, 0), arg_str(args, 1), arg_str(args, 2));
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let (to, rx) = channel();
            let (tx, from) = channel();
            let control = Watchdog::shared().view_control();
            let c = control.clone();
            let thread = std::thread::Builder::new()
                .name(format!("worker-{id}"))
                .spawn(move || worker_main(source, name, loc, rx, tx, c));
            let Ok(thread) = thread else {
                return JsValue::Number(0.0);
            };
            WORKERS.with(|w| w.borrow_mut().insert(id, Handle { to, from, control, thread }));
            JsValue::Number(id as f64)
        }),
    )?;
    runtime.register_host_function(
        "__rustkit_worker_post",
        2,
        Box::new(|args| {
            let (id, json) = (arg_id(args), arg_str(args, 1));
            WORKERS.with(|w| {
                if let Some(h) = w.borrow().get(&id) {
                    let _ = h.to.send(json);
                }
            });
            JsValue::Undefined
        }),
    )?;
    // poll(id) -> JSON array of {k:"m",d}|{k:"e",m}, ended by {k:"x"} once
    // the worker has returned and been drained; "null" for an unknown id.
    runtime.register_host_function(
        "__rustkit_worker_poll",
        1,
        Box::new(|args| {
            let id = arg_id(args);
            let mut items = Vec::new();
            let finished = WORKERS.with(|w| {
                let w = w.borrow();
                let h = w.get(&id)?;
                // Read "finished" before draining, so nothing sent just
                // before the end is missed.
                let done = h.thread.is_finished();
                while let Ok(m) = h.from.try_recv() {
                    items.push(item(m));
                }
                Some(done)
            });
            let Some(finished) = finished else {
                return JsValue::String("null".into());
            };
            if finished {
                if let Some(h) = WORKERS.with(|w| w.borrow_mut().remove(&id)) {
                    let _ = h.thread.join();
                }
                items.push(json!({"k": "x"}));
            }
            JsValue::String(Value::Array(items).to_string())
        }),
    )?;
    runtime.register_host_function(
        "__rustkit_worker_terminate",
        1,
        Box::new(|args| {
            let id = arg_id(args);
            if let Some(h) = WORKERS.with(|w| w.borrow_mut().remove(&id)) {
                h.control.stop_all();
                drop(h.to); // a worker waiting for a message wakes and ends
                ENDING.with(|e| e.borrow_mut().push(h.thread));
            }
            JsValue::Undefined
        }),
    )?;
    Ok(())
}

fn worker_main(
    source: String,
    name: String,
    location: String,
    rx: Receiver<String>,
    out: Sender<FromWorker>,
    control: Arc<ViewControl>,
) {
    let Ok(mut rt) = JsRuntime::new() else {
        let _ = out.send(FromWorker::Error("the worker's runtime failed to start".into()));
        return;
    };
    rt.set_view_control(Some(control.clone()));
    let stamp = Watchdog::shared().thread_stamp();
    let closed = Rc::new(Cell::new(false));
    {
        let tx = out.clone();
        let _ = rt.register_host_function(
            "__rk_worker_out",
            1,
            Box::new(move |args| {
                let _ = tx.send(FromWorker::Message(arg_str(args, 0)));
                JsValue::Undefined
            }),
        );
        let tx = out.clone();
        let _ = rt.register_host_function(
            "__rk_worker_error",
            1,
            Box::new(move |args| {
                let _ = tx.send(FromWorker::Error(arg_str(args, 0)));
                JsValue::Undefined
            }),
        );
        let closed = closed.clone();
        let _ = rt.register_host_function(
            "__rk_worker_close",
            0,
            Box::new(move |_| {
                closed.set(true);
                JsValue::Undefined
            }),
        );
    }
    let quoted = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    let report = |e: String, control: &ViewControl| {
        if !control.stops_all() {
            let _ = out.send(FromWorker::Error(e));
        }
    };

    rt.begin_turn(&stamp, TURN_BUDGET);
    let started = rt.evaluate_script(PRELUDE).is_ok()
        && rt
            .evaluate_script(&format!("__rk_worker_setup({}, {})", quoted(&name), quoted(&location)))
            .is_ok();
    if started {
        if let Err(e) = rt.evaluate_script(&source) {
            report(e.to_string(), &control);
        }
        let _ = rt.run_jobs();
    } else {
        report("the worker scope failed to start".into(), &control);
    }
    rt.end_turn(&stamp);

    while started && !closed.get() && !control.stops_all() {
        let Ok(json) = rx.recv() else { break };
        rt.begin_turn(&stamp, TURN_BUDGET);
        let call = format!("__rk_worker_dispatch({})", quoted(&json));
        if let Err(e) = rt.evaluate_script(&call) {
            report(e.to_string(), &control);
        }
        let _ = rt.run_jobs();
        rt.end_turn(&stamp);
    }
}
