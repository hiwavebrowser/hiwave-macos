//! # RustKit Engine
//!
//! Browser engine orchestration layer that integrates all RustKit components
//! to provide a complete multi-view browser engine.
//!
//! ## Design Goals
//!
//! 1. **Multi-view support**: Manage multiple independent browser views
//! 2. **Unified API**: Single entry point for all browser functionality
//! 3. **Event coordination**: Route events between views and host
//! 4. **Resource sharing**: Share compositor and network resources

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rustkit_bindings::DomBindings;
// Re-export IpcMessage for external use
pub use rustkit_bindings::IpcMessage;
use rustkit_compositor::Compositor;
use rustkit_core::{LoadEvent, NavigationRequest, NavigationStateMachine};
use rustkit_css::{parse_display, ComputedStyle, Rule, Stylesheet};
use rustkit_dom::{Document, Node, NodeType};
use rustkit_image::ImageManager;
use rustkit_js::JsRuntime;
use rustkit_layout::{BoxType, Dimensions, DisplayList, LayoutBox, Position, Rect};
use rustkit_net::{LoaderConfig, NetError, Request, ResourceLoader};
use rustkit_renderer::Renderer;
use rustkit_viewhost::{Bounds, ViewHost, ViewHostTrait, ViewId, WindowHandle};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};
use url::Url;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;

/// Errors that can occur in the engine.
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("View error: {0}")]
    ViewError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] NetError),

    #[error("Navigation error: {0}")]
    NavigationError(String),

    #[error("Render error: {0}")]
    RenderError(String),

    #[error("JS error: {0}")]
    JsError(String),

    #[error("View not found: {0:?}")]
    ViewNotFound(EngineViewId),
}

/// Unique identifier for an engine view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EngineViewId(u64);

impl EngineViewId {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Engine events emitted to the host application.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// Navigation started.
    NavigationStarted { view_id: EngineViewId, url: Url },
    /// Navigation committed (first bytes received).
    NavigationCommitted { view_id: EngineViewId, url: Url },
    /// Page fully loaded.
    PageLoaded {
        view_id: EngineViewId,
        url: Url,
        title: Option<String>,
    },
    /// Navigation failed.
    NavigationFailed {
        view_id: EngineViewId,
        url: Url,
        error: String,
    },
    /// Title changed.
    TitleChanged {
        view_id: EngineViewId,
        title: String,
    },
    /// Console message from JavaScript.
    ConsoleMessage {
        view_id: EngineViewId,
        level: String,
        message: String,
    },
    /// View resized.
    ViewResized {
        view_id: EngineViewId,
        width: u32,
        height: u32,
    },
    /// View received focus.
    ViewFocused { view_id: EngineViewId },
    /// Download started.
    DownloadStarted { url: Url, filename: String },
    /// Image loaded.
    ImageLoaded {
        view_id: EngineViewId,
        url: Url,
        width: u32,
        height: u32,
    },
    /// Image failed to load.
    ImageError {
        view_id: EngineViewId,
        url: Url,
        error: String,
    },
    /// Favicon detected.
    FaviconDetected { view_id: EngineViewId, url: Url },
}

/// View state.
#[allow(dead_code)]
struct ViewState {
    id: EngineViewId,
    viewhost_id: ViewId,
    url: Option<Url>,
    title: Option<String>,
    document: Option<Rc<Document>>,
    #[allow(dead_code)]
    layout: Option<LayoutBox>,
    #[allow(dead_code)]
    display_list: Option<DisplayList>,
    #[allow(dead_code)]
    bindings: Option<DomBindings>,
    navigation: NavigationStateMachine,
    #[allow(dead_code)]
    nav_event_rx: mpsc::UnboundedReceiver<LoadEvent>,
    /// Currently focused DOM node.
    focused_node: Option<rustkit_dom::NodeId>,
    /// Whether the view itself has focus.
    view_focused: bool,
    /// Current scroll offset (x, y) in pixels.
    scroll_offset: (f32, f32),
    /// Maximum scroll offset based on content size.
    max_scroll_offset: (f32, f32),
    /// External stylesheets loaded from <link> elements.
    external_stylesheets: Vec<Stylesheet>,
    /// Headless bounds (only set for headless views, None for window-based views).
    headless_bounds: Option<Bounds>,
}

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// User agent string.
    pub user_agent: String,
    /// Enable JavaScript.
    pub javascript_enabled: bool,
    /// Enable cookies.
    pub cookies_enabled: bool,
    /// Default background color.
    pub background_color: [f64; 4],
    /// Disable animations and transitions for deterministic parity captures.
    /// When true, all CSS animations and transitions are ignored during rendering.
    pub disable_animations: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            user_agent: "RustKit/1.0 HiWave/1.0".to_string(),
            javascript_enabled: true,
            cookies_enabled: true,
            background_color: [1.0, 1.0, 1.0, 1.0], // White
            disable_animations: false,
        }
    }
}

impl EngineConfig {
    /// Create a configuration for parity testing (animations disabled).
    pub fn for_parity_testing() -> Self {
        Self {
            disable_animations: true,
            ..Default::default()
        }
    }
}

/// The main browser engine.
pub struct Engine {
    config: EngineConfig,
    viewhost: ViewHost,
    compositor: Compositor,
    renderer: Option<Renderer>,
    loader: Arc<ResourceLoader>,
    image_manager: Arc<ImageManager>,
    views: HashMap<EngineViewId, ViewState>,
    event_tx: mpsc::UnboundedSender<EngineEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<EngineEvent>>,
}

