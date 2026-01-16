//! Page lifecycle events for RustKit.
//!
//! Implements the page lifecycle events:
//! - DOMContentLoaded
//! - load
//! - beforeunload
//! - unload
//! - pagehide / pageshow
//! - visibilitychange
//!
//! Also handles the document ready state.

use std::time::Instant;

/// Document ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentReadyState {
    /// The document is still loading.
    #[default]
    Loading,
    /// The document has finished loading and parsing, but sub-resources are still loading.
    Interactive,
    /// The document and all sub-resources have finished loading.
    Complete,
}

impl DocumentReadyState {
    /// Convert to JavaScript string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentReadyState::Loading => "loading",
            DocumentReadyState::Interactive => "interactive",
            DocumentReadyState::Complete => "complete",
        }
    }
}

/// Visibility state of the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityState {
    /// The page content may be at least partially visible.
    #[default]
    Visible,
    /// The page content is not visible to the user.
    Hidden,
}

impl VisibilityState {
    /// Convert to JavaScript string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            VisibilityState::Visible => "visible",
            VisibilityState::Hidden => "hidden",
        }
    }
}

/// Page lifecycle events.
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    /// Ready state changed.
    ReadyStateChange {
        state: DocumentReadyState,
    },

    /// DOMContentLoaded - the DOM has been fully parsed.
    DomContentLoaded {
        timestamp: Instant,
    },

    /// Load - all resources have been loaded.
    Load {
        timestamp: Instant,
    },

    /// Before unload - page is about to be unloaded.
    /// Returns whether to show a confirmation dialog.
    BeforeUnload {
        /// Whether to show a confirmation prompt.
        show_prompt: bool,
        /// Custom message (mostly ignored by modern browsers).
        message: Option<String>,
    },

    /// Unload - page is being unloaded.
    Unload {
        timestamp: Instant,
    },

    /// Page hide - page is being hidden (navigating away or tab hidden).
    PageHide {
        /// Whether the page might be restored from bfcache.
        persisted: bool,
    },

    /// Page show - page is being shown.
    PageShow {
        /// Whether the page was restored from bfcache.
        persisted: bool,
    },

    /// Visibility changed.
    VisibilityChange {
        state: VisibilityState,
    },
}

/// Page lifecycle state manager.
pub struct PageLifecycle {
    /// Document ready state.
    ready_state: DocumentReadyState,

    /// Visibility state.
    visibility_state: VisibilityState,

    /// Whether the page is frozen (bfcache candidate).
    frozen: bool,

    /// Event handler for beforeunload.
    beforeunload_handler: Option<BeforeUnloadHandler>,

    /// Timestamp when parsing started.
    parse_start: Option<Instant>,

    /// Timestamp when DOM was ready.
    dom_ready: Option<Instant>,

    /// Timestamp when load completed.
    load_complete: Option<Instant>,

    /// Event sender.
    event_sender: Option<tokio::sync::mpsc::UnboundedSender<LifecycleEvent>>,
}

/// Handler for beforeunload event.
pub type BeforeUnloadHandler = Box<dyn Fn() -> Option<String> + Send + Sync>;

impl PageLifecycle {
    /// Create a new page lifecycle manager.
    pub fn new() -> Self {
        Self {
            ready_state: DocumentReadyState::Loading,
            visibility_state: VisibilityState::Visible,
            frozen: false,
            beforeunload_handler: None,
            parse_start: Some(Instant::now()),
            dom_ready: None,
            load_complete: None,
            event_sender: None,
        }
    }

    /// Set the event sender.
    pub fn with_event_sender(mut self, sender: tokio::sync::mpsc::UnboundedSender<LifecycleEvent>) -> Self {
        self.event_sender = Some(sender);
        self
    }

    /// Get the current ready state.
    pub fn ready_state(&self) -> DocumentReadyState {
        self.ready_state
    }

    /// Get the current visibility state.
    pub fn visibility_state(&self) -> VisibilityState {
        self.visibility_state
    }

    /// Check if the page is hidden.
    pub fn hidden(&self) -> bool {
        self.visibility_state == VisibilityState::Hidden
    }

    /// Check if the page is frozen.
    pub fn frozen(&self) -> bool {
        self.frozen
    }

    /// Set a beforeunload handler.
    pub fn set_beforeunload_handler(&mut self, handler: BeforeUnloadHandler) {
        self.beforeunload_handler = Some(handler);
    }

    /// Clear the beforeunload handler.
    pub fn clear_beforeunload_handler(&mut self) {
        self.beforeunload_handler = None;
    }

    /// Transition to interactive state (DOM parsed).
    pub fn dom_content_loaded(&mut self) {
        if self.ready_state == DocumentReadyState::Loading {
            self.ready_state = DocumentReadyState::Interactive;
            self.dom_ready = Some(Instant::now());

            self.send_event(LifecycleEvent::ReadyStateChange {
                state: DocumentReadyState::Interactive,
            });
            self.send_event(LifecycleEvent::DomContentLoaded {
                timestamp: Instant::now(),
            });
        }
    }

    /// Transition to complete state (all resources loaded).
    pub fn load_complete(&mut self) {
        if self.ready_state != DocumentReadyState::Complete {
            self.ready_state = DocumentReadyState::Complete;
            self.load_complete = Some(Instant::now());

            self.send_event(LifecycleEvent::ReadyStateChange {
                state: DocumentReadyState::Complete,
            });
            self.send_event(LifecycleEvent::Load {
                timestamp: Instant::now(),
            });
        }
    }

