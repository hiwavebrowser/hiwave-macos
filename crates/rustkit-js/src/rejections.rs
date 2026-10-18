//! Unhandled promise rejections, for diagnostics.
//!
//! Boa calls `HostPromiseRejectionTracker` when a promise is rejected with
//! no handler (`Reject`) and when a handler is later added to one
//! (`Handle`). What is still unhandled when the embedder asks
//! ([`crate::JsRuntime::take_error_reports`], after an evaluation and
//! its jobs) is what a browser reports as `unhandledrejection`.
//!
//! Off by default: the hook then returns before it looks at anything, so a
//! page that rejects promises pays one flag read per rejection.
//!
//! The engine's own promises (a module's load and evaluation) are not the
//! page's: their rejections are reported as the module's error, so they are
//! [`forget`](RejectionHooks::forget)-ten here.
//!
//! Describing a rejection runs none of the page's code (see `diag`).

use std::cell::{Cell, RefCell};

use boa_engine::builtins::function::OrdinaryFunction;
use boa_engine::builtins::promise::{OperationType, PromiseState};
use boa_engine::context::HostHooks;
use boa_engine::job::JobCallback;
use boa_engine::js_string;
use boa_engine::object::builtins::{JsFunction, JsPromise};
use boa_engine::property::PropertyKey;
use boa_engine::vm::SourcePath;
use boa_engine::{Context, JsObject, JsResult, JsValue};

/// Most rejections held at once; past it the oldest is dropped (reported
/// nowhere: a page rejecting this many promises per evaluation is already
/// named by the ones that were kept).
const MAX_PENDING: usize = 256;
/// Engine-owned promises remembered; past it the oldest is forgotten.
const MAX_IGNORED: usize = 1024;

#[derive(Default)]
pub(crate) struct RejectionHooks {
    pub(crate) tracking: Cell<bool>,
    pending: RefCell<std::collections::VecDeque<JsObject>>,
    /// Engine-owned promises: never reported.
    ignored: RefCell<std::collections::VecDeque<JsObject>>,
    /// Rejections a handler later caught. Lit's update cycle catches the
    /// previous update's rejection inside `__enqueueUpdate` and refires it,
    /// so the original never stays unhandled. Kept only while tracking is on.
    handled: RefCell<std::collections::VecDeque<JsValue>>,
}

impl HostHooks for RejectionHooks {
    fn call_job_callback(
        &self,
        job: &JobCallback,
        this: &JsValue,
        args: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        // The census names the reaction. Reading the compiled name and the
        // script path runs none of the page's code.
        let label = callback_label(job.callback());
        let started = std::time::Instant::now();
        let result = job.callback().call(this, args, context);
        crate::job_census::note_callback(label, started.elapsed());
        result
    }

    fn promise_rejection_tracker(
        &self,
        promise: &JsObject<boa_engine::builtins::promise::Promise>,
        operation: OperationType,
        _context: &mut Context,
    ) {
        if !self.tracking.get() {
            return;
        }
        let promise = promise.clone().upcast();
        if self
            .ignored
            .borrow()
            .iter()
            .any(|p| JsObject::equals(p, &promise))
        {
            return;
        }
        match operation {
            OperationType::Reject => {
                let mut pending = self.pending.borrow_mut();
                if pending.len() >= MAX_PENDING {
                    pending.pop_front();
                }
                pending.push_back(promise);
            }
            OperationType::Handle => {
                let found = {
                    let mut pending = self.pending.borrow_mut();
                    pending
                        .iter()
                        .position(|p| JsObject::equals(p, &promise))
                        .and_then(|pos| pending.remove(pos))
                };
                let Some(object) = found else {
                    return;
                };
                let Ok(caught) = JsPromise::from_object(object) else {
                    return;
                };
                let PromiseState::Rejected(reason) = caught.state() else {
                    return;
                };
                // A primitive rejection (`Promise.reject(1).catch(...)`) is
                // not an update failure. Lit throws an Error object.
                if reason.as_object().is_none() {
                    return;
                }
                let mut handled = self.handled.borrow_mut();
                if handled.len() >= 32 {
                    handled.pop_front();
                }
                handled.push_back(reason);
            }
        }
    }
}

/// `name @path`, cut so a `data:` URL or a newline in a name cannot own the
/// log line. Boa's public code block has the path and not the function's
/// start line.
fn callback_label(func: &JsFunction) -> String {
    if let Some(ordinary) = func.downcast_ref::<OrdinaryFunction>() {
        let code = ordinary.codeblock();
        let name = flat_label(&code.name().to_std_string_escaped(), 80);
        let name = if name.is_empty() {
            "(anonymous)".to_string()
        } else {
            name
        };
        let path = match code.path() {
            SourcePath::Path(path) => flat_label(&path.to_string_lossy(), 120),
            SourcePath::Eval => "eval".to_string(),
            SourcePath::Json => "json".to_string(),
            SourcePath::None => String::new(),
        };
        return if path.is_empty() {
            name
        } else {
            format!("{name} @{path}")
        };
    }
    data_name(func).unwrap_or_else(|| "native".to_string())
}

fn data_name(func: &JsFunction) -> Option<String> {
    let object = func.try_borrow().ok()?;
    let key = PropertyKey::from(js_string!("name"));
    let value = object.properties().get(&key)?.value()?.as_string()?;
    let name = flat_label(&value.to_std_string_escaped(), 80);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn flat_label(text: &str, max: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.chars().count() >= max {
            break;
        }
        out.push(if ch.is_control() || ch == ',' {
            ' '
        } else {
            ch
        });
    }
    out
}

impl RejectionHooks {
    /// `promise` is the engine's own: drop it if it is pending, and never
    /// report it.
    pub(crate) fn forget(&self, promise: &JsObject) {
        if !self.tracking.get() {
            return;
        }
        self.pending
            .borrow_mut()
            .retain(|p| !JsObject::equals(p, promise));
        let mut ignored = self.ignored.borrow_mut();
        if ignored.len() >= MAX_IGNORED {
            ignored.pop_front();
        }
        ignored.push_back(promise.clone());
    }

    /// The reasons of the rejections still unhandled, oldest first.
    pub(crate) fn take(&self) -> Vec<JsValue> {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        pending
            .into_iter()
            .filter_map(|object| {
                let promise = JsPromise::from_object(object).ok()?;
                match promise.state() {
                    PromiseState::Rejected(reason) => Some(reason),
                    _ => None,
                }
            })
            .collect()
    }

    /// Error rejections that gained a handler since the last call. Lit's
    /// `__enqueueUpdate` catches the previous update this way.
    pub(crate) fn take_handled(&self) -> Vec<JsValue> {
        std::mem::take(&mut *self.handled.borrow_mut())
            .into_iter()
            .collect()
    }
}