impl Engine {
    /// Create a new browser engine.
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        Self::with_interceptor(config, None)
    }

    /// Create a new browser engine with an optional request interceptor.
    pub fn with_interceptor(
        config: EngineConfig,
        interceptor: Option<rustkit_net::RequestInterceptor>,
    ) -> Result<Self, EngineError> {
        info!("Initializing RustKit Engine");

        // Initialize ViewHost
        let viewhost = ViewHost::new();

        // Initialize Compositor
        let compositor = Compositor::new().map_err(|e| EngineError::RenderError(e.to_string()))?;

        // Initialize ResourceLoader
        let loader_config = LoaderConfig {
            user_agent: config.user_agent.clone(),
            cookies_enabled: config.cookies_enabled,
            ..Default::default()
        };
        let loader = Arc::new(
            ResourceLoader::with_interceptor(loader_config, interceptor)
                .map_err(EngineError::NetworkError)?,
        );

        // Initialize ImageManager
        let image_manager = Arc::new(ImageManager::new());

        // Initialize Renderer
        let renderer = Renderer::new(
            compositor.device_arc(),
            compositor.queue_arc(),
            compositor.surface_format(),
        )
        .map_err(|e| EngineError::RenderError(e.to_string()))?;

        // Event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        info!(
            adapter = ?compositor.adapter_info().name,
            "Engine initialized with GPU renderer"
        );

        Ok(Self {
            config,
            viewhost,
            compositor,
            renderer: Some(renderer),
            loader,
            image_manager,
            views: HashMap::new(),
            event_tx,
            event_rx: Some(event_rx),
        })
    }

    /// Take the event receiver.
    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<EngineEvent>> {
        self.event_rx.take()
    }

    /// Create a new view.
    #[cfg(target_os = "windows")]
    pub fn create_view(
        &mut self,
        parent: WindowHandle,
        bounds: Bounds,
    ) -> Result<EngineViewId, EngineError> {
        let id = EngineViewId::new();

        debug!(?id, ?bounds, "Creating view");

        // Create viewhost view (using trait method)
        let viewhost_id = <ViewHost as ViewHostTrait>::create_view(&self.viewhost, parent, bounds)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        // Create compositor surface
        let hwnd = <ViewHost as ViewHostTrait>::get_hwnd(&self.viewhost, viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        unsafe {
            self.compositor
                .create_surface_for_hwnd(viewhost_id, hwnd, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;
        }

        // Create navigation state machine
        let (nav_tx, nav_rx) = mpsc::unbounded_channel();
        let navigation = NavigationStateMachine::new(nav_tx);

        // Create view state
        let view_state = ViewState {
            id,
            viewhost_id,
            url: None,
            title: None,
            document: None,
            layout: None,
            display_list: None,
            bindings: None,
            navigation,
            nav_event_rx: nav_rx,
            focused_node: None,
            view_focused: false,
            scroll_offset: (0.0, 0.0),
            max_scroll_offset: (0.0, 0.0),
            external_stylesheets: Vec::new(),
            headless_bounds: None,
        };

        self.views.insert(id, view_state);

        // Render initial background
        self.compositor
            .render_solid_color(viewhost_id, self.config.background_color)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        info!(?id, "View created");
        Ok(id)
    }

    /// Create a new view (macOS stub - will be implemented in Phase 3).
    #[cfg(not(target_os = "windows"))]
    pub fn create_view(
        &mut self,
        parent: WindowHandle,
        bounds: Bounds,
    ) -> Result<EngineViewId, EngineError> {
        // TODO: Implement macOS view creation in Phase 3
        // For now, use trait method which will call the stub implementation
        let viewhost_id = <ViewHost as ViewHostTrait>::create_view(&self.viewhost, parent, bounds)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        // Create view state (without compositor surface for now)
        let (nav_tx, nav_rx) = mpsc::unbounded_channel();
        let navigation = NavigationStateMachine::new(nav_tx);

        let view_state = ViewState {
            id: EngineViewId::new(),
            viewhost_id,
            url: None,
            title: None,
            document: None,
            layout: None,
            display_list: None,
            bindings: None,
            navigation,
            nav_event_rx: nav_rx,
            focused_node: None,
            view_focused: false,
            scroll_offset: (0.0, 0.0),
            max_scroll_offset: (0.0, 0.0),
            external_stylesheets: Vec::new(),
            headless_bounds: None,
        };

        let id = view_state.id;
        self.views.insert(id, view_state);

        // Get raw window handle for compositor
        let raw_handle =
            <ViewHost as ViewHostTrait>::get_raw_window_handle(&self.viewhost, viewhost_id)
                .map_err(|e| EngineError::ViewError(e.to_string()))?;

        // Create compositor surface
        unsafe {
            self.compositor
                .create_surface_for_raw_handle(viewhost_id, raw_handle, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;
        }

        // Render initial background
        self.compositor
            .render_solid_color(viewhost_id, self.config.background_color)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        info!(?id, "View created (macOS)");
        Ok(id)
    }

    /// Create a headless view for offscreen rendering (testing/CI mode).
    ///
    /// This creates a view without requiring a window, perfect for unit tests
    /// and CI environments. Requires the "headless" feature flag.
    #[cfg(feature = "headless")]
    pub fn create_headless_view(&mut self, bounds: Bounds) -> Result<EngineViewId, EngineError> {
        let id = EngineViewId::new();
        let viewhost_id = ViewId::new();

        debug!(?id, ?bounds, "Creating headless view");

        // Create headless texture instead of surface
        self.compositor
            .create_headless_texture(viewhost_id, bounds.width, bounds.height)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        // Create navigation state machine
        let (nav_tx, nav_rx) = mpsc::unbounded_channel();
        let navigation = NavigationStateMachine::new(nav_tx);

        let view_state = ViewState {
            id,
            viewhost_id,
            url: None,
            title: None,
            document: None,
            layout: None,
            display_list: None,
            bindings: None,
            navigation,
            nav_event_rx: nav_rx,
            focused_node: None,
            view_focused: false,
            scroll_offset: (0.0, 0.0),
            max_scroll_offset: (0.0, 0.0),
            external_stylesheets: Vec::new(),
            headless_bounds: Some(bounds),
        };

        self.views.insert(id, view_state);

        // Render initial background to headless texture
        self.compositor
            .render_solid_color(viewhost_id, self.config.background_color)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        info!(?id, "Headless view created");
        Ok(id)
    }

    /// Destroy a view.
    pub fn destroy_view(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        let view = self
            .views
            .remove(&id)
            .ok_or(EngineError::ViewNotFound(id))?;

        // Destroy compositor surface
        let _ = self.compositor.destroy_surface(view.viewhost_id);

        // Destroy viewhost view
        let _ = <ViewHost as ViewHostTrait>::destroy_view(&self.viewhost, view.viewhost_id);

        info!(?id, "View destroyed");
        Ok(())
    }

    /// Resize a view.
    pub fn resize_view(&mut self, id: EngineViewId, bounds: Bounds) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let viewhost_id = view.viewhost_id;
        let is_headless = view.headless_bounds.is_some();

        debug!(?id, ?bounds, is_headless, "Resizing view");

        if is_headless {
            // Headless view: recreate headless texture with new size
            // First destroy old texture
            self.compositor.destroy_headless_texture(viewhost_id).ok(); // Ignore errors if it doesn't exist

            // Create new texture with new size
            self.compositor
                .create_headless_texture(viewhost_id, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;

            // Update headless_bounds in view state
            let view = self
                .views
                .get_mut(&id)
                .ok_or(EngineError::ViewNotFound(id))?;
            view.headless_bounds = Some(bounds);
        } else {
            // Regular view: resize viewhost and surface
            self.viewhost
                .set_bounds(viewhost_id, bounds)
                .map_err(|e| EngineError::ViewError(e.to_string()))?;

            self.compositor
                .resize_surface(viewhost_id, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;
        }

        // Re-layout if we have content
        if self
            .views
            .get(&id)
            .ok_or(EngineError::ViewNotFound(id))?
            .document
            .is_some()
        {
            self.relayout(id)?;
        }

        // Emit event
        let _ = self.event_tx.send(EngineEvent::ViewResized {
            view_id: id,
            width: bounds.width,
            height: bounds.height,
        });

        Ok(())
    }

    /// Scroll a view by the given delta.
    ///
    /// Returns true if the scroll caused a change (and thus needs a re-render).
    pub fn scroll_view(
        &mut self,
        id: EngineViewId,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<bool, EngineError> {
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;

        let old_offset = view.scroll_offset;

        // Apply scroll delta (negative delta_y means scroll down in most UIs)
        let new_x = (view.scroll_offset.0 + delta_x)
            .max(0.0)
            .min(view.max_scroll_offset.0);
        let new_y = (view.scroll_offset.1 - delta_y) // Invert Y for natural scrolling
            .max(0.0)
            .min(view.max_scroll_offset.1);

        view.scroll_offset = (new_x, new_y);

        let changed = view.scroll_offset != old_offset;
        if changed {
            debug!(?id, ?old_offset, new_offset = ?view.scroll_offset, "View scrolled");
        }

        Ok(changed)
    }

    /// Get the current scroll offset of a view.
    pub fn get_scroll_offset(&self, id: EngineViewId) -> Result<(f32, f32), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        Ok(view.scroll_offset)
    }

    /// Set the scroll offset directly.
    pub fn set_scroll_offset(
        &mut self,
        id: EngineViewId,
        x: f32,
        y: f32,
    ) -> Result<(), EngineError> {
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;

        view.scroll_offset = (
            x.max(0.0).min(view.max_scroll_offset.0),
            y.max(0.0).min(view.max_scroll_offset.1),
        );

        debug!(?id, offset = ?view.scroll_offset, "Scroll offset set");
        Ok(())
    }

    /// Focus a view.
    pub fn focus_view(&self, id: EngineViewId) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        debug!(?id, "Focusing view");

        self.viewhost
            .focus(view.viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        Ok(())
    }

    /// Set view visibility.
    pub fn set_view_visible(&self, id: EngineViewId, visible: bool) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        debug!(?id, visible, "Setting view visibility");

        self.viewhost
            .set_visible(view.viewhost_id, visible)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        Ok(())
    }

    /// Load a URL in a view.
    pub async fn load_url(&mut self, id: EngineViewId, url: Url) -> Result<(), EngineError> {
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;

        info!(?id, %url, "Loading URL");

        // Start navigation
        let request = NavigationRequest::new(url.clone());
        view.navigation
            .start_navigation(request)
            .map_err(|e| EngineError::NavigationError(e.to_string()))?;

        // Emit event
        let _ = self.event_tx.send(EngineEvent::NavigationStarted {
            view_id: id,
            url: url.clone(),
        });

        // Fetch the URL
        let request = Request::get(url.clone());
        let response = self.loader.fetch(request).await?;

        if !response.ok() {
            let error = format!("HTTP {}", response.status);
            let view = self
                .views
                .get_mut(&id)
                .ok_or(EngineError::ViewNotFound(id))?;
            view.navigation
                .fail_navigation(error.clone())
                .map_err(|e| EngineError::NavigationError(e.to_string()))?;

            let _ = self.event_tx.send(EngineEvent::NavigationFailed {
                view_id: id,
                url,
                error,
            });

            return Err(EngineError::NavigationError("HTTP error".into()));
        }

        // Commit navigation
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;
        view.navigation
            .commit_navigation()
            .map_err(|e| EngineError::NavigationError(e.to_string()))?;

        let _ = self.event_tx.send(EngineEvent::NavigationCommitted {
            view_id: id,
            url: url.clone(),
        });

        // Parse HTML
        let html = response.text().await?;
        let document =
            Document::parse_html(&html).map_err(|e| EngineError::RenderError(e.to_string()))?;
        let document = Rc::new(document);

        // Get title
        let title = document.title();

        // Store in view
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;
        view.url = Some(url.clone());
        view.document = Some(document.clone());
        view.title = title.clone();

        // Initialize JavaScript if enabled
        if self.config.javascript_enabled {
            let js_runtime = JsRuntime::new().map_err(|e| EngineError::JsError(e.to_string()))?;

            let bindings =
                DomBindings::new(js_runtime).map_err(|e| EngineError::JsError(e.to_string()))?;

            bindings
                .set_document(document.clone())
                .map_err(|e| EngineError::JsError(e.to_string()))?;

            bindings
                .set_location(&url)
                .map_err(|e| EngineError::JsError(e.to_string()))?;

            let view = self
                .views
                .get_mut(&id)
                .ok_or(EngineError::ViewNotFound(id))?;
            view.bindings = Some(bindings);
        }

        // Initial layout and render
        self.relayout(id)?;

        // Load external resources (stylesheets, images)
        // This will trigger additional relayouts as resources arrive
        if let Err(e) = self.load_subresources(id).await {
            warn!(?e, "Failed to load some subresources");
            // Continue even if some resources fail to load
        }

        // Finish navigation
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;
        view.navigation
            .finish_navigation()
            .map_err(|e| EngineError::NavigationError(e.to_string()))?;

        // Emit events
        if let Some(ref title) = title {
            let _ = self.event_tx.send(EngineEvent::TitleChanged {
                view_id: id,
                title: title.clone(),
            });
        }

        let _ = self.event_tx.send(EngineEvent::PageLoaded {
            view_id: id,
            url,
            title: view.title.clone(),
        });

        Ok(())
    }

    /// Load HTML content directly into a view.
    ///
    /// This is used for loading inline HTML content like the Chrome UI,
    /// without making an HTTP request.
    pub fn load_html(&mut self, id: EngineViewId, html: &str) -> Result<(), EngineError> {
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;

        info!(?id, len = html.len(), "Loading HTML content");

        // Use a synthetic about:blank URL for inline content
        // SAFETY: "about:blank" is a constant URL that will always parse successfully
        let url = Url::parse("about:blank").unwrap();

        // Start navigation
        let request = NavigationRequest::new(url.clone());
        view.navigation
            .start_navigation(request)
            .map_err(|e| EngineError::NavigationError(e.to_string()))?;

        // Emit event
        let _ = self.event_tx.send(EngineEvent::NavigationStarted {
            view_id: id,
            url: url.clone(),
        });

        // Commit navigation
        view.navigation
            .commit_navigation()
            .map_err(|e| EngineError::NavigationError(e.to_string()))?;

        let _ = self.event_tx.send(EngineEvent::NavigationCommitted {
            view_id: id,
            url: url.clone(),
        });

        // Parse HTML
        let document =
            Document::parse_html(html).map_err(|e| EngineError::RenderError(e.to_string()))?;
        let document = Rc::new(document);

        // Get title
        let title = document.title();

        // Store in view
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;
        view.url = Some(url.clone());
        view.document = Some(document.clone());
        view.title = title.clone();

        // Initialize JavaScript if enabled
        if self.config.javascript_enabled {
            let js_runtime = JsRuntime::new().map_err(|e| EngineError::JsError(e.to_string()))?;

            let bindings =
                DomBindings::new(js_runtime).map_err(|e| EngineError::JsError(e.to_string()))?;

            bindings
                .set_document(document.clone())
                .map_err(|e| EngineError::JsError(e.to_string()))?;

            bindings
                .set_location(&url)
                .map_err(|e| EngineError::JsError(e.to_string()))?;

            let view = self
                .views
                .get_mut(&id)
                .ok_or(EngineError::ViewNotFound(id))?;
            view.bindings = Some(bindings);
        }

        // Layout and render
        self.relayout(id)?;

        // Finish navigation
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;
        view.navigation
            .finish_navigation()
            .map_err(|e| EngineError::NavigationError(e.to_string()))?;

        // Emit events
        if let Some(ref title) = title {
            let _ = self.event_tx.send(EngineEvent::TitleChanged {
                view_id: id,
                title: title.clone(),
            });
        }

        let _ = self.event_tx.send(EngineEvent::PageLoaded {
            view_id: id,
            url,
            title: view.title.clone(),
        });

        Ok(())
    }

    /// Re-layout a view.
    #[tracing::instrument(skip(self), fields(view_id = ?id))]
    fn relayout(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        let _span = tracing::info_span!("relayout", ?id).entered();

        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let document = view
            .document
            .as_ref()
            .ok_or(EngineError::RenderError("No document".into()))?
            .clone();

        // Get view bounds (from headless_bounds if headless, otherwise from viewhost)
        let bounds = if let Some(headless_bounds) = view.headless_bounds {
            headless_bounds
        } else {
            self.viewhost
                .get_bounds(view.viewhost_id)
                .map_err(|e| EngineError::ViewError(e.to_string()))?
        };

        debug!(
            ?id,
            width = bounds.width,
            height = bounds.height,
            "Performing layout"
        );

        // Create containing block
        // Note: height is 0 because layout_block_children uses content.height as the cursor position
        // Children should start at y=0, not y=viewport_height
        let containing_block = Dimensions {
            content: Rect::new(0.0, 0.0, bounds.width as f32, 0.0),
            ..Default::default()
        };

        debug!(
            containing_width = containing_block.content.width,
            containing_height = containing_block.content.height,
            "Created containing block"
        );

        // Get external stylesheets from view state
        let external_stylesheets = self
            .views
            .get(&id)
            .map(|v| v.external_stylesheets.clone())
            .unwrap_or_default();

        // Build layout tree from DOM with tracing
        let root_box = {
            let _build_span = tracing::info_span!("build_layout_tree").entered();
            self.build_layout_from_document(&document, &external_stylesheets)
        };

        // Layout computation
        let mut root_box = root_box;
        {
            let _layout_span = tracing::info_span!("layout_compute").entered();
            // Set viewport dimensions for vh/vw unit resolution
            root_box.set_viewport(bounds.width as f32, bounds.height as f32);
            // Root establishes the initial BFC: lay out through the margin-collapse
            // path so sibling margins collapse per CSS 2.1 §8.3.1. The plain
            // layout() path stacks margins additively (gap = bottom + top), which
            // ran every text page taller than Chrome.
            let mut margin_context = rustkit_layout::MarginCollapseContext::new();
            let mut float_context = rustkit_layout::FloatContext::new();
            root_box.layout_with_collapse(
                &containing_block,
                &mut margin_context,
                &mut float_context,
            );
        }

        // Ensure body element fills viewport (common browser behavior)
        // If body has zero or minimal height, extend it to viewport height
        if !root_box.children.is_empty() {
            let body_box = &mut root_box.children[0];
            if body_box.dimensions.content.height < 1.0 {
                // Body is empty or has no content - fill viewport
                body_box.dimensions.content.height = bounds.height as f32;
                debug!(
                    "Extended empty body to fill viewport height: {}px",
                    bounds.height
                );
            }
        }

        // The root (canvas) box always covers the FULL viewport: its
        // background is the CSS canvas background (§14.2, set at build from
        // html/body), which must paint to the viewport edges even when the
        // page content is shorter.
        root_box.dimensions.content.width =
            root_box.dimensions.content.width.max(bounds.width as f32);
        root_box.dimensions.content.height = root_box
            .dimensions
            .content
            .height
            .max(bounds.height as f32);

        // Debug: log the layout box tree AFTER layout
        fn debug_layout_box(box_: &LayoutBox, depth: usize) {
            if depth > 5 {
                return;
            } // Limit depth
            let indent = "  ".repeat(depth);
            let bg = box_.style.background_color;
            let dims = &box_.dimensions;
            tracing::debug!(
                "{}[{:?}] bg=rgba({},{},{},{:.1}) dims=({:.0}x{:.0} @ {:.0},{:.0}) children={}",
                indent,
                box_.box_type,
                bg.r,
                bg.g,
                bg.b,
                bg.a,
                dims.content.width,
                dims.content.height,
                dims.content.x,
                dims.content.y,
                box_.children.len()
            );
            for child in &box_.children {
                debug_layout_box(child, depth + 1);
            }
        }
        debug_layout_box(&root_box, 0);

        // Generate display list
        let display_list = {
            let _display_list_span = tracing::info_span!("build_display_list").entered();
            DisplayList::build(&root_box)
        };

        debug!(
            ?id,
            num_commands = display_list.commands.len(),
            "Generated display list"
        );

        // Debug: log first 10 display commands
        for (i, cmd) in display_list.commands.iter().take(10).enumerate() {
            trace!("DisplayCmd[{}]: {:?}", i, cmd);
        }

        // Update max scroll offset based on content size
        let content_height = root_box.dimensions.margin_box().height;
        let viewport_height = bounds.height as f32;
        let max_scroll_y = (content_height - viewport_height).max(0.0);

        // Store
        let view = self
            .views
            .get_mut(&id)
            .ok_or(EngineError::ViewNotFound(id))?;
        view.layout = Some(root_box);
        view.display_list = Some(display_list);
        view.max_scroll_offset = (0.0, max_scroll_y); // Update max scroll

        // Render
        self.render(id)?;

        Ok(())
    }

    /// Check if a style has visible styling (dimensions, background, borders, etc.)
    fn has_visible_styling(style: &ComputedStyle) -> bool {
        // Check for explicit dimensions
        if !matches!(style.width, rustkit_css::Length::Auto)
            || !matches!(style.height, rustkit_css::Length::Auto)
        {
            return true;
        }

        // Check for visible background
        if style.background_color.a > 0.0 && style.background_color != rustkit_css::Color::WHITE {
            return true;
        }

        // Check for background gradient
        if style.background_gradient.is_some() {
            return true;
        }

        // Check for borders (need to check both Px(0.0) and Zero)
        let has_border = |len: &rustkit_css::Length| -> bool {
            !matches!(
                len,
                rustkit_css::Length::Px(0.0) | rustkit_css::Length::Zero
            )
        };
        if has_border(&style.border_top_width)
            || has_border(&style.border_right_width)
            || has_border(&style.border_bottom_width)
            || has_border(&style.border_left_width)
        {
            return true;
        }

        // Check for padding (creates visual space)
        let has_padding = |len: &rustkit_css::Length| -> bool {
            !matches!(
                len,
                rustkit_css::Length::Px(0.0) | rustkit_css::Length::Zero
            )
        };
        if has_padding(&style.padding_top)
            || has_padding(&style.padding_right)
            || has_padding(&style.padding_bottom)
            || has_padding(&style.padding_left)
        {
            return true;
        }

        false
    }

    /// Whether a box participates in inline flow (shares line boxes with
    /// adjacent inline-level siblings). Mirrors the layout-side flows_inline
    /// gate in rustkit-layout's block child loop.
    fn is_inline_level_box(b: &LayoutBox) -> bool {
        matches!(
            b.box_type,
            BoxType::Text(_) | BoxType::Image { .. } | BoxType::FormControl(_)
        ) || (matches!(b.box_type, BoxType::Inline)
            && b.style.display == rustkit_css::Display::Inline)
            || b.style.display.is_atomic_inline()
    }

    /// Check if a layout box has content children (text, images, form controls).
    /// This is used to determine if an inline wrapper should be included.
    fn has_content_children(layout_box: &LayoutBox) -> bool {
        for child in &layout_box.children {
            match &child.box_type {
                BoxType::Text(text) => {
                    if !text.trim().is_empty() {
                        return true;
                    }
                }
                BoxType::Image { .. } | BoxType::FormControl(_) => {
                    return true;
                }
                BoxType::Inline | BoxType::Block | BoxType::AnonymousBlock => {
                    // Recursively check children
                    if Self::has_content_children(child) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Build a layout tree from a DOM document.

    /// Transfer position + offsets from a computed style onto a layout box.
    /// Percent offsets resolve later (apply time) from the style itself.
    fn transfer_positioning(layout_box: &mut LayoutBox, style: &ComputedStyle) {
        layout_box.position = if std::env::var("RK_NO_POS").is_ok() {
            Position::Static
        } else {
            match style.position {
                rustkit_css::Position::Static => Position::Static,
                // Relative: geometry-only for now. Entering the positioned
                // paint path (z-reorder, stacking hoist) wrecks pages whose
                // relative boxes are mere z-index anchors (about's cards)
                // until the stacking pipeline matures. Offsets still apply
                // via the Relative arm when we re-enable; today Relative
                // boxes with no offsets are visually identical to Static.
                rustkit_css::Position::Relative => Position::Static,
                rustkit_css::Position::Absolute => Position::Absolute,
                rustkit_css::Position::Fixed => Position::Fixed,
                rustkit_css::Position::Sticky => Position::Static, // sticky pipeline unproven; ledgered
            }
        };
        if layout_box.position != Position::Static {
            let px = |l: &Option<rustkit_css::Length>| match l {
                Some(rustkit_css::Length::Px(v)) => Some(*v),
                Some(rustkit_css::Length::Zero) => Some(0.0),
                Some(rustkit_css::Length::Rem(rem)) => Some(rem * 16.0),
                Some(rustkit_css::Length::Em(em)) => {
                    let fs = match style.font_size {
                        rustkit_css::Length::Px(p) => p,
                        _ => 16.0,
                    };
                    Some(em * fs)
                }
                _ => None,
            };
            layout_box.set_offsets(px(&style.top), px(&style.right), px(&style.bottom), px(&style.left));
        }
    }

    fn build_layout_from_document(
        &self,
        document: &Document,
        external_stylesheets: &[Stylesheet],
    ) -> LayoutBox {
        // Extract stylesheets from <style> elements
        let mut stylesheets = self.extract_stylesheets(document);

        // Add external stylesheets (loaded from <link> elements)
        stylesheets.extend(external_stylesheets.iter().cloned());

        let css_vars = self.extract_css_variables(&stylesheets);

        info!(
            inline_count = stylesheets.len() - external_stylesheets.len(),
            external_count = external_stylesheets.len(),
            css_var_count = css_vars.len(),
            "Extracted stylesheets and CSS variables"
        );

        // Create root layout box for the document
        let mut root_style = ComputedStyle::new();
        root_style.background_color = rustkit_css::Color::WHITE;
        let mut root_box = LayoutBox::new(BoxType::Block, root_style);

        // Compute the <html> element's style so its inherited properties (line-height,
        // font-family, color, font-size, ...) propagate into <body> and below. Layout starts
        // at <body>, but parity-reset.css — and browsers' own UA sheet — set inherited
        // properties on <html>; building body with `parent_style = None` silently dropped them
        // (e.g. `html { line-height: 1.5 }` never reached any heading). Chrome inherits html's
        // computed values through body to every descendant.
        let html_style = document.document_element().and_then(|html| {
            if let NodeType::Element {
                tag_name,
                attributes,
                ..
            } = &html.node_type
            {
                Some(self.compute_style_for_element(
                    tag_name,
                    attributes,
                    &stylesheets,
                    &css_vars,
                    &[],
                    &[],
                    0,
                    1,
                    None,
                ))
            } else {
                None
            }
        });

        // Get the body element and build layout from it
        if let Some(body) = document.body() {
            debug!("Found body element, building layout with stylesheets");
            let body_box = self.build_layout_from_node_with_parent_style(
                &body,
                &stylesheets,
                &css_vars,
                &[],
                html_style.as_ref(),
                &[],
                0,
                1,
            );
            // CSS 2.1 §14.2: the CANVAS background comes from the html
            // element, or from body when html's is transparent. The root box
            // paints it across the whole viewport (see the post-layout
            // viewport fill). Without this, a short page painted the body's
            // background only over its content height and left the rest of
            // the viewport white — invisible on the 26 campaign pages (all
            // viewport-filling), 52% of the frame on holdout-flex-toolbar.
            let html_bg = html_style
                .as_ref()
                .map(|s| s.background_color)
                .unwrap_or(rustkit_css::Color::TRANSPARENT);
            let mut body_box = body_box;
            if html_bg.a > 0.0 {
                root_box.style.background_color = html_bg;
            } else {
                // Port-back of Athena's #18 refinement: TRANSFER the
                // background (color + gradient) to the canvas and clear it
                // from body, so it paints once with true propagation
                // semantics — the paint-twice-same-color shortcut broke on
                // gradients and translucent colors.
                //
                // background_layers MUST move too. The shorthand parser
                // dual-stores a gradient in BOTH background_gradient (legacy)
                // and background_layers (multi-layer) — see the `background`
                // handler. Clearing only the legacy field left body's
                // background_layers intact, so the canvas painted the legacy
                // copy AND body re-painted the layer copy: a translucent
                // gradient layer (about's `…, transparent 50%` hero glow)
                // composited TWICE, over-saturating to effective alpha
                // 1-(1-a)² (0.15 → 0.277). Opaque backgrounds hid it
                // (double-compositing an opaque color is idempotent).
                root_box.style.background_color = body_box.style.background_color;
                root_box.style.background_gradient = body_box.style.background_gradient.clone();
                root_box.style.background_layers = body_box.style.background_layers.clone();
                if root_box.style.background_color.a > 0.0
                    || root_box.style.background_gradient.is_some()
                    || !root_box.style.background_layers.is_empty()
                {
                    body_box.style.background_color = rustkit_css::Color::TRANSPARENT;
                    body_box.style.background_gradient = None;
                    body_box.style.background_layers.clear();
                }
            }
            root_box.children.push(body_box);
        } else if let Some(html) = document.document_element() {
            // Fallback: use html element if no body
            debug!("No body found, using html element");
            let html_box =
                self.build_layout_from_node_with_styles(&html, &stylesheets, &css_vars, &[]);
            root_box.children.push(html_box);
        } else {
            warn!("No body or html element found!");
        }

        info!(total_children = root_box.children.len(), "Root box built");
        root_box
    }

    /// Build a layout box from a DOM node with stylesheet support.
    fn build_layout_from_node_with_styles(
        &self,
        node: &Rc<Node>,
        stylesheets: &[Stylesheet],
        css_vars: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
    ) -> LayoutBox {
        self.build_layout_from_node_with_parent_style(
            node,
            stylesheets,
            css_vars,
            ancestors,
            None,
            &[],
            0,
            1,
        )
    }

    fn build_layout_from_node_with_parent_style(
        &self,
        node: &Rc<Node>,
        stylesheets: &[Stylesheet],
        css_vars: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
        parent_style: Option<&ComputedStyle>,
        siblings_before: &[(String, Vec<String>, Option<String>)],
        element_index: usize,
        sibling_count: usize,
    ) -> LayoutBox {
        match &node.node_type {
            NodeType::Element {
                tag_name,
                attributes,
                ..
            } => {
                let tag_lower = tag_name.to_lowercase();

                // Skip rendering for certain elements
                let is_hidden = matches!(
                    tag_lower.as_str(),
                    "head" | "title" | "meta" | "link" | "script" | "style" | "noscript"
                );

                if is_hidden {
                    // Return an empty block for hidden elements
                    return LayoutBox::new(BoxType::Block, ComputedStyle::new());
                }

                // Create computed style based on element, attributes, and stylesheets
                let mut style = self.compute_style_for_element(
                    tag_name,
                    attributes,
                    stylesheets,
                    css_vars,
                    ancestors,
                    siblings_before,
                    element_index,
                    sibling_count,
                    parent_style,
                );

                // CSS computed-value resolution: font-size absolutizes at
                // style time — em/% against the PARENT's computed font-size,
                // rem against the root (16px). Layout falls back to 16px on
                // any non-Px font-size, so leaving Em here made
                // h1 { font-size: 2em } render at 16px.
                let parent_font_px = parent_style
                    .map(|p| match p.font_size {
                        rustkit_css::Length::Px(px) => px,
                        _ => 16.0,
                    })
                    .unwrap_or(16.0);
                style.font_size = match style.font_size {
                    rustkit_css::Length::Em(em) => rustkit_css::Length::Px(em * parent_font_px),
                    rustkit_css::Length::Percent(pct) => {
                        rustkit_css::Length::Px(pct / 100.0 * parent_font_px)
                    }
                    rustkit_css::Length::Rem(rem) => rustkit_css::Length::Px(rem * 16.0),
                    other => other,
                };

                // `line-height` is an inherited property, but rustkit only inherited it into text
                // nodes (see the NodeType::Text arm), never from one element to a descendant
                // element. That dropped cross-element inheritance of the computed value: e.g.
                // `html { line-height: 1.5 }` (as every websuite fixture sets via parity-reset.css)
                // never reached headings/paragraphs, so they fell back to the `Normal` 1.2
                // multiplier while Chrome inherits the 1.5 factor. The vertical drift shifted every
                // block below the first heading.
                //
                // The UA defaults in `compute_style_for_element` never set `line_height`, so a value
                // of `Normal` here reliably means "not specified by UA or author": inherit the
                // parent's computed value. `Number` inherits as a factor (re-resolved against this
                // element's own font-size); `Px` inherits as the absolute length — both match CSS
                // 2.1 §10.8 computed-value inheritance.
                if let Some(parent) = parent_style {
                    if matches!(style.line_height, rustkit_css::LineHeight::Normal)
                        && !matches!(parent.line_height, rustkit_css::LineHeight::Normal)
                    {
                        style.line_height = parent.line_height.clone();
                    }
                }

                // Check for display: none
                if style.display == rustkit_css::Display::None {
                    return LayoutBox::new(BoxType::Block, ComputedStyle::new());
                }

                // Handle replaced elements (images)
                if tag_lower == "img" {
                    let src = attributes.get("src").cloned().unwrap_or_default();

                    // Parse explicit dimensions from attributes
                    let explicit_width: Option<f32> =
                        attributes.get("width").and_then(|w| w.parse().ok());
                    let explicit_height: Option<f32> =
                        attributes.get("height").and_then(|h| h.parse().ok());

                    // width=/height= attributes are presentational hints: they set
                    // the used size like CSS width/height (lowest priority), they do
                    // NOT describe the image's natural dimensions. Author CSS wins.
                    if let Some(w) = explicit_width {
                        if matches!(style.width, rustkit_css::Length::Auto) {
                            style.width = rustkit_css::Length::Px(w);
                        }
                    }
                    if let Some(h) = explicit_height {
                        if matches!(style.height, rustkit_css::Length::Auto) {
                            style.height = rustkit_css::Length::Px(h);
                        }
                    }

                    // Resolve the image's real natural size: cache hit, or a
                    // synchronous decode for data: URLs (no network involved) —
                    // the same idiom the paint path uses when uploading images.
                    // Layout previously never consulted ImageManager and gave every
                    // CSS-sized <img> a 150x150 placeholder, so pages sized by
                    // stylesheet rules (not width=/height= attributes) drifted.
                    let loaded = Url::parse(&src).ok().and_then(|parsed_url| {
                        if let Some(cached) = self.image_manager.get_cached(&parsed_url) {
                            Some(cached)
                        } else if parsed_url.scheme() == "data" {
                            self.image_manager.load_blocking(parsed_url).ok()
                        } else {
                            None
                        }
                    });

                    let (natural_width, natural_height) = match &loaded {
                        Some(image) => (image.natural_width as f32, image.natural_height as f32),
                        // Image unavailable at layout time: fall back to the
                        // width=/height= attributes, then the placeholder size.
                        None => match (explicit_width, explicit_height) {
                            (Some(w), Some(h)) => (w, h),
                            (Some(w), None) => (w, w), // Assume square if only width
                            (None, Some(h)) => (h, h), // Assume square if only height
                            (None, None) => (150.0, 150.0), // Default placeholder size
                        },
                    };

                    return LayoutBox::new(
                        BoxType::Image {
                            url: src,
                            natural_width,
                            natural_height,
                        },
                        style,
                    );
                }

                // Handle form controls
                if tag_lower == "input" {
                    let input_type = attributes
                        .get("type")
                        .cloned()
                        .unwrap_or_else(|| "text".to_string());
                    let value = attributes.get("value").cloned().unwrap_or_default();
                    let placeholder = attributes.get("placeholder").cloned().unwrap_or_default();

                    let control = match input_type.as_str() {
                        "checkbox" => rustkit_layout::FormControlType::Checkbox {
                            checked: attributes.contains_key("checked"),
                        },
                        "radio" => rustkit_layout::FormControlType::Radio {
                            checked: attributes.contains_key("checked"),
                            name: attributes.get("name").cloned().unwrap_or_default(),
                        },
                        _ => rustkit_layout::FormControlType::TextInput {
                            value,
                            placeholder,
                            input_type,
                        },
                    };

                    return LayoutBox::new(BoxType::FormControl(control), style);
                }

                if tag_lower == "button" {
                    // Get button label from inner text or value
                    let text = node.text_content();
                    let label = if text.trim().is_empty() {
                        attributes
                            .get("value")
                            .cloned()
                            .unwrap_or_else(|| "Button".to_string())
                    } else {
                        text
                    };
                    let button_type = attributes
                        .get("type")
                        .cloned()
                        .unwrap_or_else(|| "button".to_string());

                    return LayoutBox::new(
                        BoxType::FormControl(rustkit_layout::FormControlType::Button {
                            label,
                            button_type,
                        }),
                        style,
                    );
                }

                if tag_lower == "textarea" {
                    let value = node.text_content();
                    let placeholder = attributes.get("placeholder").cloned().unwrap_or_default();
                    let rows = attributes
                        .get("rows")
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(2);
                    let cols = attributes
                        .get("cols")
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(20);

                    return LayoutBox::new(
                        BoxType::FormControl(rustkit_layout::FormControlType::TextArea {
                            value,
                            placeholder,
                            rows,
                            cols,
                        }),
                        style,
                    );
                }

                if tag_lower == "select" {
                    // Get options from children
                    let options: Vec<String> = node
                        .children()
                        .into_iter()
                        .filter_map(|child| {
                            if let rustkit_dom::NodeType::Element { tag_name, .. } =
                                &child.node_type
                            {
                                if tag_name.to_lowercase() == "option" {
                                    let text = child.text_content();
                                    if !text.is_empty() {
                                        return Some(text);
                                    }
                                }
                            }
                            None
                        })
                        .collect();

                    let selected_index = if options.is_empty() { None } else { Some(0) };

                    // size > 1 (or `multiple` without size, which Chrome
                    // shows as a 4-row listbox) renders inline rows.
                    let size = attributes
                        .get("size")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(if attributes.contains_key("multiple") { 4 } else { 0 });

                    return LayoutBox::new(
                        BoxType::FormControl(rustkit_layout::FormControlType::Select {
                            options,
                            selected_index,
                            size,
                        }),
                        style,
                    );
                }

                // Box type follows the COMPUTED display, not the tag:
                // `strong { display:block }` makes a block box (full width,
                // vertical margins honored) and a styled-inline div flows on
                // line boxes. The old tag-list approach ignored authored
                // display entirely — settings' block-styled <strong> labels
                // rendered as shrink-wrapped inlines, dropping their margins
                // (one term of the page-wide vertical drift). UA defaults
                // have already stamped display for every known tag by this
                // point, so style is authoritative here.
                let box_type = match style.display {
                    rustkit_css::Display::Inline => BoxType::Inline,
                    // Atomic inlines (inline-block/-flex/-grid) lay out their
                    // CONTENTS as blocks; inline-level placement is handled by
                    // the block child loop via display, not box type.
                    _ => BoxType::Block,
                };

                let mut layout_box = LayoutBox::new(box_type, style.clone());

                Self::transfer_positioning(&mut layout_box, &style);

                // Build ancestors list for child elements with class and ID info
                // Insert at beginning so ancestors[0] is always the immediate parent
                let classes: Vec<String> = attributes
                    .get("class")
                    .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                let id = attributes.get("id").cloned();
                let mut child_ancestors = vec![(tag_lower.clone(), classes, id)];
                child_ancestors.extend(ancestors.iter().cloned());

                // Check for ::before pseudo-element
                if let Some(before_box) = self.create_pseudo_element(
                    &tag_lower,
                    attributes,
                    stylesheets,
                    css_vars,
                    ancestors,
                    "::before",
                ) {
                    layout_box.children.push(before_box);
                }

                // Process children. Sibling context (preceding element siblings, element
                // index, element count) feeds `+`/`~` combinators and positional
                // pseudo-classes; it counts DOM element children, not layout boxes, since
                // CSS sibling relationships are defined on the element tree.
                let child_nodes = node.children();
                let child_element_count = child_nodes
                    .iter()
                    .filter(|c| matches!(c.node_type, NodeType::Element { .. }))
                    .count();
                let mut preceding_siblings: Vec<(String, Vec<String>, Option<String>)> =
                    Vec::with_capacity(child_element_count);
                for child in child_nodes {
                    let child_box = self.build_layout_from_node_with_parent_style(
                        &child,
                        stylesheets,
                        css_vars,
                        &child_ancestors,
                        Some(&style),
                        &preceding_siblings,
                        preceding_siblings.len(),
                        child_element_count,
                    );
                    if let NodeType::Element {
                        tag_name,
                        attributes,
                        ..
                    } = &child.node_type
                    {
                        let child_classes: Vec<String> = attributes
                            .get("class")
                            .map(|c| c.split_whitespace().map(|s| s.to_string()).collect())
                            .unwrap_or_default();
                        preceding_siblings.push((
                            tag_name.to_lowercase(),
                            child_classes,
                            attributes.get("id").cloned(),
                        ));
                    }

                    // Determine if box should be included in layout tree
                    let should_include = match child_box.box_type {
                        BoxType::Block | BoxType::AnonymousBlock => {
                            // Include blocks if they have children, OR have visible styling
                            !child_box.children.is_empty()
                                || Self::has_visible_styling(&child_box.style)
                        }
                        BoxType::Inline => {
                            // Include inline boxes if they have content children (text, images, form controls)
                            // or have visible styling (padding, border, background)
                            Self::has_content_children(&child_box)
                                || Self::has_visible_styling(&child_box.style)
                        }
                        BoxType::Text(_) | BoxType::Image { .. } | BoxType::FormControl(_) => true,
                    };

                    if should_include {
                        layout_box.children.push(child_box);
                    }
                }

                // Check for ::after pseudo-element
                if let Some(after_box) = self.create_pseudo_element(
                    &tag_lower,
                    attributes,
                    stylesheets,
                    css_vars,
                    ancestors,
                    "::after",
                ) {
                    layout_box.children.push(after_box);
                }

                // css-text §4.2 phase 2: collapsed spaces at segment
                // boundaries do not render. A text child's leading space is
                // stripped unless the previous sibling is inline-level; its
                // trailing space unless the next sibling is inline-level.
                // Text boxes reduced to nothing (inter-block whitespace)
                // are removed entirely.
                let n = layout_box.children.len();
                for i in 0..n {
                    let prev_inline =
                        i > 0 && Self::is_inline_level_box(&layout_box.children[i - 1]);
                    let next_inline =
                        i + 1 < n && Self::is_inline_level_box(&layout_box.children[i + 1]);
                    if let BoxType::Text(ref mut t) = layout_box.children[i].box_type {
                        if !prev_inline && t.starts_with(' ') {
                            *t = t.trim_start().to_string();
                        }
                        if !next_inline && t.ends_with(' ') {
                            *t = t.trim_end().to_string();
                        }
                    }
                }
                layout_box
                    .children
                    .retain(|c| !matches!(&c.box_type, BoxType::Text(t) if t.is_empty()));

                layout_box
            }
            NodeType::Text(text) => {
                // css-text §4.1: under collapsible white-space, runs of
                // whitespace collapse to one space and EDGE spaces are kept
                // (they separate this run from inline siblings); a
                // whitespace-only node becomes a single collapsed space.
                // The child-assembly post-pass strips spaces that land at
                // segment boundaries (line starts/ends, block siblings) —
                // it has the sibling context this node-level code lacks.
                // Pre-family white-space keeps the raw text.
                let ws = parent_style.map(|p| p.white_space).unwrap_or_default();
                let collapsible = !matches!(
                    ws,
                    rustkit_css::WhiteSpace::Pre
                        | rustkit_css::WhiteSpace::PreWrap
                        | rustkit_css::WhiteSpace::PreLine
                        | rustkit_css::WhiteSpace::BreakSpaces
                );
                let content = if collapsible {
                    let mut s = String::new();
                    if text.starts_with(char::is_whitespace) {
                        s.push(' ');
                    }
                    let mut first = true;
                    for w in text.split_whitespace() {
                        if !first {
                            s.push(' ');
                        }
                        s.push_str(w);
                        first = false;
                    }
                    if !first && text.ends_with(char::is_whitespace) {
                        s.push(' ');
                    }
                    s // whitespace-only input -> " " (leading-ws branch only)
                } else {
                    text.clone()
                };
                if content.is_empty() {
                    // Nothing at all (empty text node)
                    LayoutBox::new(BoxType::Inline, ComputedStyle::new())
                } else {
                    // Inherit font properties from parent style
                    let style = if let Some(parent) = parent_style {
                        let mut s = ComputedStyle::new();
                        // Inherit text-related properties
                        s.font_family = parent.font_family.clone();
                        s.font_size = parent.font_size.clone();
                        s.font_weight = parent.font_weight;
                        s.font_style = parent.font_style;
                        s.color = parent.color;
                        s.line_height = parent.line_height.clone();
                        s.text_align = parent.text_align;
                        s.text_decoration_line = parent.text_decoration_line;
                        s.text_decoration_color = parent.text_decoration_color;
                        s.letter_spacing = parent.letter_spacing.clone();
                        s.word_spacing = parent.word_spacing.clone();
                        s.text_transform = parent.text_transform;
                        // Wrapping behavior is inherited; without these a
                        // nowrap/pre parent's text still wrapped (shelf bug).
                        s.white_space = parent.white_space;
                        s.word_break = parent.word_break;
                        s.font_stretch = parent.font_stretch;
                        // NOT CSS inheritance — feature plumbing: gradient
                        // text (background-clip:text + transparent fill) is
                        // detected on the TEXT box at paint time
                        // (render_text's is_gradient_text), but the
                        // properties live on the ELEMENT. Without this copy
                        // the whole path was dead: about's hero painted the
                        // gradient as a full-width slab with white glyphs.
                        s.background_clip = parent.background_clip;
                        s.webkit_text_fill_color = parent.webkit_text_fill_color;
                        s.background_gradient = parent.background_gradient.clone();
                        s
                    } else {
                        let mut s = ComputedStyle::new();
                        s.color = rustkit_css::Color::BLACK;
                        s
                    };
                    LayoutBox::new(BoxType::Text(content), style)
                }
            }
            NodeType::Comment(_) => {
                // Comments should not create layout boxes - return an inline box that will be filtered out
                LayoutBox::new(BoxType::Inline, ComputedStyle::new())
            }
            _ => {
                // For other node types (Document, etc.), return empty box
                LayoutBox::new(BoxType::Block, ComputedStyle::new())
            }
        }
    }

    /// Create a pseudo-element (::before or ::after) if applicable.
    fn create_pseudo_element(
        &self,
        tag_name: &str,
        attributes: &std::collections::HashMap<String, String>,
        stylesheets: &[Stylesheet],
        _css_vars: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
        pseudo: &str,
    ) -> Option<LayoutBox> {
        // Compute style for the pseudo-element by matching selectors with the pseudo suffix
        let mut pseudo_style = ComputedStyle::new();

        // Collect matching rules for this element + pseudo
        // Use (a, b, c) specificity tuple converted to u32 for sorting
        let mut matching_rules: Vec<((usize, usize, usize), &Rule)> = Vec::new();

        for stylesheet in stylesheets {
            for rule in &stylesheet.rules {
                let selector = &rule.selector;

                // Check for explicit pseudo-element in selector
                if selector.ends_with(pseudo) || selector.ends_with(&pseudo.replace("::", ":")) {
                    // Get the base selector (without pseudo)
                    let base_selector = selector
                        .trim_end_matches(pseudo)
                        .trim_end_matches(&pseudo.replace("::", ":"));

                    // Check if base selector matches this element
                    // Use 0, 1 for element_index, sibling_count since we don't need sibling selectors for pseudo-elements
                    if self.selector_matches(
                        base_selector.trim(),
                        tag_name,
                        attributes,
                        ancestors,
                        &[],
                        0,
                        1,
                    ) {
                        let specificity = self.selector_specificity(selector);
                        matching_rules.push((specificity, rule));
                    }
                }
            }
        }

        // If no rules match, no pseudo-element
        if matching_rules.is_empty() {
            return None;
        }

        // Sort by specificity (a, b, c)
        matching_rules.sort_by_key(|(spec, _)| *spec);

        // Apply matching rules
        for (_, rule) in matching_rules {
            for declaration in &rule.declarations {
                let value_str = match &declaration.value {
                    rustkit_css::PropertyValue::Specified(s) => s.as_str(),
                    rustkit_css::PropertyValue::Inherit => continue,
                    rustkit_css::PropertyValue::Initial => continue,
                };
                self.apply_style_property(&mut pseudo_style, &declaration.property, value_str);
            }
        }

        // Only create pseudo-element if content property is set
        let content = pseudo_style.content.as_ref()?;

        // Create the pseudo-element box
        let mut pseudo_box = LayoutBox::new(BoxType::Inline, pseudo_style.clone());
        if std::env::var("RK_NO_PSEUDO_POS").is_err() {
            Self::transfer_positioning(&mut pseudo_box, &pseudo_style);
        }

        // If content is not empty, add a text child
        if !content.is_empty() {
            let mut text_style = pseudo_style.clone();
            text_style.content = None;
            let text_box = LayoutBox::new(BoxType::Text(content.clone()), text_style);
            pseudo_box.children.push(text_box);
        }

        Some(pseudo_box)
    }

    /// Compute a basic style for an element based on its tag and attributes.
    fn compute_style_for_element(
        &self,
        tag_name: &str,
        attributes: &std::collections::HashMap<String, String>,
        stylesheets: &[Stylesheet],
        css_vars: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
        siblings_before: &[(String, Vec<String>, Option<String>)],
        element_index: usize,
        sibling_count: usize,
        parent_style: Option<&ComputedStyle>,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::new();
        style.color = rustkit_css::Color::BLACK;

        // CSS inheritance (CSS 2.1 §6.2): inherited properties default to the
        // PARENT's computed value; UA defaults and author rules below
        // override. Until now only TEXT nodes inherited — an element with no
        // matching font-size rule silently reset to 16px, so
        // `body { font-size: 14px }` never reached any descendant div/span
        // (css-selectors: every unruled row +2px tall, sections drifting
        // +29px by the page bottom). font-size seeds the parent's already-
        // absolutized px; a relative author value (em/%) still resolves
        // against the parent right after cascade in the build walk.
        // white-space / line-height are NOT seeded here: line-height has its
        // own inheritance pass, and white-space is handled separately.
        //
        // text-align IS seeded (it is a genuinely inherited CSS property).
        // The old "double-shift" fear was a PRE-Slice-A artifact: back then
        // both the leaf AND the parent shifted a centered run. IFC Slice A
        // made the line-owning block the SOLE owner of horizontal alignment
        // (apply_text_align_offset reads the block's own text_align; leaves
        // never self-align). So `<div style="text-align:center"><h1>…</h1>`
        // only centers if the h1 itself carries Center — a block child is
        // never shifted by its parent's line alignment, so there is no
        // double-shift. Without this, about's hero (logo/tagline/version) and
        // any centered-container heading left-aligned.
        if let Some(parent) = parent_style {
            style.font_size = parent.font_size.clone();
            style.font_family = parent.font_family.clone();
            style.font_weight = parent.font_weight;
            style.font_style = parent.font_style;
            style.font_stretch = parent.font_stretch;
            style.color = parent.color;
            style.letter_spacing = parent.letter_spacing.clone();
            style.word_spacing = parent.word_spacing.clone();
            style.text_align = parent.text_align;
        }

        // Apply tag-specific default styles (user-agent stylesheet)
        // Apply tag-specific default styles (Chrome UA stylesheet alignment)
        // Reference: https://chromium.googlesource.com/chromium/blink/+/master/Source/core/css/html.css
        match tag_name.to_lowercase().as_str() {
            "html" => {
                style.display = rustkit_css::Display::Block;
            }
            "body" => {
                style.display = rustkit_css::Display::Block;
                style.background_color = rustkit_css::Color::WHITE;
                style.margin_top = rustkit_css::Length::Px(8.0);
                style.margin_right = rustkit_css::Length::Px(8.0);
                style.margin_bottom = rustkit_css::Length::Px(8.0);
                style.margin_left = rustkit_css::Length::Px(8.0);
            }
            // Headings (Chrome uses em units, we convert to px assuming 16px base)
            "h1" => {
                style.display = rustkit_css::Display::Block;
                style.font_size = rustkit_css::Length::Px(32.0); // 2em
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(21.44); // 0.67em * 32px
                style.margin_bottom = rustkit_css::Length::Px(21.44);
            }
            "h2" => {
                style.display = rustkit_css::Display::Block;
                style.font_size = rustkit_css::Length::Px(24.0); // 1.5em
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(19.92); // 0.83em * 24px
                style.margin_bottom = rustkit_css::Length::Px(19.92);
            }
            "h3" => {
                style.display = rustkit_css::Display::Block;
                style.font_size = rustkit_css::Length::Px(18.72); // 1.17em
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(18.72); // 1em
                style.margin_bottom = rustkit_css::Length::Px(18.72);
            }
            "h4" => {
                style.display = rustkit_css::Display::Block;
                style.font_size = rustkit_css::Length::Px(16.0); // 1em
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(21.28); // 1.33em
                style.margin_bottom = rustkit_css::Length::Px(21.28);
            }
            "h5" => {
                style.display = rustkit_css::Display::Block;
                style.font_size = rustkit_css::Length::Px(13.28); // 0.83em
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(22.17); // 1.67em
                style.margin_bottom = rustkit_css::Length::Px(22.17);
            }
            "h6" => {
                style.display = rustkit_css::Display::Block;
                style.font_size = rustkit_css::Length::Px(10.72); // 0.67em
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(25.0); // 2.33em
                style.margin_bottom = rustkit_css::Length::Px(25.0);
            }
            // Paragraphs and text blocks
            "p" => {
                style.display = rustkit_css::Display::Block;
                style.margin_top = rustkit_css::Length::Px(16.0); // 1em
                style.margin_bottom = rustkit_css::Length::Px(16.0);
            }
            "div" => {
                style.display = rustkit_css::Display::Block;
            }
            "span" => {
                style.display = rustkit_css::Display::Inline;
            }
            // Links
            "a" => {
                style.display = rustkit_css::Display::Inline;
                style.color = rustkit_css::Color::new(0, 0, 238, 1.0); // #0000EE
                style.text_decoration_line = rustkit_css::TextDecorationLine::UNDERLINE;
            }
            // Text formatting
            "strong" | "b" => {
                style.display = rustkit_css::Display::Inline;
                style.font_weight = rustkit_css::FontWeight::BOLD;
            }
            "em" | "i" => {
                style.display = rustkit_css::Display::Inline;
                style.font_style = rustkit_css::FontStyle::Italic;
            }
            "u" => {
                style.display = rustkit_css::Display::Inline;
                style.text_decoration_line = rustkit_css::TextDecorationLine::UNDERLINE;
            }
            "s" | "strike" | "del" => {
                style.display = rustkit_css::Display::Inline;
                style.text_decoration_line = rustkit_css::TextDecorationLine::LINE_THROUGH;
            }
            // Form controls do NOT inherit the document font in Chrome's UA
            // sheet — they get the system control font at 13.333px unless
            // the author sets one (css-selectors buttons: Chrome labels
            // measured ~11% narrower than our inherited 14px serif-stack
            // labels; every composed control width ran wide).
            "button" | "input" | "select" | "textarea" => {
                // Chrome's UA sheet computes inline-block for form controls.
                // Without this the Display default (Block) sends every
                // control down the block path: css-selectors §6 stacked its
                // three buttons vertically (h=124.6 vs Chrome 39) with the
                // interstitial whitespace text runs each taking a line.
                style.display = rustkit_css::Display::InlineBlock;
                style.font_size = rustkit_css::Length::Px(13.333);
                style.font_family = "system-ui".to_string();
            }
            "small" => {
                style.display = rustkit_css::Display::Inline;
                style.font_size = rustkit_css::Length::Px(13.0); // smaller
            }
            "big" => {
                style.display = rustkit_css::Display::Inline;
                style.font_size = rustkit_css::Length::Px(19.0); // larger
            }
            "sub" => {
                style.display = rustkit_css::Display::Inline;
                style.font_size = rustkit_css::Length::Px(13.0); // smaller
                                                                 // vertical-align: sub (not implemented)
            }
            "sup" => {
                style.display = rustkit_css::Display::Inline;
                style.font_size = rustkit_css::Length::Px(13.0); // smaller
                                                                 // vertical-align: super (not implemented)
            }
            // Code and preformatted
            "pre" => {
                style.display = rustkit_css::Display::Block;
                style.font_family = "monospace".to_string();
                style.margin_top = rustkit_css::Length::Px(16.0); // 1em
                style.margin_bottom = rustkit_css::Length::Px(16.0);
                // white-space: pre (not implemented)
            }
            "code" | "kbd" | "samp" | "tt" => {
                style.display = rustkit_css::Display::Inline;
                style.font_family = "monospace".to_string();
            }
            // Lists
            "ul" | "ol" => {
                style.display = rustkit_css::Display::Block;
                style.margin_top = rustkit_css::Length::Px(16.0); // 1em
                style.margin_bottom = rustkit_css::Length::Px(16.0);
                style.padding_left = rustkit_css::Length::Px(40.0);
            }
            "li" => {
                style.display = rustkit_css::Display::Block; // list-item
            }
            "dl" => {
                style.display = rustkit_css::Display::Block;
                style.margin_top = rustkit_css::Length::Px(16.0);
                style.margin_bottom = rustkit_css::Length::Px(16.0);
            }
            "dt" => {
                style.display = rustkit_css::Display::Block;
            }
            "dd" => {
                style.display = rustkit_css::Display::Block;
                style.margin_left = rustkit_css::Length::Px(40.0);
            }
            // Quotes
            "blockquote" => {
                style.display = rustkit_css::Display::Block;
                style.margin_top = rustkit_css::Length::Px(16.0); // 1em
                style.margin_bottom = rustkit_css::Length::Px(16.0);
                style.margin_left = rustkit_css::Length::Px(40.0);
                style.margin_right = rustkit_css::Length::Px(40.0);
            }
            "q" => {
                style.display = rustkit_css::Display::Inline;
                // quotes: auto (not implemented)
            }
            // Horizontal rule
            "hr" => {
                style.display = rustkit_css::Display::Block;
                style.border_top_width = rustkit_css::Length::Px(1.0);
                style.border_top_color = rustkit_css::Color::new(128, 128, 128, 1.0);
                style.margin_top = rustkit_css::Length::Px(8.0); // 0.5em
                style.margin_bottom = rustkit_css::Length::Px(8.0);
            }
            // Sections
            "article" | "aside" | "footer" | "header" | "main" | "nav" | "section" => {
                style.display = rustkit_css::Display::Block;
            }
            // Figure
            "figure" => {
                style.display = rustkit_css::Display::Block;
                style.margin_top = rustkit_css::Length::Px(16.0); // 1em
                style.margin_bottom = rustkit_css::Length::Px(16.0);
                style.margin_left = rustkit_css::Length::Px(40.0);
                style.margin_right = rustkit_css::Length::Px(40.0);
            }
            "figcaption" => {
                style.display = rustkit_css::Display::Block;
            }
            // Address
            "address" => {
                style.display = rustkit_css::Display::Block;
                style.font_style = rustkit_css::FontStyle::Italic;
            }
            // Form elements
            "form" => {
                style.display = rustkit_css::Display::Block;
            }
            "fieldset" => {
                style.display = rustkit_css::Display::Block;
                style.margin_left = rustkit_css::Length::Px(2.0);
                style.margin_right = rustkit_css::Length::Px(2.0);
                style.padding_top = rustkit_css::Length::Px(8.0); // 0.35em
                style.padding_bottom = rustkit_css::Length::Px(10.0); // 0.625em
                style.padding_left = rustkit_css::Length::Px(12.0); // 0.75em
                style.padding_right = rustkit_css::Length::Px(12.0);
                style.border_top_width = rustkit_css::Length::Px(2.0);
                style.border_right_width = rustkit_css::Length::Px(2.0);
                style.border_bottom_width = rustkit_css::Length::Px(2.0);
                style.border_left_width = rustkit_css::Length::Px(2.0);
                style.border_top_color = rustkit_css::Color::new(192, 192, 192, 1.0);
                style.border_right_color = rustkit_css::Color::new(192, 192, 192, 1.0);
                style.border_bottom_color = rustkit_css::Color::new(192, 192, 192, 1.0);
                style.border_left_color = rustkit_css::Color::new(192, 192, 192, 1.0);
            }
            "legend" => {
                style.display = rustkit_css::Display::Block;
                style.padding_left = rustkit_css::Length::Px(2.0);
                style.padding_right = rustkit_css::Length::Px(2.0);
            }
            "label" => {
                style.display = rustkit_css::Display::Inline;
            }
            "input" => {
                style.display = rustkit_css::Display::Inline;
                // Intrinsic sizing handled elsewhere
            }
            "button" => {
                style.display = rustkit_css::Display::Inline;
            }
            "select" => {
                style.display = rustkit_css::Display::Inline;
            }
            "textarea" => {
                style.display = rustkit_css::Display::Inline;
                style.font_family = "monospace".to_string();
            }
            // Table elements
            "table" => {
                style.display = rustkit_css::Display::Block; // Should be table
                                                             // border-collapse: separate (not implemented)
            }
            "caption" => {
                style.display = rustkit_css::Display::Block; // Should be table-caption
            }
            "thead" | "tbody" | "tfoot" => {
                style.display = rustkit_css::Display::Block; // Should be table-row-group
            }
            "tr" => {
                style.display = rustkit_css::Display::Block; // Should be table-row
            }
            "th" => {
                style.display = rustkit_css::Display::Block; // Should be table-cell
                style.font_weight = rustkit_css::FontWeight::BOLD;
            }
            "td" => {
                style.display = rustkit_css::Display::Block; // Should be table-cell
            }
            // Media
            "img" => {
                style.display = rustkit_css::Display::Inline;
            }
            "video" | "audio" => {
                style.display = rustkit_css::Display::Inline;
            }
            "canvas" => {
                style.display = rustkit_css::Display::Inline;
            }
            "iframe" => {
                style.display = rustkit_css::Display::Inline;
            }
            // Misc
            "br" => {
                style.display = rustkit_css::Display::Inline;
            }
            "mark" => {
                style.display = rustkit_css::Display::Inline;
                style.background_color = rustkit_css::Color::new(255, 255, 0, 1.0);
                // yellow
            }
            "abbr" | "acronym" => {
                style.display = rustkit_css::Display::Inline;
            }
            "cite" | "dfn" | "var" => {
                style.display = rustkit_css::Display::Inline;
                style.font_style = rustkit_css::FontStyle::Italic;
            }
            "ins" => {
                style.display = rustkit_css::Display::Inline;
                style.text_decoration_line = rustkit_css::TextDecorationLine::UNDERLINE;
            }
            _ => {}
        }

        // Collect matching rules with specificity for ordering
        let mut matching_rules: Vec<(&Rule, (usize, usize, usize), usize)> = Vec::new();
        let mut rule_index = 0;

        for stylesheet in stylesheets {
            for rule in &stylesheet.rules {
                if self.selector_matches(
                    &rule.selector,
                    tag_name,
                    attributes,
                    ancestors,
                    siblings_before,
                    element_index,
                    sibling_count,
                ) {
                    let specificity = self.selector_specificity(&rule.selector);
                    matching_rules.push((rule, specificity, rule_index));
                }
                rule_index += 1;
            }
        }

        // Sort by specificity (lower first, so they get overwritten by higher)
        matching_rules.sort_by(|a, b| {
            // Compare specificity: (ids, classes, tags)
            a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2))
        });

        // Apply matching rules in order
        for (rule, _, _) in matching_rules {
            for decl in &rule.declarations {
                // Extract string value from PropertyValue
                let value_str = match &decl.value {
                    rustkit_css::PropertyValue::Specified(s) => s.clone(),
                    rustkit_css::PropertyValue::Inherit => continue, // Skip inherit for now
                    rustkit_css::PropertyValue::Initial => continue, // Skip initial for now
                };
                let resolved_value = self.resolve_css_variables(&value_str, css_vars);
                if value_str != resolved_value {
                    trace!(
                        property = decl.property.as_str(),
                        original = value_str.as_str(),
                        resolved = resolved_value.as_str(),
                        "Resolved CSS variable"
                    );
                }
                self.apply_style_property(&mut style, &decl.property, &resolved_value);
            }
        }

        // Parse inline style attribute if present (highest specificity)
        if let Some(style_attr) = attributes.get("style") {
            self.apply_inline_style(&mut style, style_attr, css_vars);
        }

        style
    }

    /// Apply inline style attribute to computed style.
    fn apply_inline_style(
        &self,
        style: &mut ComputedStyle,
        style_attr: &str,
        css_vars: &HashMap<String, String>,
    ) {
        for declaration in style_attr.split(';') {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                continue;
            }
            if let Some((property, value)) = declaration.split_once(':') {
                let property = property.trim().to_lowercase();
                let value = value.trim();
                // Resolve CSS variables in the value
                let resolved_value = self.resolve_css_variables(value, css_vars);
                self.apply_style_property(style, &property, &resolved_value);
            }
        }
    }

    /// Apply a single CSS property to a computed style.
    fn apply_style_property(&self, style: &mut ComputedStyle, property: &str, value: &str) {
        let value = value.trim();

        // Handle CSS-wide keywords
        // inherit: use the computed value from the parent (already handled by inherit_from)
        // initial: use the property's initial value
        // unset: for inherited properties, acts like inherit; for non-inherited, acts like initial
        match value {
            "inherit" => {
                // Skip - the property will keep its inherited value
                return;
            }
            "initial" => {
                // Reset to initial value based on property
                self.apply_initial_value(style, property);
                return;
            }
            "unset" => {
                // For inherited properties (color, font-*), skip (keeps inherited value)
                // For non-inherited properties, apply initial
                if is_inherited_property(property) {
                    return;
                } else {
                    self.apply_initial_value(style, property);
                    return;
                }
            }
            _ => {}
        }

        match property {
            "color" => {
                if let Some(color) = parse_color(value) {
                    style.color = color;
                }
            }
            "background-color" => {
                if let Some(color) = parse_color(value) {
                    style.background_color = color;
                }
            }
            "background" | "background-image" => {
                // Handle multiple backgrounds (comma-separated)
                // CSS background layers are painted bottom-to-top
                // In the shorthand, the first layer is topmost, last is bottommost
                let layer_strs: Vec<&str> = split_by_comma(value);

                // Clear existing layers when setting new background
                style.background_layers.clear();

                // Process layers in reverse order so index 0 is bottommost
                for layer_str in layer_strs.iter().rev() {
                    let layer_str = layer_str.trim();
                    if layer_str.is_empty() {
                        continue;
                    }

                    // Check for color (goes to background_color, not layers)
                    if let Some(color) = parse_color(layer_str) {
                        style.background_color = color;
                        continue;
                    }

                    // Parse as a background layer (gradient or url)
                    if let Some(layer) = parse_background_layer(layer_str) {
                        style.background_layers.push(layer.clone());
                        // Also set legacy field for backwards compatibility
                        if let rustkit_css::BackgroundImage::Gradient(ref gradient) = layer.image {
                            style.background_gradient = Some(gradient.clone());
                        }
                    }
                }
            }
            "background-size" => {
                // Can be comma-separated for multiple layers
                // CSS order: first size applies to first (topmost) layer
                // Our array: index 0 is bottommost, last index is topmost
                // So we need to apply in reverse order
                let sizes: Vec<&str> = split_by_comma(value);
                let num_layers = style.background_layers.len();
                for (i, size_str) in sizes.iter().enumerate() {
                    let size = parse_background_size(size_str);
                    // Map CSS index to our reversed array: CSS[0] -> layers[n-1]
                    let layer_idx = num_layers.saturating_sub(i + 1);
                    if layer_idx < num_layers {
                        style.background_layers[layer_idx].size = size;
                    }
                }
            }
            "background-position" => {
                // Can be comma-separated for multiple layers
                // Same reversal logic as background-size
                let positions: Vec<&str> = split_by_comma(value);
                let num_layers = style.background_layers.len();
                for (i, pos_str) in positions.iter().enumerate() {
                    let position = parse_background_position(pos_str);
                    let layer_idx = num_layers.saturating_sub(i + 1);
                    if layer_idx < num_layers {
                        style.background_layers[layer_idx].position = position;
                    }
                }
            }
            "background-repeat" => {
                // Can be comma-separated for multiple layers
                // Same reversal logic as background-size
                let repeats: Vec<&str> = split_by_comma(value);
                let num_layers = style.background_layers.len();
                for (i, repeat_str) in repeats.iter().enumerate() {
                    let repeat = parse_background_repeat(repeat_str);
                    let layer_idx = num_layers.saturating_sub(i + 1);
                    if layer_idx < num_layers {
                        style.background_layers[layer_idx].repeat = repeat;
                    }
                }
            }
            "background-origin" => {
                // Same reversal logic as background-size
                let origins: Vec<&str> = split_by_comma(value);
                let num_layers = style.background_layers.len();
                for (i, origin_str) in origins.iter().enumerate() {
                    let origin = parse_background_origin(origin_str);
                    let layer_idx = num_layers.saturating_sub(i + 1);
                    if layer_idx < num_layers {
                        style.background_layers[layer_idx].origin = origin;
                    }
                }
            }
            "font-size" => {
                if let Some(length) = parse_length(value) {
                    style.font_size = length;
                }
            }
            "font-weight" => {
                // css-fonts-4 §2.2: keywords, any <number> in [1,1000], and the
                // lighter/bolder relative table. `style.font_weight` holds the
                // inherited weight at apply time, so relative keywords resolve
                // against it. The old arm accepted only bold/700/800/900 and
                // normal/400 — 100..300, 500, 600 and lighter/bolder were
                // silently dropped, so `font-weight: 300` text shaped Regular.
                let inherited = style.font_weight.0;
                let resolved = match value {
                    "normal" => Some(400),
                    "bold" => Some(700),
                    "bolder" => Some(match inherited {
                        0..=349 => 400,
                        350..=549 => 700,
                        550..=899 => 900,
                        _ => inherited, // ≥900: already at the top of the table
                    }),
                    "lighter" => Some(match inherited {
                        0..=99 => inherited, // <100: already at the bottom
                        100..=549 => 100,
                        550..=749 => 400,
                        _ => 700,
                    }),
                    _ => value.parse::<f32>().ok().and_then(|n| {
                        (1.0..=1000.0).contains(&n).then_some(n.round() as u16)
                    }),
                };
                if let Some(w) = resolved {
                    style.font_weight = rustkit_css::FontWeight(w);
                }
            }
            "font-family" => {
                style.font_family = value.trim_matches(|c| c == '"' || c == '\'').to_string();
            }
            "font-style" => {
                if value == "italic" {
                    style.font_style = rustkit_css::FontStyle::Italic;
                } else if value == "normal" {
                    style.font_style = rustkit_css::FontStyle::Normal;
                }
            }
            "line-height" => {
                // CSS line-height can be:
                // - "normal" (use font metrics)
                // - a unitless number (multiplier of font-size)
                // - a length with units (absolute value)
                // - a percentage (of font-size, treated as multiplier)
                if value == "normal" {
                    style.line_height = rustkit_css::LineHeight::Normal;
                } else if let Ok(lh) = value.parse::<f32>() {
                    // Unitless number - multiplier
                    style.line_height = rustkit_css::LineHeight::Number(lh);
                } else if let Some(length) = parse_length(value) {
                    match length {
                        // Absolute pixel value
                        rustkit_css::Length::Px(px) => {
                            style.line_height = rustkit_css::LineHeight::Px(px);
                        }
                        // Em is relative to font-size, so treat as multiplier
                        rustkit_css::Length::Em(em) => {
                            style.line_height = rustkit_css::LineHeight::Number(em);
                        }
                        // Percentage is relative to font-size, treat as multiplier
                        rustkit_css::Length::Percent(pct) => {
                            style.line_height = rustkit_css::LineHeight::Number(pct / 100.0);
                        }
                        // Rem - convert to multiplier (assuming 16px root font)
                        rustkit_css::Length::Rem(rem) => {
                            // This is approximate - ideally we'd track actual root font size
                            style.line_height = rustkit_css::LineHeight::Px(rem * 16.0);
                        }
                        _ => {}
                    }
                }
            }
            "margin" => {
                // Shorthand: margin can have 1-4 values
                if let Some((t, r, b, l)) = parse_shorthand_4(value) {
                    style.margin_top = t;
                    style.margin_right = r;
                    style.margin_bottom = b;
                    style.margin_left = l;
                }
            }
            "margin-top" => {
                if let Some(length) = parse_length(value) {
                    style.margin_top = length;
                }
            }
            "margin-right" => {
                if let Some(length) = parse_length(value) {
                    style.margin_right = length;
                }
            }
            "margin-bottom" => {
                if let Some(length) = parse_length(value) {
                    style.margin_bottom = length;
                }
            }
            "margin-left" => {
                if let Some(length) = parse_length(value) {
                    style.margin_left = length;
                }
            }
            "padding" => {
                // Shorthand: padding can have 1-4 values
                if let Some((t, r, b, l)) = parse_shorthand_4(value) {
                    style.padding_top = t;
                    style.padding_right = r;
                    style.padding_bottom = b;
                    style.padding_left = l;
                }
            }
            "padding-top" => {
                if let Some(length) = parse_length(value) {
                    style.padding_top = length;
                }
            }
            "padding-right" => {
                if let Some(length) = parse_length(value) {
                    style.padding_right = length;
                }
            }
            "padding-bottom" => {
                if let Some(length) = parse_length(value) {
                    style.padding_bottom = length;
                }
            }
            "padding-left" => {
                if let Some(length) = parse_length(value) {
                    style.padding_left = length;
                }
            }
            "border" => {
                // Shorthand: <width> || <style> || <color> — the old code fed the
                // whole value to parse_length, so `border: 2px solid #333` was
                // silently dropped and only a bare `border: 2px` ever applied.
                if let Some((width, color)) = parse_border_shorthand(value) {
                    style.border_top_width = width.clone();
                    style.border_right_width = width.clone();
                    style.border_bottom_width = width.clone();
                    style.border_left_width = width;
                    if let Some(color) = color {
                        style.border_top_color = color;
                        style.border_right_color = color;
                        style.border_bottom_color = color;
                        style.border_left_color = color;
                    }
                }
            }
            "border-width" => {
                // 1–4 length values, standard sides expansion
                if let Some((t, r, b, l)) = parse_shorthand_4(value) {
                    style.border_top_width = t;
                    style.border_right_width = r;
                    style.border_bottom_width = b;
                    style.border_left_width = l;
                }
            }
            "border-top" => {
                if let Some((width, color)) = parse_border_shorthand(value) {
                    style.border_top_width = width;
                    if let Some(color) = color {
                        style.border_top_color = color;
                    }
                }
            }
            "border-right" => {
                if let Some((width, color)) = parse_border_shorthand(value) {
                    style.border_right_width = width;
                    if let Some(color) = color {
                        style.border_right_color = color;
                    }
                }
            }
            "border-bottom" => {
                if let Some((width, color)) = parse_border_shorthand(value) {
                    style.border_bottom_width = width;
                    if let Some(color) = color {
                        style.border_bottom_color = color;
                    }
                }
            }
            "border-left" => {
                if let Some((width, color)) = parse_border_shorthand(value) {
                    style.border_left_width = width;
                    if let Some(color) = color {
                        style.border_left_color = color;
                    }
                }
            }
            "border-color" => {
                if let Some(color) = parse_color(value) {
                    style.border_top_color = color;
                    style.border_right_color = color;
                    style.border_bottom_color = color;
                    style.border_left_color = color;
                }
            }
            "display" => {
                if let Some(display) = parse_display(value) {
                    style.display = display;
                }
            }
            // Flexbox properties
            "flex-grow" => {
                if let Ok(grow) = value.parse::<f32>() {
                    style.flex_grow = grow;
                }
            }
            "flex-shrink" => {
                if let Ok(shrink) = value.parse::<f32>() {
                    style.flex_shrink = shrink;
                }
            }
            "flex-basis" => {
                if value == "auto" {
                    style.flex_basis = rustkit_css::FlexBasis::Auto;
                } else if value == "content" {
                    style.flex_basis = rustkit_css::FlexBasis::Content;
                } else if let Some(length) = parse_length(value) {
                    match length {
                        rustkit_css::Length::Px(px) => {
                            style.flex_basis = rustkit_css::FlexBasis::Length(px)
                        }
                        rustkit_css::Length::Percent(pct) => {
                            style.flex_basis = rustkit_css::FlexBasis::Percent(pct)
                        }
                        _ => {}
                    }
                }
            }
            "flex" => {
                // Shorthand: flex: <grow> [<shrink>] [<basis>]
                let parts: Vec<&str> = value.split_whitespace().collect();
                if parts.len() >= 1 {
                    if let Ok(grow) = parts[0].parse::<f32>() {
                        style.flex_grow = grow;
                    }
                }
                if parts.len() >= 2 {
                    if let Ok(shrink) = parts[1].parse::<f32>() {
                        style.flex_shrink = shrink;
                    }
                }
                if parts.len() >= 3 {
                    if let Some(length) = parse_length(parts[2]) {
                        match length {
                            rustkit_css::Length::Px(px) => {
                                style.flex_basis = rustkit_css::FlexBasis::Length(px)
                            }
                            rustkit_css::Length::Percent(pct) => {
                                style.flex_basis = rustkit_css::FlexBasis::Percent(pct)
                            }
                            _ => {}
                        }
                    }
                }
            }
            "flex-direction" => {
                style.flex_direction = match value.trim() {
                    "row" => rustkit_css::FlexDirection::Row,
                    "row-reverse" => rustkit_css::FlexDirection::RowReverse,
                    "column" => rustkit_css::FlexDirection::Column,
                    "column-reverse" => rustkit_css::FlexDirection::ColumnReverse,
                    _ => rustkit_css::FlexDirection::Row,
                };
            }
            "flex-wrap" => {
                style.flex_wrap = match value.trim() {
                    "nowrap" => rustkit_css::FlexWrap::NoWrap,
                    "wrap" => rustkit_css::FlexWrap::Wrap,
                    "wrap-reverse" => rustkit_css::FlexWrap::WrapReverse,
                    _ => rustkit_css::FlexWrap::NoWrap,
                };
            }
            "justify-content" => {
                style.justify_content = match value.trim() {
                    "flex-start" | "start" => rustkit_css::JustifyContent::FlexStart,
                    "flex-end" | "end" => rustkit_css::JustifyContent::FlexEnd,
                    "center" => rustkit_css::JustifyContent::Center,
                    "space-between" => rustkit_css::JustifyContent::SpaceBetween,
                    "space-around" => rustkit_css::JustifyContent::SpaceAround,
                    "space-evenly" => rustkit_css::JustifyContent::SpaceEvenly,
                    _ => rustkit_css::JustifyContent::FlexStart,
                };
            }
            "align-items" => {
                style.align_items = match value.trim() {
                    "flex-start" | "start" => rustkit_css::AlignItems::FlexStart,
                    "flex-end" | "end" => rustkit_css::AlignItems::FlexEnd,
                    "center" => rustkit_css::AlignItems::Center,
                    "baseline" => rustkit_css::AlignItems::Baseline,
                    "stretch" => rustkit_css::AlignItems::Stretch,
                    _ => rustkit_css::AlignItems::Stretch,
                };
            }
            "align-content" => {
                style.align_content = match value.trim() {
                    "flex-start" | "start" => rustkit_css::AlignContent::FlexStart,
                    "flex-end" | "end" => rustkit_css::AlignContent::FlexEnd,
                    "center" => rustkit_css::AlignContent::Center,
                    "space-between" => rustkit_css::AlignContent::SpaceBetween,
                    "space-around" => rustkit_css::AlignContent::SpaceAround,
                    "stretch" => rustkit_css::AlignContent::Stretch,
                    _ => rustkit_css::AlignContent::Stretch,
                };
            }
            "align-self" => {
                style.align_self = match value.trim() {
                    "auto" => rustkit_css::AlignSelf::Auto,
                    "flex-start" | "start" => rustkit_css::AlignSelf::FlexStart,
                    "flex-end" | "end" => rustkit_css::AlignSelf::FlexEnd,
                    "center" => rustkit_css::AlignSelf::Center,
                    "baseline" => rustkit_css::AlignSelf::Baseline,
                    "stretch" => rustkit_css::AlignSelf::Stretch,
                    _ => rustkit_css::AlignSelf::Auto,
                };
            }
            "gap" | "grid-gap" => {
                // gap shorthand (row-gap column-gap or single value)
                if let Some(length) = parse_length(value) {
                    style.row_gap = length.clone();
                    style.column_gap = length;
                }
            }
            "row-gap" => {
                if let Some(length) = parse_length(value) {
                    style.row_gap = length;
                }
            }
            "column-gap" => {
                if let Some(length) = parse_length(value) {
                    style.column_gap = length;
                }
            }
            "order" => {
                if let Ok(order) = value.parse::<i32>() {
                    style.order = order;
                }
            }
            "aspect-ratio" => {
                // Parse aspect-ratio: width / height or auto
                let value = value.trim();
                if value == "auto" {
                    // Auto is the default, do nothing
                } else if let Some(slash_pos) = value.find('/') {
                    // Format: width / height
                    let width_str = value[..slash_pos].trim();
                    let height_str = value[slash_pos + 1..].trim();
                    if let (Ok(w), Ok(h)) = (width_str.parse::<f32>(), height_str.parse::<f32>()) {
                        if h > 0.0 {
                            style.aspect_ratio = Some(w / h);
                        }
                    }
                } else if let Ok(ratio) = value.parse::<f32>() {
                    // Single number (ratio to 1)
                    style.aspect_ratio = Some(ratio);
                }
            }
            "vertical-align" => {
                // Sixth parsed-but-never-applied property found this week
                // (text-align, background-clip, inheritance, bold system
                // font, control font — now this). Slice C reads Baseline
                // and Middle; other values parse and fall through to
                // baseline behavior at layout (documented subset).
                style.vertical_align = match value.trim() {
                    "middle" => rustkit_css::VerticalAlign::Middle,
                    "top" => rustkit_css::VerticalAlign::Top,
                    "bottom" => rustkit_css::VerticalAlign::Bottom,
                    "text-top" => rustkit_css::VerticalAlign::TextTop,
                    "text-bottom" => rustkit_css::VerticalAlign::TextBottom,
                    "sub" => rustkit_css::VerticalAlign::Sub,
                    "super" => rustkit_css::VerticalAlign::Super,
                    _ => rustkit_css::VerticalAlign::Baseline,
                };
            }
            "text-align" => {
                // Never applied until 2026-07-10: this arm silently dropped
                // the declaration, so every `text-align: center/right` on
                // every fixture was a no-op — centered headlines painted
                // left-aligned and the alignment machinery in rustkit-layout
                // only ever saw the Left default.
                style.text_align = match value.trim() {
                    "center" => rustkit_css::TextAlign::Center,
                    "right" | "end" => rustkit_css::TextAlign::Right,
                    "justify" => rustkit_css::TextAlign::Justify,
                    _ => rustkit_css::TextAlign::Left,
                };
            }
            "border-radius" => {
                // Parse border-radius (shorthand: all corners same)
                if let Some(length) = rustkit_css::parse_length(value) {
                    style.border_top_left_radius = length.clone();
                    style.border_top_right_radius = length.clone();
                    style.border_bottom_right_radius = length.clone();
                    style.border_bottom_left_radius = length;
                }
            }
            "border-top-left-radius" => {
                if let Some(length) = rustkit_css::parse_length(value) {
                    style.border_top_left_radius = length;
                }
            }
            "border-top-right-radius" => {
                if let Some(length) = rustkit_css::parse_length(value) {
                    style.border_top_right_radius = length;
                }
            }
            "border-bottom-right-radius" => {
                if let Some(length) = rustkit_css::parse_length(value) {
                    style.border_bottom_right_radius = length;
                }
            }
            "border-bottom-left-radius" => {
                if let Some(length) = rustkit_css::parse_length(value) {
                    style.border_bottom_left_radius = length;
                }
            }
            "box-shadow" => {
                // Parse box-shadow: offset-x offset-y blur spread color [inset]
                // Simple parser for common formats
                if let Some(shadow) = parse_box_shadow(value) {
                    style.box_shadows.push(shadow);
                }
            }
            "width" => {
                if let Some(length) = parse_length(value) {
                    style.width = length;
                }
            }
            "height" => {
                if let Some(length) = parse_length(value) {
                    style.height = length;
                }
            }
            "min-width" => {
                if let Some(length) = parse_length(value) {
                    style.min_width = length;
                }
            }
            "min-height" => {
                if let Some(length) = parse_length(value) {
                    style.min_height = length;
                }
            }
            "max-width" => {
                if let Some(length) = parse_length(value) {
                    style.max_width = length;
                }
            }
            "max-height" => {
                if let Some(length) = parse_length(value) {
                    style.max_height = length;
                }
            }
            "opacity" => {
                if let Ok(opacity) = value.parse::<f32>() {
                    style.opacity = opacity.clamp(0.0, 1.0);
                }
            }
            "position" => {
                style.position = match value.trim() {
                    "static" => rustkit_css::Position::Static,
                    "relative" => rustkit_css::Position::Relative,
                    "absolute" => rustkit_css::Position::Absolute,
                    "fixed" => rustkit_css::Position::Fixed,
                    "sticky" => rustkit_css::Position::Sticky,
                    _ => rustkit_css::Position::Static,
                };
            }
            "top" => {
                if let Some(length) = parse_length(value) {
                    style.top = Some(length);
                }
            }
            "right" => {
                if let Some(length) = parse_length(value) {
                    style.right = Some(length);
                }
            }
            "bottom" => {
                if let Some(length) = parse_length(value) {
                    style.bottom = Some(length);
                }
            }
            "left" => {
                if let Some(length) = parse_length(value) {
                    style.left = Some(length);
                }
            }
            "inset" => {
                // Shorthand: inset: top right bottom left (or 1-4 values)
                let parts: Vec<&str> = value.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        if let Some(length) = parse_length(parts[0]) {
                            style.top = Some(length.clone());
                            style.right = Some(length.clone());
                            style.bottom = Some(length.clone());
                            style.left = Some(length);
                        }
                    }
                    2 => {
                        if let (Some(tb), Some(lr)) =
                            (parse_length(parts[0]), parse_length(parts[1]))
                        {
                            style.top = Some(tb.clone());
                            style.bottom = Some(tb);
                            style.right = Some(lr.clone());
                            style.left = Some(lr);
                        }
                    }
                    4 => {
                        if let (Some(t), Some(r), Some(b), Some(l)) = (
                            parse_length(parts[0]),
                            parse_length(parts[1]),
                            parse_length(parts[2]),
                            parse_length(parts[3]),
                        ) {
                            style.top = Some(t);
                            style.right = Some(r);
                            style.bottom = Some(b);
                            style.left = Some(l);
                        }
                    }
                    _ => {}
                }
            }
            "overflow" => {
                style.overflow_x = parse_overflow(value);
                style.overflow_y = parse_overflow(value);
            }
            "overflow-x" => {
                style.overflow_x = parse_overflow(value);
            }
            "overflow-y" => {
                style.overflow_y = parse_overflow(value);
            }
            "z-index" => {
                if let Ok(z) = value.parse::<i32>() {
                    style.z_index = z;
                }
            }
            "text-decoration" | "text-decoration-line" => {
                match value.trim().to_lowercase().as_str() {
                    "none" => style.text_decoration_line = rustkit_css::TextDecorationLine::NONE,
                    "underline" => {
                        style.text_decoration_line = rustkit_css::TextDecorationLine::UNDERLINE
                    }
                    "overline" => {
                        style.text_decoration_line = rustkit_css::TextDecorationLine::OVERLINE
                    }
                    "line-through" => {
                        style.text_decoration_line = rustkit_css::TextDecorationLine::LINE_THROUGH
                    }
                    _ => {
                        // Handle combined values like "underline line-through"
                        let mut decoration = rustkit_css::TextDecorationLine::NONE;
                        for part in value.split_whitespace() {
                            match part.to_lowercase().as_str() {
                                "underline" => decoration.underline = true,
                                "overline" => decoration.overline = true,
                                "line-through" => decoration.line_through = true,
                                _ => {}
                            }
                        }
                        style.text_decoration_line = decoration;
                    }
                }
            }
            "text-decoration-color" => {
                if let Some(color) = parse_color(value) {
                    style.text_decoration_color = Some(color);
                }
            }
            "text-decoration-style" => {
                style.text_decoration_style = match value.trim().to_lowercase().as_str() {
                    "solid" => rustkit_css::TextDecorationStyle::Solid,
                    "double" => rustkit_css::TextDecorationStyle::Double,
                    "dotted" => rustkit_css::TextDecorationStyle::Dotted,
                    "dashed" => rustkit_css::TextDecorationStyle::Dashed,
                    "wavy" => rustkit_css::TextDecorationStyle::Wavy,
                    _ => rustkit_css::TextDecorationStyle::Solid,
                };
            }
            "letter-spacing" => {
                if let Some(length) = parse_length(value) {
                    style.letter_spacing = length;
                }
            }
            "word-spacing" => {
                if let Some(length) = parse_length(value) {
                    style.word_spacing = length;
                }
            }
            "text-transform" => {
                style.text_transform = match value.trim().to_lowercase().as_str() {
                    "uppercase" => rustkit_css::TextTransform::Uppercase,
                    "lowercase" => rustkit_css::TextTransform::Lowercase,
                    "capitalize" => rustkit_css::TextTransform::Capitalize,
                    _ => rustkit_css::TextTransform::None,
                };
            }
            "white-space" => {
                style.white_space = match value.trim().to_lowercase().as_str() {
                    "pre" => rustkit_css::WhiteSpace::Pre,
                    "nowrap" => rustkit_css::WhiteSpace::Nowrap,
                    "pre-wrap" => rustkit_css::WhiteSpace::PreWrap,
                    "pre-line" => rustkit_css::WhiteSpace::PreLine,
                    _ => rustkit_css::WhiteSpace::Normal,
                };
            }
            "border-top-width" => {
                if let Some(length) = parse_length(value) {
                    style.border_top_width = length;
                }
            }
            "border-right-width" => {
                if let Some(length) = parse_length(value) {
                    style.border_right_width = length;
                }
            }
            "border-bottom-width" => {
                if let Some(length) = parse_length(value) {
                    style.border_bottom_width = length;
                }
            }
            "border-left-width" => {
                if let Some(length) = parse_length(value) {
                    style.border_left_width = length;
                }
            }
            "border-top-color" => {
                if let Some(color) = parse_color(value) {
                    style.border_top_color = color;
                }
            }
            "border-right-color" => {
                if let Some(color) = parse_color(value) {
                    style.border_right_color = color;
                }
            }
            "border-bottom-color" => {
                if let Some(color) = parse_color(value) {
                    style.border_bottom_color = color;
                }
            }
            "border-left-color" => {
                if let Some(color) = parse_color(value) {
                    style.border_left_color = color;
                }
            }
            // CSS Grid properties
            "grid-template-columns" => {
                if let Some(template) = parse_grid_template(value) {
                    style.grid_template_columns = template;
                }
            }
            "grid-template-rows" => {
                if let Some(template) = parse_grid_template(value) {
                    style.grid_template_rows = template;
                }
            }
            "grid-column" => {
                // Shorthand: grid-column: start / end
                if let Some((start, end)) = parse_grid_line_shorthand(value) {
                    style.grid_column_start = start;
                    style.grid_column_end = end;
                }
            }
            "grid-column-start" => {
                if let Some(line) = parse_grid_line(value) {
                    style.grid_column_start = line;
                }
            }
            "grid-column-end" => {
                if let Some(line) = parse_grid_line(value) {
                    style.grid_column_end = line;
                }
            }
            "grid-row" => {
                // Shorthand: grid-row: start / end
                if let Some((start, end)) = parse_grid_line_shorthand(value) {
                    style.grid_row_start = start;
                    style.grid_row_end = end;
                }
            }
            "grid-row-start" => {
                if let Some(line) = parse_grid_line(value) {
                    style.grid_row_start = line;
                }
            }
            "grid-row-end" => {
                if let Some(line) = parse_grid_line(value) {
                    style.grid_row_end = line;
                }
            }
            "grid-auto-flow" => {
                style.grid_auto_flow = match value.trim() {
                    "row" => rustkit_css::GridAutoFlow::Row,
                    "column" => rustkit_css::GridAutoFlow::Column,
                    "row dense" | "dense row" => rustkit_css::GridAutoFlow::RowDense,
                    "column dense" | "dense column" => rustkit_css::GridAutoFlow::ColumnDense,
                    "dense" => rustkit_css::GridAutoFlow::RowDense,
                    _ => rustkit_css::GridAutoFlow::Row,
                };
            }
            "grid-auto-columns" => {
                if let Some(size) = parse_track_size(value) {
                    style.grid_auto_columns = size;
                }
            }
            "grid-auto-rows" => {
                if let Some(size) = parse_track_size(value) {
                    style.grid_auto_rows = size;
                }
            }
            // ==================== Transforms ====================
            "transform" => {
                if let Some(transform_list) = parse_transform(value) {
                    style.transform = transform_list;
                }
            }
            "transform-origin" => {
                if let Some(origin) = parse_transform_origin(value) {
                    style.transform_origin = origin;
                }
            }
            // ==================== Transitions (parsed, not executed) ====================
            "transition" => {
                // Shorthand: property duration timing-function delay
                let parts: Vec<&str> = value.split_whitespace().collect();
                if !parts.is_empty() {
                    style.transition_property = parts[0].to_string();
                }
                if parts.len() > 1 {
                    if let Some(dur) = parse_time(parts[1]) {
                        style.transition_duration = dur;
                    }
                }
                if parts.len() > 2 {
                    style.transition_timing_function = parse_timing_function(parts[2]);
                }
                if parts.len() > 3 {
                    if let Some(delay) = parse_time(parts[3]) {
                        style.transition_delay = delay;
                    }
                }
            }
            "transition-property" => {
                style.transition_property = value.trim().to_string();
            }
            "transition-duration" => {
                if let Some(dur) = parse_time(value) {
                    style.transition_duration = dur;
                }
            }
            "transition-timing-function" => {
                style.transition_timing_function = parse_timing_function(value);
            }
            "transition-delay" => {
                if let Some(delay) = parse_time(value) {
                    style.transition_delay = delay;
                }
            }
            // ==================== Animations (parsed, not executed) ====================
            "animation" => {
                // Shorthand: name duration timing-function delay iteration-count direction fill-mode play-state
                let parts: Vec<&str> = value.split_whitespace().collect();
                for (i, part) in parts.iter().enumerate() {
                    // First non-time value is usually the name
                    if i == 0 && !part.ends_with('s') && !part.ends_with("ms") {
                        style.animation_name = part.to_string();
                    } else if let Some(t) = parse_time(part) {
                        if style.animation_duration == 0.0 {
                            style.animation_duration = t;
                        } else {
                            style.animation_delay = t;
                        }
                    } else {
                        match *part {
                            "infinite" => {
                                style.animation_iteration_count =
                                    rustkit_css::AnimationIterationCount::Infinite
                            }
                            "normal" => {
                                style.animation_direction = rustkit_css::AnimationDirection::Normal
                            }
                            "reverse" => {
                                style.animation_direction = rustkit_css::AnimationDirection::Reverse
                            }
                            "alternate" => {
                                style.animation_direction =
                                    rustkit_css::AnimationDirection::Alternate
                            }
                            "alternate-reverse" => {
                                style.animation_direction =
                                    rustkit_css::AnimationDirection::AlternateReverse
                            }
                            "forwards" => {
                                style.animation_fill_mode = rustkit_css::AnimationFillMode::Forwards
                            }
                            "backwards" => {
                                style.animation_fill_mode =
                                    rustkit_css::AnimationFillMode::Backwards
                            }
                            "both" => {
                                style.animation_fill_mode = rustkit_css::AnimationFillMode::Both
                            }
                            "paused" => {
                                style.animation_play_state = rustkit_css::AnimationPlayState::Paused
                            }
                            "running" => {
                                style.animation_play_state =
                                    rustkit_css::AnimationPlayState::Running
                            }
                            _ => {
                                // Could be timing function or name
                                if i == 0 || style.animation_name.is_empty() {
                                    style.animation_name = part.to_string();
                                } else {
                                    style.animation_timing_function = parse_timing_function(part);
                                }
                            }
                        }
                    }
                }
            }
            "animation-name" => {
                style.animation_name = value.trim().to_string();
            }
            "animation-duration" => {
                if let Some(dur) = parse_time(value) {
                    style.animation_duration = dur;
                }
            }
            "animation-timing-function" => {
                style.animation_timing_function = parse_timing_function(value);
            }
            "animation-delay" => {
                if let Some(delay) = parse_time(value) {
                    style.animation_delay = delay;
                }
            }
            "animation-iteration-count" => {
                let v = value.trim();
                if v == "infinite" {
                    style.animation_iteration_count =
                        rustkit_css::AnimationIterationCount::Infinite;
                } else if let Ok(n) = v.parse::<f32>() {
                    style.animation_iteration_count =
                        rustkit_css::AnimationIterationCount::Count(n);
                }
            }
            "animation-direction" => {
                style.animation_direction = match value.trim() {
                    "normal" => rustkit_css::AnimationDirection::Normal,
                    "reverse" => rustkit_css::AnimationDirection::Reverse,
                    "alternate" => rustkit_css::AnimationDirection::Alternate,
                    "alternate-reverse" => rustkit_css::AnimationDirection::AlternateReverse,
                    _ => rustkit_css::AnimationDirection::Normal,
                };
            }
            "animation-fill-mode" => {
                style.animation_fill_mode = match value.trim() {
                    "none" => rustkit_css::AnimationFillMode::None,
                    "forwards" => rustkit_css::AnimationFillMode::Forwards,
                    "backwards" => rustkit_css::AnimationFillMode::Backwards,
                    "both" => rustkit_css::AnimationFillMode::Both,
                    _ => rustkit_css::AnimationFillMode::None,
                };
            }
            "animation-play-state" => {
                style.animation_play_state = match value.trim() {
                    "running" => rustkit_css::AnimationPlayState::Running,
                    "paused" => rustkit_css::AnimationPlayState::Paused,
                    _ => rustkit_css::AnimationPlayState::Running,
                };
            }
            // ==================== Box Sizing ====================
            "box-sizing" => {
                style.box_sizing = match value.trim() {
                    "content-box" => rustkit_css::BoxSizing::ContentBox,
                    "border-box" => rustkit_css::BoxSizing::BorderBox,
                    _ => rustkit_css::BoxSizing::ContentBox,
                };
            }
            // ==================== Pseudo-element content ====================
            "content" => {
                let v = value.trim();
                if v == "none" || v == "normal" {
                    style.content = None;
                } else if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
                    // Quoted string content
                    style.content = Some(v[1..v.len() - 1].to_string());
                } else if v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2 {
                    // Single-quoted string content
                    style.content = Some(v[1..v.len() - 1].to_string());
                } else if v == "''" || v == "\"\"" {
                    // Empty string
                    style.content = Some(String::new());
                }
            }
            // ==================== Background clip (for gradient text) ====================
            "background-clip" | "-webkit-background-clip" => {
                style.background_clip = match value.trim() {
                    "border-box" => rustkit_css::BackgroundClip::BorderBox,
                    "padding-box" => rustkit_css::BackgroundClip::PaddingBox,
                    "content-box" => rustkit_css::BackgroundClip::ContentBox,
                    "text" => rustkit_css::BackgroundClip::Text,
                    _ => rustkit_css::BackgroundClip::BorderBox,
                };
            }
            "-webkit-text-fill-color" => {
                if let Some(color) = parse_color(value) {
                    style.webkit_text_fill_color = Some(color);
                } else if value.trim() == "transparent" {
                    style.webkit_text_fill_color = Some(rustkit_css::Color::TRANSPARENT);
                }
            }
            _ => {
                // Unknown property, ignore
            }
        }
    }

    /// Apply the initial (default) value for a CSS property.
    fn apply_initial_value(&self, style: &mut ComputedStyle, property: &str) {
        match property {
            "color" => style.color = rustkit_css::Color::BLACK,
            "background-color" => style.background_color = rustkit_css::Color::TRANSPARENT,
            "font-size" => style.font_size = rustkit_css::Length::Px(16.0),
            "font-weight" => style.font_weight = rustkit_css::FontWeight::NORMAL,
            "font-style" => style.font_style = rustkit_css::FontStyle::Normal,
            "font-family" => style.font_family = String::new(),
            "line-height" => style.line_height = rustkit_css::LineHeight::Normal,
            "margin" | "margin-top" => style.margin_top = rustkit_css::Length::Zero,
            "margin-right" => style.margin_right = rustkit_css::Length::Zero,
            "margin-bottom" => style.margin_bottom = rustkit_css::Length::Zero,
            "margin-left" => style.margin_left = rustkit_css::Length::Zero,
            "padding" | "padding-top" => style.padding_top = rustkit_css::Length::Zero,
            "padding-right" => style.padding_right = rustkit_css::Length::Zero,
            "padding-bottom" => style.padding_bottom = rustkit_css::Length::Zero,
            "padding-left" => style.padding_left = rustkit_css::Length::Zero,
            "border-width" | "border-top-width" => {
                style.border_top_width = rustkit_css::Length::Zero
            }
            "border-right-width" => style.border_right_width = rustkit_css::Length::Zero,
            "border-bottom-width" => style.border_bottom_width = rustkit_css::Length::Zero,
            "border-left-width" => style.border_left_width = rustkit_css::Length::Zero,
            "width" => style.width = rustkit_css::Length::Auto,
            "height" => style.height = rustkit_css::Length::Auto,
            "display" => style.display = rustkit_css::Display::Block,
            "opacity" => style.opacity = 1.0,
            _ => {
                // Unknown property, do nothing
            }
        }
    }

    /// Extract CSS text from <style> elements in the document.
    fn extract_stylesheets(&self, document: &Document) -> Vec<Stylesheet> {
        let mut stylesheets = Vec::new();

        // Find all <style> elements
        let style_elements = document.get_elements_by_tag_name("style");

        for style_el in style_elements {
            // Get text content
            let mut css_text = String::new();
            for child in style_el.children() {
                if let NodeType::Text(text) = &child.node_type {
                    css_text.push_str(text);
                }
            }

            if !css_text.is_empty() {
                match Stylesheet::parse(&css_text) {
                    Ok(stylesheet) => {
                        debug!(rules = stylesheet.rules.len(), "Parsed stylesheet");
                        stylesheets.push(stylesheet);
                    }
                    Err(e) => {
                        warn!(?e, "Failed to parse stylesheet");
                    }
                }
            }
        }

        stylesheets
    }

    /// Discover external stylesheets from <link> elements.
    fn discover_external_stylesheets(
        &self,
        document: &Document,
        base_url: Option<&Url>,
    ) -> Vec<Url> {
        let mut urls = Vec::new();

        // Find all <link rel="stylesheet"> elements
        let link_elements = document.get_elements_by_tag_name("link");

        for link_el in link_elements {
            if let NodeType::Element { attributes, .. } = &link_el.node_type {
                // Check if this is a stylesheet link
                let rel = attributes.get("rel").map(|s| s.to_lowercase());
                if rel.as_deref() != Some("stylesheet") {
                    continue;
                }

                // Get href
                if let Some(href) = attributes.get("href") {
                    // Resolve relative URL
                    let resolved = if let Some(base) = base_url {
                        base.join(href).ok()
                    } else {
                        Url::parse(href).ok()
                    };

                    if let Some(url) = resolved {
                        debug!(%url, "Discovered external stylesheet");
                        urls.push(url);
                    }
                }
            }
        }

        urls
    }

    /// Discover images from <img> elements.
    fn discover_images(&self, document: &Document, base_url: Option<&Url>) -> Vec<(String, Url)> {
        let mut images = Vec::new();

        // Find all <img> elements
        let img_elements = document.get_elements_by_tag_name("img");

        for img_el in img_elements {
            if let NodeType::Element { attributes, .. } = &img_el.node_type {
                if let Some(src) = attributes.get("src") {
                    // Resolve relative URL
                    let resolved = if let Some(base) = base_url {
                        base.join(src).ok()
                    } else {
                        Url::parse(src).ok()
                    };

                    if let Some(url) = resolved {
                        debug!(%url, "Discovered image");
                        images.push((src.clone(), url));
                    }
                }
            }
        }

        images
    }

    /// Load external stylesheets asynchronously.
    pub async fn load_external_stylesheets(
        &mut self,
        id: EngineViewId,
    ) -> Result<Vec<Stylesheet>, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let Some(document) = &view.document else {
            return Ok(Vec::new());
        };

        let base_url = view.url.as_ref();
        let urls = self.discover_external_stylesheets(document.as_ref(), base_url);

        let mut stylesheets = Vec::new();

        for url in urls {
            info!(%url, "Loading external stylesheet");

            match self.loader.fetch(Request::get(url.clone())).await {
                Ok(response) => {
                    if response.ok() {
                        match response.text().await {
                            Ok(css_text) => match Stylesheet::parse(&css_text) {
                                Ok(stylesheet) => {
                                    debug!(rules = stylesheet.rules.len(), %url, "Parsed external stylesheet");
                                    stylesheets.push(stylesheet);
                                }
                                Err(e) => {
                                    warn!(?e, %url, "Failed to parse external stylesheet");
                                }
                            },
                            Err(e) => {
                                warn!(?e, %url, "Failed to read stylesheet body");
                            }
                        }
                    } else {
                        warn!(status = %response.status, %url, "Failed to fetch stylesheet");
                    }
                }
                Err(e) => {
                    warn!(?e, %url, "Failed to fetch stylesheet");
                }
            }
        }

        Ok(stylesheets)
    }

    /// Load images asynchronously and store in cache.
    pub async fn load_images(&mut self, id: EngineViewId) -> Result<usize, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let Some(document) = &view.document else {
            return Ok(0);
        };

        let base_url = view.url.as_ref();
        let images = self.discover_images(document.as_ref(), base_url);

        let mut loaded = 0;
        let image_manager = self.image_manager.clone();

        for (_src, url) in images {
            // Skip if already cached
            if image_manager.is_cached(&url) {
                debug!(%url, "Image already cached");
                loaded += 1;
                continue;
            }

            info!(%url, "Loading image via ImageManager");

            // Use ImageManager to fetch, decode, and cache the image
            match image_manager.load(url.clone()).await {
                Ok(image) => {
                    debug!(
                        %url,
                        width = image.natural_width,
                        height = image.natural_height,
                        "Image loaded and cached"
                    );
                    loaded += 1;
                }
                Err(e) => {
                    warn!(?e, %url, "Failed to load image");
                }
            }
        }

        Ok(loaded)
    }

    /// Load all subresources (stylesheets, images) for a view.
    pub async fn load_subresources(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        // Load external stylesheets
        let external_stylesheets = self.load_external_stylesheets(id).await?;

        if !external_stylesheets.is_empty() {
            info!(
                count = external_stylesheets.len(),
                "Loaded external stylesheets"
            );
            // Store for use during relayout
            if let Some(view) = self.views.get_mut(&id) {
                view.external_stylesheets = external_stylesheets;
            }
            // Trigger relayout with new styles
            self.relayout(id)?;
        }

        // Load images
        let image_count = self.load_images(id).await?;
        if image_count > 0 {
            info!(count = image_count, "Loaded images");
            // Trigger repaint for images
            self.relayout(id)?;
        }

        Ok(())
    }

    /// Extract CSS variables from :root rules.
    fn extract_css_variables(&self, stylesheets: &[Stylesheet]) -> HashMap<String, String> {
        let mut variables = HashMap::new();

        for stylesheet in stylesheets {
            for rule in &stylesheet.rules {
                // Check for :root selector
                if rule.selector.trim() == ":root" {
                    for decl in &rule.declarations {
                        // CSS custom properties start with --
                        if decl.property.starts_with("--") {
                            // Extract the string value from PropertyValue
                            let value_str = match &decl.value {
                                rustkit_css::PropertyValue::Specified(s) => s.clone(),
                                rustkit_css::PropertyValue::Inherit => "inherit".to_string(),
                                rustkit_css::PropertyValue::Initial => "initial".to_string(),
                            };
                            variables.insert(decl.property.clone(), value_str);
                        }
                    }
                }
            }
        }

        debug!(count = variables.len(), "Extracted CSS variables");
        variables
    }

    /// Resolve CSS variable references in a value.
    fn resolve_css_variables(&self, value: &str, css_vars: &HashMap<String, String>) -> String {
        let mut result = value.to_string();

        // Look for var(--name) or var(--name, fallback)
        while let Some(start) = result.find("var(") {
            let after_var = &result[start + 4..];
            if let Some(end) = after_var.find(')') {
                let var_content = &after_var[..end];

                // Parse variable name and optional fallback
                let (var_name, fallback) = if let Some(comma_pos) = var_content.find(',') {
                    (
                        var_content[..comma_pos].trim(),
                        Some(var_content[comma_pos + 1..].trim()),
                    )
                } else {
                    (var_content.trim(), None)
                };

                // Look up variable value
                let replacement = css_vars
                    .get(var_name)
                    .map(|s| s.as_str())
                    .or(fallback)
                    .unwrap_or("");

                // Replace var(...) with the resolved value
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    replacement,
                    &after_var[end + 1..]
                );
            } else {
                break; // Malformed var(), stop processing
            }
        }

        result
    }

    /// Check if a selector matches an element.
    ///
    /// `ancestors` is a list of (tag_name, classes, id) tuples from parent to root.
    /// `siblings_before` is a list of (tag_name, classes, id) tuples for preceding siblings.
    /// `element_index` is the 0-based index of this element among its siblings.
    /// `sibling_count` is the total number of siblings.
    fn selector_matches(
        &self,
        selector: &str,
        tag_name: &str,
        attributes: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
        siblings_before: &[(String, Vec<String>, Option<String>)],
        element_index: usize,
        sibling_count: usize,
    ) -> bool {
        let selector = selector.trim();

        // Handle multiple selectors (comma-separated)
        if selector.contains(',') {
            return selector.split(',').any(|s| {
                self.selector_matches(
                    s.trim(),
                    tag_name,
                    attributes,
                    ancestors,
                    siblings_before,
                    element_index,
                    sibling_count,
                )
            });
        }

        // A pseudo-ELEMENT selector styles a generated box, never its host:
        // `.card::before { position:absolute }` must not absolutize `.card`.
        // Before this guard, pseudo rules bled onto host elements — harmless
        // while box.position was never honored, catastrophic the day it was
        // (about.html: every card/feature/quote left normal flow at once).
        // Pseudo boxes get these rules through create_pseudo_element's own
        // suffix-matching path; the normal cascade must skip them entirely.
        let sel_lower = selector;
        if sel_lower.contains("::")
            || sel_lower.ends_with(":before")
            || sel_lower.ends_with(":after")
            || sel_lower.contains(":before ")
            || sel_lower.contains(":after ")
        {
            return false;
        }

        // Tokenize selector into parts and combinators
        let tokens = self.tokenize_selector(selector);

        if tokens.is_empty() {
            return false;
        }

        // The last token must match the current element
        let last_token = &tokens[tokens.len() - 1];
        if !last_token.1.is_empty() {
            // There's a combinator before this - we need to handle it
            return false; // Simplified - we'll handle this below
        }

        if !self.simple_selector_matches_with_pseudo(
            &last_token.0,
            tag_name,
            attributes,
            element_index,
            sibling_count,
        ) {
            return false;
        }

        // If there's only one token, we're done
        if tokens.len() == 1 {
            return true;
        }

        // Handle combinators by walking backwards through tokens
        // Track current position in ancestor chain
        let mut ancestor_idx = 0;

        for i in (0..tokens.len() - 1).rev() {
            let (sel_part, combinator) = &tokens[i];

            match combinator.as_str() {
                " " => {
                    // Descendant combinator: some ancestor (from current position) must match
                    let mut found = false;
                    let mut found_idx = ancestor_idx;
                    for (idx, (anc_tag, anc_classes, anc_id)) in
                        ancestors.iter().enumerate().skip(ancestor_idx)
                    {
                        if self.simple_selector_matches_ancestor(
                            sel_part,
                            anc_tag,
                            anc_classes,
                            anc_id.as_ref(),
                        ) {
                            found = true;
                            found_idx = idx + 1; // Next position after this ancestor
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                    ancestor_idx = found_idx;
                }
                ">" => {
                    // Child combinator: immediate parent (at current position) must match
                    if let Some((parent_tag, parent_classes, parent_id)) =
                        ancestors.get(ancestor_idx)
                    {
                        if !self.simple_selector_matches_ancestor(
                            sel_part,
                            parent_tag,
                            parent_classes,
                            parent_id.as_ref(),
                        ) {
                            return false;
                        }
                        ancestor_idx += 1; // Move to next ancestor
                    } else {
                        return false;
                    }
                }
                "+" => {
                    // Adjacent sibling combinator: immediate previous sibling must match
                    // Note: sibling combinators only apply at the element level, not up the tree
                    if let Some((prev_tag, prev_classes, prev_id)) = siblings_before.last() {
                        if !self.simple_selector_matches_ancestor(
                            sel_part,
                            prev_tag,
                            prev_classes,
                            prev_id.as_ref(),
                        ) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                "~" => {
                    // General sibling combinator: any previous sibling must match
                    let mut found = false;
                    for (sib_tag, sib_classes, sib_id) in siblings_before {
                        if self.simple_selector_matches_ancestor(
                            sel_part,
                            sib_tag,
                            sib_classes,
                            sib_id.as_ref(),
                        ) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
                _ => {
                    // Unknown combinator, skip
                }
            }
        }

        true
    }

    /// Tokenize a selector into (simple_selector, combinator) pairs.
    /// The combinator is the one that follows this selector part.
    fn tokenize_selector(&self, selector: &str) -> Vec<(String, String)> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut chars = selector.chars().peekable();
        let mut in_brackets = false;
        let mut in_quotes = false;
        let mut quote_char = ' ';

        while let Some(c) = chars.next() {
            if in_quotes {
                current.push(c);
                if c == quote_char {
                    in_quotes = false;
                }
                continue;
            }

            if c == '"' || c == '\'' {
                in_quotes = true;
                quote_char = c;
                current.push(c);
                continue;
            }

            if c == '[' {
                in_brackets = true;
                current.push(c);
                continue;
            }

            if c == ']' {
                in_brackets = false;
                current.push(c);
                continue;
            }

            if in_brackets {
                current.push(c);
                continue;
            }

            // Check for combinators
            if c == '>' || c == '+' || c == '~' {
                if !current.trim().is_empty() {
                    tokens.push((current.trim().to_string(), c.to_string()));
                    current = String::new();
                }
                continue;
            }

            if c.is_whitespace() {
                // Could be a descendant combinator or just whitespace around other combinators
                if !current.trim().is_empty() {
                    // Peek ahead to see if there's a combinator
                    while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
                        chars.next();
                    }

                    if let Some(&next) = chars.peek() {
                        if next == '>' || next == '+' || next == '~' {
                            // Don't push yet - the actual combinator character will be handled
                            // when we process it. Keep current intact for the combinator handler.
                        } else if next.is_alphanumeric()
                            || next == '.'
                            || next == '#'
                            || next == '['
                            || next == ':'
                            || next == '*'
                        {
                            // Descendant combinator (space between selectors)
                            tokens.push((current.trim().to_string(), " ".to_string()));
                            current = String::new();
                        }
                    }
                }
                continue;
            }

            current.push(c);
        }

        // Add the last token with empty combinator
        if !current.trim().is_empty() {
            tokens.push((current.trim().to_string(), String::new()));
        }

        tokens
    }

    /// Check if a simple selector matches an element with pseudo-class context.
    fn simple_selector_matches_with_pseudo(
        &self,
        selector: &str,
        tag_name: &str,
        attributes: &HashMap<String, String>,
        element_index: usize,
        sibling_count: usize,
    ) -> bool {
        // Universal selector
        if selector == "*" {
            return true;
        }

        // :root pseudo-class matches html element
        if selector == ":root" {
            return tag_name.eq_ignore_ascii_case("html");
        }

        // ID selector: #id
        if let Some(id) = selector.strip_prefix('#') {
            if let Some(el_id) = attributes.get("id") {
                return el_id == id;
            }
            return false;
        }

        // Class selector: .class (can be chained: .a.b)
        if selector.starts_with('.') && !selector.contains(|c| c == '#' || c == '[' || c == ':') {
            let classes: Vec<&str> = selector[1..].split('.').filter(|s| !s.is_empty()).collect();
            if let Some(el_class) = attributes.get("class") {
                let el_classes: Vec<&str> = el_class.split_whitespace().collect();
                return classes.iter().all(|c| el_classes.contains(c));
            }
            return false;
        }

        // Type selector (element name)
        // May have class, ID, attribute, or pseudo-class attached: div.class or div#id or div[attr] or div:first-child
        let mut remaining = selector;

        // Extract tag part
        let tag_end = remaining
            .find(|c| c == '.' || c == '#' || c == ':' || c == '[')
            .unwrap_or(remaining.len());
        let tag_part = &remaining[..tag_end];
        remaining = &remaining[tag_end..];

        // Check tag name (if specified)
        if !tag_part.is_empty() && !tag_part.eq_ignore_ascii_case(tag_name) {
            return false;
        }

        // Check remaining parts (classes, IDs, attributes, pseudo-classes)
        while !remaining.is_empty() {
            if let Some(rest) = remaining.strip_prefix('.') {
                // Class
                let class_end = rest
                    .find(|c| c == '.' || c == '#' || c == ':' || c == '[')
                    .unwrap_or(rest.len());
                let class_name = &rest[..class_end];
                remaining = &rest[class_end..];

                if let Some(el_class) = attributes.get("class") {
                    if !el_class.split_whitespace().any(|c| c == class_name) {
                        return false;
                    }
                } else {
                    return false;
                }
            } else if let Some(rest) = remaining.strip_prefix('#') {
                // ID
                let id_end = rest
                    .find(|c| c == '.' || c == '#' || c == ':' || c == '[')
                    .unwrap_or(rest.len());
                let id_name = &rest[..id_end];
                remaining = &rest[id_end..];

                if attributes.get("id").map(|s| s.as_str()) != Some(id_name) {
                    return false;
                }
            } else if let Some(rest) = remaining.strip_prefix('[') {
                // Attribute selector with operators
                let bracket_end = rest.find(']').unwrap_or(rest.len());
                let attr_selector = &rest[..bracket_end];
                remaining = if bracket_end < rest.len() {
                    &rest[bracket_end + 1..]
                } else {
                    ""
                };

                if !self.match_attribute_selector(attr_selector, attributes) {
                    return false;
                }
            } else if let Some(rest) = remaining.strip_prefix(':') {
                // Pseudo-class
                let (pseudo_name, pseudo_arg, consumed) = self.parse_pseudo_class(rest);
                remaining = &rest[consumed..];

                if !self.match_pseudo_class(
                    &pseudo_name,
                    pseudo_arg.as_deref(),
                    tag_name,
                    element_index,
                    sibling_count,
                    attributes,
                ) {
                    return false;
                }
            } else {
                // Unknown, skip
                break;
            }
        }

        true
    }

    /// Match an attribute selector with operators.
    fn match_attribute_selector(
        &self,
        attr_selector: &str,
        attributes: &HashMap<String, String>,
    ) -> bool {
        // Determine the operator
        let operators = ["~=", "|=", "^=", "$=", "*=", "="];

        for op in &operators {
            if let Some(pos) = attr_selector.find(op) {
                let attr_name = attr_selector[..pos].trim();
                let mut attr_value = attr_selector[pos + op.len()..].trim();

                // Remove quotes if present
                if (attr_value.starts_with('"') && attr_value.ends_with('"'))
                    || (attr_value.starts_with('\'') && attr_value.ends_with('\''))
                {
                    attr_value = &attr_value[1..attr_value.len() - 1];
                }

                if let Some(el_attr) = attributes.get(attr_name) {
                    return match *op {
                        "=" => el_attr == attr_value,
                        "~=" => el_attr.split_whitespace().any(|w| w == attr_value),
                        "|=" => {
                            el_attr == attr_value
                                || el_attr.starts_with(&format!("{}-", attr_value))
                        }
                        "^=" => el_attr.starts_with(attr_value),
                        "$=" => el_attr.ends_with(attr_value),
                        "*=" => el_attr.contains(attr_value),
                        _ => false,
                    };
                } else {
                    return false;
                }
            }
        }

        // Just [attr] - check presence
        let attr_name = attr_selector.trim();
        attributes.contains_key(attr_name)
    }

    /// Parse a pseudo-class, returning (name, optional_arg, chars_consumed).
    fn parse_pseudo_class(&self, rest: &str) -> (String, Option<String>, usize) {
        // Handle :not(...) and :nth-child(...) with parentheses
        let name_end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '-')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_string();

        if rest[name_end..].starts_with('(') {
            // Find matching closing paren
            let paren_start = name_end + 1;
            let mut depth = 1;
            let mut paren_end = paren_start;
            for (i, c) in rest[paren_start..].chars().enumerate() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            paren_end = paren_start + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let arg = rest[paren_start..paren_end].to_string();
            (name, Some(arg), paren_end + 1)
        } else {
            (name, None, name_end)
        }
    }

    /// Match a pseudo-class.
    fn match_pseudo_class(
        &self,
        name: &str,
        arg: Option<&str>,
        tag_name: &str,
        element_index: usize,
        sibling_count: usize,
        attributes: &HashMap<String, String>,
    ) -> bool {
        match name {
            "first-child" => element_index == 0,
            "last-child" => element_index == sibling_count.saturating_sub(1),
            "only-child" => sibling_count == 1,
            "nth-child" => {
                if let Some(arg) = arg {
                    self.match_nth(arg, element_index + 1) // nth-child is 1-indexed
                } else {
                    false
                }
            }
            "nth-last-child" => {
                if let Some(arg) = arg {
                    let from_end = sibling_count - element_index;
                    self.match_nth(arg, from_end)
                } else {
                    false
                }
            }
            "not" => {
                if let Some(arg) = arg {
                    // :not() negates the inner selector
                    // Pass element_index and sibling_count for pseudo-class support inside :not()
                    // This enables :not(:first-child), :not(:nth-child(2)), etc.
                    !self.simple_selector_matches_with_pseudo(
                        arg,
                        tag_name,
                        attributes,
                        element_index,
                        sibling_count,
                    )
                } else {
                    true
                }
            }
            "hover" | "focus" | "active" | "visited" => {
                // Dynamic pseudo-classes - always false in static rendering
                false
            }
            "disabled" => attributes.contains_key("disabled"),
            "enabled" => !attributes.contains_key("disabled"),
            "checked" => attributes.contains_key("checked"),
            "empty" => false, // Would need DOM context
            "root" => false,  // Handled separately
            _ => true,        // Unknown pseudo-classes pass through
        }
    }

    /// Match an nth-child expression like "2n+1", "odd", "even", or a number.
    fn match_nth(&self, expr: &str, n: usize) -> bool {
        let expr = expr.trim().to_lowercase();

        if expr == "odd" {
            return n % 2 == 1;
        }
        if expr == "even" {
            return n % 2 == 0;
        }

        // Try parsing as a simple number
        if let Ok(num) = expr.parse::<usize>() {
            return n == num;
        }

        // Parse An+B formula
        // Examples: 2n, 2n+1, -n+3, n+2
        let mut a = 0i32;
        let mut b = 0i32;

        if let Some(n_pos) = expr.find('n') {
            let a_part = &expr[..n_pos].trim();
            a = if a_part.is_empty() || *a_part == "+" {
                1
            } else if *a_part == "-" {
                -1
            } else {
                a_part.parse().unwrap_or(0)
            };

            let b_part = expr[n_pos + 1..].trim();
            if !b_part.is_empty() {
                b = b_part.replace('+', "").trim().parse().unwrap_or(0);
            }
        } else {
            // Just a number
            b = expr.parse().unwrap_or(0);
        }

        // Check if n matches An+B for some non-negative integer
        let n = n as i32;
        if a == 0 {
            return n == b;
        }

        // n = a*k + b for some k >= 0
        // k = (n - b) / a
        let diff = n - b;
        if a > 0 {
            diff >= 0 && diff % a == 0
        } else {
            diff <= 0 && diff % a == 0
        }
    }

    /// Match a simple selector against an ancestor/sibling with full info.
    fn simple_selector_matches_ancestor(
        &self,
        selector: &str,
        tag_name: &str,
        classes: &[String],
        id: Option<&String>,
    ) -> bool {
        // Universal selector
        if selector == "*" {
            return true;
        }

        // Parse selector parts: tag, classes, id
        let mut required_tag: Option<&str> = None;
        let mut required_classes: Vec<&str> = Vec::new();
        let mut required_id: Option<&str> = None;

        let mut i = 0;
        let chars: Vec<char> = selector.chars().collect();
        let mut current_start = 0;

        while i <= chars.len() {
            let at_end = i == chars.len();
            let is_delimiter = !at_end
                && (chars[i] == '.' || chars[i] == '#' || chars[i] == ':' || chars[i] == '[');

            if at_end || is_delimiter {
                if i > current_start {
                    let part = &selector[current_start..i];
                    if current_start == 0 && !part.starts_with('.') && !part.starts_with('#') {
                        // Tag name at the start
                        required_tag = Some(part);
                    }
                }

                if !at_end {
                    if chars[i] == '.' {
                        // Find class name
                        let start = i + 1;
                        i += 1;
                        while i < chars.len()
                            && chars[i] != '.'
                            && chars[i] != '#'
                            && chars[i] != ':'
                            && chars[i] != '['
                        {
                            i += 1;
                        }
                        if i > start {
                            required_classes.push(&selector[start..i]);
                        }
                        current_start = i;
                        continue;
                    } else if chars[i] == '#' {
                        // Find ID
                        let start = i + 1;
                        i += 1;
                        while i < chars.len()
                            && chars[i] != '.'
                            && chars[i] != '#'
                            && chars[i] != ':'
                            && chars[i] != '['
                        {
                            i += 1;
                        }
                        if i > start {
                            required_id = Some(&selector[start..i]);
                        }
                        current_start = i;
                        continue;
                    } else if chars[i] == ':' || chars[i] == '[' {
                        // Skip pseudo-classes and attribute selectors for ancestor matching
                        break;
                    }
                }
            }
            i += 1;
        }

        // Check tag match
        if let Some(req_tag) = required_tag {
            if !req_tag.eq_ignore_ascii_case(tag_name) {
                return false;
            }
        }

        // Check class match
        for req_class in required_classes {
            if !classes.iter().any(|c| c == req_class) {
                return false;
            }
        }

        // Check ID match
        if let Some(req_id) = required_id {
            match id {
                Some(el_id) if el_id == req_id => {}
                _ => return false,
            }
        }

        true
    }

    /// Calculate selector specificity for ordering.
    /// Returns (a, b, c) where:
    /// - a = number of ID selectors
    /// - b = number of class selectors, attribute selectors, and pseudo-classes
    /// - c = number of type selectors and pseudo-elements
    fn selector_specificity(&self, selector: &str) -> (usize, usize, usize) {
        let mut ids = 0; // (a)
        let mut classes = 0; // (b)
        let mut tags = 0; // (c)

        // Handle comma-separated selectors - take max specificity
        if selector.contains(',') {
            let mut max_spec = (0, 0, 0);
            for part in selector.split(',') {
                let spec = self.selector_specificity(part.trim());
                if spec > max_spec {
                    max_spec = spec;
                }
            }
            return max_spec;
        }

        // Process each part of the selector (space-separated for descendants)
        for part in selector.split_whitespace() {
            // Skip combinators
            if part == ">" || part == "+" || part == "~" {
                continue;
            }

            let chars: Vec<char> = part.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                match chars[i] {
                    '#' => {
                        // ID selector
                        ids += 1;
                        i += 1;
                        // Skip the ID name
                        while i < chars.len()
                            && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                        {
                            i += 1;
                        }
                    }
                    '.' => {
                        // Class selector
                        classes += 1;
                        i += 1;
                        // Skip the class name
                        while i < chars.len()
                            && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                        {
                            i += 1;
                        }
                    }
                    '[' => {
                        // Attribute selector
                        classes += 1;
                        i += 1;
                        // Skip until ]
                        while i < chars.len() && chars[i] != ']' {
                            i += 1;
                        }
                        if i < chars.len() {
                            i += 1; // Skip ]
                        }
                    }
                    ':' => {
                        i += 1;
                        if i < chars.len() && chars[i] == ':' {
                            // Pseudo-element (::before, ::after, etc.)
                            tags += 1;
                            i += 1;
                            // Skip the pseudo-element name
                            while i < chars.len()
                                && (chars[i].is_alphanumeric()
                                    || chars[i] == '-'
                                    || chars[i] == '_')
                            {
                                i += 1;
                            }
                        } else {
                            // Pseudo-class
                            // Check for functional pseudo-classes
                            let start = i;
                            while i < chars.len()
                                && (chars[i].is_alphanumeric()
                                    || chars[i] == '-'
                                    || chars[i] == '_')
                            {
                                i += 1;
                            }
                            let name: String = chars[start..i].iter().collect();

                            if i < chars.len() && chars[i] == '(' {
                                // Functional pseudo-class
                                if name == "not" || name == "is" {
                                    // :not() and :is() - add specificity of argument
                                    i += 1; // Skip (
                                    let mut paren_depth = 1;
                                    let arg_start = i;
                                    while i < chars.len() && paren_depth > 0 {
                                        if chars[i] == '(' {
                                            paren_depth += 1;
                                        } else if chars[i] == ')' {
                                            paren_depth -= 1;
                                        }
                                        i += 1;
                                    }
                                    let arg: String =
                                        chars[arg_start..i.saturating_sub(1)].iter().collect();
                                    let (a, b, c) = self.selector_specificity(&arg);
                                    ids += a;
                                    classes += b;
                                    tags += c;
                                } else if name == "where" {
                                    // :where() has zero specificity
                                    i += 1; // Skip (
                                    let mut paren_depth = 1;
                                    while i < chars.len() && paren_depth > 0 {
                                        if chars[i] == '(' {
                                            paren_depth += 1;
                                        } else if chars[i] == ')' {
                                            paren_depth -= 1;
                                        }
                                        i += 1;
                                    }
                                } else {
                                    // Other functional pseudo-class (e.g., :nth-child(n))
                                    classes += 1;
                                    i += 1; // Skip (
                                    let mut paren_depth = 1;
                                    while i < chars.len() && paren_depth > 0 {
                                        if chars[i] == '(' {
                                            paren_depth += 1;
                                        } else if chars[i] == ')' {
                                            paren_depth -= 1;
                                        }
                                        i += 1;
                                    }
                                }
                            } else {
                                // Simple pseudo-class (:hover, :first-child, etc.)
                                classes += 1;
                            }
                        }
                    }
                    '*' => {
                        // Universal selector - no specificity
                        i += 1;
                    }
                    _ if chars[i].is_alphabetic() || chars[i] == '_' => {
                        // Type selector (element name)
                        tags += 1;
                        i += 1;
                        // Skip the element name
                        while i < chars.len()
                            && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                        {
                            i += 1;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
        }

        (ids, classes, tags)
    }

    /// Render a view (public API for continuous rendering).
    pub fn render_view(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        self.render(id)
    }

    /// Render all views.
    pub fn render_all_views(&mut self) {
        let view_ids: Vec<_> = self.views.keys().copied().collect();
        for id in view_ids {
            if let Err(e) = self.render(id) {
                trace!(?id, error = %e, "Failed to render view");
            }
        }
    }

    /// Capture a frame from a view to a PPM file.
    ///
    /// This renders the current display list to an offscreen texture and saves it.
    /// This is useful for deterministic testing and visual debugging.
    /// The output is a PPM file (simple portable format).
    pub fn capture_frame(&mut self, id: EngineViewId, path: &str) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let viewhost_id = view.viewhost_id;
        let display_list = view.display_list.clone();

        info!(?id, path, "Capturing frame");

        // Get surface size
        let (width, height) = self
            .compositor
            .get_surface_size(viewhost_id)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        if width == 0 || height == 0 {
            return Err(EngineError::RenderError(
                "Cannot capture zero-size frame".into(),
            ));
        }

        // If we have a display list and renderer, render to offscreen texture
        match (&display_list, &mut self.renderer) {
            (Some(display_list), Some(renderer)) => {
                // Update viewport size for correct coordinate transforms
                renderer.set_viewport_size(width, height);

                // Capture with actual display list rendering
                self.compositor
                    .capture_frame_with_renderer(
                        viewhost_id,
                        path,
                        renderer,
                        &display_list.commands,
                    )
                    .map_err(|e| EngineError::RenderError(e.to_string()))
            }
            _ => {
                // Fallback to magenta test pattern if no display list
                self.compositor
                    .capture_frame_to_file(viewhost_id, path)
                    .map_err(|e| EngineError::RenderError(e.to_string()))
            }
        }
    }

    /// Export the layout tree for a view as JSON.
    ///
    /// This exports the current layout tree with dimensions for each box,
    /// which can be compared against Chromium's DOMRect data for layout parity testing.
    pub fn export_layout_json(&self, id: EngineViewId, path: &str) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let layout = view
            .layout
            .as_ref()
            .ok_or_else(|| EngineError::RenderError("No layout tree available".into()))?;

        // Convert layout tree to JSON-serializable structure
        fn layout_box_to_json(layout_box: &LayoutBox) -> serde_json::Value {
            let dims = &layout_box.dimensions;
            let content = &dims.content;
            let margin_box = dims.margin_box();
            let padding_box = dims.padding_box();
            let border_box = dims.border_box();

            let box_type = match &layout_box.box_type {
                BoxType::Block => "block",
                BoxType::Inline => "inline",
                BoxType::AnonymousBlock => "anonymous_block",
                BoxType::Text(t) => {
                    return serde_json::json!({
                        "type": "text",
                        "text": t.chars().take(50).collect::<String>(),
                        "rect": {
                            "x": content.x,
                            "y": content.y,
                            "width": content.width,
                            "height": content.height
                        }
                    })
                }
                BoxType::Image {
                    natural_width,
                    natural_height,
                    ..
                } => {
                    return serde_json::json!({
                        "type": "image",
                        "natural_width": natural_width,
                        "natural_height": natural_height,
                        "rect": {
                            "x": content.x,
                            "y": content.y,
                            "width": content.width,
                            "height": content.height
                        }
                    })
                }
                BoxType::FormControl(ctrl) => {
                    return serde_json::json!({
                        "type": "form_control",
                        "control_type": format!("{:?}", ctrl),
                        "rect": {
                            "x": content.x,
                            "y": content.y,
                            "width": content.width,
                            "height": content.height
                        }
                    })
                }
            };

            let children: Vec<serde_json::Value> =
                layout_box.children.iter().map(layout_box_to_json).collect();

            serde_json::json!({
                "type": box_type,
                "content_rect": {
                    "x": content.x,
                    "y": content.y,
                    "width": content.width,
                    "height": content.height
                },
                "padding_box": {
                    "x": padding_box.x,
                    "y": padding_box.y,
                    "width": padding_box.width,
                    "height": padding_box.height
                },
                "border_box": {
                    "x": border_box.x,
                    "y": border_box.y,
                    "width": border_box.width,
                    "height": border_box.height
                },
                "margin_box": {
                    "x": margin_box.x,
                    "y": margin_box.y,
                    "width": margin_box.width,
                    "height": margin_box.height
                },
                "margin": {
                    "top": dims.margin.top,
                    "right": dims.margin.right,
                    "bottom": dims.margin.bottom,
                    "left": dims.margin.left
                },
                "padding": {
                    "top": dims.padding.top,
                    "right": dims.padding.right,
                    "bottom": dims.padding.bottom,
                    "left": dims.padding.left
                },
                "border": {
                    "top": dims.border.top,
                    "right": dims.border.right,
                    "bottom": dims.border.bottom,
                    "left": dims.border.left
                },
                "children": children
            })
        }

        let layout_json = layout_box_to_json(layout);

        // Get viewport size from compositor
        let (width, height) = self
            .compositor
            .get_surface_size(view.viewhost_id)
            .unwrap_or((0, 0));

        let wrapper = serde_json::json!({
            "version": 1,
            "viewport": {
                "width": width,
                "height": height
            },
            "root": layout_json
        });

        let json_str = serde_json::to_string_pretty(&wrapper)
            .map_err(|e| EngineError::RenderError(format!("JSON serialization failed: {}", e)))?;

        std::fs::write(path, json_str)
            .map_err(|e| EngineError::RenderError(format!("Failed to write layout file: {}", e)))?;

        info!(?id, path, "Layout tree exported");
        Ok(())
    }

    /// Render a view (internal).
    #[tracing::instrument(skip(self), fields(view_id = ?id))]
    fn render(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        let _span = tracing::info_span!("render", ?id).entered();

        // Extract needed values from view, avoiding long-lived borrows
        let (viewhost_id, has_display_list, cmd_count, is_headless) = {
            let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
            (
                view.viewhost_id,
                view.display_list.is_some(),
                view.display_list
                    .as_ref()
                    .map(|dl| dl.commands.len())
                    .unwrap_or(0),
                view.headless_bounds.is_some(),
            )
        };

        trace!(
            ?id,
            has_display_list,
            cmd_count,
            is_headless,
            "Rendering view"
        );

        // Get surface size and update renderer viewport before rendering
        let (surface_width, surface_height) = {
            let _surface_span = tracing::debug_span!("get_surface_size").entered();
            self.compositor
                .get_surface_size(viewhost_id)
                .map_err(|e| EngineError::RenderError(e.to_string()))?
        };

        if let Some(renderer) = &mut self.renderer {
            renderer.set_viewport_size(surface_width, surface_height);
        }

        // Upload images from cache to renderer before drawing
        // Need to re-borrow view here to get display_list
        if let Some(view) = self.views.get(&id) {
            if let Some(display_list) = &view.display_list {
                // Clone commands to break the borrow on self.views
                let commands = display_list.commands.clone();
                // Borrow is dropped when scope ends
                self.upload_display_list_images(&commands);
            }
        }

        // Re-get display_list reference for rendering
        let display_list = self.views.get(&id).and_then(|v| v.display_list.as_ref());

        // Render based on whether view is headless or not
        if is_headless {
            // Headless rendering path - no surface, no present
            let texture_view = {
                let _texture_span = tracing::debug_span!("get_headless_texture_view").entered();
                self.compositor
                    .get_headless_texture_view(viewhost_id)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?
            };

            let _execute_span = tracing::info_span!("renderer_execute", cmd_count).entered();
            if let (Some(renderer), Some(display_list)) = (&mut self.renderer, display_list) {
                renderer
                    .execute(&display_list.commands, &texture_view)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?;
            } else if let Some(renderer) = &mut self.renderer {
                // No display list, render empty (will clear to white or debug color)
                renderer
                    .execute(&[], &texture_view)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?;
            } else {
                // Fallback to compositor solid color
                self.compositor
                    .render_solid_color(viewhost_id, self.config.background_color)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?;
            }

            // No present() needed for headless - texture is already updated
        } else {
            // Regular surface rendering path
            let (output, texture_view) = {
                let _texture_span = tracing::debug_span!("get_surface_texture").entered();
                self.compositor
                    .get_surface_texture(viewhost_id)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?
            };

            // Render using display list if available, otherwise just clear to background
            {
                let _execute_span = tracing::info_span!("renderer_execute", cmd_count).entered();
                if let (Some(renderer), Some(display_list)) = (&mut self.renderer, display_list) {
                    renderer
                        .execute(&display_list.commands, &texture_view)
                        .map_err(|e| EngineError::RenderError(e.to_string()))?;
                } else if let Some(renderer) = &mut self.renderer {
                    // No display list, render empty (will clear to white or debug color)
                    renderer
                        .execute(&[], &texture_view)
                        .map_err(|e| EngineError::RenderError(e.to_string()))?;
                } else {
                    // Fallback to compositor solid color (shouldn't normally happen)
                    drop(output); // Release the texture
                    self.compositor
                        .render_solid_color(viewhost_id, self.config.background_color)
                        .map_err(|e| EngineError::RenderError(e.to_string()))?;
                    return Ok(());
                }
            }

            // Present surface texture
            self.compositor.present(output);
        }

        Ok(())
    }

    /// Upload images referenced in display commands to the renderer's texture cache.
    ///
    /// This scans the display list for BackgroundImage and Image commands and ensures
    /// any cached images are uploaded to the GPU before rendering.
    /// For data: URLs, images are loaded synchronously on-demand.
    fn upload_display_list_images(&mut self, commands: &[rustkit_layout::DisplayCommand]) {
        use std::collections::HashSet;
        use std::time::Duration;

        // Early exit if no renderer
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        // Collect unique image URLs from display list
        let mut urls_to_upload: Vec<(String, std::sync::Arc<rustkit_image::LoadedImage>)> =
            Vec::new();
        let mut urls_seen = HashSet::new();

        for cmd in commands {
            // Extract URL from both BackgroundImage and Image commands
            let url = match cmd {
                rustkit_layout::DisplayCommand::BackgroundImage { url, .. } => url,
                rustkit_layout::DisplayCommand::Image { url, .. } => url,
                _ => continue,
            };

            if !urls_seen.insert(url.clone()) {
                continue; // Already processed
            }

            // Skip if already in renderer
            if renderer.has_image(url) {
                continue;
            }

            // Try to parse as URL
            let Ok(parsed_url) = url::Url::parse(url) else {
                tracing::warn!(%url, "Invalid URL for image");
                continue;
            };

            // Try to get from cache or load data: URLs synchronously
            let image = if let Some(cached) = self.image_manager.get_cached(&parsed_url) {
                Some(cached)
            } else if parsed_url.scheme() == "data" {
                // For data: URLs, load synchronously since they don't require network
                match self.image_manager.load_blocking(parsed_url) {
                    Ok(img) => Some(img),
                    Err(e) => {
                        tracing::warn!(?e, %url, "Failed to decode data URL image");
                        None
                    }
                }
            } else {
                // Image not cached and not a data: URL - it will render when loaded
                None
            };

            if let Some(img) = image {
                urls_to_upload.push((url.clone(), img));
            }
        }

        // Now upload all collected images
        for (url_str, image) in urls_to_upload {
            let frame = image.current_frame(Duration::ZERO);
            if let Err(e) =
                renderer.upload_image(&url_str, frame.width(), frame.height(), frame.data())
            {
                tracing::warn!(?e, %url_str, "Failed to upload image to renderer");
            } else {
                tracing::debug!(%url_str, "Uploaded image to renderer");
            }
        }
    }

    /// Execute JavaScript in a view.
    pub fn execute_script(
        &mut self,
        id: EngineViewId,
        script: &str,
    ) -> Result<String, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let bindings = view
            .bindings
            .as_ref()
            .ok_or(EngineError::JsError("JavaScript not initialized".into()))?;

        let result = bindings
            .evaluate(script)
            .map_err(|e| EngineError::JsError(e.to_string()))?;

        Ok(format!("{:?}", result))
    }

    /// Get the current URL of a view.
    pub fn get_url(&self, id: EngineViewId) -> Option<Url> {
        self.views.get(&id).and_then(|v| v.url.clone())
    }

    /// Get the title of a view.
    pub fn get_title(&self, id: EngineViewId) -> Option<String> {
        self.views.get(&id).and_then(|v| v.title.clone())
    }

    /// Check if a view can go back.
    pub fn can_go_back(&self, id: EngineViewId) -> bool {
        self.views
            .get(&id)
            .map(|v| v.navigation.can_go_back())
            .unwrap_or(false)
    }

    /// Check if a view can go forward.
    pub fn can_go_forward(&self, id: EngineViewId) -> bool {
        self.views
            .get(&id)
            .map(|v| v.navigation.can_go_forward())
            .unwrap_or(false)
    }

    /// Get the number of views.
    pub fn view_count(&self) -> usize {
        self.views.len()
    }

    /// Get the download manager.
    pub fn download_manager(&self) -> Arc<rustkit_net::DownloadManager> {
        self.loader.download_manager()
    }

    /// Get GPU info.
    pub fn gpu_info(&self) -> String {
        format!("{:?}", self.compositor.adapter_info())
    }

    /// Handle a view event from the viewhost.
    #[cfg(windows)]
    pub fn handle_view_event(&mut self, event: rustkit_viewhost::ViewEvent) {
        use rustkit_viewhost::ViewEvent;

        match event {
            ViewEvent::Resized {
                view_id: viewhost_id,
                bounds,
                dpi: _,
            } => {
                // Find engine view id for this viewhost id
                if let Some((id, _)) = self
                    .views
                    .iter()
                    .find(|(_, v)| v.viewhost_id == viewhost_id)
                {
                    let id = *id;
                    let _ = self.resize_view(
                        id,
                        rustkit_viewhost::Bounds::new(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                        ),
                    );
                }
            }
            ViewEvent::Focused {
                view_id: viewhost_id,
            } => {
                if let Some((id, view)) = self
                    .views
                    .iter_mut()
                    .find(|(_, v)| v.viewhost_id == viewhost_id)
                {
                    view.view_focused = true;
                    let _ = self
                        .event_tx
                        .send(EngineEvent::ViewFocused { view_id: *id });
                }
            }
            ViewEvent::Blurred {
                view_id: viewhost_id,
            } => {
                if let Some(view) = self
                    .views
                    .values_mut()
                    .find(|v| v.viewhost_id == viewhost_id)
                {
                    view.view_focused = false;
                }
            }
            ViewEvent::Input {
                view_id: viewhost_id,
                event: input_event,
            } => {
                self.handle_input_event(viewhost_id, input_event);
            }
            _ => {}
        }
    }

    /// Handle an input event.
    #[cfg(windows)]
    fn handle_input_event(&mut self, viewhost_id: ViewId, event: rustkit_core::InputEvent) {
        use rustkit_core::InputEvent;

        // Find the view
        let engine_id = self
            .views
            .iter()
            .find(|(_, v)| v.viewhost_id == viewhost_id)
            .map(|(id, _)| *id);

        let Some(engine_id) = engine_id else {
            return;
        };

        match event {
            InputEvent::Mouse(mouse_event) => {
                self.handle_mouse_event(engine_id, mouse_event);
            }
            InputEvent::Key(key_event) => {
                self.handle_key_event(engine_id, key_event);
            }
            InputEvent::Focus(focus_event) => {
                // Focus events are handled via ViewEvent::Focused/Blurred
                let _ = focus_event;
            }
        }
    }

    /// Handle a mouse event.
    #[cfg(windows)]
    fn handle_mouse_event(&mut self, view_id: EngineViewId, event: rustkit_core::MouseEvent) {
        use rustkit_core::MouseEventType;
        use rustkit_dom::MouseEventData;

        let view = match self.views.get_mut(&view_id) {
            Some(v) => v,
            None => return,
        };

        // Perform hit testing if we have layout
        let hit_result = view
            .layout
            .as_ref()
            .and_then(|layout| layout.hit_test(event.position.x as f32, event.position.y as f32));

        // Convert to DOM event
        let dom_event_type = match event.event_type {
            MouseEventType::MouseDown => "mousedown",
            MouseEventType::MouseUp => "mouseup",
            MouseEventType::MouseMove => "mousemove",
            MouseEventType::MouseEnter => "mouseenter",
            MouseEventType::MouseLeave => "mouseleave",
            MouseEventType::Wheel => "wheel",
            MouseEventType::ContextMenu => "contextmenu",
        };

        let _mouse_data = MouseEventData {
            client_x: event.position.x,
            client_y: event.position.y,
            screen_x: event.screen_position.x,
            screen_y: event.screen_position.y,
            offset_x: hit_result.as_ref().map(|r| r.local_x as f64).unwrap_or(0.0),
            offset_y: hit_result.as_ref().map(|r| r.local_y as f64).unwrap_or(0.0),
            button: event.button.button_index(),
            buttons: event.buttons,
            ctrl_key: event.modifiers.ctrl,
            alt_key: event.modifiers.alt,
            shift_key: event.modifiers.shift,
            meta_key: event.modifiers.meta,
            related_target: None,
        };

        // If we have a hit and a document, dispatch the event
        if let (Some(hit), Some(document)) = (&hit_result, &view.document) {
            // NOTE: Full mouse event dispatch requires node_id tracking in layout tree.
            // The layout tree (HitTestResult) currently doesn't track which DOM node
            // each layout box corresponds to (LayoutBox.element_id is always None).
            // To fully implement mouse event dispatch:
            // 1. Add node_id: NodeId field to LayoutBox
            // 2. Set it during layout tree construction
            // 3. Include it in HitTestResult
            // 4. Use it here to dispatch MouseEvent to the correct DOM node
            //
            // For now, we log the hit but cannot dispatch to the specific element.
            trace!(
                ?view_id,
                event_type = dom_event_type,
                depth = hit.depth,
                "Mouse event (node dispatch pending layout node_id tracking)"
            );
        }

        // Handle click focus change
        if event.event_type == MouseEventType::MouseDown && hit_result.is_some() {
            // TODO: Focus the clicked element if focusable
            // This also requires node_id tracking in HitTestResult to know which
            // element was clicked. Once that's available, check if the element is
            // focusable (form controls, elements with tabindex) and call focus_element().
            trace!(
                ?view_id,
                "Click focus change pending layout node_id tracking"
            );
        }
    }

    /// Handle a keyboard event.
    #[cfg(windows)]
    fn handle_key_event(&mut self, view_id: EngineViewId, event: rustkit_core::KeyEvent) {
        use rustkit_core::{KeyCode, KeyEventType};

        let view = match self.views.get_mut(&view_id) {
            Some(v) => v,
            None => return,
        };

        // Only process keyboard events if the view has focus
        if !view.view_focused {
            return;
        }

        trace!(?view_id, key = ?event.key_code, event_type = ?event.event_type, "Key event");

        // Handle Tab key for focus navigation
        if event.event_type == KeyEventType::KeyDown && event.key_code == KeyCode::Tab {
            // TODO: Implement Tab navigation between focusable elements
            // This requires traversing the DOM to find all focusable elements
            // (elements with tabindex, form controls, links, buttons)
            // and moving focus to the next/previous one based on Shift key
            trace!(?view_id, "Tab navigation not yet implemented");
        }

        // Dispatch to focused element via DOM events
        if let (Some(focused_id), Some(document)) = (view.focused_node, &view.document) {
            use rustkit_dom::events::{DomEvent, Event, EventDispatcher, KeyboardEventData};

            if let Some(focused_node) = document.get_node(focused_id) {
                let event_type_str = match event.event_type {
                    KeyEventType::KeyDown => "keydown",
                    KeyEventType::KeyUp => "keyup",
                    KeyEventType::Char => "keypress",
                };

                let key_str = match event.key_code {
                    KeyCode::Enter => "Enter".to_string(),
                    KeyCode::Tab => "Tab".to_string(),
                    KeyCode::Backspace => "Backspace".to_string(),
                    KeyCode::Escape => "Escape".to_string(),
                    KeyCode::Space => " ".to_string(),
                    KeyCode::Left => "ArrowLeft".to_string(),
                    KeyCode::Right => "ArrowRight".to_string(),
                    KeyCode::Up => "ArrowUp".to_string(),
                    KeyCode::Down => "ArrowDown".to_string(),
                    KeyCode::Home => "Home".to_string(),
                    KeyCode::End => "End".to_string(),
                    KeyCode::PageUp => "PageUp".to_string(),
                    KeyCode::PageDown => "PageDown".to_string(),
                    KeyCode::Delete => "Delete".to_string(),
                    KeyCode::Insert => "Insert".to_string(),
                    KeyCode::Char(c) => c.to_string(),
                    _ => format!("{:?}", event.key_code),
                };

                let keyboard_event = Event::new_trusted(event_type_str, true, true);
                let keyboard_data = KeyboardEventData {
                    key: key_str.clone(),
                    code: format!("{:?}", event.key_code),
                    repeat: false, // TODO: track repeat state
                    ctrl_key: event.modifiers.ctrl,
                    alt_key: event.modifiers.alt,
                    shift_key: event.modifiers.shift,
                    meta_key: event.modifiers.meta,
                    location: 0, // TODO: detect key location
                };

                let mut dom_event = DomEvent::Keyboard(keyboard_event, keyboard_data);

                // Build ancestor chain
                let mut ancestors = Vec::new();
                let mut current = focused_node.parent();
                while let Some(node) = current {
                    ancestors.push(node.clone());
                    current = node.parent();
                }
                ancestors.reverse(); // Root to parent order

                let prevented =
                    !EventDispatcher::dispatch(&mut dom_event, &focused_node, &ancestors);

                if prevented {
                    trace!(?view_id, key = ?key_str, "Keyboard event default prevented");
                }
            }
        }
    }

    /// Focus a DOM node in a view.
    pub fn focus_element(
        &mut self,
        view_id: EngineViewId,
        node_id: rustkit_dom::NodeId,
    ) -> Result<(), EngineError> {
        use rustkit_dom::events::{DomEvent, Event, EventDispatcher, FocusEventData};

        let view = self
            .views
            .get_mut(&view_id)
            .ok_or(EngineError::ViewNotFound(view_id))?;

        let old_focused = view.focused_node;
        view.focused_node = Some(node_id);

        // Dispatch blur event to old focused element
        if let (Some(old_id), Some(document)) = (old_focused, &view.document) {
            if let Some(old_node) = document.get_node(old_id) {
                let blur_event = Event::new_trusted("blur", false, false);
                let focus_data = FocusEventData {
                    related_target: Some(node_id),
                };
                let mut dom_event = DomEvent::Focus(blur_event, focus_data);

                // Build ancestor chain
                let mut ancestors = Vec::new();
                let mut current = old_node.parent();
                while let Some(node) = current {
                    ancestors.push(node.clone());
                    current = node.parent();
                }
                ancestors.reverse(); // Root to parent order

                EventDispatcher::dispatch(&mut dom_event, &old_node, &ancestors);
            }
        }

        // Dispatch focus event to new focused element
        if let Some(document) = &view.document {
            if let Some(new_node) = document.get_node(node_id) {
                let focus_event = Event::new_trusted("focus", false, false);
                let focus_data = FocusEventData {
                    related_target: old_focused,
                };
                let mut dom_event = DomEvent::Focus(focus_event, focus_data);

                // Build ancestor chain
                let mut ancestors = Vec::new();
                let mut current = new_node.parent();
                while let Some(node) = current {
                    ancestors.push(node.clone());
                    current = node.parent();
                }
                ancestors.reverse(); // Root to parent order

                EventDispatcher::dispatch(&mut dom_event, &new_node, &ancestors);
            }
        }

        debug!(?view_id, ?node_id, ?old_focused, "Focus changed");
        Ok(())
    }

    /// Blur the currently focused element.
    pub fn blur_element(&mut self, view_id: EngineViewId) -> Result<(), EngineError> {
        use rustkit_dom::events::{DomEvent, Event, EventDispatcher, FocusEventData};

        let view = self
            .views
            .get_mut(&view_id)
            .ok_or(EngineError::ViewNotFound(view_id))?;

        let old_focused = view.focused_node.take();

        // Dispatch blur event to old focused element
        if let (Some(old_id), Some(document)) = (old_focused, &view.document) {
            if let Some(old_node) = document.get_node(old_id) {
                let blur_event = Event::new_trusted("blur", false, false);
                let focus_data = FocusEventData {
                    related_target: None,
                };
                let mut dom_event = DomEvent::Focus(blur_event, focus_data);

                // Build ancestor chain
                let mut ancestors = Vec::new();
                let mut current = old_node.parent();
                while let Some(node) = current {
                    ancestors.push(node.clone());
                    current = node.parent();
                }
                ancestors.reverse(); // Root to parent order

                EventDispatcher::dispatch(&mut dom_event, &old_node, &ancestors);
            }
        }

        debug!(?view_id, ?old_focused, "Element blurred");
        Ok(())
    }

    /// Get the currently focused node in a view.
    pub fn get_focused_element(&self, view_id: EngineViewId) -> Option<rustkit_dom::NodeId> {
        self.views.get(&view_id).and_then(|v| v.focused_node)
    }

    /// Load an image from a URL.
    pub async fn load_image(&self, view_id: EngineViewId, url: Url) -> Result<(), EngineError> {
        let image_manager = self.image_manager.clone();
        let event_tx = self.event_tx.clone();

        match image_manager.load(url.clone()).await {
            Ok(image) => {
                let _ = event_tx.send(EngineEvent::ImageLoaded {
                    view_id,
                    url,
                    width: image.natural_width,
                    height: image.natural_height,
                });
                Ok(())
            }
            Err(e) => {
                let error = e.to_string();
                let _ = event_tx.send(EngineEvent::ImageError {
                    view_id,
                    url: url.clone(),
                    error: error.clone(),
                });
                Err(EngineError::RenderError(format!(
                    "Image load failed: {}",
                    error
                )))
            }
        }
    }

    /// Preload an image (non-blocking).
    pub fn preload_image(&self, url: Url) {
        self.image_manager.preload(url);
    }

    /// Check if an image is cached.
    pub fn is_image_cached(&self, url: &Url) -> bool {
        self.image_manager.is_cached(url)
    }

    /// Get a cached image's dimensions.
    pub fn get_image_dimensions(&self, url: &Url) -> Option<(u32, u32)> {
        self.image_manager
            .get_cached(url)
            .map(|img| (img.natural_width, img.natural_height))
    }

    /// Get the image manager for direct access.
    pub fn image_manager(&self) -> Arc<ImageManager> {
        self.image_manager.clone()
    }

    /// Clear the image cache.
    pub fn clear_image_cache(&self) {
        self.image_manager.clear_cache();
    }

    /// Drain IPC messages from all views.
    ///
    /// Returns a Vec of (EngineViewId, IpcMessage) tuples for messages received
    /// via `window.ipc.postMessage()` from JavaScript in any view.
    ///
    /// This should be called periodically (e.g., during the message loop) to
    /// process IPC messages from the Chrome UI, Shelf, and Content views.
    pub fn drain_ipc_messages(&self) -> Vec<(EngineViewId, IpcMessage)> {
        let mut messages = Vec::new();

        for (&view_id, view_state) in &self.views {
            if let Some(ref bindings) = view_state.bindings {
                for ipc_msg in bindings.drain_ipc_queue() {
                    messages.push((view_id, ipc_msg));
                }
            }
        }

        messages
    }

    /// Check if any view has pending IPC messages.
    pub fn has_pending_ipc(&self) -> bool {
        self.views.values().any(|v| {
            v.bindings
                .as_ref()
                .map(|b| b.has_pending_ipc())
                .unwrap_or(false)
        })
    }
}

/// Builder for Engine.
pub struct EngineBuilder {
    config: EngineConfig,
    interceptor: Option<rustkit_net::RequestInterceptor>,
}

impl EngineBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: EngineConfig::default(),
            interceptor: None,
        }
    }

    /// Set a request interceptor for filtering network requests.
    pub fn request_interceptor(mut self, interceptor: rustkit_net::RequestInterceptor) -> Self {
        self.interceptor = Some(interceptor);
        self
    }

    /// Set the user agent.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.config.user_agent = user_agent.into();
        self
    }

    /// Enable or disable JavaScript.
    pub fn javascript_enabled(mut self, enabled: bool) -> Self {
        self.config.javascript_enabled = enabled;
        self
    }

    /// Enable or disable cookies.
    pub fn cookies_enabled(mut self, enabled: bool) -> Self {
        self.config.cookies_enabled = enabled;
        self
    }

    /// Set the default background color.
    pub fn background_color(mut self, color: [f64; 4]) -> Self {
        self.config.background_color = color;
        self
    }

    /// Set the entire configuration at once.
    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Disable animations for deterministic parity testing.
    pub fn disable_animations(mut self, disable: bool) -> Self {
        self.config.disable_animations = disable;
        self
    }

    /// Build the engine.
    pub fn build(self) -> Result<Engine, EngineError> {
        Engine::with_interceptor(self.config, self.interceptor)
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a color value from CSS.
fn parse_color(value: &str) -> Option<rustkit_css::Color> {
    // Delegate to rustkit-css: single source of truth for color parsing.
    // This engine-local copy knew 11 named colors and silently dropped every
    // declaration using any other name (`background-color: coral` painted
    // nothing). Lowercase here because rustkit_css::parse_color matches
    // functional prefixes case-sensitively.
    rustkit_css::parse_color(&value.trim().to_lowercase())
}

/// Parse a CSS gradient value (linear-gradient or radial-gradient).
fn parse_gradient(value: &str) -> Option<rustkit_css::Gradient> {
    let value = value.trim();

    // Linear gradients
    if value.starts_with("linear-gradient(") && value.ends_with(')') {
        return parse_linear_gradient(value, false);
    }
    if value.starts_with("repeating-linear-gradient(") && value.ends_with(')') {
        return parse_linear_gradient(value, true);
    }

    // Radial gradients
    if value.starts_with("radial-gradient(") && value.ends_with(')') {
        return parse_radial_gradient(value, false);
    }
    if value.starts_with("repeating-radial-gradient(") && value.ends_with(')') {
        return parse_radial_gradient(value, true);
    }

    // Conic gradients
    if value.starts_with("conic-gradient(") && value.ends_with(')') {
        return parse_conic_gradient(value, false);
    }
    if value.starts_with("repeating-conic-gradient(") && value.ends_with(')') {
        return parse_conic_gradient(value, true);
    }

    None
}

/// Parse a linear-gradient CSS function.
fn parse_linear_gradient(value: &str, repeating: bool) -> Option<rustkit_css::Gradient> {
    // Strip prefix and suffix
    let prefix = if repeating {
        "repeating-linear-gradient("
    } else {
        "linear-gradient("
    };
    let inner = value.strip_prefix(prefix)?.strip_suffix(')')?.trim();

    // Split by commas, being careful about nested parentheses
    let parts = split_by_comma(inner);
    if parts.is_empty() {
        return None;
    }

    let mut direction = rustkit_css::GradientDirection::ToBottom; // default
    let mut stops_start = 0;

    // Check if first part is a direction
    let first = parts[0].trim();
    if first.starts_with("to ") {
        direction = parse_gradient_direction(first)?;
        stops_start = 1;
    } else if first.ends_with("deg") {
        // SAFETY: strip_suffix will succeed because we just checked ends_with("deg")
        if let Ok(deg) = first.strip_suffix("deg").unwrap().trim().parse::<f32>() {
            direction = rustkit_css::GradientDirection::Angle(deg);
            stops_start = 1;
        }
    }

    // Parse color stops
    let mut stops = Vec::new();
    for part in &parts[stops_start..] {
        if let Some(stop) = parse_color_stop(part) {
            stops.push(stop);
        }
    }

    if stops.is_empty() {
        return None;
    }

    let gradient = if repeating {
        rustkit_css::LinearGradient::new_repeating(direction, stops)
    } else {
        rustkit_css::LinearGradient::new(direction, stops)
    };
    Some(rustkit_css::Gradient::Linear(gradient))
}

/// Parse a radial-gradient CSS function.
fn parse_radial_gradient(value: &str, repeating: bool) -> Option<rustkit_css::Gradient> {
    // Strip prefix and suffix
    let prefix = if repeating {
        "repeating-radial-gradient("
    } else {
        "radial-gradient("
    };
    let inner = value.strip_prefix(prefix)?.strip_suffix(')')?.trim();

    let parts = split_by_comma(inner);
    if parts.is_empty() {
        return None;
    }

    let mut shape = rustkit_css::RadialShape::Ellipse;
    let size = rustkit_css::RadialSize::FarthestCorner;
    let mut center = (0.5, 0.5);
    let mut stops_start = 0;

    // Check for shape/size/position in first part
    let first = parts[0].trim().to_lowercase();
    if first.contains("circle") || first.contains("ellipse") || first.contains("at ") {
        if first.contains("circle") {
            shape = rustkit_css::RadialShape::Circle;
        }
        // Parse "at" position
        if let Some(at_idx) = first.find(" at ") {
            let pos_str = &first[at_idx + 4..];
            let pos_parts: Vec<&str> = pos_str.split_whitespace().collect();
            if pos_parts.len() >= 2 {
                // CSS <position> keywords are order-INDEPENDENT: `top right`
                // == `right top` (left/right -> horizontal, top/bottom ->
                // vertical). Only length/percentage values are positional
                // (first = horizontal, second = vertical). Routing purely by
                // token order sent `at top right` to (0.0,1.0)=bottom-left.
                let (cx, cy) = resolve_position_pair(pos_parts[0], pos_parts[1]);
                center.0 = cx;
                center.1 = cy;
            } else if pos_parts.len() == 1 {
                // Single keyword: interpret as axis-specific position
                // "top"/"bottom" are vertical - horizontal stays centered
                // "left"/"right" are horizontal - vertical stays centered
                let keyword = pos_parts[0].trim().to_lowercase();
                match keyword.as_str() {
                    "top" => {
                        center.0 = 0.5;
                        center.1 = 0.0;
                    }
                    "bottom" => {
                        center.0 = 0.5;
                        center.1 = 1.0;
                    }
                    "left" => {
                        center.0 = 0.0;
                        center.1 = 0.5;
                    }
                    "right" => {
                        center.0 = 1.0;
                        center.1 = 0.5;
                    }
                    "center" => {
                        center.0 = 0.5;
                        center.1 = 0.5;
                    }
                    _ => {
                        // Percentage or other value - apply to both
                        let val = parse_position_value(pos_parts[0]);
                        center.0 = val;
                        center.1 = val;
                    }
                }
            }
        }
        stops_start = 1;
    }

    // Parse color stops
    let mut stops = Vec::new();
    for part in &parts[stops_start..] {
        if let Some(stop) = parse_color_stop(part) {
            stops.push(stop);
        }
    }

    if stops.is_empty() {
        return None;
    }

    let gradient = if repeating {
        rustkit_css::RadialGradient::new_repeating(shape, size, center, stops)
    } else {
        rustkit_css::RadialGradient::new(shape, size, center, stops)
    };
    Some(rustkit_css::Gradient::Radial(gradient))
}

/// Parse a conic-gradient CSS function.
fn parse_conic_gradient(value: &str, repeating: bool) -> Option<rustkit_css::Gradient> {
    // Strip prefix and suffix
    let prefix = if repeating {
        "repeating-conic-gradient("
    } else {
        "conic-gradient("
    };
    let inner = value.strip_prefix(prefix)?.strip_suffix(')')?.trim();

    let parts = split_by_comma(inner);
    if parts.is_empty() {
        return None;
    }

    let mut from_angle = 0.0;
    let mut center = (0.5, 0.5);
    let mut stops_start = 0;

    // Check for "from" angle and "at" position in first part
    let first = parts[0].trim().to_lowercase();
    if first.starts_with("from ") || first.contains(" at ") {
        // Parse "from Xdeg"
        if first.starts_with("from ") {
            let rest = &first[5..];
            if let Some(deg_end) = rest.find("deg") {
                if let Ok(deg) = rest[..deg_end].trim().parse::<f32>() {
                    from_angle = deg;
                }
            }
        }

        // Parse "at X Y"
        if let Some(at_idx) = first.find(" at ") {
            let pos_str = &first[at_idx + 4..];
            let pos_parts: Vec<&str> = pos_str.split_whitespace().collect();
            if pos_parts.len() >= 2 {
                // Order-independent keyword <position> (see resolve_position_pair);
                // conic shared the same order-only bug as radial.
                let (cx, cy) = resolve_position_pair(pos_parts[0], pos_parts[1]);
                center.0 = cx;
                center.1 = cy;
            } else if pos_parts.len() == 1 {
                // Single keyword: interpret as axis-specific position
                let keyword = pos_parts[0].trim().to_lowercase();
                match keyword.as_str() {
                    "top" => {
                        center.0 = 0.5;
                        center.1 = 0.0;
                    }
                    "bottom" => {
                        center.0 = 0.5;
                        center.1 = 1.0;
                    }
                    "left" => {
                        center.0 = 0.0;
                        center.1 = 0.5;
                    }
                    "right" => {
                        center.0 = 1.0;
                        center.1 = 0.5;
                    }
                    "center" => {
                        center.0 = 0.5;
                        center.1 = 0.5;
                    }
                    _ => {
                        let val = parse_position_value(pos_parts[0]);
                        center.0 = val;
                        center.1 = val;
                    }
                }
            }
        }
        stops_start = 1;
    }

    // Parse color stops
    let mut stops = Vec::new();
    for part in &parts[stops_start..] {
        if let Some(stop) = parse_color_stop(part) {
            stops.push(stop);
        }
    }

    if stops.is_empty() {
        return None;
    }

    let gradient = if repeating {
        rustkit_css::ConicGradient::new_repeating(from_angle, center, stops)
    } else {
        rustkit_css::ConicGradient::new(from_angle, center, stops)
    };
    Some(rustkit_css::Gradient::Conic(gradient))
}

/// Parse a gradient direction keyword.
fn parse_gradient_direction(value: &str) -> Option<rustkit_css::GradientDirection> {
    match value.trim().to_lowercase().as_str() {
        "to top" => Some(rustkit_css::GradientDirection::ToTop),
        "to bottom" => Some(rustkit_css::GradientDirection::ToBottom),
        "to left" => Some(rustkit_css::GradientDirection::ToLeft),
        "to right" => Some(rustkit_css::GradientDirection::ToRight),
        "to top left" | "to left top" => Some(rustkit_css::GradientDirection::ToTopLeft),
        "to top right" | "to right top" => Some(rustkit_css::GradientDirection::ToTopRight),
        "to bottom left" | "to left bottom" => Some(rustkit_css::GradientDirection::ToBottomLeft),
        "to bottom right" | "to right bottom" => {
            Some(rustkit_css::GradientDirection::ToBottomRight)
        }
        _ => None,
    }
}

/// Parse a color stop (color with optional position).
fn parse_color_stop(value: &str) -> Option<rustkit_css::ColorStop> {
    let value = value.trim();

    // Try to find where the color ends and position begins
    // This is tricky because colors can be rgb(), rgba(), etc.
    let mut paren_depth = 0;
    let mut last_space = None;

    for (i, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ' ' if paren_depth == 0 => last_space = Some(i),
            _ => {}
        }
    }

    if let Some(space_idx) = last_space {
        let color_str = &value[..space_idx];
        let pos_str = &value[space_idx + 1..];
        let color = parse_color(color_str)?;

        if pos_str.ends_with('%') {
            // Percentage position (normalized to 0-1)
            let percent = pos_str
                .strip_suffix('%')
                .and_then(|s| s.parse::<f32>().ok())?;
            Some(rustkit_css::ColorStop::with_percent(color, percent / 100.0))
        } else if pos_str.ends_with("px") {
            // Pixel position - store as pixels for conversion at render time
            let pixels = pos_str
                .strip_suffix("px")
                .and_then(|s| s.parse::<f32>().ok())?;
            Some(rustkit_css::ColorStop::with_pixels(color, pixels))
        } else {
            // No recognized unit, try parsing as a number (treat as percentage)
            if let Ok(val) = pos_str.parse::<f32>() {
                Some(rustkit_css::ColorStop::with_percent(color, val / 100.0))
            } else {
                // No valid position, just the color
                Some(rustkit_css::ColorStop {
                    color,
                    position: None,
                })
            }
        }
    } else {
        // No position, just the color
        let color = parse_color(value)?;
        Some(rustkit_css::ColorStop {
            color,
            position: None,
        })
    }
}

