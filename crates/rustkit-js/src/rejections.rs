//! Unhandled promise rejections, for diagnostics.
//!
//! Boa calls `HostPromiseRejectionTracker` when a promise is rejected with
//! no handler (`Reject`) and when a handler is later added to one
//! (`Handle`). What is still unhandled when the embedder asks
//! ([`crate::JsRuntime::take_unhandled_rejections`], after an evaluation and
//! its jobs) is what a browser reports as `unhandledrejection`.
//!
//! Off by default: the hook then returns before it looks at anything, so a
//! page that rejects promises pays one flag read per rejection.

use std::cell::{Cell, RefCell};

use boa_engine::builtins::promise::{OperationType, PromiseState};
use boa_engine::context::HostHooks;
use boa_engine::object::builtins::JsPromise;
use boa_engine::{js_string, Context, JsObject};

use crate::UnhandledRejection;

/// Most rejections held at once; past it the oldest is dropped (reported
/// nowhere: a page rejecting this many promises per evaluation is already
/// named by the ones that were kept).
const MAX_PENDING: usize = 256;

#[derive(Default)]
pub(crate) struct RejectionHooks {
    pub(crate) tracking: Cell<bool>,
    pending: RefCell<Vec<JsObject>>,
}

impl HostHooks for RejectionHooks {
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
        let mut pending = self.pending.borrow_mut();
        match operation {
            OperationType::Reject => {
                if pending.len() >= MAX_PENDING {
                    pending.remove(0);
                }
                pending.push(promise);
            }
            OperationType::Handle => pending.retain(|p| !JsObject::equals(p, &promise)),
        }
    }
}

impl RejectionHooks {
    /// The rejections still unhandled, described, oldest first.
    pub(crate) fn take(&self, context: &mut Context) -> Vec<UnhandledRejection> {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        pending
            .into_iter()
            .filter_map(|object| {
                let promise = JsPromise::from_object(object).ok()?;
                let PromiseState::Rejected(reason) = promise.state() else {
                    return None;
                };
                let message = reason
                    .to_string(context)
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|_| "<unprintable rejection>".to_string());
                let stack = reason
                    .as_object()
                    .and_then(|o| o.get(js_string!("stack"), context).ok())
                    .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                    .filter(|s| !s.is_empty());
                Some(UnhandledRejection { message, stack })
            })
            .collect()
    }
}
