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
use rustkit_css::{ComputedStyle, Stylesheet, Rule, parse_display};
use rustkit_dom::{Document, Node, NodeType};
use rustkit_image::ImageManager;
use rustkit_js::JsRuntime;
use rustkit_layout::{BoxType, Dimensions, DisplayList, LayoutBox, Rect};
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
    FaviconDetected {
        view_id: EngineViewId,
        url: Url,
    },
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
        ).map_err(|e| EngineError::RenderError(e.to_string()))?;

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
        let viewhost_id = <ViewHost as ViewHostTrait>::create_view(
            &self.viewhost,
            parent,
            bounds,
        )
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
        let viewhost_id = <ViewHost as ViewHostTrait>::create_view(
            &self.viewhost,
            parent,
            bounds,
        )
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
        let raw_handle = <ViewHost as ViewHostTrait>::get_raw_window_handle(&self.viewhost, viewhost_id)
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
    pub fn create_headless_view(
        &mut self,
        bounds: Bounds,
    ) -> Result<EngineViewId, EngineError> {
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
            self.compositor
                .destroy_headless_texture(viewhost_id)
                .ok(); // Ignore errors if it doesn't exist

            // Create new texture with new size
            self.compositor
                .create_headless_texture(viewhost_id, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;

            // Update headless_bounds in view state
            let view = self.views.get_mut(&id).unwrap();
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
        if self.views.get(&id).unwrap().document.is_some() {
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
    pub fn scroll_view(&mut self, id: EngineViewId, delta_x: f32, delta_y: f32) -> Result<bool, EngineError> {
        let view = self.views.get_mut(&id).ok_or(EngineError::ViewNotFound(id))?;
        
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
    pub fn set_scroll_offset(&mut self, id: EngineViewId, x: f32, y: f32) -> Result<(), EngineError> {
        let view = self.views.get_mut(&id).ok_or(EngineError::ViewNotFound(id))?;
        
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
            let view = self.views.get_mut(&id).unwrap();
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
        let view = self.views.get_mut(&id).unwrap();
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
        let view = self.views.get_mut(&id).unwrap();
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

            let view = self.views.get_mut(&id).unwrap();
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
        let view = self.views.get_mut(&id).unwrap();
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
        let view = self.views.get_mut(&id).unwrap();
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

            let view = self.views.get_mut(&id).unwrap();
            view.bindings = Some(bindings);
        }

        // Layout and render
        self.relayout(id)?;

        // Finish navigation
        let view = self.views.get_mut(&id).unwrap();
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
        let external_stylesheets = self.views.get(&id)
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
            root_box.layout(&containing_block);
        }

        // Ensure body element fills viewport (common browser behavior)
        // If body has zero or minimal height, extend it to viewport height
        if !root_box.children.is_empty() {
            let body_box = &mut root_box.children[0];
            if body_box.dimensions.content.height < 1.0 {
                // Body is empty or has no content - fill viewport
                body_box.dimensions.content.height = bounds.height as f32;
                debug!("Extended empty body to fill viewport height: {}px", bounds.height);
            }
        }

        // Debug: log the layout box tree AFTER layout
        fn debug_layout_box(box_: &LayoutBox, depth: usize) {
            if depth > 5 { return; } // Limit depth
            let indent = "  ".repeat(depth);
            let bg = box_.style.background_color;
            let dims = &box_.dimensions;
            tracing::debug!(
                "{}[{:?}] bg=rgba({},{},{},{:.1}) dims=({:.0}x{:.0} @ {:.0},{:.0}) children={}",
                indent,
                box_.box_type,
                bg.r, bg.g, bg.b, bg.a,
                dims.content.width, dims.content.height,
                dims.content.x, dims.content.y,
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
        let view = self.views.get_mut(&id).unwrap();
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
        if !matches!(style.width, rustkit_css::Length::Auto) ||
           !matches!(style.height, rustkit_css::Length::Auto) {
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

        // Check for borders
        if !matches!(style.border_top_width, rustkit_css::Length::Px(0.0)) ||
           !matches!(style.border_right_width, rustkit_css::Length::Px(0.0)) ||
           !matches!(style.border_bottom_width, rustkit_css::Length::Px(0.0)) ||
           !matches!(style.border_left_width, rustkit_css::Length::Px(0.0)) {
            return true;
        }

        // Check for padding (creates visual space)
        if !matches!(style.padding_top, rustkit_css::Length::Px(0.0)) ||
           !matches!(style.padding_right, rustkit_css::Length::Px(0.0)) ||
           !matches!(style.padding_bottom, rustkit_css::Length::Px(0.0)) ||
           !matches!(style.padding_left, rustkit_css::Length::Px(0.0)) {
            return true;
        }

        false
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
    fn build_layout_from_document(&self, document: &Document, external_stylesheets: &[Stylesheet]) -> LayoutBox {
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

        // Get the body element and build layout from it
        if let Some(body) = document.body() {
            debug!("Found body element, building layout with stylesheets");
            let body_box = self.build_layout_from_node_with_styles(&body, &stylesheets, &css_vars, &[]);
            root_box.children.push(body_box);
        } else if let Some(html) = document.document_element() {
            // Fallback: use html element if no body
            debug!("No body found, using html element");
            let html_box = self.build_layout_from_node_with_styles(&html, &stylesheets, &css_vars, &[]);
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
        ancestors: &[String],
    ) -> LayoutBox {
        self.build_layout_from_node_with_parent_style(node, stylesheets, css_vars, ancestors, None)
    }
    
    fn build_layout_from_node_with_parent_style(
        &self,
        node: &Rc<Node>,
        stylesheets: &[Stylesheet],
        css_vars: &HashMap<String, String>,
        ancestors: &[String],
        parent_style: Option<&ComputedStyle>,
    ) -> LayoutBox {
        match &node.node_type {
            NodeType::Element { tag_name, attributes, .. } => {
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
                let style = self.compute_style_for_element(tag_name, attributes, stylesheets, css_vars, ancestors);
                
                // Check for display: none
                if style.display == rustkit_css::Display::None {
                    return LayoutBox::new(BoxType::Block, ComputedStyle::new());
                }

                // Handle replaced elements (images)
                if tag_lower == "img" {
                    let src = attributes.get("src").cloned().unwrap_or_default();
                    
                    // Parse explicit dimensions from attributes
                    let explicit_width: Option<f32> = attributes.get("width")
                        .and_then(|w| w.parse().ok());
                    let explicit_height: Option<f32> = attributes.get("height")
                        .and_then(|h| h.parse().ok());
                    
                    // For now, use explicit dimensions or defaults
                    // Real implementation would load image to get natural size
                    let (natural_width, natural_height) = match (explicit_width, explicit_height) {
                        (Some(w), Some(h)) => (w, h),
                        (Some(w), None) => (w, w),  // Assume square if only width
                        (None, Some(h)) => (h, h),  // Assume square if only height
                        (None, None) => (150.0, 150.0),  // Default placeholder size
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
                    let input_type = attributes.get("type").cloned().unwrap_or_else(|| "text".to_string());
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
                        attributes.get("value").cloned().unwrap_or_else(|| "Button".to_string())
                    } else {
                        text
                    };
                    let button_type = attributes.get("type").cloned().unwrap_or_else(|| "button".to_string());
                    
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
                    let rows = attributes.get("rows").and_then(|r| r.parse().ok()).unwrap_or(3);
                    let cols = attributes.get("cols").and_then(|c| c.parse().ok()).unwrap_or(20);
                    
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
                    let options: Vec<String> = node.children()
                        .into_iter()
                        .filter_map(|child| {
                            if let rustkit_dom::NodeType::Element { tag_name, .. } = &child.node_type {
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
                    
                    return LayoutBox::new(
                        BoxType::FormControl(rustkit_layout::FormControlType::Select {
                            options,
                            selected_index,
                        }),
                        style,
                    );
                }
                
                // Determine box type based on tag for non-replaced elements
                let is_inline = matches!(
                    tag_lower.as_str(),
                    "a" | "span" | "strong" | "b" | "em" | "i" | "u" | "code" | "small" | "big" | "sub" | "sup" | "abbr" | "cite" | "q" | "mark" | "label"
                );

                let box_type = if is_inline {
                    BoxType::Inline
                } else {
                    BoxType::Block
                };

                let mut layout_box = LayoutBox::new(box_type, style.clone());

                // Build ancestors list for child elements
                let mut child_ancestors = ancestors.to_vec();
                child_ancestors.push(tag_lower.clone());

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

                // Process children
                for child in node.children() {
                    let child_box = self.build_layout_from_node_with_parent_style(&child, stylesheets, css_vars, &child_ancestors, Some(&style));

                    // Determine if box should be included in layout tree
                    let should_include = match child_box.box_type {
                        BoxType::Block | BoxType::AnonymousBlock => {
                            // Include blocks if they have children, OR have visible styling
                            !child_box.children.is_empty() ||
                            Self::has_visible_styling(&child_box.style)
                        }
                        BoxType::Inline => {
                            // Include inline boxes if they have content children (text, images, form controls)
                            // or have visible styling (padding, border, background)
                            Self::has_content_children(&child_box) ||
                            Self::has_visible_styling(&child_box.style)
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

                layout_box
            }
            NodeType::Text(text) => {
                // Create text box for non-empty text
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    // Skip whitespace-only text - return an inline box that won't be included
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
                        s
                    } else {
                        let mut s = ComputedStyle::new();
                        s.color = rustkit_css::Color::BLACK;
                        s
                    };
                    LayoutBox::new(BoxType::Text(trimmed.to_string()), style)
                }
            }
            _ => {
                // For other node types (Document, Comment, etc.), return empty box
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
        ancestors: &[String],
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
                    if self.selector_matches(base_selector.trim(), tag_name, attributes, ancestors, &[], 0, 1) {
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
        ancestors: &[String],
    ) -> ComputedStyle {
        let mut style = ComputedStyle::new();
        style.color = rustkit_css::Color::BLACK;

        // Apply tag-specific default styles (user-agent stylesheet)
        match tag_name.to_lowercase().as_str() {
            "body" => {
                style.background_color = rustkit_css::Color::WHITE;
                style.margin_top = rustkit_css::Length::Px(8.0);
                style.margin_right = rustkit_css::Length::Px(8.0);
                style.margin_bottom = rustkit_css::Length::Px(8.0);
                style.margin_left = rustkit_css::Length::Px(8.0);
            }
            "h1" => {
                style.font_size = rustkit_css::Length::Px(32.0);
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(21.44);
                style.margin_bottom = rustkit_css::Length::Px(21.44);
            }
            "h2" => {
                style.font_size = rustkit_css::Length::Px(24.0);
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(19.92);
                style.margin_bottom = rustkit_css::Length::Px(19.92);
            }
            "h3" => {
                style.font_size = rustkit_css::Length::Px(18.72);
                style.font_weight = rustkit_css::FontWeight::BOLD;
                style.margin_top = rustkit_css::Length::Px(18.72);
                style.margin_bottom = rustkit_css::Length::Px(18.72);
            }
            "p" => {
                style.margin_top = rustkit_css::Length::Px(16.0);
                style.margin_bottom = rustkit_css::Length::Px(16.0);
            }
            "div" => {
                // Block element with no special styling
            }
            "a" => {
                style.color = rustkit_css::Color::new(0, 0, 238, 1.0); // Blue
            }
            "strong" | "b" => {
                style.font_weight = rustkit_css::FontWeight::BOLD;
            }
            "em" | "i" => {
                style.font_style = rustkit_css::FontStyle::Italic;
            }
            "pre" | "code" => {
                style.font_family = "monospace".to_string();
            }
            "ul" | "ol" => {
                style.margin_top = rustkit_css::Length::Px(16.0);
                style.margin_bottom = rustkit_css::Length::Px(16.0);
                style.padding_left = rustkit_css::Length::Px(40.0);
            }
            "li" => {
                // List items are blocks
            }
            "blockquote" => {
                style.margin_top = rustkit_css::Length::Px(16.0);
                style.margin_bottom = rustkit_css::Length::Px(16.0);
                style.margin_left = rustkit_css::Length::Px(40.0);
                style.margin_right = rustkit_css::Length::Px(40.0);
            }
            "hr" => {
                style.border_top_width = rustkit_css::Length::Px(1.0);
                style.border_top_color = rustkit_css::Color::new(128, 128, 128, 1.0);
                style.margin_top = rustkit_css::Length::Px(8.0);
                style.margin_bottom = rustkit_css::Length::Px(8.0);
            }
            _ => {}
        }

        // Collect matching rules with specificity for ordering
        let mut matching_rules: Vec<(&Rule, (usize, usize, usize), usize)> = Vec::new();
        let mut rule_index = 0;
        
        // For now, we don't track siblings during style computation
        // TODO: Pass sibling info from build_layout_from_node_with_styles
        let empty_siblings: Vec<(String, Vec<String>, Option<String>)> = Vec::new();
        let element_index = 0;
        let sibling_count = 1;
        
        for stylesheet in stylesheets {
            for rule in &stylesheet.rules {
                if self.selector_matches(
                    &rule.selector,
                    tag_name,
                    attributes,
                    ancestors,
                    &empty_siblings,
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
                    trace!(property = decl.property.as_str(), original = value_str.as_str(), resolved = resolved_value.as_str(), "Resolved CSS variable");
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
    fn apply_inline_style(&self, style: &mut ComputedStyle, style_attr: &str, css_vars: &HashMap<String, String>) {
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
                    "background-color" | "background" | "background-image" => {
                        debug!(value = value, "Applying background");
                        // Check for gradient first
                        if let Some(gradient) = parse_gradient(value) {
                            debug!("Parsed gradient background");
                            style.background_gradient = Some(gradient);
                        } else if let Some(color) = parse_color(value) {
                            debug!(?color, "Parsed background color");
                            style.background_color = color;
                        } else {
                            debug!("Failed to parse background");
                        }
                    }
                    "font-size" => {
                        if let Some(length) = parse_length(value) {
                            style.font_size = length;
                        }
                    }
                    "font-weight" => {
                        if value == "bold" || value == "700" || value == "800" || value == "900" {
                            style.font_weight = rustkit_css::FontWeight::BOLD;
                } else if value == "normal" || value == "400" {
                    style.font_weight = rustkit_css::FontWeight::NORMAL;
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
                // line_height is an f32 (multiplier), parse it
                if let Ok(lh) = value.parse::<f32>() {
                    style.line_height = lh;
                } else if let Some(length) = parse_length(value) {
                    // Convert length to a multiplier (very rough approximation)
                    match length {
                        rustkit_css::Length::Px(px) => style.line_height = px / 16.0,
                        rustkit_css::Length::Em(em) => style.line_height = em,
                        rustkit_css::Length::Percent(pct) => style.line_height = pct / 100.0,
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
            "border" | "border-width" => {
                if let Some(length) = parse_length(value) {
                    style.border_top_width = length.clone();
                    style.border_right_width = length.clone();
                    style.border_bottom_width = length.clone();
                    style.border_left_width = length;
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
                        rustkit_css::Length::Px(px) => style.flex_basis = rustkit_css::FlexBasis::Length(px),
                        rustkit_css::Length::Percent(pct) => style.flex_basis = rustkit_css::FlexBasis::Percent(pct),
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
                            rustkit_css::Length::Px(px) => style.flex_basis = rustkit_css::FlexBasis::Length(px),
                            rustkit_css::Length::Percent(pct) => style.flex_basis = rustkit_css::FlexBasis::Percent(pct),
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
            "text-align" => {
                // Store text-align if ComputedStyle supports it
                // For now, just ignore
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
                        if let (Some(tb), Some(lr)) = (parse_length(parts[0]), parse_length(parts[1])) {
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
                    "underline" => style.text_decoration_line = rustkit_css::TextDecorationLine::UNDERLINE,
                    "overline" => style.text_decoration_line = rustkit_css::TextDecorationLine::OVERLINE,
                    "line-through" => style.text_decoration_line = rustkit_css::TextDecorationLine::LINE_THROUGH,
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
                            "infinite" => style.animation_iteration_count = rustkit_css::AnimationIterationCount::Infinite,
                            "normal" => style.animation_direction = rustkit_css::AnimationDirection::Normal,
                            "reverse" => style.animation_direction = rustkit_css::AnimationDirection::Reverse,
                            "alternate" => style.animation_direction = rustkit_css::AnimationDirection::Alternate,
                            "alternate-reverse" => style.animation_direction = rustkit_css::AnimationDirection::AlternateReverse,
                            "forwards" => style.animation_fill_mode = rustkit_css::AnimationFillMode::Forwards,
                            "backwards" => style.animation_fill_mode = rustkit_css::AnimationFillMode::Backwards,
                            "both" => style.animation_fill_mode = rustkit_css::AnimationFillMode::Both,
                            "paused" => style.animation_play_state = rustkit_css::AnimationPlayState::Paused,
                            "running" => style.animation_play_state = rustkit_css::AnimationPlayState::Running,
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
                    style.animation_iteration_count = rustkit_css::AnimationIterationCount::Infinite;
                } else if let Ok(n) = v.parse::<f32>() {
                    style.animation_iteration_count = rustkit_css::AnimationIterationCount::Count(n);
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
                    style.content = Some(v[1..v.len()-1].to_string());
                } else if v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2 {
                    // Single-quoted string content
                    style.content = Some(v[1..v.len()-1].to_string());
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
            "line-height" => style.line_height = 1.2,
            "margin" | "margin-top" => style.margin_top = rustkit_css::Length::Zero,
            "margin-right" => style.margin_right = rustkit_css::Length::Zero,
            "margin-bottom" => style.margin_bottom = rustkit_css::Length::Zero,
            "margin-left" => style.margin_left = rustkit_css::Length::Zero,
            "padding" | "padding-top" => style.padding_top = rustkit_css::Length::Zero,
            "padding-right" => style.padding_right = rustkit_css::Length::Zero,
            "padding-bottom" => style.padding_bottom = rustkit_css::Length::Zero,
            "padding-left" => style.padding_left = rustkit_css::Length::Zero,
            "border-width" | "border-top-width" => style.border_top_width = rustkit_css::Length::Zero,
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
    fn discover_external_stylesheets(&self, document: &Document, base_url: Option<&Url>) -> Vec<Url> {
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
    pub async fn load_external_stylesheets(&mut self, id: EngineViewId) -> Result<Vec<Stylesheet>, EngineError> {
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
                            Ok(css_text) => {
                                match Stylesheet::parse(&css_text) {
                                    Ok(stylesheet) => {
                                        debug!(rules = stylesheet.rules.len(), %url, "Parsed external stylesheet");
                                        stylesheets.push(stylesheet);
                                    }
                                    Err(e) => {
                                        warn!(?e, %url, "Failed to parse external stylesheet");
                                    }
                                }
                            }
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
        
        for (_src, url) in images {
            info!(%url, "Loading image");
            
            match self.loader.fetch(Request::get(url.clone())).await {
                Ok(response) => {
                    if response.ok() {
                        match response.bytes().await {
                            Ok(bytes) => {
                                // Store in image cache (to be implemented)
                                debug!(len = bytes.len(), %url, "Image loaded");
                                loaded += 1;
                            }
                            Err(e) => {
                                warn!(?e, %url, "Failed to read image body");
                            }
                        }
                    } else {
                        warn!(status = %response.status, %url, "Failed to fetch image");
                    }
                }
                Err(e) => {
                    warn!(?e, %url, "Failed to fetch image");
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
            info!(count = external_stylesheets.len(), "Loaded external stylesheets");
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
                    (var_content[..comma_pos].trim(), Some(var_content[comma_pos + 1..].trim()))
                } else {
                    (var_content.trim(), None)
                };
                
                // Look up variable value
                let replacement = css_vars.get(var_name)
                    .map(|s| s.as_str())
                    .or(fallback)
                    .unwrap_or("");
                
                // Replace var(...) with the resolved value
                result = format!("{}{}{}", &result[..start], replacement, &after_var[end + 1..]);
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
        ancestors: &[String],
        siblings_before: &[(String, Vec<String>, Option<String>)],
        element_index: usize,
        sibling_count: usize,
    ) -> bool {
        let selector = selector.trim();
        
        // Handle multiple selectors (comma-separated)
        if selector.contains(',') {
            return selector.split(',')
                .any(|s| self.selector_matches(
                    s.trim(), tag_name, attributes, ancestors,
                    siblings_before, element_index, sibling_count
                ));
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
            &last_token.0, tag_name, attributes, element_index, sibling_count
        ) {
            return false;
        }
        
        // If there's only one token, we're done
        if tokens.len() == 1 {
            return true;
        }
        
        // Handle combinators by walking backwards through tokens
        let current_ancestors = ancestors;
        let current_siblings = siblings_before;
        
        for i in (0..tokens.len() - 1).rev() {
            let (sel_part, combinator) = &tokens[i];
            
            match combinator.as_str() {
                " " => {
                    // Descendant combinator: some ancestor must match
                    let mut found = false;
                    for ancestor in current_ancestors {
                        if self.simple_selector_matches_tag_only(sel_part, ancestor) {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return false;
                    }
                }
                ">" => {
                    // Child combinator: immediate parent must match
                    if let Some(parent) = current_ancestors.first() {
                        if !self.simple_selector_matches_tag_only(sel_part, parent) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                "+" => {
                    // Adjacent sibling combinator: immediate previous sibling must match
                    if let Some((prev_tag, _prev_classes, _prev_id)) = current_siblings.last() {
                        if !self.simple_selector_matches_tag_only(sel_part, prev_tag) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                "~" => {
                    // General sibling combinator: any previous sibling must match
                    let mut found = false;
                    for (sib_tag, _sib_classes, _sib_id) in current_siblings {
                        if self.simple_selector_matches_tag_only(sel_part, sib_tag) {
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
                            // The combinator will be handled in the next iteration
                            tokens.push((current.trim().to_string(), " ".to_string()));
                            current = String::new();
                        } else if next.is_alphanumeric() || next == '.' || next == '#' || next == '[' || next == ':' || next == '*' {
                            // Descendant combinator
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

    /// Check if a simple selector matches an element (without pseudo-class context).
    fn simple_selector_matches(&self, selector: &str, tag_name: &str, attributes: &HashMap<String, String>) -> bool {
        self.simple_selector_matches_with_pseudo(selector, tag_name, attributes, 0, 1)
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
        let tag_end = remaining.find(|c| c == '.' || c == '#' || c == ':' || c == '[')
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
                let class_end = rest.find(|c| c == '.' || c == '#' || c == ':' || c == '[')
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
                let id_end = rest.find(|c| c == '.' || c == '#' || c == ':' || c == '[')
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
                remaining = if bracket_end < rest.len() { &rest[bracket_end + 1..] } else { "" };
                
                if !self.match_attribute_selector(attr_selector, attributes) {
                    return false;
                }
            } else if let Some(rest) = remaining.strip_prefix(':') {
                // Pseudo-class
                let (pseudo_name, pseudo_arg, consumed) = self.parse_pseudo_class(rest);
                remaining = &rest[consumed..];
                
                if !self.match_pseudo_class(&pseudo_name, pseudo_arg.as_deref(), element_index, sibling_count, attributes) {
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
    fn match_attribute_selector(&self, attr_selector: &str, attributes: &HashMap<String, String>) -> bool {
        // Determine the operator
        let operators = ["~=", "|=", "^=", "$=", "*=", "="];
        
        for op in &operators {
            if let Some(pos) = attr_selector.find(op) {
                let attr_name = attr_selector[..pos].trim();
                let mut attr_value = attr_selector[pos + op.len()..].trim();
                
                // Remove quotes if present
                if (attr_value.starts_with('"') && attr_value.ends_with('"')) ||
                   (attr_value.starts_with('\'') && attr_value.ends_with('\'')) {
                    attr_value = &attr_value[1..attr_value.len() - 1];
                }
                
                if let Some(el_attr) = attributes.get(attr_name) {
                    return match *op {
                        "=" => el_attr == attr_value,
                        "~=" => el_attr.split_whitespace().any(|w| w == attr_value),
                        "|=" => el_attr == attr_value || el_attr.starts_with(&format!("{}-", attr_value)),
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
        let name_end = rest.find(|c: char| !c.is_alphanumeric() && c != '-')
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
                    // For simplicity, we only support simple selectors inside :not()
                    !self.simple_selector_matches(arg, "", attributes)
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
            "root" => false, // Handled separately
            _ => true, // Unknown pseudo-classes pass through
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

    /// Simplified selector match for ancestor checking (only checks tag and class).
    fn simple_selector_matches_tag_only(&self, selector: &str, ancestor_tag: &str) -> bool {
        if selector == "*" {
            return true;
        }
        if selector.starts_with('.') {
            return false; // Can't check class for ancestors in this simplified version
        }
        let tag_end = selector.find(|c| c == '.' || c == '#' || c == ':')
            .unwrap_or(selector.len());
        let tag_part = &selector[..tag_end];
        tag_part.is_empty() || tag_part.eq_ignore_ascii_case(ancestor_tag)
    }

    /// Calculate selector specificity for ordering.
    fn selector_specificity(&self, selector: &str) -> (usize, usize, usize) {
        let mut ids = 0;
        let mut classes = 0;
        let mut tags = 0;
        
        for part in selector.split_whitespace() {
            for segment in part.split(',') {
                let segment = segment.trim();
                // Count IDs
                ids += segment.matches('#').count();
                // Count classes and pseudo-classes
                classes += segment.matches('.').count();
                classes += segment.matches(':').count();
                // Count type selectors
                if !segment.is_empty() && 
                   !segment.starts_with('.') && 
                   !segment.starts_with('#') && 
                   !segment.starts_with(':') &&
                   segment != "*" {
                    tags += 1;
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
        let (width, height) = self.compositor
            .get_surface_size(viewhost_id)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        if width == 0 || height == 0 {
            return Err(EngineError::RenderError("Cannot capture zero-size frame".into()));
        }

        // If we have a display list and renderer, render to offscreen texture
        match (&display_list, &mut self.renderer) {
            (Some(display_list), Some(renderer)) => {
                // Update viewport size for correct coordinate transforms
                renderer.set_viewport_size(width, height);

                // Capture with actual display list rendering
                self.compositor
                    .capture_frame_with_renderer(viewhost_id, path, renderer, &display_list.commands)
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
        
        let layout = view.layout.as_ref().ok_or_else(|| {
            EngineError::RenderError("No layout tree available".into())
        })?;
        
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
                BoxType::Text(t) => return serde_json::json!({
                    "type": "text",
                    "text": t.chars().take(50).collect::<String>(),
                    "rect": {
                        "x": content.x,
                        "y": content.y,
                        "width": content.width,
                        "height": content.height
                    }
                }),
                BoxType::Image { natural_width, natural_height, .. } => return serde_json::json!({
                    "type": "image",
                    "natural_width": natural_width,
                    "natural_height": natural_height,
                    "rect": {
                        "x": content.x,
                        "y": content.y,
                        "width": content.width,
                        "height": content.height
                    }
                }),
                BoxType::FormControl(ctrl) => return serde_json::json!({
                    "type": "form_control",
                    "control_type": format!("{:?}", ctrl),
                    "rect": {
                        "x": content.x,
                        "y": content.y,
                        "width": content.width,
                        "height": content.height
                    }
                }),
            };
            
            let children: Vec<serde_json::Value> = layout_box.children
                .iter()
                .map(layout_box_to_json)
                .collect();
            
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
        let (width, height) = self.compositor
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

        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let viewhost_id = view.viewhost_id;
        let display_list = view.display_list.as_ref();
        let has_display_list = display_list.is_some();
        let cmd_count = display_list.map(|dl| dl.commands.len()).unwrap_or(0);
        let is_headless = view.headless_bounds.is_some();

        trace!(?id, has_display_list, cmd_count, is_headless, "Rendering view");

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
                renderer.execute(&display_list.commands, &texture_view)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?;
            } else if let Some(renderer) = &mut self.renderer {
                // No display list, render empty (will clear to white or debug color)
                renderer.execute(&[], &texture_view)
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
                    renderer.execute(&display_list.commands, &texture_view)
                        .map_err(|e| EngineError::RenderError(e.to_string()))?;
                } else if let Some(renderer) = &mut self.renderer {
                    // No display list, render empty (will clear to white or debug color)
                    renderer.execute(&[], &texture_view)
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
        if let (Some(_hit), Some(_document)) = (hit_result, &view.document) {
            // TODO: Map hit result to DOM node and dispatch event
            // For now, just log
            trace!(?view_id, event_type = dom_event_type, "Mouse event");
        }

        // Handle click focus change
        if event.event_type == MouseEventType::MouseDown {
            // TODO: Focus the clicked element if focusable
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
        }

        // Dispatch to focused element via DOM events
        // TODO: Dispatch KeyboardEvent to focused DOM node
    }

    /// Focus a DOM node in a view.
    pub fn focus_element(
        &mut self,
        view_id: EngineViewId,
        node_id: rustkit_dom::NodeId,
    ) -> Result<(), EngineError> {
        let view = self
            .views
            .get_mut(&view_id)
            .ok_or(EngineError::ViewNotFound(view_id))?;

        let old_focused = view.focused_node;
        view.focused_node = Some(node_id);

        // TODO: Dispatch blur event to old focused element
        // TODO: Dispatch focus event to new focused element

        debug!(?view_id, ?node_id, ?old_focused, "Focus changed");
        Ok(())
    }

    /// Blur the currently focused element.
    pub fn blur_element(&mut self, view_id: EngineViewId) -> Result<(), EngineError> {
        let view = self
            .views
            .get_mut(&view_id)
            .ok_or(EngineError::ViewNotFound(view_id))?;

        let old_focused = view.focused_node.take();

        // TODO: Dispatch blur event to old focused element

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
                Err(EngineError::RenderError(format!("Image load failed: {}", error)))
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
    let value = value.trim().to_lowercase();

    // Named colors
    match value.as_str() {
        "black" => return Some(rustkit_css::Color::BLACK),
        "white" => return Some(rustkit_css::Color::WHITE),
        "red" => return Some(rustkit_css::Color::new(255, 0, 0, 1.0)),
        "green" => return Some(rustkit_css::Color::new(0, 128, 0, 1.0)),
        "blue" => return Some(rustkit_css::Color::new(0, 0, 255, 1.0)),
        "yellow" => return Some(rustkit_css::Color::new(255, 255, 0, 1.0)),
        "cyan" => return Some(rustkit_css::Color::new(0, 255, 255, 1.0)),
        "magenta" => return Some(rustkit_css::Color::new(255, 0, 255, 1.0)),
        "gray" | "grey" => return Some(rustkit_css::Color::new(128, 128, 128, 1.0)),
        "transparent" => return Some(rustkit_css::Color::TRANSPARENT),
        _ => {}
    }

    // Hex colors
    if value.starts_with('#') {
        let hex = &value[1..];
        let (r, g, b) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                (r, g, b)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b)
            }
            _ => return None,
        };
        return Some(rustkit_css::Color::from_rgb(r, g, b));
    }

    // rgb() and rgba()
    if value.starts_with("rgb(") || value.starts_with("rgba(") {
        let inner = value
            .trim_start_matches("rgba(")
            .trim_start_matches("rgb(")
            .trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let r: u8 = parts[0].trim().parse().ok()?;
            let g: u8 = parts[1].trim().parse().ok()?;
            let b: u8 = parts[2].trim().parse().ok()?;
            let a: f32 = if parts.len() >= 4 {
                parts[3].trim().parse().ok()?
            } else {
                1.0
            };
            return Some(rustkit_css::Color::new(r, g, b, a));
        }
    }

    None
}

/// Parse a CSS gradient value (linear-gradient or radial-gradient).
fn parse_gradient(value: &str) -> Option<rustkit_css::Gradient> {
    let value = value.trim();
    
    if value.starts_with("linear-gradient(") && value.ends_with(')') {
        return parse_linear_gradient(value);
    }
    
    if value.starts_with("radial-gradient(") && value.ends_with(')') {
        return parse_radial_gradient(value);
    }
    
    None
}

/// Parse a linear-gradient CSS function.
fn parse_linear_gradient(value: &str) -> Option<rustkit_css::Gradient> {
    // Strip "linear-gradient(" and ")"
    let inner = value
        .strip_prefix("linear-gradient(")?
        .strip_suffix(')')?
        .trim();
    
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
    
    Some(rustkit_css::Gradient::Linear(rustkit_css::LinearGradient::new(direction, stops)))
}

/// Parse a radial-gradient CSS function.
fn parse_radial_gradient(value: &str) -> Option<rustkit_css::Gradient> {
    // Strip "radial-gradient(" and ")"
    let inner = value
        .strip_prefix("radial-gradient(")?
        .strip_suffix(')')?
        .trim();
    
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
                center.0 = parse_position_value(pos_parts[0]);
                center.1 = parse_position_value(pos_parts[1]);
            } else if pos_parts.len() == 1 {
                center.0 = parse_position_value(pos_parts[0]);
                center.1 = center.0;
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
    
    Some(rustkit_css::Gradient::Radial(rustkit_css::RadialGradient::new(shape, size, center, stops)))
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
        "to bottom right" | "to right bottom" => Some(rustkit_css::GradientDirection::ToBottomRight),
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
    
    let (color_str, position) = if let Some(space_idx) = last_space {
        let pos_str = &value[space_idx + 1..];
        let pos = if pos_str.ends_with('%') {
            pos_str.strip_suffix('%').and_then(|s| s.parse::<f32>().ok()).map(|p| p / 100.0)
        } else if pos_str.ends_with("px") {
            // Ignore pixel positions for now, they require container size
            None
        } else {
            None
        };
        (&value[..space_idx], pos)
    } else {
        (value, None)
    };
    
    let color = parse_color(color_str)?;
    Some(rustkit_css::ColorStop::new(color, position))
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

/// Parse a position value (percentage, keyword, or length).
fn parse_position_value(value: &str) -> f32 {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        "left" | "top" => 0.0,
        "center" => 0.5,
        "right" | "bottom" => 1.0,
        _ if value.ends_with('%') => {
            value.strip_suffix('%')
                .and_then(|s| s.parse::<f32>().ok())
                .map(|p| p / 100.0)
                .unwrap_or(0.5)
        }
        _ => 0.5,
    }
}

/// Parse a length value from CSS.
fn parse_length(value: &str) -> Option<rustkit_css::Length> {
    let value = value.trim();

    if value == "0" || value == "auto" {
        return Some(if value == "auto" {
            rustkit_css::Length::Auto
        } else {
            rustkit_css::Length::Zero
        });
    }
    
    // Handle calc() expressions (simplified)
    if value.starts_with("calc(") && value.ends_with(')') {
        return parse_calc(value);
    }
    
    // Handle min() function
    if value.starts_with("min(") && value.ends_with(')') {
        return parse_min_max_clamp(value, "min");
    }
    
    // Handle max() function
    if value.starts_with("max(") && value.ends_with(')') {
        return parse_min_max_clamp(value, "max");
    }
    
    // Handle clamp() function
    if value.starts_with("clamp(") && value.ends_with(')') {
        return parse_min_max_clamp(value, "clamp");
    }

    if value.ends_with("px") {
        let num: f32 = value.trim_end_matches("px").trim().parse().ok()?;
        return Some(rustkit_css::Length::Px(num));
    }

    // Check "rem" before "em" since "rem" ends with "em"
    if value.ends_with("rem") {
        let num: f32 = value.trim_end_matches("rem").trim().parse().ok()?;
        return Some(rustkit_css::Length::Rem(num));
    }

    if value.ends_with("em") {
        let num: f32 = value.trim_end_matches("em").trim().parse().ok()?;
        return Some(rustkit_css::Length::Em(num));
    }
    
    // Viewport units (check vmin/vmax before vh/vw since they're longer)
    if value.ends_with("vmin") {
        let num: f32 = value.trim_end_matches("vmin").trim().parse().ok()?;
        return Some(rustkit_css::Length::Vmin(num));
    }
    
    if value.ends_with("vmax") {
        let num: f32 = value.trim_end_matches("vmax").trim().parse().ok()?;
        return Some(rustkit_css::Length::Vmax(num));
    }
    
    if value.ends_with("vh") {
        let num: f32 = value.trim_end_matches("vh").trim().parse().ok()?;
        return Some(rustkit_css::Length::Vh(num));
    }
    
    if value.ends_with("vw") {
        let num: f32 = value.trim_end_matches("vw").trim().parse().ok()?;
        return Some(rustkit_css::Length::Vw(num));
    }

    if value.ends_with('%') {
        let num: f32 = value.trim_end_matches('%').trim().parse().ok()?;
        return Some(rustkit_css::Length::Percent(num));
    }

    // Bare number (treat as pixels)
    if let Ok(num) = value.parse::<f32>() {
        return Some(rustkit_css::Length::Px(num));
    }

    None
}

/// Parse a calc() expression (simplified - only handles basic patterns).
/// Supports: calc(100% - 20px), calc(50% + 10px), etc.
fn parse_calc(value: &str) -> Option<rustkit_css::Length> {
    let inner = value.strip_prefix("calc(")?.strip_suffix(')')?;
    let inner = inner.trim();
    
    // Look for + or - operator (not at the start, and not inside a number like -20px)
    let mut op_idx = None;
    let mut op_char = '+';
    let chars: Vec<char> = inner.chars().collect();
    
    for (i, &c) in chars.iter().enumerate() {
        if i == 0 {
            continue;
        }
        if (c == '+' || c == '-') && chars.get(i.saturating_sub(1)).map(|&prev| prev.is_whitespace()).unwrap_or(false) {
            op_idx = Some(i);
            op_char = c;
            break;
        }
    }
    
    if let Some(idx) = op_idx {
        let left = inner[..idx].trim();
        let right = inner[idx + 1..].trim();
        
        // For now, we can only handle simple cases where one is % and one is px
        // Return the dominant type (percent if present, otherwise first)
        if let (Some(left_len), Some(right_len)) = (parse_length(left), parse_length(right)) {
            // If left is percent and right is px, return a "Calc" type
            // For now, just return the percent part as a simplification
            match (&left_len, &right_len) {
                (rustkit_css::Length::Percent(p), rustkit_css::Length::Px(_px)) => {
                    // Can't properly represent this without a Calc type, so approximate
                    // by returning percent (the px offset will be ignored)
                    return Some(rustkit_css::Length::Percent(*p));
                }
                (rustkit_css::Length::Px(_px), rustkit_css::Length::Percent(p)) => {
                    return Some(rustkit_css::Length::Percent(*p));
                }
                (rustkit_css::Length::Px(px1), rustkit_css::Length::Px(px2)) => {
                    let result = if op_char == '+' { px1 + px2 } else { px1 - px2 };
                    return Some(rustkit_css::Length::Px(result));
                }
                _ => {
                    // Return the first value as fallback
                    return Some(left_len);
                }
            }
        }
    }
    
    // Fallback: try to parse as a single length
    parse_length(inner)
}

/// Parse min(), max(), or clamp() CSS functions.
fn parse_min_max_clamp(value: &str, func: &str) -> Option<rustkit_css::Length> {
    // Strip the function name and parentheses
    let prefix_len = func.len() + 1; // "min(" or "max(" or "clamp("
    let inner = &value[prefix_len..value.len() - 1];
    
    // Split by comma, but be careful of nested functions
    let args = split_css_args(inner);
    
    match func {
        "min" => {
            if args.len() >= 2 {
                let a = parse_length(args[0].trim())?;
                let b = parse_length(args[1].trim())?;
                Some(rustkit_css::Length::Min(Box::new((a, b))))
            } else {
                None
            }
        }
        "max" => {
            if args.len() >= 2 {
                let a = parse_length(args[0].trim())?;
                let b = parse_length(args[1].trim())?;
                Some(rustkit_css::Length::Max(Box::new((a, b))))
            } else {
                None
            }
        }
        "clamp" => {
            if args.len() >= 3 {
                let min_val = parse_length(args[0].trim())?;
                let preferred = parse_length(args[1].trim())?;
                let max_val = parse_length(args[2].trim())?;
                Some(rustkit_css::Length::Clamp(Box::new((min_val, preferred, max_val))))
            } else {
                None
            }
        }
        _ => None,
    }
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
fn parse_shorthand_4(value: &str) -> Option<(rustkit_css::Length, rustkit_css::Length, rustkit_css::Length, rustkit_css::Length)> {
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
        (value.strip_prefix("inset").unwrap().trim(), true)
    } else if value.ends_with("inset") {
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
        value[..value.len() - 2].parse::<f32>().ok().map(|v| v / 1000.0)
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
            let inner = value.trim_start_matches("cubic-bezier(").trim_end_matches(')');
            let parts: Vec<f32> = inner.split(',').filter_map(|s| s.trim().parse().ok()).collect();
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
                let jump_start = parts.get(1).map(|s| *s == "jump-start" || *s == "start").unwrap_or(false);
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
            let y = parts.get(1).and_then(|s| parse_length(s)).unwrap_or(rustkit_css::Length::Zero);
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
            let sy = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(sx);
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
    if value.ends_with("deg") {
        value[..value.len() - 3].parse().ok()
    } else if value.ends_with("rad") {
        value[..value.len() - 3].parse::<f32>().ok().map(|r| r.to_degrees())
    } else if value.ends_with("turn") {
        value[..value.len() - 4].parse::<f32>().ok().map(|t| t * 360.0)
    } else if value.ends_with("grad") {
        value[..value.len() - 4].parse::<f32>().ok().map(|g| g * 0.9)
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
                if let (Some(min), Some(max)) = (parse_track_size(min_str), parse_track_size(max_str)) {
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
                return Some(rustkit_css::TrackSize::FitContent(length.to_px(16.0, 16.0, 0.0)));
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
fn parse_grid_line_shorthand(value: &str) -> Option<(rustkit_css::GridLine, rustkit_css::GridLine)> {
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
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
        };
        
        // Build layout tree from document
        let layout = engine.build_layout_from_document(&document, &[]);
        
        // Verify layout tree is not empty
        assert!(!layout.children.is_empty(), "Layout tree should have children from body");
        
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
        assert!(text_count >= 2, "Should have at least 2 text boxes (h1 and p content), got {}", text_count);
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
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader")),
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
        assert!(!display_list.commands.is_empty(), "Display list should have commands, got {:?}", display_list.commands);
    }

    #[test]
    fn test_parse_color() {
        // Test named colors
        assert_eq!(parse_color("black"), Some(rustkit_css::Color::BLACK));
        assert_eq!(parse_color("white"), Some(rustkit_css::Color::WHITE));
        
        // Test hex colors
        assert_eq!(parse_color("#fff"), Some(rustkit_css::Color::from_rgb(255, 255, 255)));
        assert_eq!(parse_color("#000000"), Some(rustkit_css::Color::from_rgb(0, 0, 0)));
        assert_eq!(parse_color("#ff0000"), Some(rustkit_css::Color::from_rgb(255, 0, 0)));
        
        // Test rgb colors
        assert_eq!(parse_color("rgb(255, 0, 0)"), Some(rustkit_css::Color::new(255, 0, 0, 1.0)));
    }

    #[test]
    fn test_parse_length() {
        assert_eq!(parse_length("0"), Some(rustkit_css::Length::Zero));
        assert_eq!(parse_length("auto"), Some(rustkit_css::Length::Auto));
        assert_eq!(parse_length("10px"), Some(rustkit_css::Length::Px(10.0)));
        assert_eq!(parse_length("1.5em"), Some(rustkit_css::Length::Em(1.5)));
        assert_eq!(parse_length("2rem"), Some(rustkit_css::Length::Rem(2.0)));
        assert_eq!(parse_length("50%"), Some(rustkit_css::Length::Percent(50.0)));
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
        assert!(matches!(parse_timing_function("ease"), rustkit_css::TimingFunction::Ease));
        assert!(matches!(parse_timing_function("linear"), rustkit_css::TimingFunction::Linear));
        assert!(matches!(parse_timing_function("ease-in"), rustkit_css::TimingFunction::EaseIn));
        assert!(matches!(parse_timing_function("ease-out"), rustkit_css::TimingFunction::EaseOut));
        
        // Test cubic-bezier
        if let rustkit_css::TimingFunction::CubicBezier(x1, y1, x2, y2) = parse_timing_function("cubic-bezier(0.1, 0.2, 0.3, 0.4)") {
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
            assert_eq!(linear.stops[0].color, rustkit_css::Color::from_rgb(255, 0, 0));
            assert_eq!(linear.stops[0].position, Some(0.0));
            assert_eq!(linear.stops[1].color, rustkit_css::Color::from_rgb(0, 0, 255));
            assert_eq!(linear.stops[1].position, Some(1.0));
        } else {
            panic!("Expected Linear gradient");
        }
        
        // Test with angle
        let gradient = parse_gradient("linear-gradient(45deg, red 0%, blue 100%)");
        assert!(gradient.is_some(), "Should parse gradient with angle");
        
        if let Some(rustkit_css::Gradient::Linear(linear)) = gradient {
            assert!(matches!(linear.direction, rustkit_css::GradientDirection::Angle(a) if (a - 45.0).abs() < 0.01));
        } else {
            panic!("Expected Linear gradient with angle");
        }
        
        // Test default direction (to bottom)
        let gradient = parse_gradient("linear-gradient(#667eea, #764ba2)");
        assert!(gradient.is_some(), "Should parse gradient without direction");
        
        if let Some(rustkit_css::Gradient::Linear(linear)) = gradient {
            assert_eq!(linear.direction, rustkit_css::GradientDirection::ToBottom);
        } else {
            panic!("Expected Linear gradient with default direction");
        }
    }

    #[test]
    fn test_parse_radial_gradient() {
        // Test simple radial gradient
        let gradient = parse_gradient("radial-gradient(circle at center, #667eea 0%, #764ba2 100%)");
        assert!(gradient.is_some(), "Should parse radial gradient");
        
        if let Some(rustkit_css::Gradient::Radial(radial)) = gradient {
            assert_eq!(radial.shape, rustkit_css::RadialShape::Circle);
            assert_eq!(radial.stops.len(), 2);
        } else {
            panic!("Expected Radial gradient");
        }
        
        // Test ellipse
        let gradient = parse_gradient("radial-gradient(ellipse at top left, #f093fb 0%, #f5576c 100%)");
        assert!(gradient.is_some(), "Should parse ellipse radial gradient");
        
        if let Some(rustkit_css::Gradient::Radial(radial)) = gradient {
            assert_eq!(radial.shape, rustkit_css::RadialShape::Ellipse);
            assert!((radial.center.0 - 0.0).abs() < 0.01, "center.0 should be 0.0 for left");
            assert!((radial.center.1 - 0.0).abs() < 0.01, "center.1 should be 0.0 for top");
        } else {
            panic!("Expected Radial gradient with ellipse");
        }
    }

    #[test]
    fn test_parse_color_stop() {
        // Test color with percentage position
        let stop = parse_color_stop("#ff0000 50%");
        assert!(stop.is_some());
        let stop = stop.unwrap();
        assert_eq!(stop.color, rustkit_css::Color::from_rgb(255, 0, 0));
        assert_eq!(stop.position, Some(0.5));
        
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
        assert_eq!(stop.position, Some(0.25));
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
}
