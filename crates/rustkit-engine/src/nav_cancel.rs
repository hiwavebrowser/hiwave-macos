//! Cooperative load cancel (threads plan M3b).
//!
//! Every navigation of a view gets a `nav_id`, one more than the view's
//! previous one. A [`NavCanceller`] cancels a navigation from any thread:
//! it raises the view's cancel watermark (`ViewControl::cancel_nav`), sets
//! the interrupt word if that navigation's load is in progress, so a script
//! of the load that is running now stops at its next call or loop back-edge
//! (M1), and wakes the load wherever it waits. The load checks the
//! watermark between scripts and between its subresource phases, and races
//! every network wait against the cancel, so a cancelled load drops its
//! pending fetches and applies nothing more: no script, no stylesheet,
//! image or font, no layout, no `PageLoaded`.
//!
//! What it does not do: abort a socket that is already sending bytes (the
//! fetch future is dropped, which closes an HTTP/1 connection but cannot
//! recall what the server already did), or stop a page thread that is
//! inside one long native call such as a layout; the cancel takes effect
//! when it returns (invariant 10 covers a call that never does).

use std::future::Future;
use std::sync::Arc;

use rustkit_js::ViewControl;
use tokio::sync::Notify;

/// Cancels one view's navigations. `Send + Sync`, cheap to clone; the
/// window thread (from M4) and tests hold one while the page thread loads.
#[derive(Clone, Debug)]
pub struct NavCanceller {
    control: Arc<ViewControl>,
    signal: Arc<Notify>,
}

impl NavCanceller {
    pub(crate) fn new(control: Arc<ViewControl>, signal: Arc<Notify>) -> Self {
        Self { control, signal }
    }

    pub(crate) fn control(&self) -> &Arc<ViewControl> {
        &self.control
    }

    /// Cancel navigation `nav_id` and every earlier one of this view.
    /// Whether a load in progress was cancelled; a cancel for a load that
    /// has finished changes nothing about its document.
    pub fn cancel_nav(&self, nav_id: u64) -> bool {
        let hit = self.control.cancel_nav(nav_id);
        self.signal.notify_waiters();
        hit
    }

    /// Cancel the view's newest navigation (the user's Stop).
    pub fn cancel_current(&self) -> bool {
        self.cancel_nav(self.control.current_nav())
    }

    /// The view's newest navigation id; 0 = none yet.
    pub fn current_nav(&self) -> u64 {
        self.control.current_nav()
    }

    /// The navigation whose load is in progress; 0 = none.
    pub fn loading_nav(&self) -> u64 {
        self.control.loading_nav()
    }

    /// Whether `nav_id` has been cancelled.
    pub fn is_cancelled(&self, nav_id: u64) -> bool {
        self.control.nav_cancelled(nav_id)
    }

    /// Resolves once `nav_id` is cancelled.
    pub(crate) async fn cancelled(&self, nav_id: u64) {
        loop {
            let mut notified = std::pin::pin!(self.signal.notified());
            // Registered before the check, so a cancel between the check
            // and the await still wakes it.
            notified.as_mut().enable();
            if self.is_cancelled(nav_id) {
                return;
            }
            notified.await;
        }
    }

    /// `fut`'s output, or `None` if `nav_id` is cancelled first. The cancel
    /// is polled first, so a response that is ready on the same wake as a
    /// cancel is dropped with `fut`.
    pub(crate) async fn unless_cancelled<F: Future>(
        &self,
        nav_id: u64,
        fut: F,
    ) -> Option<F::Output> {
        let cancelled = self.cancelled(nav_id);
        futures::pin_mut!(cancelled, fut);
        match futures::future::select(cancelled, fut).await {
            futures::future::Either::Left(_) => None,
            futures::future::Either::Right((output, _)) => Some(output),
        }
    }
}

/// A load's navigation and its canceller, for the steps the load calls.
#[derive(Clone, Debug)]
pub(crate) struct NavWatch {
    pub canceller: NavCanceller,
    pub nav_id: u64,
}

impl NavWatch {
    pub fn is_cancelled(&self) -> bool {
        self.canceller.is_cancelled(self.nav_id)
    }

    pub async fn unless_cancelled<F: Future>(&self, fut: F) -> Option<F::Output> {
        self.canceller.unless_cancelled(self.nav_id, fut).await
    }
}

/// [`NavWatch::is_cancelled`] for an optional watch: a step run outside a
/// load (the public `load_subresources`) is never cancelled.
pub(crate) fn cancelled(watch: Option<&NavWatch>) -> bool {
    watch.is_some_and(NavWatch::is_cancelled)
}

/// `fut` raced against an optional watch.
pub(crate) async fn unless_cancelled<F: Future>(
    watch: Option<&NavWatch>,
    fut: F,
) -> Option<F::Output> {
    match watch {
        Some(watch) => watch.unless_cancelled(fut).await,
        None => Some(fut.await),
    }
}

/// Ends the load's "in progress" mark however the load returns, a dropped
/// future included, so a later cancel for it leaves the word alone.
pub(crate) struct LoadInProgress {
    pub control: Arc<ViewControl>,
    pub nav_id: u64,
}

impl Drop for LoadInProgress {
    fn drop(&mut self) {
        self.control.end_load(self.nav_id);
    }
}

#[cfg(test)]
pub(crate) type PhaseHook = Box<dyn FnMut(&str)>;

#[cfg(test)]
thread_local! {
    /// Called at named points of a load (`parse`, `script:N`), so a test can
    /// cancel exactly there.
    pub(crate) static LOAD_PHASE_HOOK: std::cell::RefCell<Option<PhaseHook>> =
        const { std::cell::RefCell::new(None) };
}

/// A named point of a load, for [`LOAD_PHASE_HOOK`]; nothing outside tests.
#[inline]
pub(crate) fn load_phase(_name: &str) {
    #[cfg(test)]
    LOAD_PHASE_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(_name);
        }
    });
}