/// Split a string by commas, respecting parentheses.
fn split_by_comma(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut paren_depth = 0;

    for (i, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ',' if paren_depth == 0 => {
                parts.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    if start < value.len() {
        parts.push(&value[start..]);
    }

    parts
}

// ==================== Background Layer Parsing ====================

/// Parse a background-size value.
fn parse_background_size(value: &str) -> rustkit_css::BackgroundSize {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "cover" => rustkit_css::BackgroundSize::Cover,
        "contain" => rustkit_css::BackgroundSize::Contain,
        "auto" => rustkit_css::BackgroundSize::Auto,
        _ => {
            // Parse explicit size (e.g., "100px 50px" or "50% auto")
            let parts: Vec<&str> = value.split_whitespace().collect();
            let width = parts
                .first()
                .and_then(|s| parse_background_size_dimension(s));
            let height = parts
                .get(1)
                .and_then(|s| parse_background_size_dimension(s));
            rustkit_css::BackgroundSize::Explicit { width, height }
        }
    }
}

/// Parse a single dimension for background-size (px, %, or auto).
fn parse_background_size_dimension(value: &str) -> Option<f32> {
    let value = value.trim();
    if value == "auto" {
        return None;
    }
    if value.ends_with("px") {
        return value.strip_suffix("px").and_then(|s| s.parse().ok());
    }
    if value.ends_with('%') {
        // Return percentage as negative value to indicate it's a percentage
        // (will be resolved during layout)
        return value
            .strip_suffix('%')
            .and_then(|s| s.parse::<f32>().ok())
            .map(|p| -p);
    }
    value.parse().ok()
}

/// Parse a background-repeat value.
fn parse_background_repeat(value: &str) -> rustkit_css::BackgroundRepeat {
    match value.trim().to_lowercase().as_str() {
        "repeat" => rustkit_css::BackgroundRepeat::Repeat,
        "repeat-x" => rustkit_css::BackgroundRepeat::RepeatX,
        "repeat-y" => rustkit_css::BackgroundRepeat::RepeatY,
        "no-repeat" => rustkit_css::BackgroundRepeat::NoRepeat,
        "space" => rustkit_css::BackgroundRepeat::Space,
        "round" => rustkit_css::BackgroundRepeat::Round,
        _ => rustkit_css::BackgroundRepeat::default(),
    }
}

/// Parse a background-position value.
fn parse_background_position(value: &str) -> rustkit_css::BackgroundPosition {
    let value = value.trim().to_lowercase();
    let parts: Vec<&str> = value.split_whitespace().collect();

    let x = parts
        .first()
        .map(|s| parse_background_position_value(s))
        .unwrap_or(rustkit_css::BackgroundPositionValue::Percent(0.0));
    let y = parts
        .get(1)
        .map(|s| parse_background_position_value(s))
        .unwrap_or_else(|| {
            // If only one value, center the other axis for keywords, or use same for lengths
            match &x {
                rustkit_css::BackgroundPositionValue::Percent(_) => {
                    rustkit_css::BackgroundPositionValue::Percent(0.5)
                }
                rustkit_css::BackgroundPositionValue::Px(_) => {
                    rustkit_css::BackgroundPositionValue::Percent(0.5)
                }
            }
        });

    rustkit_css::BackgroundPosition { x, y }
}

/// Parse a single background-position dimension.
fn parse_background_position_value(value: &str) -> rustkit_css::BackgroundPositionValue {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "left" | "top" => rustkit_css::BackgroundPositionValue::Percent(0.0),
        "center" => rustkit_css::BackgroundPositionValue::Percent(0.5),
        "right" | "bottom" => rustkit_css::BackgroundPositionValue::Percent(1.0),
        _ if value.ends_with('%') => value
            .strip_suffix('%')
            .and_then(|s| s.parse::<f32>().ok())
            .map(|p| rustkit_css::BackgroundPositionValue::Percent(p / 100.0))
            .unwrap_or(rustkit_css::BackgroundPositionValue::Percent(0.0)),
        _ if value.ends_with("px") => value
            .strip_suffix("px")
            .and_then(|s| s.parse::<f32>().ok())
            .map(rustkit_css::BackgroundPositionValue::Px)
            .unwrap_or(rustkit_css::BackgroundPositionValue::Percent(0.0)),
        _ => {
            // Try parsing as a number (assumed px)
            value
                .parse::<f32>()
                .ok()
                .map(rustkit_css::BackgroundPositionValue::Px)
                .unwrap_or(rustkit_css::BackgroundPositionValue::Percent(0.0))
        }
    }
}

