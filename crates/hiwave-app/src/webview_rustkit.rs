//! RustKit WebView adapter for HiWave macOS
//!
//! This module provides the RustKit engine as the default WebView backend for content rendering.
//! It wraps `rustkit_engine::Engine` and provides a WRY-like interface.


use super::shield_adapter::create_shield_interceptor_with_counter;
use super::webview::IWebContent;
use hiwave_core::{HiWaveError, HiWaveResult};
use rustkit_engine::{Engine, EngineBuilder, EngineEvent, EngineViewId};
use rustkit_viewhost::Bounds;
use std::cell::RefCell;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tao::window::Window;
use tao::rwh_06::HasWindowHandle;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use url::Url;

/// A RustKit-based WebView that implements IWebContent.
///
/// # Thread Safety
///
/// This type is NOT Send or Sync. All operations must be performed on the main thread.
pub struct RustKitView {
    /// The engine managing this view.
    engine: RefCell<Engine>,
    /// The view ID within the engine.
    view_id: Option<EngineViewId>,
    /// Current URL (cached).
    current_url: RefCell<Option<String>>,
    /// Current title (cached).
    current_title: RefCell<Option<String>>,
    /// Current zoom level.
    zoom_level: RefCell<f64>,
    /// Visibility state.
    visible: RefCell<bool>,
    /// Loading state.
    loading: RefCell<bool>,
    /// Event receiver for engine events.
    event_rx: Option<mpsc::UnboundedReceiver<EngineEvent>>,
    /// Counter for blocked requests (shared with shield).
    #[allow(dead_code)]
    blocked_counter: Option<Arc<AtomicU64>>,
    /// Navigation history stack.
    history: RefCell<Vec<Url>>,
    /// Current position in navigation history.
    history_index: RefCell<usize>,
}

impl RustKitView {
    /// Create a new RustKit view.
    pub fn new(window: &Window, bounds: Bounds) -> HiWaveResult<Self> {
        Self::with_shield_counter(window, bounds, None)
    }

    /// Create a new RustKit view with a shared blocked request counter.
    pub fn with_shield_counter(
        window: &Window,
        bounds: Bounds,
        blocked_counter: Option<Arc<AtomicU64>>,
    ) -> HiWaveResult<Self> {
        info!("Creating RustKit view");

        // Get raw window handle from TAO window
        let raw_handle = window.window_handle()
            .map_err(|e| HiWaveError::WebView(format!("Failed to get window handle: {}", e)))?
            .as_raw();

        // Create engine builder with shield interceptor if counter provided
        // Browser-plausible UA. "HiWave/1.0 RustKit/1.0" alone got us
        // instantly rate-limited as a scraper even by Wikimedia (HTTP 429 on
        // every thumbnail, 2026-08-05 live session) — servers gate on the
        // Mozilla/AppleWebKit shape. Platform is reported honestly; HiWave
        // stays visible as the product token.
        let mut builder = EngineBuilder::new()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15 HiWave/1.0")
            .javascript_enabled(true)
            .cookies_enabled(true);

        // Add shield interceptor if counter is provided
        let counter_clone = blocked_counter.clone();
        if let Some(counter) = blocked_counter.clone() {
            info!("RustKit engine with shield ad-blocking enabled");
            let interceptor = create_shield_interceptor_with_counter(counter);
            builder = builder.request_interceptor(interceptor);
        }

        let mut engine = builder
            .build()
            .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;

        // Take event receiver for processing events
        let event_rx = engine.take_event_receiver();

        // Create view
        let view_id = engine
            .create_view(raw_handle, bounds)
            .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;

        info!(?view_id, "RustKit view created");

        Ok(Self {
            engine: RefCell::new(engine),
            view_id: Some(view_id),
            current_url: RefCell::new(None),
            current_title: RefCell::new(None),
            zoom_level: RefCell::new(1.0),
            visible: RefCell::new(true),
            loading: RefCell::new(false),
            event_rx,
            blocked_counter: counter_clone,
            history: RefCell::new(Vec::new()),
            history_index: RefCell::new(0),
        })
    }

    /// Process pending engine events (call this in the event loop).
    /// Note: This requires a tokio runtime to be available.
    pub fn process_events(&self) {
        // For now, event processing is handled by the engine's internal event loop
        // We'll implement proper event handling when we have a tokio runtime in the event loop
        // TODO: Integrate with main event loop's tokio runtime
    }

    /// Render the view (call this in the event loop).
    pub fn render(&self) {
        let mut engine = self.engine.borrow_mut();
        engine.render_all_views();
    }

    /// Go back in this view's navigation history.
    ///
    /// Uses the view-local history Vec (every navigate() and load records
    /// into it, including engine link clicks routed through
    /// UserEvent::Navigate). NOT the SessionHistory-canonical shape from
    /// the fleet pin — the full #81-style port is the named follow-up; this
    /// makes the button work tonight without inventing a third stack.
    pub fn nav_back(&self) -> bool {
        let target = {
            let mut index = self.history_index.borrow_mut();
            if *index == 0 {
                return false;
            }
            *index -= 1;
            self.history.borrow()[*index].clone()
        };
        *self.current_url.borrow_mut() = Some(target.to_string());
        self.load_url_blocking(target.as_str());
        true
    }

