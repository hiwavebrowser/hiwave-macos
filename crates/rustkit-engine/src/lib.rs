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
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            user_agent: "RustKit/1.0 HiWave/1.0".to_string(),
            javascript_enabled: true,
            cookies_enabled: true,
            background_color: [1.0, 1.0, 1.0, 1.0], // White
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
        let loader =
            Arc::new(ResourceLoader::new(loader_config).map_err(EngineError::NetworkError)?);

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

        debug!(?id, ?bounds, "Resizing view");

        // Resize viewhost
        self.viewhost
            .set_bounds(view.viewhost_id, bounds)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        // Resize compositor surface
        self.compositor
            .resize_surface(view.viewhost_id, bounds.width, bounds.height)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

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
    fn relayout(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        info!(?id, "Starting relayout");
        
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let document = view
            .document
            .as_ref()
            .ok_or(EngineError::RenderError("No document".into()))?
            .clone();
        
        info!(?id, "Got document for relayout");

        // Get view bounds
        let bounds = self
            .viewhost
            .get_bounds(view.viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        debug!(
            ?id,
            width = bounds.width,
            height = bounds.height,
            "Performing layout"
        );

        // Create containing block
        let containing_block = Dimensions {
            content: Rect::new(0.0, 0.0, bounds.width as f32, bounds.height as f32),
            ..Default::default()
        };

        // Build layout tree from DOM
        let mut root_box = self.build_layout_from_document(&document);

        // Layout
        root_box.layout(&containing_block);

        // Generate display list
        let display_list = DisplayList::build(&root_box);

        debug!(
            ?id,
            num_commands = display_list.commands.len(),
            "Generated display list"
        );

        // Store
        let view = self.views.get_mut(&id).unwrap();
        view.layout = Some(root_box);
        view.display_list = Some(display_list);

        // Render
        info!(?id, "Calling render after relayout");
        self.render(id)?;
        info!(?id, "Render complete");

        Ok(())
    }

    /// Build a layout tree from a DOM document.
    fn build_layout_from_document(&self, document: &Document) -> LayoutBox {
        // Extract stylesheets from <style> elements
        let stylesheets = self.extract_stylesheets(document);
        let css_vars = self.extract_css_variables(&stylesheets);
        
        info!(
            stylesheet_count = stylesheets.len(),
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
        match &node.node_type {
            NodeType::Element { tag_name, attributes, .. } => {
                // Determine box type based on tag
                let is_inline = matches!(
                    tag_name.to_lowercase().as_str(),
                    "a" | "span" | "strong" | "b" | "em" | "i" | "u" | "code" | "small" | "big" | "sub" | "sup" | "abbr" | "cite" | "q" | "mark" | "label"
                );

                // Skip rendering for certain elements
                let is_hidden = matches!(
                    tag_name.to_lowercase().as_str(),
                    "head" | "title" | "meta" | "link" | "script" | "style" | "noscript"
                );

                if is_hidden {
                    // Return an empty block for hidden elements
                    return LayoutBox::new(BoxType::Block, ComputedStyle::new());
                }

                let box_type = if is_inline {
                    BoxType::Inline
                } else {
                    BoxType::Block
                };

                // Create computed style based on element, attributes, and stylesheets
                let style = self.compute_style_for_element(tag_name, attributes, stylesheets, css_vars, ancestors);
                
                // Check for display: none
                if style.display == rustkit_css::Display::None {
                    return LayoutBox::new(BoxType::Block, ComputedStyle::new());
                }

                let mut layout_box = LayoutBox::new(box_type, style);

                // Build ancestors list for child elements
                let mut child_ancestors = ancestors.to_vec();
                child_ancestors.push(tag_name.to_lowercase());

                // Process children
                for child in node.children() {
                    let child_box = self.build_layout_from_node_with_styles(&child, stylesheets, css_vars, &child_ancestors);
                    // Only add non-empty boxes
                    if !matches!(child_box.box_type, BoxType::Block) || !child_box.children.is_empty() 
                        || matches!(child_box.box_type, BoxType::Text(_)) {
                        layout_box.children.push(child_box);
                    }
                }

                layout_box
            }
            NodeType::Text(text) => {
                // Create text box for non-empty text
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    // Return minimal box for whitespace-only text
                    LayoutBox::new(BoxType::Block, ComputedStyle::new())
                } else {
                    let mut style = ComputedStyle::new();
                    style.color = rustkit_css::Color::BLACK;
                    LayoutBox::new(BoxType::Text(trimmed.to_string()), style)
                }
            }
            _ => {
                // For other node types (Document, Comment, etc.), return empty box
                LayoutBox::new(BoxType::Block, ComputedStyle::new())
            }
        }
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
        
        for stylesheet in stylesheets {
            for rule in &stylesheet.rules {
                if self.selector_matches(&rule.selector, tag_name, attributes, ancestors) {
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
        match property {
                    "color" => {
                        if let Some(color) = parse_color(value) {
                            style.color = color;
                        }
                    }
                    "background-color" | "background" => {
                        if let Some(color) = parse_color(value) {
                            style.background_color = color;
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
                        if let Some(length) = parse_length(value) {
                            style.margin_top = length;
                            style.margin_right = length;
                            style.margin_bottom = length;
                            style.margin_left = length;
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
                        if let Some(length) = parse_length(value) {
                            style.padding_top = length;
                            style.padding_right = length;
                            style.padding_bottom = length;
                            style.padding_left = length;
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
                    style.border_top_width = length;
                    style.border_right_width = length;
                    style.border_bottom_width = length;
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
            "text-align" => {
                // Store text-align if ComputedStyle supports it
                // For now, just ignore
            }
            "border-radius" => {
                // border-radius not yet in ComputedStyle, ignore for now
            }
            "max-width" => {
                if let Some(length) = parse_length(value) {
                    style.max_width = length;
                }
            }
            _ => {
                // Unknown property, ignore
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
    fn selector_matches(&self, selector: &str, tag_name: &str, attributes: &HashMap<String, String>, ancestors: &[String]) -> bool {
        let selector = selector.trim();
        
        // Handle multiple selectors (comma-separated)
        if selector.contains(',') {
            return selector.split(',')
                .any(|s| self.selector_matches(s.trim(), tag_name, attributes, ancestors));
        }
        
        // Handle descendant combinator (space-separated)
        if selector.contains(' ') {
            let parts: Vec<&str> = selector.split_whitespace().collect();
            if let Some((last, ancestor_selectors)) = parts.split_last() {
                // Last part must match current element
                if !self.simple_selector_matches(last, tag_name, attributes) {
                    return false;
                }
                // Ancestor selectors must match some ancestor (in order)
                let mut ancestor_idx = 0;
                for sel in ancestor_selectors {
                    while ancestor_idx < ancestors.len() {
                        if self.simple_selector_matches_tag_only(sel, &ancestors[ancestor_idx]) {
                            ancestor_idx += 1;
                            break;
                        }
                        ancestor_idx += 1;
                    }
                }
                return true; // Simplified - just check if element matches
            }
        }
        
        // Simple selector (no combinators)
        self.simple_selector_matches(selector, tag_name, attributes)
    }

    /// Check if a simple selector matches an element.
    fn simple_selector_matches(&self, selector: &str, tag_name: &str, attributes: &HashMap<String, String>) -> bool {
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
        if selector.starts_with('.') {
            let classes: Vec<&str> = selector[1..].split('.').collect();
            if let Some(el_class) = attributes.get("class") {
                let el_classes: Vec<&str> = el_class.split_whitespace().collect();
                return classes.iter().all(|c| el_classes.contains(c));
            }
            return false;
        }
        
        // Type selector (element name)
        // May have class or ID attached: div.class or div#id
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
        
        // Check remaining parts (classes, IDs)
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
            } else {
                // Unknown, skip
                break;
            }
        }
        
        true
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
    /// This is useful for deterministic testing and visual debugging.
    /// The output is a PPM file (simple portable format).
    pub fn capture_frame(&self, id: EngineViewId, path: &str) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let viewhost_id = view.viewhost_id;

        info!(?id, path, "Capturing frame");

        self.compositor
            .capture_frame_to_file(viewhost_id, path)
            .map_err(|e| EngineError::RenderError(e.to_string()))
    }

    /// Render a view (internal).
    fn render(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;
        let viewhost_id = view.viewhost_id;
        let display_list = view.display_list.as_ref();
        let has_display_list = display_list.is_some();
        let cmd_count = display_list.map(|dl| dl.commands.len()).unwrap_or(0);

        trace!(?id, has_display_list, cmd_count, "Rendering view");

        // Get surface size and update renderer viewport before rendering
        // This ensures the vertex shader transforms coordinates correctly
        let (surface_width, surface_height) = self.compositor
            .get_surface_size(viewhost_id)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        if let Some(renderer) = &mut self.renderer {
            renderer.set_viewport_size(surface_width, surface_height);
            debug!(?id, surface_width, surface_height, "Updated renderer viewport size");
        }

        // Get surface texture
        let (output, texture_view) = self.compositor
            .get_surface_texture(viewhost_id)
            .map_err(|e| EngineError::RenderError(e.to_string()))?;

        // Render using display list if available, otherwise just clear to background
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

        // Present
        self.compositor.present(output);

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
}

impl EngineBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: EngineConfig::default(),
        }
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

    /// Build the engine.
    pub fn build(self) -> Result<Engine, EngineError> {
        Engine::new(self.config)
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
        let layout = engine.build_layout_from_document(&document);
        
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
        
        let mut layout = engine.build_layout_from_document(&document);
        
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
}