    /// Attempt to unload the page.
    /// Returns true if navigation should proceed, false if cancelled.
    pub fn try_unload(&mut self) -> bool {
        // Check beforeunload handler
        if let Some(ref handler) = self.beforeunload_handler {
            if let Some(message) = handler() {
                self.send_event(LifecycleEvent::BeforeUnload {
                    show_prompt: true,
                    message: Some(message),
                });
                // In a real implementation, we'd wait for user confirmation
                // For now, we'll allow the navigation
            }
        }

        // Fire pagehide
        self.send_event(LifecycleEvent::PageHide {
            persisted: false,
        });

        // Fire unload
        self.send_event(LifecycleEvent::Unload {
            timestamp: Instant::now(),
        });

        true
    }

    /// Show the page (e.g., after restoring from bfcache).
    pub fn page_show(&mut self, persisted: bool) {
        self.send_event(LifecycleEvent::PageShow { persisted });
    }

    /// Hide the page (e.g., before bfcache or tab switch).
    pub fn page_hide(&mut self, persisted: bool) {
        self.send_event(LifecycleEvent::PageHide { persisted });
    }

    /// Set visibility state.
    pub fn set_visibility(&mut self, state: VisibilityState) {
        if self.visibility_state != state {
            self.visibility_state = state;
            self.send_event(LifecycleEvent::VisibilityChange { state });
        }
    }

    /// Freeze the page (for bfcache).
    pub fn freeze(&mut self) {
        self.frozen = true;
        self.page_hide(true);
    }

    /// Resume the page (from bfcache).
    pub fn resume(&mut self) {
        self.frozen = false;
        self.page_show(true);
    }

    /// Get timing metrics.
    pub fn timing(&self) -> PageTiming {
        PageTiming {
            parse_start: self.parse_start,
            dom_ready: self.dom_ready,
            load_complete: self.load_complete,
        }
    }

    fn send_event(&self, event: LifecycleEvent) {
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(event);
        }
    }
}

impl Default for PageLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

/// Page timing metrics.
#[derive(Debug, Clone, Copy)]
pub struct PageTiming {
    /// When parsing started.
    pub parse_start: Option<Instant>,
    /// When DOM was ready (DOMContentLoaded).
    pub dom_ready: Option<Instant>,
    /// When load completed.
    pub load_complete: Option<Instant>,
}

impl PageTiming {
    /// Get time from parse start to DOM ready.
    pub fn dom_interactive_time(&self) -> Option<std::time::Duration> {
        match (self.parse_start, self.dom_ready) {
            (Some(start), Some(ready)) => Some(ready.duration_since(start)),
            _ => None,
        }
    }

    /// Get time from parse start to load complete.
    pub fn load_time(&self) -> Option<std::time::Duration> {
        match (self.parse_start, self.load_complete) {
            (Some(start), Some(complete)) => Some(complete.duration_since(start)),
            _ => None,
        }
    }
}

/// Before unload result.
#[derive(Debug, Clone)]
pub struct BeforeUnloadResult {
    /// Whether to show a confirmation prompt.
    pub show_prompt: bool,
    /// Custom message (ignored by most browsers).
    pub message: Option<String>,
}

impl BeforeUnloadResult {
    /// Allow navigation without prompt.
    pub fn allow() -> Self {
        Self {
            show_prompt: false,
            message: None,
        }
    }

    /// Request confirmation prompt.
    pub fn confirm(message: impl Into<String>) -> Self {
        Self {
            show_prompt: true,
            message: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ready_state_transitions() {
        let mut lifecycle = PageLifecycle::new();

        assert_eq!(lifecycle.ready_state(), DocumentReadyState::Loading);

        lifecycle.dom_content_loaded();
        assert_eq!(lifecycle.ready_state(), DocumentReadyState::Interactive);

        lifecycle.load_complete();
        assert_eq!(lifecycle.ready_state(), DocumentReadyState::Complete);
    }

    #[test]
    fn test_visibility_state() {
        let mut lifecycle = PageLifecycle::new();

        assert_eq!(lifecycle.visibility_state(), VisibilityState::Visible);
        assert!(!lifecycle.hidden());

        lifecycle.set_visibility(VisibilityState::Hidden);
        assert_eq!(lifecycle.visibility_state(), VisibilityState::Hidden);
        assert!(lifecycle.hidden());
    }

    #[test]
    fn test_ready_state_string() {
        assert_eq!(DocumentReadyState::Loading.as_str(), "loading");
        assert_eq!(DocumentReadyState::Interactive.as_str(), "interactive");
        assert_eq!(DocumentReadyState::Complete.as_str(), "complete");
    }

    #[test]
    fn test_visibility_state_string() {
        assert_eq!(VisibilityState::Visible.as_str(), "visible");
        assert_eq!(VisibilityState::Hidden.as_str(), "hidden");
    }

    #[test]
    fn test_freeze_resume() {
        let mut lifecycle = PageLifecycle::new();

        assert!(!lifecycle.frozen());

        lifecycle.freeze();
        assert!(lifecycle.frozen());

        lifecycle.resume();
        assert!(!lifecycle.frozen());
    }

    #[test]
    fn test_try_unload() {
        let mut lifecycle = PageLifecycle::new();

        // Without handler, should allow
        assert!(lifecycle.try_unload());
    }

    #[test]
    fn test_before_unload_result() {
        let allow = BeforeUnloadResult::allow();
        assert!(!allow.show_prompt);

        let confirm = BeforeUnloadResult::confirm("Are you sure?");
        assert!(confirm.show_prompt);
        assert_eq!(confirm.message, Some("Are you sure?".into()));
    }
}