    /// Go forward in this view's navigation history.
    pub fn nav_forward(&self) -> bool {
        let target = {
            let mut index = self.history_index.borrow_mut();
            let len = self.history.borrow().len();
            if *index + 1 >= len {
                return false;
            }
            *index += 1;
            self.history.borrow()[*index].clone()
        };
        *self.current_url.borrow_mut() = Some(target.to_string());
        self.load_url_blocking(target.as_str());
        true
    }

    /// Reload the current page.
    pub fn nav_reload(&self) -> bool {
        let url = self.current_url.borrow().clone();
        match url {
            Some(u) => {
                self.load_url_blocking(&u);
                true
            }
            None => false,
        }
    }

    /// Route OS keyboard delivery to the content view (first responder).
    pub fn grab_keyboard(&self) {
        let engine = self.engine.borrow();
        if let Some(view_id) = self.view_id {
            engine.grab_keyboard(view_id);
        }
    }

    /// Deliver a key to the focused form control, if any.
    ///
    /// Returns true when the control consumed it (value or caret changed),
    /// which tells the caller NOT to fall back to scrolling.
    pub fn handle_text_key(
        &self,
        key_code: u32,
        key: &str,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) -> bool {
        let mut engine = self.engine.borrow_mut();
        self.view_id
            .map(|view_id| engine.handle_text_key(view_id, key_code, key, ctrl, shift, alt))
            .unwrap_or(false)
    }

    /// URL to navigate to if the focused field's form were submitted.
    /// `None` when there is no form, no fields, or the form is not a GET.
    pub fn form_submit_url(&self) -> Option<String> {
        let engine = self.engine.borrow();
        self.view_id
            .and_then(|view_id| engine.form_submission_for_focus(view_id))
            .filter(|sub| sub.is_self_target())
            .map(|sub| sub.url)
    }

    /// Whether a content element currently holds focus.
    pub fn has_focused_element(&self) -> bool {
        let engine = self.engine.borrow();
        self.view_id
            .and_then(|view_id| engine.focused_node(view_id))
            .is_some()
    }

    /// Rebuild layout and repaint after an edit changed a control's value.
    pub fn relayout(&self) {
        let mut engine = self.engine.borrow_mut();
        if let Some(view_id) = self.view_id {
            if let Err(e) = engine.relayout(view_id) {
                debug!(error = %e, "relayout after edit failed");
            }
        }
    }

    /// Focus whatever focusable element sits at these viewport coordinates,
    /// clearing focus when nothing focusable is there. Returns the focused
    /// element's tag name.
    pub fn focus_at_point(&self, x: f32, y: f32) -> Option<String> {
        let mut engine = self.engine.borrow_mut();
        self.view_id
            .and_then(|view_id| engine.focus_at_point(view_id, x, y))
    }

    /// Resolve a click at viewport coordinates to a link URL, if any.
    pub fn link_at_point(&self, x: f32, y: f32) -> Option<String> {
        let engine = self.engine.borrow();
        self.view_id
            .and_then(|view_id| engine.link_at_point(view_id, x, y))
    }

    /// Scroll the view by a wheel delta in physical pixels.
    ///
    /// Returns true if the scroll offset changed (a re-render is needed).
    /// Wheel events that reach the tao window loop are exactly the ones the
    /// UI-frame WebView did not consume, i.e. the pointer was over content.
    pub fn scroll_by(&self, delta_x: f32, delta_y: f32) -> bool {
        let mut engine = self.engine.borrow_mut();
        if let Some(view_id) = self.view_id {
            match engine.scroll_view(view_id, delta_x, delta_y) {
                Ok(changed) => changed,
                Err(e) => {
                    debug!(error = %e, "scroll_by failed");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Load HTML content directly.
    pub fn load_html_internal(&self, html: &str) -> HiWaveResult<()> {
        let mut engine = self.engine.borrow_mut();
        if let Some(view_id) = self.view_id {
            engine
                .load_html(view_id, html)
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
        }
        Ok(())
    }

    /// Set the bounds of the view.
    pub fn set_bounds_internal(&self, bounds: Bounds) -> HiWaveResult<()> {
        let mut engine = self.engine.borrow_mut();
        if let Some(view_id) = self.view_id {
            engine
                .resize_view(view_id, bounds)
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
        }
        Ok(())
    }

    /// Get the current title.
    pub fn title(&self) -> Option<String> {
        self.current_title.borrow().clone()
    }

    /// Load HTML content (WRY compatibility method).
    pub fn wry_load_html(&self, html: &str) -> Result<(), String> {
        self.load_html_internal(html)
            .map_err(|e| format!("Failed to load HTML: {}", e))
    }

    /// Load URL (WRY compatibility method).
    pub fn wry_load_url(&self, url: &str) -> Result<(), String> {
        let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

        // Update cached URL
        *self.current_url.borrow_mut() = Some(url.to_string());

        // Update navigation history
        let mut history = self.history.borrow_mut();
        let mut index = self.history_index.borrow_mut();

        // If we're in the middle of history (user went back), truncate forward history
        if *index < history.len() {
            history.truncate(*index + 1);
        }

        // Add new URL to history
        history.push(parsed);
        *index = history.len() - 1;

        drop(history);
        drop(index);

        self.load_url_blocking(url);
        Ok(())
    }

    /// Evaluate script (WRY compatibility method).
    pub fn wry_evaluate_script(&self, script: &str) -> Result<(), String> {
        self.execute_script_sync(script);
        Ok(())
    }

    /// Set bounds (WRY compatibility method).
    pub fn wry_set_bounds(&self, rect: wry::Rect) -> Result<(), String> {
        use rustkit_viewhost::Bounds;
        let bounds = Bounds::new(
            rect.position.to_logical::<f64>(1.0).x as i32,
            rect.position.to_logical::<f64>(1.0).y as i32,
            rect.size.to_logical::<f64>(1.0).width as u32,
            rect.size.to_logical::<f64>(1.0).height as u32,
        );
        self.set_bounds_internal(bounds)
            .map_err(|e| format!("Failed to set bounds: {}", e))
    }

    /// Get the view ID.
    pub fn view_id(&self) -> Option<EngineViewId> {
        self.view_id
    }

    /// Load a URL using a blocking runtime.
    fn load_url_blocking(&self, url: &str) {
        let parsed = match Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                error!(error = %e, url = url, "Invalid URL");
                return;
            }
        };

        // Create a single-threaded tokio runtime for this operation
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                error!(error = %e, "Failed to create runtime");
                return;
            }
        };

        let mut engine = self.engine.borrow_mut();
        if let Some(view_id) = self.view_id {
            rt.block_on(async {
                if let Err(e) = engine.load_url(view_id, parsed).await {
                    error!(error = %e, "Failed to load URL");
                }
            });
        }
    }

