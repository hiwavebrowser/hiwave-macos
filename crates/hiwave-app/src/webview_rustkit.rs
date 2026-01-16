//! RustKit WebView adapter for HiWave macOS
//!
//! This module provides the RustKit engine as the default WebView backend for content rendering.
//! It wraps `rustkit_engine::Engine` and provides a WRY-like interface.

#![allow(dead_code)]

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
        let mut builder = EngineBuilder::new()
            .user_agent("HiWave/1.0 RustKit/1.0")
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
        // Update cached URL
        *self.current_url.borrow_mut() = Some(url.to_string());
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
        // TODO: Implement navigation history
        false
    }

    fn can_go_forward(&self) -> bool {
        // TODO: Implement navigation history
        false
    }

    fn go_back(&mut self) -> HiWaveResult<()> {
        // TODO: Implement navigation history
        Err(hiwave_core::HiWaveError::WebView("Not yet implemented".to_string()))
    }

    fn go_forward(&mut self) -> HiWaveResult<()> {
        // TODO: Implement navigation history
        Err(hiwave_core::HiWaveError::WebView("Not yet implemented".to_string()))
    }

    fn reload(&mut self) -> HiWaveResult<()> {
        // TODO: Implement reload
        if let Some(url) = self.current_url.borrow().clone() {
            self.load_url_blocking(&url);
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::WebView("No URL to reload".to_string()))
        }
    }
}