/// Parse a background-origin value.
fn parse_background_origin(value: &str) -> rustkit_css::BackgroundOrigin {
    match value.trim().to_lowercase().as_str() {
        "border-box" => rustkit_css::BackgroundOrigin::BorderBox,
        "padding-box" => rustkit_css::BackgroundOrigin::PaddingBox,
        "content-box" => rustkit_css::BackgroundOrigin::ContentBox,
        _ => rustkit_css::BackgroundOrigin::default(),
    }
}

/// Parse a single background layer from CSS (may contain image, position, size, repeat).
fn parse_background_layer(value: &str) -> Option<rustkit_css::BackgroundLayer> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let mut layer = rustkit_css::BackgroundLayer::default();

    // Check for gradient
    if let Some(gradient) = parse_gradient(value) {
        layer.image = rustkit_css::BackgroundImage::Gradient(gradient);
        return Some(layer);
    }

    // Check for url()
    if value.starts_with("url(") {
        if let Some(end) = value.find(')') {
            let url = value[4..end].trim().trim_matches(|c| c == '"' || c == '\'');
            layer.image = rustkit_css::BackgroundImage::Url(url.to_string());
            return Some(layer);
        }
    }

    // Check if it's a color (these don't create image layers)
    if parse_color(value).is_some() {
        return None;
    }

    // Check for keywords like "none"
    if value == "none" {
        return None;
    }

    None
}