    /// Execute JavaScript synchronously.
    pub fn execute_script_sync(&self, script: &str) -> Option<String> {
        let mut engine = self.engine.borrow_mut();
        if let Some(view_id) = self.view_id {
            match engine.execute_script(view_id, script) {
                Ok(result) => Some(result),
                Err(e) => {
                    debug!(error = %e, "Script execution failed");
                    None
                }
            }
        } else {
            None
        }
    }
}

impl Drop for RustKitView {
    fn drop(&mut self) {
        if let Some(view_id) = self.view_id {
            let mut engine = self.engine.borrow_mut();
            if let Err(e) = engine.destroy_view(view_id) {
                warn!(error = %e, "Failed to destroy RustKit view");
            }
        }
    }
}

// ============================================================================
// IWebContent Implementation
// ============================================================================

impl IWebContent for RustKitView {
    fn navigate(&mut self, url: &Url) -> HiWaveResult<()> {
        // Update cached URL
        *self.current_url.borrow_mut() = Some(url.to_string());

        // Update navigation history
        let mut history = self.history.borrow_mut();
        let mut index = self.history_index.borrow_mut();

        // If we're in the middle of history (user went back), truncate forward history
        if *index < history.len() {
            history.truncate(*index + 1);
        }

        // Add new URL to history
        history.push(url.clone());
        *index = history.len() - 1;

        drop(history);
        drop(index);

        self.load_url_blocking(url.as_str());
        Ok(())
    }

    fn execute_script(&self, script: &str) -> HiWaveResult<String> {
        self.execute_script_sync(script)
            .ok_or_else(|| hiwave_core::HiWaveError::WebView("Script execution failed".to_string()))
    }

    fn get_url(&self) -> Option<Url> {
        // Return cached URL
        if let Some(url) = self.current_url.borrow().clone() {
            return Url::parse(&url).ok();
        }
        None
    }

    fn can_go_back(&self) -> bool {
        *self.history_index.borrow() > 0
    }

    fn can_go_forward(&self) -> bool {
        let index = *self.history_index.borrow();
        let history_len = self.history.borrow().len();
        index < history_len.saturating_sub(1)
    }

    fn go_back(&mut self) -> HiWaveResult<()> {
        if !self.can_go_back() {
            return Err(hiwave_core::HiWaveError::WebView("Cannot go back".to_string()));
        }

        let mut index = self.history_index.borrow_mut();
        *index -= 1;
        let url = self.history.borrow()[*index].clone();
        drop(index);

        self.load_url_blocking(url.as_str());
        *self.current_url.borrow_mut() = Some(url.to_string());
        Ok(())
    }

    fn go_forward(&mut self) -> HiWaveResult<()> {
        if !self.can_go_forward() {
            return Err(hiwave_core::HiWaveError::WebView("Cannot go forward".to_string()));
        }

        let mut index = self.history_index.borrow_mut();
        *index += 1;
        let url = self.history.borrow()[*index].clone();
        drop(index);

        self.load_url_blocking(url.as_str());
        *self.current_url.borrow_mut() = Some(url.to_string());
        Ok(())
    }

    fn reload(&mut self) -> HiWaveResult<()> {
        if let Some(url) = self.current_url.borrow().clone() {
            self.load_url_blocking(&url);
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::WebView("No URL to reload".to_string()))
        }
    }
}

