//! RustKit Chrome View - RustKit-rendered browser chrome UI
//!
//! This module provides a RustKit-based chrome view that replaces WRY/WebKit
//! for rendering the browser's tab strip, navigation bar, and sidebar.
//!
//! Note: Currently feature-flagged for gradual rollout.

#![allow(dead_code)]

#[cfg(all(target_os = "macos", feature = "rustkit"))]
use hiwave_core::{HiWaveError, HiWaveResult};
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use rustkit_engine::{Engine, EngineBuilder, EngineEvent, EngineViewId};
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use rustkit_viewhost::Bounds;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use std::cell::RefCell;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use tao::rwh_06::HasWindowHandle;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use tao::window::Window;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use tokio::sync::mpsc;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use tracing::{error, info};
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use wry::Rect;

/// The HTML content for the RustKit-rendered browser chrome
#[cfg(all(target_os = "macos", feature = "rustkit"))]
pub const CHROME_RUSTKIT_HTML: &str = include_str!("ui/chrome_rustkit.html");

/// Chrome event types that can be emitted to the main app
#[cfg(all(target_os = "macos", feature = "rustkit"))]
#[derive(Debug, Clone)]
pub enum ChromeEvent {
    /// New tab requested
    NewTab,
    /// Tab close requested
    CloseTab(usize),
    /// Tab selection changed
    SelectTab(usize),
    /// Navigation back
    Back,
    /// Navigation forward
    Forward,
    /// Reload page
    Reload,
    /// URL bar focus
    UrlBarFocus,
    /// URL bar submit
    UrlBarSubmit(String),
    /// Menu button clicked
    MenuClick,
    /// Sidebar item clicked
    SidebarItem(String),
}

/// A RustKit-based chrome view.
///
/// This view renders the browser's tab strip, navigation bar, and sidebar
/// using the RustKit engine instead of WebKit.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
pub struct RustKitChromeView {
    /// The engine managing this view.
    engine: RefCell<Engine>,
    /// The view ID within the engine.
    view_id: Option<EngineViewId>,
    /// Current chrome state.
    state: RefCell<ChromeState>,
    /// Event receiver for engine events.
    event_rx: Option<mpsc::UnboundedReceiver<EngineEvent>>,
}

/// Chrome UI state.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
#[derive(Debug, Clone, Default)]
pub struct ChromeState {
    /// Current tabs
    pub tabs: Vec<TabInfo>,
    /// Active tab index
    pub active_tab: usize,
    /// Current URL
    pub url: String,
    /// Can go back
    pub can_go_back: bool,
    /// Can go forward
    pub can_go_forward: bool,
    /// Shield blocked count
    pub blocked_count: u32,
    /// Workspace name
    pub workspace_name: String,
}

/// Tab information.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
#[derive(Debug, Clone)]
pub struct TabInfo {
    pub title: String,
    pub favicon: Option<String>,
}

#[cfg(all(target_os = "macos", feature = "rustkit"))]
impl RustKitChromeView {
    /// Create a new RustKit chrome view.
    pub fn new(window: &Window, bounds: Bounds) -> HiWaveResult<Self> {
        info!("Creating RustKit chrome view");

        // Get raw window handle from TAO window
        let raw_handle = window
            .window_handle()
            .map_err(|e| HiWaveError::WebView(format!("Failed to get window handle: {}", e)))?
            .as_raw();

        // Create engine
        let mut engine = EngineBuilder::new()
            .user_agent("HiWave/1.0 RustKit-Chrome/1.0")
            .javascript_enabled(false) // No JS for now
            .build()
            .map_err(|e| HiWaveError::WebView(format!("Failed to create engine: {}", e)))?;

        // Create view
        let view_id = engine
            .create_view(raw_handle, bounds)
            .map_err(|e| HiWaveError::WebView(format!("Failed to create view: {}", e)))?;

        // Load chrome HTML
        engine
            .load_html(view_id, CHROME_RUSTKIT_HTML)
            .map_err(|e| HiWaveError::WebView(format!("Failed to load HTML: {}", e)))?;

        // Initial render
        engine
            .render_view(view_id)
            .map_err(|e| HiWaveError::WebView(format!("Failed to render: {}", e)))?;

        let state = ChromeState {
            tabs: vec![TabInfo {
                title: "New Tab".to_string(),
                favicon: None,
            }],
            active_tab: 0,
            url: "hiwave://newtab".to_string(),
            can_go_back: false,
            can_go_forward: false,
            blocked_count: 0,
            workspace_name: "Default".to_string(),
        };

        Ok(Self {
            engine: RefCell::new(engine),
            view_id: Some(view_id),
            state: RefCell::new(state),
            event_rx: None,
        })
    }

    /// Update chrome state and re-render.
    pub fn update_state(&self, state: ChromeState) -> HiWaveResult<()> {
        *self.state.borrow_mut() = state;
        // TODO: Update HTML to reflect new state
        // For now, just re-render
        if let Some(view_id) = self.view_id {
            self.engine
                .borrow_mut()
                .render_view(view_id)
                .map_err(|e| HiWaveError::WebView(format!("Failed to render: {}", e)))?;
        }
        Ok(())
    }

    /// Update the URL display.
    pub fn set_url(&self, url: &str) {
        self.state.borrow_mut().url = url.to_string();
        // TODO: Update URL bar in the HTML
    }

    /// Update tab information.
    pub fn set_tabs(&self, tabs: Vec<TabInfo>, active: usize) {
        let mut state = self.state.borrow_mut();
        state.tabs = tabs;
        state.active_tab = active;
        // TODO: Update tab strip in the HTML
    }

    /// Update navigation button states.
    pub fn set_navigation(&self, can_go_back: bool, can_go_forward: bool) {
        let mut state = self.state.borrow_mut();
        state.can_go_back = can_go_back;
        state.can_go_forward = can_go_forward;
        // TODO: Update navigation buttons in the HTML
    }

    /// Update blocked count.
    pub fn set_blocked_count(&self, count: u32) {
        self.state.borrow_mut().blocked_count = count;
        // TODO: Update shield count in the HTML
    }

    /// Process events from the engine.
    pub fn process_events(&self) {
        // TODO: Handle click events when input is implemented
    }

    /// Render the chrome.
    pub fn render(&self) {
        if let Some(view_id) = self.view_id {
            if let Err(e) = self.engine.borrow_mut().render_view(view_id) {
                error!(?e, "Failed to render chrome");
            }
        }
    }

    /// Set the bounds of the chrome view.
    pub fn set_bounds(&self, bounds: Bounds) -> HiWaveResult<()> {
        if let Some(view_id) = self.view_id {
            self.engine
                .borrow_mut()
                .resize_view(view_id, bounds)
                .map_err(|e| HiWaveError::WebView(format!("Failed to resize: {}", e)))?;
        }
        Ok(())
    }

    /// WRY-like set_bounds for compatibility.
    pub fn wry_set_bounds(&self, rect: Rect) -> Result<(), String> {
        let bounds = Bounds::new(
            rect.position.to_logical::<f64>(1.0).x as i32,
            rect.position.to_logical::<f64>(1.0).y as i32,
            rect.size.to_logical::<f64>(1.0).width as u32,
            rect.size.to_logical::<f64>(1.0).height as u32,
        );
        self.set_bounds(bounds)
            .map_err(|e| format!("{}", e))
    }
}