/// Parse a position value (percentage, keyword, or length).
fn parse_position_value(value: &str) -> f32 {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "left" | "top" => 0.0,
        "center" => 0.5,
        "right" | "bottom" => 1.0,
        _ if value.ends_with('%') => value
            .strip_suffix('%')
            .and_then(|s| s.parse::<f32>().ok())
            .map(|p| p / 100.0)
            .unwrap_or(0.5),
        _ => 0.5,
    }
}

/// Resolve a two-value CSS `<position>` (as used in `gradient(... at A B ...)`)
/// into `(x, y)` fractions of the box.
///
/// Per CSS Values §<position>, when both values are KEYWORDS the order does
/// not matter: `top right` and `right top` both mean x=right, y=top, because
/// left/right always name the horizontal axis and top/bottom the vertical.
/// Only length/percentage values are positional (first = horizontal, second =
/// vertical). `center` is axis-neutral and takes whichever axis is left.
///
/// The previous code assigned `a -> x, b -> y` unconditionally, so `at top
/// right` resolved to (0.0, 1.0) = bottom-left. Symmetric positions
/// (`top left`, `bottom right`, `center`) are swap-invariant, which is why the
/// bug only surfaced on asymmetric keyword pairs like `top right`.
fn resolve_position_pair(a: &str, b: &str) -> (f32, f32) {
    let ka = a.trim().to_lowercase();
    let kb = b.trim().to_lowercase();
    let is_horiz = |k: &str| matches!(k, "left" | "right");
    let is_vert = |k: &str| matches!(k, "top" | "bottom");

    if is_vert(&ka) || is_horiz(&kb) {
        // First value names the vertical axis (or second names horizontal),
        // e.g. `top right` / `center left` -> swap so x gets the horizontal.
        (parse_position_value(&kb), parse_position_value(&ka))
    } else {
        // Natural order: `left top`, `right center`, or numeric `50% 25%`.
        (parse_position_value(&ka), parse_position_value(&kb))
    }
}

