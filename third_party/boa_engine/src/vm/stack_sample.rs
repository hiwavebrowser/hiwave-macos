//! HIWAVE PATCH (trace): high-water mark of the JS call-stack depth.
//!
//! `push_frame` calls [`note`] on every call. Disarmed, that is one relaxed
//! atomic load and nothing else. Armed, the mark is the deepest
//! `frames.len() - 1` seen since [`arm`]`(true)` (the dummy frame at index 0
//! is not a JS frame).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static ARMED: AtomicBool = AtomicBool::new(false);
static MAX: AtomicUsize = AtomicUsize::new(0);

/// Record `depth` when a trace is being sampled.
#[inline]
pub(crate) fn note(depth: usize) {
    if depth == 0 || !ARMED.load(Ordering::Relaxed) {
        return;
    }
    let mut cur = MAX.load(Ordering::Relaxed);
    while depth > cur {
        match MAX.compare_exchange_weak(cur, depth, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => cur = observed,
        }
    }
}

/// Start or stop sampling. Arming and disarming both clear the mark, so a
/// reader takes [`max_depth`] before disarming.
pub fn arm(on: bool) {
    if on {
        MAX.store(0, Ordering::Relaxed);
    }
    ARMED.store(on, Ordering::Relaxed);
    if !on {
        MAX.store(0, Ordering::Relaxed);
    }
}

/// The deepest JS stack observed since the last [`arm`]`(true)`.
pub fn max_depth() -> usize {
    MAX.load(Ordering::Relaxed)
}