/// Parse a length value from CSS.
///
/// Delegates to rustkit-css: single source of truth for length parsing
/// (units, calc/min/max/clamp, unitless zero). This engine kept a full
/// parallel implementation for months — the exact one-copy-fixed,
/// other-copy-drifts factory Prometheus's duplication audit flagged as P0.
/// The css version is a superset; the local copy and its private
/// calc/min/max/clamp helpers are deleted.
fn parse_length(value: &str) -> Option<rustkit_css::Length> {
    rustkit_css::parse_length(value)
}


/// Split CSS function arguments, respecting nested parentheses.
fn split_css_args(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    // Don't forget the last argument
    if start < s.len() {
        result.push(&s[start..]);
    }

    result
}

/// Parse a shorthand value with 1-4 parts (like margin, padding).
/// Returns (top, right, bottom, left).
/// Parse a `border` / `border-<side>` shorthand: `<width> || <style> || <color>`.
/// ComputedStyle has no border-style field, so the style keyword only matters
/// for `none`/`hidden` (which force a zero width, matching how the box would
/// paint). Color tokens may contain spaces (`rgb(1, 2, 3)`), so everything
/// that isn't a width or style keyword is re-joined and handed to parse_color.
fn parse_border_shorthand(
    value: &str,
) -> Option<(rustkit_css::Length, Option<rustkit_css::Color>)> {
    const STYLE_KEYWORDS: [&str; 10] = [
        "solid", "dashed", "dotted", "double", "groove", "ridge", "inset", "outset", "none",
        "hidden",
    ];

    let mut width: Option<rustkit_css::Length> = None;
    let mut style_none = false;
    let mut saw_style = false;
    let mut color_parts: Vec<&str> = Vec::new();

    for token in value.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if STYLE_KEYWORDS.contains(&lower.as_str()) {
            saw_style = true;
            style_none = matches!(lower.as_str(), "none" | "hidden");
        } else if width.is_none() && (lower == "thin" || lower == "medium" || lower == "thick") {
            width = Some(rustkit_css::Length::Px(match lower.as_str() {
                "thin" => 1.0,
                "thick" => 5.0,
                _ => 3.0,
            }));
        } else if width.is_none() && parse_length(token).is_some() {
            width = parse_length(token);
        } else {
            color_parts.push(token);
        }
    }

    if width.is_none() && !saw_style && color_parts.is_empty() {
        return None; // Nothing recognizable — drop, don't guess.
    }

    // `border: solid` etc. → medium width per spec.
    let mut resolved_width = width.unwrap_or(rustkit_css::Length::Px(3.0));
    if style_none {
        resolved_width = rustkit_css::Length::Zero;
    }
    let color = if color_parts.is_empty() {
        None
    } else {
        parse_color(&color_parts.join(" "))
    };
    Some((resolved_width, color))
}

fn parse_shorthand_4(
    value: &str,
) -> Option<(
    rustkit_css::Length,
    rustkit_css::Length,
    rustkit_css::Length,
    rustkit_css::Length,
)> {
    let parts: Vec<&str> = value.split_whitespace().collect();

    match parts.len() {
        1 => {
            let v = parse_length(parts[0])?;
            Some((v.clone(), v.clone(), v.clone(), v))
        }
        2 => {
            let tb = parse_length(parts[0])?;
            let lr = parse_length(parts[1])?;
            Some((tb.clone(), lr.clone(), tb, lr))
        }
        3 => {
            let t = parse_length(parts[0])?;
            let lr = parse_length(parts[1])?;
            let b = parse_length(parts[2])?;
            Some((t, lr.clone(), b, lr))
        }
        4 => {
            let t = parse_length(parts[0])?;
            let r = parse_length(parts[1])?;
            let b = parse_length(parts[2])?;
            let l = parse_length(parts[3])?;
            Some((t, r, b, l))
        }
        _ => None,
    }
}

/// Check if a CSS property is inherited by default.
fn is_inherited_property(property: &str) -> bool {
    matches!(
        property,
        "color"
            | "font"
            | "font-family"
            | "font-size"
            | "font-style"
            | "font-weight"
            | "line-height"
            | "text-align"
            | "text-decoration"
            | "text-transform"
            | "letter-spacing"
            | "word-spacing"
            | "white-space"
            | "visibility"
            | "cursor"
            | "direction"
            | "writing-mode"
    )
}

/// Parse a box-shadow value from CSS.
/// Supports: offset-x offset-y [blur [spread]] color [inset]
fn parse_box_shadow(value: &str) -> Option<rustkit_css::BoxShadow> {
    let value = value.trim();
    if value.is_empty() || value == "none" {
        return None;
    }

    let mut shadow = rustkit_css::BoxShadow::new();

    // Check for "inset" keyword
    let (value, inset) = if value.starts_with("inset") {
        // SAFETY: strip_prefix will succeed because we just checked starts_with("inset")
        (value.strip_prefix("inset").unwrap().trim(), true)
    } else if value.ends_with("inset") {
        // SAFETY: strip_suffix will succeed because we just checked ends_with("inset")
        (value.strip_suffix("inset").unwrap().trim(), true)
    } else {
        (value, false)
    };
    shadow.inset = inset;

    // Split into tokens, being careful about rgba() which contains commas
    let mut parts: Vec<&str> = Vec::new();
    let mut current_start = 0;
    let mut paren_depth = 0;

    for (i, ch) in value.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ' ' if paren_depth == 0 => {
                let part = value[current_start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                current_start = i + 1;
            }
            _ => {}
        }
    }
    // Don't forget the last part
    let last_part = value[current_start..].trim();
    if !last_part.is_empty() {
        parts.push(last_part);
    }

    // Parse parts: expect at least 2 lengths + 1 color
    // Format: offset-x offset-y [blur [spread]] color
    let mut lengths: Vec<f32> = Vec::new();
    let mut color_value = None;

    for part in parts {
        // Try as length first
        if let Some(length) = parse_length(part) {
            lengths.push(length.to_px(16.0, 16.0, 0.0));
        } else {
            // Must be a color
            if let Some(c) = parse_color(part) {
                color_value = Some(c);
            }
        }
    }

    // Assign lengths
    if lengths.len() >= 2 {
        shadow.offset_x = lengths[0];
        shadow.offset_y = lengths[1];
    } else {
        return None; // Need at least offset-x and offset-y
    }

    if lengths.len() >= 3 {
        shadow.blur_radius = lengths[2].max(0.0);
    }

    if lengths.len() >= 4 {
        shadow.spread_radius = lengths[3];
    }

    // Set color
    shadow.color = color_value.unwrap_or(rustkit_css::Color::new(0, 0, 0, 0.5));

    Some(shadow)
}

/// Parse an overflow value.
fn parse_overflow(value: &str) -> rustkit_css::Overflow {
    match value.trim() {
        "visible" => rustkit_css::Overflow::Visible,
        "hidden" => rustkit_css::Overflow::Hidden,
        "scroll" => rustkit_css::Overflow::Scroll,
        "auto" => rustkit_css::Overflow::Auto,
        "clip" => rustkit_css::Overflow::Clip,
        _ => rustkit_css::Overflow::Visible,
    }
}

/// Parse a CSS time value (e.g., "0.3s", "300ms") into seconds.
fn parse_time(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.ends_with("ms") {
        value[..value.len() - 2]
            .parse::<f32>()
            .ok()
            .map(|v| v / 1000.0)
    } else if value.ends_with('s') {
        value[..value.len() - 1].parse::<f32>().ok()
    } else {
        None
    }
}

/// Parse a CSS timing function.
fn parse_timing_function(value: &str) -> rustkit_css::TimingFunction {
    let value = value.trim();
    match value {
        "ease" => rustkit_css::TimingFunction::Ease,
        "linear" => rustkit_css::TimingFunction::Linear,
        "ease-in" => rustkit_css::TimingFunction::EaseIn,
        "ease-out" => rustkit_css::TimingFunction::EaseOut,
        "ease-in-out" => rustkit_css::TimingFunction::EaseInOut,
        "step-start" => rustkit_css::TimingFunction::StepStart,
        "step-end" => rustkit_css::TimingFunction::StepEnd,
        _ if value.starts_with("cubic-bezier(") => {
            // Parse cubic-bezier(x1, y1, x2, y2)
            let inner = value
                .trim_start_matches("cubic-bezier(")
                .trim_end_matches(')');
            let parts: Vec<f32> = inner
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if parts.len() == 4 {
                rustkit_css::TimingFunction::CubicBezier(parts[0], parts[1], parts[2], parts[3])
            } else {
                rustkit_css::TimingFunction::Ease
            }
        }
        _ if value.starts_with("steps(") => {
            // Parse steps(count, jump-start|jump-end)
            let inner = value.trim_start_matches("steps(").trim_end_matches(')');
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if let Some(count) = parts.first().and_then(|s| s.parse::<u32>().ok()) {
                let jump_start = parts
                    .get(1)
                    .map(|s| *s == "jump-start" || *s == "start")
                    .unwrap_or(false);
                rustkit_css::TimingFunction::Steps(count, jump_start)
            } else {
                rustkit_css::TimingFunction::StepEnd
            }
        }
        _ => rustkit_css::TimingFunction::Ease,
    }
}

/// Parse a CSS transform value into a TransformList.
fn parse_transform(value: &str) -> Option<rustkit_css::TransformList> {
    let value = value.trim();
    if value == "none" {
        return Some(rustkit_css::TransformList::none());
    }

    let mut ops = Vec::new();
    let mut remaining = value;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();

        // Find the function name
        if let Some(paren_pos) = remaining.find('(') {
            let func_name = &remaining[..paren_pos];
            let after_paren = &remaining[paren_pos + 1..];

            // Find matching closing paren
            if let Some(close_pos) = find_matching_paren(after_paren) {
                let args = &after_paren[..close_pos];
                remaining = &after_paren[close_pos + 1..];

                if let Some(op) = parse_transform_op(func_name, args) {
                    ops.push(op);
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if ops.is_empty() {
        None
    } else {
        Some(rustkit_css::TransformList { ops })
    }
}

/// Parse a single transform operation.
fn parse_transform_op(func: &str, args: &str) -> Option<rustkit_css::TransformOp> {
    let args = args.trim();
    let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();

    match func.trim() {
        "translate" => {
            let x = parse_length(parts.first()?)?;
            let y = parts
                .get(1)
                .and_then(|s| parse_length(s))
                .unwrap_or(rustkit_css::Length::Zero);
            Some(rustkit_css::TransformOp::Translate(x, y))
        }
        "translateX" => {
            let x = parse_length(parts.first()?)?;
            Some(rustkit_css::TransformOp::TranslateX(x))
        }
        "translateY" => {
            let y = parse_length(parts.first()?)?;
            Some(rustkit_css::TransformOp::TranslateY(y))
        }
        "scale" => {
            let sx = parts.first()?.parse::<f32>().ok()?;
            let sy = parts
                .get(1)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(sx);
            Some(rustkit_css::TransformOp::Scale(sx, sy))
        }
        "scaleX" => {
            let s = parts.first()?.parse::<f32>().ok()?;
            Some(rustkit_css::TransformOp::ScaleX(s))
        }
        "scaleY" => {
            let s = parts.first()?.parse::<f32>().ok()?;
            Some(rustkit_css::TransformOp::ScaleY(s))
        }
        "rotate" => {
            let angle = parse_angle(parts.first()?)?;
            Some(rustkit_css::TransformOp::Rotate(angle))
        }
        "skew" => {
            let ax = parse_angle(parts.first()?)?;
            let ay = parts.get(1).and_then(|s| parse_angle(s)).unwrap_or(0.0);
            Some(rustkit_css::TransformOp::Skew(ax, ay))
        }
        "skewX" => {
            let angle = parse_angle(parts.first()?)?;
            Some(rustkit_css::TransformOp::SkewX(angle))
        }
        "skewY" => {
            let angle = parse_angle(parts.first()?)?;
            Some(rustkit_css::TransformOp::SkewY(angle))
        }
        "matrix" => {
            if parts.len() >= 6 {
                let a = parts[0].parse::<f32>().ok()?;
                let b = parts[1].parse::<f32>().ok()?;
                let c = parts[2].parse::<f32>().ok()?;
                let d = parts[3].parse::<f32>().ok()?;
                let e = parts[4].parse::<f32>().ok()?;
                let f = parts[5].parse::<f32>().ok()?;
                Some(rustkit_css::TransformOp::Matrix(a, b, c, d, e, f))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse a CSS angle value (e.g., "45deg", "1rad", "0.5turn") into degrees.
fn parse_angle(value: &str) -> Option<f32> {
    let value = value.trim();
    // LONGEST SUFFIX FIRST. `grad` ends with `rad`, so testing `rad` first
    // swallows every gradian angle: "200grad" matched the rad arm, lost three
    // characters instead of four, failed to parse, and returned None — while
    // the grad arm below sat unreachable, looking like support. Silent None,
    // not a wrong number: rotate(200grad) simply did not rotate.
    //
    // Same shape as rem-before-em. Any new unit whose suffix ends with an
    // existing one goes ABOVE it, and the round-trip test below is what
    // catches it if this comment is not read.
    if value.ends_with("grad") {
        value[..value.len() - 4]
            .parse::<f32>()
            .ok()
            .map(|g| g * 0.9)
    } else if value.ends_with("turn") {
        value[..value.len() - 4]
            .parse::<f32>()
            .ok()
            .map(|t| t * 360.0)
    } else if value.ends_with("deg") {
        value[..value.len() - 3].parse().ok()
    } else if value.ends_with("rad") {
        value[..value.len() - 3]
            .parse::<f32>()
            .ok()
            .map(|r| r.to_degrees())
    } else {
        // Try parsing as number (defaults to degrees)
        value.parse().ok()
    }
}

/// Parse transform-origin value.
fn parse_transform_origin(value: &str) -> Option<rustkit_css::TransformOrigin> {
    let parts: Vec<&str> = value.split_whitespace().collect();

    let parse_component = |s: &str| -> Option<rustkit_css::Length> {
        match s {
            "left" => Some(rustkit_css::Length::Percent(0.0)),
            "center" => Some(rustkit_css::Length::Percent(50.0)),
            "right" => Some(rustkit_css::Length::Percent(100.0)),
            "top" => Some(rustkit_css::Length::Percent(0.0)),
            "bottom" => Some(rustkit_css::Length::Percent(100.0)),
            _ => parse_length(s),
        }
    };

    match parts.len() {
        1 => {
            let x = parse_component(parts[0])?;
            Some(rustkit_css::TransformOrigin {
                x,
                y: rustkit_css::Length::Percent(50.0),
            })
        }
        2 | 3 => {
            let x = parse_component(parts[0])?;
            let y = parse_component(parts[1])?;
            Some(rustkit_css::TransformOrigin { x, y })
        }
        _ => None,
    }
}

/// Parse a grid-template-columns or grid-template-rows value.
/// Supports: repeat(N, 1fr), explicit track sizes, and combinations.
fn parse_grid_template(value: &str) -> Option<rustkit_css::GridTemplate> {
    let value = value.trim();

    if value == "none" || value.is_empty() {
        return Some(rustkit_css::GridTemplate::none());
    }

    let mut tracks = Vec::new();

    // Check for repeat() function
    if let Some(repeat_start) = value.find("repeat(") {
        let after_repeat = &value[repeat_start + 7..];
        if let Some(close_paren) = find_matching_paren(after_repeat) {
            let repeat_content = &after_repeat[..close_paren];

            // Parse repeat(count, track-size)
            if let Some(comma_pos) = repeat_content.find(',') {
                let count_str = repeat_content[..comma_pos].trim();
                let track_str = repeat_content[comma_pos + 1..].trim();

                // Parse count (could be number, auto-fill, auto-fit)
                let count: Option<u32> = if count_str == "auto-fill" || count_str == "auto-fit" {
                    // For now, default to a reasonable number
                    Some(4)
                } else {
                    count_str.parse().ok()
                };

                if let (Some(count), Some(track_size)) = (count, parse_track_size(track_str)) {
                    for _ in 0..count {
                        tracks.push(rustkit_css::TrackDefinition::simple(track_size.clone()));
                    }
                }
            }
        }
    } else {
        // Parse space-separated track sizes
        for part in value.split_whitespace() {
            if let Some(track_size) = parse_track_size(part) {
                tracks.push(rustkit_css::TrackDefinition::simple(track_size));
            }
        }
    }

    if tracks.is_empty() {
        return None;
    }

    Some(rustkit_css::GridTemplate {
        tracks,
        repeats: Vec::new(),
        final_line_names: Vec::new(),
    })
}

/// Find the position of the matching closing parenthesis.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a single track size (e.g., "1fr", "100px", "auto", "minmax(...)").
fn parse_track_size(value: &str) -> Option<rustkit_css::TrackSize> {
    let value = value.trim();

    if value == "auto" {
        return Some(rustkit_css::TrackSize::Auto);
    }

    if value == "min-content" {
        return Some(rustkit_css::TrackSize::MinContent);
    }

    if value == "max-content" {
        return Some(rustkit_css::TrackSize::MaxContent);
    }

    // Check for fr unit
    if let Some(fr_str) = value.strip_suffix("fr") {
        if let Ok(fr) = fr_str.trim().parse::<f32>() {
            return Some(rustkit_css::TrackSize::Fr(fr));
        }
    }

    // Check for px unit
    if let Some(px_str) = value.strip_suffix("px") {
        if let Ok(px) = px_str.trim().parse::<f32>() {
            return Some(rustkit_css::TrackSize::Px(px));
        }
    }

    // Check for percent
    if let Some(pct_str) = value.strip_suffix('%') {
        if let Ok(pct) = pct_str.trim().parse::<f32>() {
            return Some(rustkit_css::TrackSize::Percent(pct));
        }
    }

    // Check for minmax()
    if value.starts_with("minmax(") {
        if let Some(close) = find_matching_paren(&value[7..]) {
            let content = &value[7..7 + close];
            if let Some(comma) = content.find(',') {
                let min_str = content[..comma].trim();
                let max_str = content[comma + 1..].trim();
                if let (Some(min), Some(max)) =
                    (parse_track_size(min_str), parse_track_size(max_str))
                {
                    return Some(rustkit_css::TrackSize::MinMax(Box::new(min), Box::new(max)));
                }
            }
        }
    }

    // Check for fit-content()
    if value.starts_with("fit-content(") {
        if let Some(close) = find_matching_paren(&value[12..]) {
            let content = &value[12..12 + close];
            if let Some(length) = parse_length(content) {
                return Some(rustkit_css::TrackSize::FitContent(
                    length.to_px(16.0, 16.0, 0.0),
                ));
            }
        }
    }

    None
}

/// Parse a grid line value (e.g., "1", "span 2", "auto").
fn parse_grid_line(value: &str) -> Option<rustkit_css::GridLine> {
    let value = value.trim();

    if value == "auto" {
        return Some(rustkit_css::GridLine::Auto);
    }

    // Check for "span N"
    if let Some(span_str) = value.strip_prefix("span") {
        let span_str = span_str.trim();
        if let Ok(span) = span_str.parse::<u32>() {
            return Some(rustkit_css::GridLine::Span(span));
        }
    }

    // Try as a number
    if let Ok(num) = value.parse::<i32>() {
        return Some(rustkit_css::GridLine::Number(num));
    }

    // Could be a named line (just use auto for now)
    Some(rustkit_css::GridLine::Auto)
}

/// Parse a grid-column or grid-row shorthand (e.g., "1 / 3", "span 2").
fn parse_grid_line_shorthand(
    value: &str,
) -> Option<(rustkit_css::GridLine, rustkit_css::GridLine)> {
    let value = value.trim();

    // Check for "start / end" format
    if let Some(slash_pos) = value.find('/') {
        let start_str = value[..slash_pos].trim();
        let end_str = value[slash_pos + 1..].trim();

        let start = parse_grid_line(start_str)?;
        let end = parse_grid_line(end_str)?;

        return Some((start, end));
    }

    // Single value - applies to start, end is auto
    let start = parse_grid_line(value)?;
    Some((start, rustkit_css::GridLine::Auto))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_view_id_uniqueness() {
        let id1 = EngineViewId::new();
        let id2 = EngineViewId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_engine_config_default() {
        let config = EngineConfig::default();
        assert!(config.javascript_enabled);
        assert!(config.cookies_enabled);
    }

    #[test]
    fn test_engine_builder() {
        let builder = EngineBuilder::new()
            .user_agent("Test/1.0")
            .javascript_enabled(false);

        assert_eq!(builder.config.user_agent, "Test/1.0");
        assert!(!builder.config.javascript_enabled);
    }

    #[test]
    fn test_layout_tree_from_document() {
        // Parse a simple HTML document
        let html = r#"<!DOCTYPE html>
            <html>
            <head><title>Test</title></head>
            <body>
                <h1>Hello World</h1>
                <p>This is a paragraph.</p>
            </body>
            </html>"#;

        let document = Document::parse_html(html).expect("Failed to parse HTML");
        let document = Rc::new(document);

        // Verify document structure
        assert!(document.body().is_some(), "Document should have a body");

        // Create a dummy engine - skip test if GPU is not available
        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        // Build layout tree from document
        let layout = engine.build_layout_from_document(&document, &[]);

        // Verify layout tree is not empty
        assert!(
            !layout.children.is_empty(),
            "Layout tree should have children from body"
        );

        // The body should contain h1 and p elements
        let body_box = &layout.children[0];

        // Count text boxes (h1 content "Hello World" and p content "This is a paragraph.")
        fn count_text_boxes(layout_box: &LayoutBox) -> usize {
            let mut count = if matches!(layout_box.box_type, BoxType::Text(_)) {
                1
            } else {
                0
            };
            for child in &layout_box.children {
                count += count_text_boxes(child);
            }
            count
        }

        let text_count = count_text_boxes(body_box);
        assert!(
            text_count >= 2,
            "Should have at least 2 text boxes (h1 and p content), got {}",
            text_count
        );
    }

    #[test]
    fn test_sibling_combinators_and_positional_pseudo_classes() {
        // `+`/`~` and :first-child/:last-child depend on the sibling context that
        // build_layout_from_node_with_parent_style threads into style computation.
        // Before that context existed, every element saw "no previous siblings,
        // index 0 of 1": sibling combinators never matched and :first-child/
        // :last-child matched everything.
        let html = r#"<!DOCTYPE html>
            <html>
            <head><style>
                div, p { background: rgb(200, 200, 200); }
                .a + .b { background: rgb(0, 128, 0); }
                .a ~ .c { background: rgb(0, 0, 255); }
                p:first-child { background: rgb(255, 0, 0); }
                p:last-child { background: rgb(255, 165, 0); }
            </style></head>
            <body>
                <div class="wrap">
                    <div class="a">A</div>
                    <div class="b">B</div>
                    <div class="x">X</div>
                    <div class="c">C</div>
                </div>
                <section>
                    <p>first</p>
                    <p>middle</p>
                    <p>last</p>
                </section>
            </body>
            </html>"#;

        let document = Document::parse_html(html).expect("Failed to parse HTML");
        let document = Rc::new(document);

        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let layout = engine.build_layout_from_document(&document, &[]);
        let body_box = &layout.children[0];
        let wrap = &body_box.children[0];
        let section = &body_box.children[1];

        let bg = |b: &LayoutBox| {
            (
                b.style.background_color.r,
                b.style.background_color.g,
                b.style.background_color.b,
            )
        };

        // .wrap children: a, b, x, c
        assert_eq!(
            bg(&wrap.children[0]),
            (200, 200, 200),
            ".a matches no sibling rule"
        );
        assert_eq!(
            bg(&wrap.children[1]),
            (0, 128, 0),
            ".a + .b should match adjacent sibling"
        );
        assert_eq!(
            bg(&wrap.children[2]),
            (200, 200, 200),
            ".x matches no sibling rule"
        );
        assert_eq!(
            bg(&wrap.children[3]),
            (0, 0, 255),
            ".a ~ .c should match general sibling"
        );

        // section children: p, p, p
        assert_eq!(
            bg(&section.children[0]),
            (255, 0, 0),
            "first p is :first-child"
        );
        assert_eq!(
            bg(&section.children[1]),
            (200, 200, 200),
            "middle p is neither first nor last child"
        );
        assert_eq!(
            bg(&section.children[2]),
            (255, 165, 0),
            "last p is :last-child"
        );
    }

    #[test]
    fn test_display_list_generation() {
        // Parse a document with styled content
        let html = r#"<!DOCTYPE html>
            <html>
            <body style="background-color: white">
                <h1>Title</h1>
            </body>
            </html>"#;

        let document = Document::parse_html(html).expect("Failed to parse HTML");
        let document = Rc::new(document);

        // Skip test if GPU is not available
        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let mut layout = engine.build_layout_from_document(&document, &[]);

        // Perform layout with a containing block
        let containing_block = Dimensions {
            content: Rect::new(0.0, 0.0, 800.0, 600.0),
            ..Default::default()
        };
        layout.layout(&containing_block);

        // Generate display list
        let display_list = DisplayList::build(&layout);

        // Display list should have commands (at least background colors)
        assert!(
            !display_list.commands.is_empty(),
            "Display list should have commands, got {:?}",
            display_list.commands
        );
    }

    #[test]
    fn test_parse_color() {
        // Test named colors
        assert_eq!(parse_color("black"), Some(rustkit_css::Color::BLACK));
        assert_eq!(parse_color("white"), Some(rustkit_css::Color::WHITE));

        // Test hex colors
        assert_eq!(
            parse_color("#fff"),
            Some(rustkit_css::Color::from_rgb(255, 255, 255))
        );
        assert_eq!(
            parse_color("#000000"),
            Some(rustkit_css::Color::from_rgb(0, 0, 0))
        );
        assert_eq!(
            parse_color("#ff0000"),
            Some(rustkit_css::Color::from_rgb(255, 0, 0))
        );

        // Test rgb colors
        assert_eq!(
            parse_color("rgb(255, 0, 0)"),
            Some(rustkit_css::Color::new(255, 0, 0, 1.0))
        );

        // Extended named colors — the engine-local parser knew only 11 names
        // and silently dropped the rest (bg-solid's coral swatch, 2026-07-08).
        assert_eq!(
            parse_color("coral"),
            Some(rustkit_css::Color::from_rgb(255, 127, 80))
        );
        assert_eq!(
            parse_color("tomato"),
            Some(rustkit_css::Color::from_rgb(255, 99, 71))
        );
        assert_eq!(
            parse_color("Orange"),
            Some(rustkit_css::Color::from_rgb(255, 165, 0))
        );

        // Case-insensitive functional syntax must keep working post-delegation
        assert_eq!(
            parse_color("RGB(0, 128, 0)"),
            Some(rustkit_css::Color::new(0, 128, 0, 1.0))
        );

        // hsl with negative hue wraps (engine semantics preserved)
        assert_eq!(
            parse_color("hsl(-120, 50%, 50%)"),
            parse_color("hsl(240, 50%, 50%)")
        );
    }

    #[test]
    fn test_em_font_size_absolutizes_against_parent() {
        // font-size: 2em must COMPUTE to Px(32) (2 × body's 16px) at style
        // time — layout falls back to 16px on any non-Px font-size, so an
        // unresolved Em(2.0) rendered h1s at body size (bg-solid, 2026-07-08).
        let html = r#"<!DOCTYPE html>
            <html><body>
                <h1 style="font-size: 2em">Em</h1>
                <div style="font-size: 150%">Percent</div>
                <div style="font-size: 1.5rem">Rem</div>
            </body></html>"#;
        let document = Document::parse_html(html).expect("Failed to parse HTML");
        let document = Rc::new(document);

        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let layout = engine.build_layout_from_document(&document, &[]);

        fn collect_font_sizes(b: &LayoutBox, out: &mut Vec<(String, rustkit_css::Length)>) {
            if let BoxType::Text(t) = &b.box_type {
                out.push((t.clone(), b.style.font_size.clone()));
            }
            for c in &b.children {
                collect_font_sizes(c, out);
            }
        }
        let mut sizes = Vec::new();
        collect_font_sizes(&layout, &mut sizes);

        let get = |needle: &str| {
            sizes
                .iter()
                .find(|(t, _)| t.contains(needle))
                .map(|(_, s)| s.clone())
                .unwrap_or_else(|| panic!("no text box containing {:?}", needle))
        };
        assert_eq!(get("Em"), rustkit_css::Length::Px(32.0), "2em vs body 16px");
        assert_eq!(
            get("Percent"),
            rustkit_css::Length::Px(24.0),
            "150% vs body 16px"
        );
        assert_eq!(
            get("Rem"),
            rustkit_css::Length::Px(24.0),
            "1.5rem vs root 16px"
        );
    }

    #[test]
    fn test_font_weight_numeric_and_relative_values_compute() {
        // css-fonts-4 §2.2: any <number> in [1,1000] plus lighter/bolder
        // against the inherited weight. The old arm accepted only
        // bold/700/800/900 and normal/400 — `font-weight: 300` was silently
        // dropped, so about's .tagline shaped with the Regular face and
        // wrapped where Chrome (Light, 659px < 672px) keeps one line.
        let html = r#"<!DOCTYPE html>
            <html><body>
                <div style="font-weight: 300">W300</div>
                <div style="font-weight: 500">W500</div>
                <div style="font-weight: 600">W600</div>
                <div style="font-weight: bold"><span style="font-weight: lighter">Lighter</span></div>
                <div><span style="font-weight: bolder">Bolder</span></div>
                <div style="font-weight: 1001">Clamp</div>
            </body></html>"#;
        let document = Document::parse_html(html).expect("Failed to parse HTML");
        let document = Rc::new(document);

        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let layout = engine.build_layout_from_document(&document, &[]);

        fn collect_weights(b: &LayoutBox, out: &mut Vec<(String, u16)>) {
            if let BoxType::Text(t) = &b.box_type {
                out.push((t.clone(), b.style.font_weight.0));
            }
            for c in &b.children {
                collect_weights(c, out);
            }
        }
        let mut weights = Vec::new();
        collect_weights(&layout, &mut weights);

        let get = |needle: &str| {
            weights
                .iter()
                .find(|(t, _)| t.contains(needle))
                .map(|(_, w)| *w)
                .unwrap_or_else(|| panic!("no text box containing {:?}", needle))
        };
        assert_eq!(get("W300"), 300, "numeric 300 must compute, not drop");
        assert_eq!(get("W500"), 500, "numeric 500 must compute, not drop");
        assert_eq!(get("W600"), 600, "numeric 600 must compute, not drop");
        assert_eq!(get("Lighter"), 400, "lighter against inherited 700");
        assert_eq!(get("Bolder"), 700, "bolder against inherited 400");
        assert_eq!(get("Clamp"), 400, "out-of-range weight is invalid, keeps inherited");
    }

    #[test]
    fn test_button_ua_display_inline_block_one_line() {
        // Chrome's UA sheet computes inline-block for form controls; RustKit's
        // Display default is Block, which sent every control down the block
        // path — css-selectors §6 stacked its three buttons vertically
        // (h=124.6 vs Chrome 39), the whitespace runs between them each
        // taking their own line. Drive the real engine: three sibling
        // buttons must share one line box.
        let html = r#"<!DOCTYPE html>
            <html><body>
              <div>
                <button>One</button>
                <button>Two</button>
                <button>Three</button>
              </div>
            </body></html>"#;
        let document = Rc::new(Document::parse_html(html).expect("Failed to parse HTML"));

        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let mut layout = engine.build_layout_from_document(&document, &[]);
        let containing_block = Dimensions {
            content: Rect::new(0.0, 0.0, 800.0, 600.0),
            ..Default::default()
        };
        layout.layout(&containing_block);

        fn collect_controls(b: &LayoutBox, out: &mut Vec<(f32, f32, rustkit_css::Display)>) {
            if matches!(b.box_type, BoxType::FormControl(_)) {
                out.push((
                    b.dimensions.content.x,
                    b.dimensions.content.y,
                    b.style.display,
                ));
            }
            for c in &b.children {
                collect_controls(c, out);
            }
        }
        let mut controls = Vec::new();
        collect_controls(&layout, &mut controls);

        assert_eq!(controls.len(), 3, "three button boxes expected");
        for (_, _, display) in &controls {
            assert_eq!(
                *display,
                rustkit_css::Display::InlineBlock,
                "button UA display must compute to inline-block"
            );
        }
        let y0 = controls[0].1;
        for (i, (_, y, _)) in controls.iter().enumerate() {
            assert!(
                (y - y0).abs() <= 1.0,
                "button {} not on the shared line: y={} vs {}",
                i,
                y,
                y0
            );
        }
        assert!(
            controls[0].0 < controls[1].0 && controls[1].0 < controls[2].0,
            "buttons must advance horizontally: xs {:?}",
            controls.iter().map(|c| c.0).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_bare_form_control_heights_match_chrome() {
        // form-controls t8 dig (2026-07-17): Chrome CfT-148 builds bare
        // single-line controls as a ~19px border-box (input/button/select at
        // the UA 13.333px font), checkbox 13x13, textarea 15px/row + 2 —
        // RustKit's old blobs measured 28/32/16 and every section below slid
        // by the deficit. Author padding must still compose (DIG-1/DIG-2):
        // a pad-8 button stays ~31.
        let html = r#"<!DOCTYPE html>
            <html><body>
              <div><input type="text" placeholder="bare"></div>
              <div><button>Bare</button></div>
              <div><input type="checkbox"></div>
              <div><textarea placeholder="two rows"></textarea></div>
              <div><button style="padding: 8px 16px;">Padded</button></div>
            </body></html>"#;
        let document = Rc::new(Document::parse_html(html).expect("Failed to parse HTML"));

        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let mut layout = engine.build_layout_from_document(&document, &[]);
        let containing_block = Dimensions {
            content: Rect::new(0.0, 0.0, 800.0, 600.0),
            ..Default::default()
        };
        layout.layout(&containing_block);

        fn collect_control_heights(b: &LayoutBox, out: &mut Vec<f32>) {
            if matches!(b.box_type, BoxType::FormControl(_)) {
                out.push(b.dimensions.content.height);
            }
            for c in &b.children {
                collect_control_heights(c, out);
            }
        }
        let mut heights = Vec::new();
        collect_control_heights(&layout, &mut heights);
        assert_eq!(heights.len(), 5, "five control boxes expected: {:?}", heights);

        let expect = [
            ("bare text input", 19.0, 1.0),
            ("bare button", 19.0, 1.0),
            ("checkbox", 13.0, 1.0),
            ("default textarea (2 rows)", 32.0, 1.0),
            ("author-padded button", 31.0, 1.0),
        ];
        for ((name, want, tol), got) in expect.iter().zip(&heights) {
            assert!(
                (got - want).abs() <= *tol,
                "{name}: height {got} not within {tol} of Chrome's {want} (all: {heights:?})"
            );
        }
    }

    #[test]
    fn test_line_height_inherits_from_html_through_body() {
        // `line-height` is inherited, but rustkit only inherited it into text nodes, never
        // element->element, and layout began at <body> with no parent — so a value set on
        // <html> (as every parity fixture does via parity-reset.css: `html{line-height:1.5}`)
        // never reached headings/paragraphs, which fell back to Normal (×1.2). Chrome inherits
        // the 1.5 factor to every descendant. Assert both the html->body hop and multi-level
        // element inheritance now carry it, while leaving an unset subtree at Normal.
        let html = r#"<!DOCTYPE html>
            <html style="line-height: 1.5">
              <body>
                <h1>Head</h1>
                <div><p>Para</p></div>
              </body>
            </html>"#;
        let document = Rc::new(Document::parse_html(html).expect("Failed to parse HTML"));

        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: GPU not available ({:?})", e);
                return;
            }
        };
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        let layout = engine.build_layout_from_document(&document, &[]);

        fn collect(b: &LayoutBox, out: &mut Vec<(String, rustkit_css::LineHeight)>) {
            if let BoxType::Text(t) = &b.box_type {
                out.push((t.clone(), b.style.line_height.clone()));
            }
            for c in &b.children {
                collect(c, out);
            }
        }
        let mut lhs = Vec::new();
        collect(&layout, &mut lhs);
        let get = |needle: &str| {
            lhs.iter()
                .find(|(t, _)| t.contains(needle))
                .map(|(_, s)| s.clone())
                .unwrap_or_else(|| panic!("no text box containing {:?}", needle))
        };
        // html -> body -> h1 (unitless factor inherits as the number, re-resolved per font-size)
        assert!(
            matches!(get("Head"), rustkit_css::LineHeight::Number(n) if (n - 1.5).abs() < 1e-4),
            "h1 line-height did not inherit html's 1.5: {:?}",
            get("Head")
        );
        // html -> body -> div -> p (multi-level element inheritance)
        assert!(
            matches!(get("Para"), rustkit_css::LineHeight::Number(n) if (n - 1.5).abs() < 1e-4),
            "p line-height did not inherit through the div: {:?}",
            get("Para")
        );
    }

    #[test]
    fn test_parse_length() {
        assert_eq!(parse_length("0"), Some(rustkit_css::Length::Zero));
        assert_eq!(parse_length("auto"), Some(rustkit_css::Length::Auto));
        assert_eq!(parse_length("10px"), Some(rustkit_css::Length::Px(10.0)));
        assert_eq!(parse_length("1.5em"), Some(rustkit_css::Length::Em(1.5)));
        assert_eq!(parse_length("2rem"), Some(rustkit_css::Length::Rem(2.0)));
        assert_eq!(
            parse_length("50%"),
            Some(rustkit_css::Length::Percent(50.0))
        );
    }

    #[test]
    fn test_parse_min_max_clamp() {
        // Test min()
        if let Some(rustkit_css::Length::Min(pair)) = parse_length("min(100px, 50%)") {
            assert_eq!(pair.0, rustkit_css::Length::Px(100.0));
            assert_eq!(pair.1, rustkit_css::Length::Percent(50.0));
        } else {
            panic!("Failed to parse min()");
        }

        // Test max()
        if let Some(rustkit_css::Length::Max(pair)) = parse_length("max(200px, 30%)") {
            assert_eq!(pair.0, rustkit_css::Length::Px(200.0));
            assert_eq!(pair.1, rustkit_css::Length::Percent(30.0));
        } else {
            panic!("Failed to parse max()");
        }

        // Test clamp()
        if let Some(rustkit_css::Length::Clamp(triple)) = parse_length("clamp(100px, 50%, 300px)") {
            assert_eq!(triple.0, rustkit_css::Length::Px(100.0));
            assert_eq!(triple.1, rustkit_css::Length::Percent(50.0));
            assert_eq!(triple.2, rustkit_css::Length::Px(300.0));
        } else {
            panic!("Failed to parse clamp()");
        }
    }

    #[test]
    fn test_parse_transform() {
        // Test translateX
        let transform = parse_transform("translateX(10px)").unwrap();
        assert_eq!(transform.ops.len(), 1);
        if let rustkit_css::TransformOp::TranslateX(x) = &transform.ops[0] {
            assert_eq!(*x, rustkit_css::Length::Px(10.0));
        } else {
            panic!("Expected TranslateX");
        }

        // Test scale
        let transform = parse_transform("scale(1.5)").unwrap();
        assert_eq!(transform.ops.len(), 1);
        if let rustkit_css::TransformOp::Scale(sx, sy) = transform.ops[0] {
            assert_eq!(sx, 1.5);
            assert_eq!(sy, 1.5);
        } else {
            panic!("Expected Scale");
        }

        // Test rotate
        let transform = parse_transform("rotate(45deg)").unwrap();
        assert_eq!(transform.ops.len(), 1);
        if let rustkit_css::TransformOp::Rotate(angle) = transform.ops[0] {
            assert!((angle - 45.0).abs() < 0.01);
        } else {
            panic!("Expected Rotate");
        }

        // Test multiple transforms
        let transform = parse_transform("translateX(10px) scale(2) rotate(90deg)").unwrap();
        assert_eq!(transform.ops.len(), 3);
    }

    #[test]
    fn test_parse_transform_origin() {
        // Test center
        let origin = parse_transform_origin("center").unwrap();
        assert_eq!(origin.x, rustkit_css::Length::Percent(50.0));
        assert_eq!(origin.y, rustkit_css::Length::Percent(50.0));

        // Test top left
        let origin = parse_transform_origin("top left").unwrap();
        assert_eq!(origin.x, rustkit_css::Length::Percent(0.0));
        assert_eq!(origin.y, rustkit_css::Length::Percent(0.0));

        // Test pixel values
        let origin = parse_transform_origin("10px 20px").unwrap();
        assert_eq!(origin.x, rustkit_css::Length::Px(10.0));
        assert_eq!(origin.y, rustkit_css::Length::Px(20.0));
    }

    #[test]
    fn test_parse_timing_function() {
        assert!(matches!(
            parse_timing_function("ease"),
            rustkit_css::TimingFunction::Ease
        ));
        assert!(matches!(
            parse_timing_function("linear"),
            rustkit_css::TimingFunction::Linear
        ));
        assert!(matches!(
            parse_timing_function("ease-in"),
            rustkit_css::TimingFunction::EaseIn
        ));
        assert!(matches!(
            parse_timing_function("ease-out"),
            rustkit_css::TimingFunction::EaseOut
        ));

        // Test cubic-bezier
        if let rustkit_css::TimingFunction::CubicBezier(x1, y1, x2, y2) =
            parse_timing_function("cubic-bezier(0.1, 0.2, 0.3, 0.4)")
        {
            assert!((x1 - 0.1).abs() < 0.01);
            assert!((y1 - 0.2).abs() < 0.01);
            assert!((x2 - 0.3).abs() < 0.01);
            assert!((y2 - 0.4).abs() < 0.01);
        } else {
            panic!("Expected CubicBezier");
        }
    }

    #[test]
    fn test_engine_config_for_parity() {
        let config = EngineConfig::for_parity_testing();
        assert!(config.disable_animations);
    }

    #[test]
    fn test_parse_linear_gradient() {
        // Test simple linear gradient
        let gradient = parse_gradient("linear-gradient(to right, #ff0000 0%, #0000ff 100%)");
        assert!(gradient.is_some(), "Should parse simple linear gradient");

        if let Some(rustkit_css::Gradient::Linear(linear)) = gradient {
            assert_eq!(linear.direction, rustkit_css::GradientDirection::ToRight);
            assert_eq!(linear.stops.len(), 2);
            assert_eq!(
                linear.stops[0].color,
                rustkit_css::Color::from_rgb(255, 0, 0)
            );
            assert_eq!(
                linear.stops[0].position,
                Some(rustkit_css::StopPosition::Percent(0.0))
            );
            assert_eq!(
                linear.stops[1].color,
                rustkit_css::Color::from_rgb(0, 0, 255)
            );
            assert_eq!(
                linear.stops[1].position,
                Some(rustkit_css::StopPosition::Percent(1.0))
            );
        } else {
            panic!("Expected Linear gradient");
        }

        // Test with angle
        let gradient = parse_gradient("linear-gradient(45deg, red 0%, blue 100%)");
        assert!(gradient.is_some(), "Should parse gradient with angle");

        if let Some(rustkit_css::Gradient::Linear(linear)) = gradient {
            assert!(
                matches!(linear.direction, rustkit_css::GradientDirection::Angle(a) if (a - 45.0).abs() < 0.01)
            );
        } else {
            panic!("Expected Linear gradient with angle");
        }

        // Test default direction (to bottom)
        let gradient = parse_gradient("linear-gradient(#667eea, #764ba2)");
        assert!(
            gradient.is_some(),
            "Should parse gradient without direction"
        );

        if let Some(rustkit_css::Gradient::Linear(linear)) = gradient {
            assert_eq!(linear.direction, rustkit_css::GradientDirection::ToBottom);
        } else {
            panic!("Expected Linear gradient with default direction");
        }
    }

    #[test]
    fn test_parse_radial_gradient() {
        // Test simple radial gradient
        let gradient =
            parse_gradient("radial-gradient(circle at center, #667eea 0%, #764ba2 100%)");
        assert!(gradient.is_some(), "Should parse radial gradient");

        if let Some(rustkit_css::Gradient::Radial(radial)) = gradient {
            assert_eq!(radial.shape, rustkit_css::RadialShape::Circle);
            assert_eq!(radial.stops.len(), 2);
        } else {
            panic!("Expected Radial gradient");
        }

        // Test ellipse
        let gradient =
            parse_gradient("radial-gradient(ellipse at top left, #f093fb 0%, #f5576c 100%)");
        assert!(gradient.is_some(), "Should parse ellipse radial gradient");

        if let Some(rustkit_css::Gradient::Radial(radial)) = gradient {
            assert_eq!(radial.shape, rustkit_css::RadialShape::Ellipse);
            assert!(
                (radial.center.0 - 0.0).abs() < 0.01,
                "center.0 should be 0.0 for left"
            );
            assert!(
                (radial.center.1 - 0.0).abs() < 0.01,
                "center.1 should be 0.0 for top"
            );
        } else {
            panic!("Expected Radial gradient with ellipse");
        }

        // Regression: `at top right` is an ASYMMETRIC keyword position, so it
        // exposes the axis-routing bug that `top left` (swap-invariant) hides.
        // `top right` must be x=right=1.0, y=top=0.0 — NOT (0.0,1.0)=bottom-left.
        // This is the settings-page body glow (radial ... at top right, #0f172a).
        let center_of = |css: &str| -> (f32, f32) {
            match parse_gradient(css) {
                Some(rustkit_css::Gradient::Radial(r)) => r.center,
                other => panic!("expected radial gradient, got {:?}", other.is_some()),
            }
        };
        let approx =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01;

        let tr = center_of("radial-gradient(circle at top right, #06b6d4, transparent 40%)");
        assert!(
            approx(tr, (1.0, 0.0)),
            "at top right => (1.0,0.0), got {:?}",
            tr
        );

        // Keyword order must not matter: `right top` == `top right`.
        let rt = center_of("radial-gradient(circle at right top, #06b6d4, transparent 40%)");
        assert!(
            approx(rt, (1.0, 0.0)),
            "at right top => (1.0,0.0), got {:?}",
            rt
        );

        // `bottom left` — the other asymmetric pair.
        let bl = center_of("radial-gradient(circle at bottom left, #06b6d4, transparent)");
        assert!(
            approx(bl, (0.0, 1.0)),
            "at bottom left => (0.0,1.0), got {:?}",
            bl
        );

        // `center right` => right-center; `top center` => top-center.
        let cr = center_of("radial-gradient(circle at center right, #06b6d4, transparent)");
        assert!(
            approx(cr, (1.0, 0.5)),
            "at center right => (1.0,0.5), got {:?}",
            cr
        );
        let tc = center_of("radial-gradient(circle at top center, #06b6d4, transparent)");
        assert!(
            approx(tc, (0.5, 0.0)),
            "at top center => (0.5,0.0), got {:?}",
            tc
        );

        // Numeric values stay positional (first = x, second = y).
        let pct = center_of("radial-gradient(circle at 25% 75%, #06b6d4, transparent)");
        assert!(
            approx(pct, (0.25, 0.75)),
            "at 25% 75% => (0.25,0.75), got {:?}",
            pct
        );
    }

    #[test]
    fn test_parse_color_stop() {
        // Test color with percentage position
        let stop = parse_color_stop("#ff0000 50%");
        assert!(stop.is_some());
        let stop = stop.unwrap();
        assert_eq!(stop.color, rustkit_css::Color::from_rgb(255, 0, 0));
        assert_eq!(stop.position, Some(rustkit_css::StopPosition::Percent(0.5)));

        // Test color without position
        let stop = parse_color_stop("blue");
        assert!(stop.is_some());
        let stop = stop.unwrap();
        assert_eq!(stop.color, rustkit_css::Color::from_rgb(0, 0, 255));
        assert_eq!(stop.position, None);

        // Test rgba color with position
        let stop = parse_color_stop("rgba(255, 255, 255, 0.5) 25%");
        assert!(stop.is_some());
        let stop = stop.unwrap();
        assert_eq!(stop.color.r, 255);
        assert_eq!(stop.color.g, 255);
        assert_eq!(stop.color.b, 255);
        assert!((stop.color.a - 0.5).abs() < 0.01);
        assert_eq!(
            stop.position,
            Some(rustkit_css::StopPosition::Percent(0.25))
        );
    }

    #[test]
    fn test_split_by_comma() {
        // Simple case
        let parts = split_by_comma("a, b, c");
        assert_eq!(parts, vec!["a", " b", " c"]);

        // With nested parentheses
        let parts = split_by_comma("rgb(255, 0, 0), blue, rgba(0, 255, 0, 0.5)");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "rgb(255, 0, 0)");
        assert_eq!(parts[1].trim(), "blue");
        assert_eq!(parts[2].trim(), "rgba(0, 255, 0, 0.5)");
    }

    #[test]
    fn test_selector_specificity() {
        // Create a minimal engine for testing
        let compositor = match Compositor::new() {
            Ok(c) => c,
            Err(_) => {
                eprintln!("Skipping test: GPU not available");
                return;
            }
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine {
            config: EngineConfig::default(),
            views: HashMap::new(),
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };

        // Test type selector: (0, 0, 1)
        assert_eq!(engine.selector_specificity("div"), (0, 0, 1));
        assert_eq!(engine.selector_specificity("p"), (0, 0, 1));

        // Test class selector: (0, 1, 0)
        assert_eq!(engine.selector_specificity(".class"), (0, 1, 0));
        assert_eq!(engine.selector_specificity(".a.b"), (0, 2, 0));

        // Test ID selector: (1, 0, 0)
        assert_eq!(engine.selector_specificity("#id"), (1, 0, 0));

        // Test combined selectors
        assert_eq!(engine.selector_specificity("div.class"), (0, 1, 1));
        assert_eq!(engine.selector_specificity("div#id"), (1, 0, 1));
        assert_eq!(engine.selector_specificity("#id.class"), (1, 1, 0));

        // Test pseudo-classes: (0, 1, 0) each
        assert_eq!(engine.selector_specificity(":hover"), (0, 1, 0));
        assert_eq!(engine.selector_specificity(":first-child"), (0, 1, 0));
        assert_eq!(engine.selector_specificity("div:first-child"), (0, 1, 1));

        // Test pseudo-elements: (0, 0, 1) each
        assert_eq!(engine.selector_specificity("::before"), (0, 0, 1));
        assert_eq!(engine.selector_specificity("div::before"), (0, 0, 2));

        // Test attribute selectors: (0, 1, 0) each
        assert_eq!(engine.selector_specificity("[type]"), (0, 1, 0));
        assert_eq!(engine.selector_specificity("[type=text]"), (0, 1, 0));
        assert_eq!(engine.selector_specificity("input[type=text]"), (0, 1, 1));

        // Test descendant selectors
        assert_eq!(engine.selector_specificity("body div"), (0, 0, 2));
        assert_eq!(engine.selector_specificity("body .class"), (0, 1, 1));
        assert_eq!(engine.selector_specificity("#id .class div"), (1, 1, 1));

        // Test :not() - adds specificity of argument
        assert_eq!(engine.selector_specificity(":not(.class)"), (0, 1, 0));
        assert_eq!(engine.selector_specificity("div:not(.class)"), (0, 1, 1));

        // Test universal selector: (0, 0, 0)
        assert_eq!(engine.selector_specificity("*"), (0, 0, 0));

        // Test complex selectors
        assert_eq!(engine.selector_specificity("div.a.b#id:hover"), (1, 3, 1));

        // Test ID beats multiple classes
        let id_spec = engine.selector_specificity("#test");
        let multi_class_spec = engine.selector_specificity(".a.b.c.d.e");
        assert!(
            id_spec > multi_class_spec,
            "ID should beat multiple classes"
        );
    }
}

#[cfg(test)]
mod grad_suffix_tests {
    use super::*;

    // `grad` ends with `rad`. parse_angle tested `rad` FIRST, so every gradian
    // angle matched the radian arm, had three characters stripped instead of
    // four, failed to parse, and returned None — and the `grad` arm below it
    // was unreachable dead code that looked like support.
    //
    // Silent None, not a wrong number: `transform: rotate(200grad)` did not
    // rotate at all. Found by Athena on Windows (#48); macOS carried it
    // verbatim. Same shape as the rem-before-em bug she fixed in #38 — a
    // shorter unit suffix tested before a longer one that ends with it.

    #[test]
    fn grad_is_not_swallowed_by_the_rad_arm() {
        // 200grad == 180deg. If `rad` wins, this is None.
        assert_eq!(parse_angle("200grad"), Some(180.0));
        assert_eq!(parse_angle("100grad"), Some(90.0));
        assert_eq!(parse_angle("400grad"), Some(360.0));
    }

    #[test]
    fn rad_still_works_after_the_reorder() {
        let half_turn = parse_angle("3.14159rad").expect("rad must still parse");
        assert!((half_turn - 180.0).abs() < 0.01, "got {half_turn}");
    }

    #[test]
    fn the_other_angle_units_are_unaffected() {
        assert_eq!(parse_angle("90deg"), Some(90.0));
        assert_eq!(parse_angle("0.5turn"), Some(180.0));
        assert_eq!(parse_angle("45"), Some(45.0), "bare number defaults to deg");
    }

    #[test]
    fn every_unit_round_trips_to_the_same_quarter_turn() {
        // One angle, five spellings. If any suffix is being eaten by another,
        // exactly one of these disagrees — which is the property a per-unit
        // test cannot see.
        for input in ["90deg", "100grad", "0.25turn", "1.5708rad", "90"] {
            let got = parse_angle(input)
                .unwrap_or_else(|| panic!("{input} did not parse at all"));
            assert!(
                (got - 90.0).abs() < 0.01,
                "{input} parsed to {got}, expected ~90"
            );
        }
    }
}
