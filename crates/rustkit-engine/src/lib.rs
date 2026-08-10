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
use rustkit_layout::{
    FontLoader,
    BoxType, Dimensions, DisplayList, ElementIdentity, LayoutBox, Position, Rect,
};
use std::cell::Cell;
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
    /// Live text-editing state for this view's form controls, keyed by raw
    /// `NodeId`.
    ///
    /// Per-VIEW, not engine-global: `NodeId` is per-DOCUMENT (every
    /// `Document` restarts its counter at 1), so a global map lets one
    /// document's node 4 collide with another's. Living on the view also
    /// makes the lifetime obvious — drop the view, drop the map — and gives
    /// document replacement one clear place to clear.
    ///
    /// This is the IDL-value side table browsers keep separate from the
    /// content attribute: `getAttribute("value")` is the authored default,
    /// this is the live value. Mutating the DOM instead would conflate the
    /// two and break form-reset semantics.
    edit_states: std::collections::HashMap<usize, rustkit_dom::forms::TextEditState>,
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
    /// Webfont loader. PER-ENGINE AND Arc-SHARED per the 2026-08-08 design
    /// pin: one loader for the whole engine, with cross-site isolation
    /// provided by the partition key on every operation rather than by
    /// handing each view its own object. Per-view loaders would refetch the
    /// same face for every tab; an unpartitioned shared one would be a
    /// cross-site timing oracle. The partitioned shared loader is the only
    /// shape that is both.
    font_loader: Arc<FontLoader>,
    views: HashMap<EngineViewId, ViewState>,
    event_tx: mpsc::UnboundedSender<EngineEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<EngineEvent>>,
    /// Cascade provenance, recorded by the REAL cascade when armed.
    ///
    /// `None` = not recording, which is the default and costs nothing. See
    /// [`Engine::set_style_recording`] for why this is a side table rather
    /// than a field on `ComputedStyle`.
    style_trace: std::cell::RefCell<Option<Vec<StyleRecord>>>,
    /// Views whose most recent render attempt failed. A view entering this
    /// set logs one `warn!`; leaving it logs recovery. Without this, a
    /// persistently failing render (e.g. a wedged surface) freezes the
    /// screen while the log stays silent.
    render_failing: std::collections::HashSet<EngineViewId>,
    /// View currently being laid out, so the view-agnostic recursive builder
    /// can read that view's edit states (and only that view's).
    building_view: std::cell::Cell<Option<EngineViewId>>,
    /// Focused node of the view currently being laid out.
    ///
    /// Focus is per-VIEW but the recursive layout builder is view-agnostic,
    /// so the alternative was checking "is this node focused in ANY view",
    /// which would mark a node focused in one view while building another.
    /// Set by `relayout` around the build; `None` outside one.
    building_focus: std::cell::Cell<Option<rustkit_dom::NodeId>>,
    /// Parsed SVG documents keyed by URL. SVGs referenced from <img> are
    /// vector content — they bypass ImageManager's raster decode (which
    /// rejects them as "Unknown image format") and are spliced into the
    /// display list as vector commands at build time.
    svg_cache: std::collections::HashMap<String, rustkit_svg::SvgDocument>,
}

/// One author declaration that MATCHED an element, win or lose.
///
/// Losers are kept deliberately. "Parsed but dead" is the bug class this
/// exists to catch, and you cannot see that a declaration was overridden by
/// looking at the value that survived — only by seeing what it beat.
#[derive(Debug, Clone)]
pub struct DeclarationRecord {
    /// Property name exactly as authored, so `background` stays `background`
    /// rather than being reported as the longhands it happens to set.
    pub property: String,
    /// Declared value, after CSS-variable substitution.
    pub value: String,
    /// The selector of the rule that carried it — the citable rule.
    pub selector: String,
    /// `(ids, classes, tags)`, as the cascade itself computed it.
    pub specificity: (usize, usize, usize),
    /// `author` or `author-inline`. The UA sheet cannot appear here; it is a
    /// hardcoded Rust `match`, not parsed rules, so it has no selector to cite.
    pub origin: &'static str,
    /// Whether the declaration carried `!important`.
    ///
    /// Recorded but NOT acted on: this cascade orders by specificity alone.
    /// An `!important` declaration that lost is therefore a real engine bug,
    /// and reporting the flag is how the tool shows it instead of hiding it.
    pub important: bool,
    /// Position in application order. The highest `order` for a given
    /// property is the winner, because it wrote the field last.
    pub order: usize,
}

/// The subset of selector syntax `export_style_json` accepts as a QUERY.
///
/// Deliberately not the engine's full selector matcher. Matching a descendant
/// or sibling selector needs the tree context the cascade had, which the
/// trace does not keep; accepting `div p` and quietly matching every `p`
/// would answer the wrong question in a way the caller could not see.
/// Refusing what it cannot do is the point.
#[derive(Debug, Default)]
struct SimpleSelector {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl SimpleSelector {
    fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() || input.contains([' ', '>', '+', '~', ',', '[', ':', '*']) {
            return None;
        }
        let mut query = SimpleSelector::default();
        // Leading run before any '.'/'#' is the tag name, if present.
        let first_marker = input.find(['.', '#']).unwrap_or(input.len());
        if first_marker > 0 {
            query.tag = Some(input[..first_marker].to_lowercase());
        }
        let mut rest = &input[first_marker..];
        while !rest.is_empty() {
            let kind = rest.as_bytes()[0];
            let end = rest[1..].find(['.', '#']).map(|i| i + 1).unwrap_or(rest.len());
            let name = &rest[1..end];
            if name.is_empty() {
                return None;
            }
            match kind {
                b'.' => query.classes.push(name.to_string()),
                b'#' => query.id = Some(name.to_string()),
                _ => return None,
            }
            rest = &rest[end..];
        }
        if query.tag.is_none() && query.id.is_none() && query.classes.is_empty() {
            return None;
        }
        Some(query)
    }

    fn matches(&self, record: &StyleRecord) -> bool {
        if let Some(tag) = &self.tag {
            if &record.tag != tag {
                return false;
            }
        }
        if let Some(id) = &self.id {
            if record.id.as_deref() != Some(id.as_str()) {
                return false;
            }
        }
        self.classes.iter().all(|c| record.classes.contains(c))
    }
}

/// The cascade as it ran for one element.
#[derive(Debug, Clone)]
pub struct StyleRecord {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub declarations: Vec<DeclarationRecord>,
    /// Computed values, read back off the `ComputedStyle` this cascade
    /// produced — so the reported value is the one layout actually used.
    pub computed: Vec<(String, String)>,
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
            
            font_loader: Arc::new(FontLoader::new()),event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
        })
    }

    /// Arm or disarm cascade provenance recording.
    ///
    /// Off by default. When on, the cascade records every author declaration
    /// that matched each element — including the ones that lost — so
    /// `export_style_json` can name the winning rule rather than only the
    /// value that survived.
    ///
    /// This is a side table rather than a field on `ComputedStyle` on
    /// purpose: `ComputedStyle` is cloned per box (and per pseudo-element,
    /// and per text child), so hanging a provenance map off it would put the
    /// cost on every page instead of on the pages someone is debugging. The
    /// recording happens inside the ONE real cascade, not a re-implementation
    /// of it — a second cascade written for the exporter could disagree with
    /// the first, and then the tool would be inventing an answer.
    ///
    /// The trace is cleared and refilled on each layout build.
    pub fn set_style_recording(&mut self, on: bool) {
        *self.style_trace.borrow_mut() = if on { Some(Vec::new()) } else { None };
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
            edit_states: std::collections::HashMap::new(),
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
    ///
    /// Gated on macOS specifically, not `not(windows)`: the body calls
    /// `ViewHostTrait::get_raw_window_handle` and
    /// `Compositor::create_surface_for_raw_handle`, both of which are
    /// themselves `cfg(target_os = "macos")`. The wider gate promised a
    /// non-Windows build it could not deliver, so the crate failed to compile
    /// anywhere else — including the headless path, which needs no window at
    /// all. No behaviour changes on macOS or Windows: both gates agree there.
    #[cfg(target_os = "macos")]
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
            edit_states: std::collections::HashMap::new(),
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
            edit_states: std::collections::HashMap::new(),
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
        self.render_failing.remove(&id);

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

    /// Hit-test a click in VIEWPORT coordinates and return the link URL it
    /// resolves to, if any.
    ///
    /// The click point is translated into document coordinates by the
    /// current scroll offset (layout lives in document space; scroll is a
    /// render-time translate), then the nearest `<a href>` ancestor's raw
    /// href is resolved against the view's URL. Returns `None` for clicks
    /// that hit no link — including `javascript:` links, which are dropped
    /// at layout time.
    pub fn link_at_point(
        &self,
        id: EngineViewId,
        viewport_x: f32,
        viewport_y: f32,
    ) -> Option<String> {
        let view = self.views.get(&id)?;
        let doc_x = viewport_x + view.scroll_offset.0;
        let doc_y = viewport_y + view.scroll_offset.1;
        let hit = view.layout.as_ref()?.hit_test(doc_x, doc_y)?;
        let href = hit.link_href?;
        match view.url.as_ref() {
            Some(base) => base.join(&href).ok().map(|u| u.to_string()),
            // No base (e.g. loaded HTML with no URL): only absolute hrefs
            // can navigate.
            None => Url::parse(&href).ok().map(|u| u.to_string()),
        }
    }

    /// Hit-test a click in VIEWPORT coordinates and focus the element under
    /// it if that element is focusable (a form control today).
    ///
    /// Returns the tag name of the newly focused element, or `None` when the
    /// click landed on nothing focusable — in which case focus is CLEARED,
    /// matching the behavior of clicking a page's background.
    pub fn focus_at_point(
        &mut self,
        id: EngineViewId,
        viewport_x: f32,
        viewport_y: f32,
    ) -> Option<String> {
        let (doc_x, doc_y) = {
            let view = self.views.get(&id)?;
            (viewport_x + view.scroll_offset.0, viewport_y + view.scroll_offset.1)
        };

        let hit_node = self
            .views
            .get(&id)
            .and_then(|v| v.layout.as_ref())
            .and_then(|l| l.hit_test(doc_x, doc_y))
            .and_then(|h| h.node_id);

        // Resolve focusability against the DOM, not the layout box: a
        // FormControl box type would miss `contenteditable` and tabindex
        // later, and the tag name is what callers want reported.
        let focusable = hit_node.and_then(|raw| {
            let view = self.views.get(&id)?;
            let doc = view.document.as_ref()?;
            let node = doc.get_node(rustkit_dom::NodeId::new(raw))?;
            match &node.node_type {
                NodeType::Element { tag_name, .. } => {
                    let tag = tag_name.to_lowercase();
                    matches!(tag.as_str(), "input" | "textarea" | "select")
                        .then_some((raw, tag))
                }
                _ => None,
            }
        });

        // Seed edit state from the element's authored value the FIRST time it
        // is focused. Re-focusing must not reset what the user has typed, so
        // the seed is guarded by the entry being absent.
        if let Some((raw, ref tag)) = focusable {
            let already_seeded = self
                .views
                .get(&id)
                .map(|v| v.edit_states.contains_key(&raw))
                .unwrap_or(false);
            if matches!(tag.as_str(), "input" | "textarea") && !already_seeded {
                let seed = self
                    .views
                    .get(&id)
                    .and_then(|v| v.document.as_ref())
                    .and_then(|d| d.get_node(rustkit_dom::NodeId::new(raw)))
                    .map(|node| match &node.node_type {
                        NodeType::Element { attributes, .. } => {
                            if tag == "textarea" {
                                node.text_content()
                            } else {
                                attributes.get("value").cloned().unwrap_or_default()
                            }
                        }
                        _ => String::new(),
                    })
                    .unwrap_or_default();
                let state = rustkit_dom::forms::TextEditState::with_value(seed);
                state.move_to_end(false);
                if let Some(v) = self.views.get_mut(&id) {
                    v.edit_states.insert(raw, state);
                }
            }
        }

        let view = self.views.get_mut(&id)?;
        match focusable {
            Some((raw, tag)) => {
                view.focused_node = Some(rustkit_dom::NodeId::new(raw));
                debug!(?id, %tag, "Focused element");
                Some(tag)
            }
            None => {
                view.focused_node = None;
                None
            }
        }
    }

    /// Deliver a key to the focused form control.
    ///
    /// `key_code` uses the Win32 virtual-key numbering that
    /// `rustkit_dom::forms::keyboard` already speaks (the model predates any
    /// platform wiring); `key` carries the typed character for insertions.
    /// Returns true when the control's value or caret changed, i.e. when the
    /// caller must relayout.
    pub fn handle_text_key(
        &mut self,
        id: EngineViewId,
        key_code: u32,
        key: &str,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) -> bool {
        use rustkit_dom::forms::{keyboard, KeyHandleResult};

        let Some(focused) = self.views.get(&id).and_then(|v| v.focused_node) else {
            return false;
        };
        let Some(state) = self
            .views
            .get(&id)
            .and_then(|v| v.edit_states.get(&focused.raw()))
        else {
            return false;
        };

        let result = keyboard::handle_input_key(state, key_code, key, ctrl, shift, alt);
        matches!(
            result,
            KeyHandleResult::ValueChanged | KeyHandleResult::SelectionChanged
        )
    }

    /// Build the submission for the form containing the focused control.
    ///
    /// Returns `None` when nothing is focused, the focused control has no
    /// enclosing `<form>`, or the form has no submittable fields. Values come
    /// from live edit state where it exists, so a submit carries what the
    /// user actually typed rather than the authored attribute.
    ///
    /// GET only for now: the loader's public surface takes a URL, so a POST
    /// body has nowhere to go until `load_url` grows a request variant. That
    /// is a named follow-up, not a silent omission — a POST form returns
    /// `None` rather than being submitted as a GET, because quietly changing
    /// a form's method is worse than not submitting it.
    pub fn form_submission_for_focus(
        &self,
        id: EngineViewId,
    ) -> Option<rustkit_dom::forms::FormSubmission> {
        use rustkit_dom::forms::{FormDataEntry, FormDataValue, FormState};

        let view = self.views.get(&id)?;
        let focused = view.focused_node?;
        let document = view.document.as_ref()?;
        let base = view.url.as_ref()?.to_string();

        // Walk up to the enclosing <form>.
        let form = {
            let mut cur = document.get_node(focused)?;
            loop {
                match &cur.node_type {
                    NodeType::Element { tag_name, .. } if tag_name.eq_ignore_ascii_case("form") => {
                        break cur
                    }
                    _ => cur = cur.parent()?,
                }
            }
        };

        let NodeType::Element {
            attributes: form_attrs,
            ..
        } = &form.node_type
        else {
            return None;
        };

        let state = FormState::new();
        state.set_action(form_attrs.get("action").cloned().unwrap_or_default());
        if let Some(m) = form_attrs.get("method") {
            state.set_method(rustkit_dom::forms::FormMethod::from_str(m));
        }

        // Only GET is wired; see the doc comment.
        if state.method() != rustkit_dom::forms::FormMethod::Get {
            debug!(?id, "Form submit skipped: only GET is wired");
            return None;
        }

        // Collect successful controls in document order.
        let mut entries = Vec::new();
        fn collect(
            node: &std::rc::Rc<Node>,
            engine: &Engine,
            view_id: EngineViewId,
            out: &mut Vec<FormDataEntry>,
        ) {
            if let NodeType::Element {
                tag_name,
                attributes,
                ..
            } = &node.node_type
            {
                let tag = tag_name.to_lowercase();
                if matches!(tag.as_str(), "input" | "textarea") {
                    // A control without a name is not successful (HTML §4.10),
                    // and disabled controls never submit.
                    let name = attributes.get("name").cloned().unwrap_or_default();
                    let disabled = attributes.contains_key("disabled");
                    let kind = attributes
                        .get("type")
                        .map(|t| t.to_lowercase())
                        .unwrap_or_else(|| "text".into());
                    let skip = matches!(kind.as_str(), "submit" | "button" | "reset" | "file")
                        || (matches!(kind.as_str(), "checkbox" | "radio")
                            && !attributes.contains_key("checked"));
                    if !name.is_empty() && !disabled && !skip {
                        let value = engine
                            .edit_value_in(view_id, node.id.raw())
                            .map(|(v, _)| v)
                            .unwrap_or_else(|| {
                                if tag == "textarea" {
                                    node.text_content()
                                } else {
                                    attributes.get("value").cloned().unwrap_or_default()
                                }
                            });
                        out.push(FormDataEntry {
                            name,
                            value: FormDataValue::String(value),
                        });
                    }
                }
            }
            for child in node.children() {
                collect(&child, engine, view_id, out);
            }
        }
        collect(&form, self, id, &mut entries);

        if entries.is_empty() {
            return None;
        }
        Some(state.create_submission(&base, &entries))
    }

    /// Current value of a control's live edit state, if it has one.
    ///
    /// Layout reads through this so an edited field renders its typed text
    /// while the DOM attribute stays untouched.
    pub fn edit_value(&self, node_raw: usize) -> Option<(String, usize)> {
        // Resolves against the view currently being laid out; outside a
        // build there is no unambiguous answer, so it declines rather than
        // guessing across views.
        let view_id = self.building_view.get()?;
        self.edit_value_in(view_id, node_raw)
    }

    /// Resolve a possibly-relative resource URL against the document being
    /// laid out.
    ///
    /// The load path already resolves and caches under the ABSOLUTE url
    /// (`discover_images`), while layout and paint used the raw attribute —
    /// so `src="portal/img/logo.png"` was cached under
    /// `https://.../portal/img/logo.png` and then looked up under
    /// `portal/img/logo.png`, matching nothing. Paint additionally re-parsed
    /// it and logged `Invalid URL for image` once per image PER FRAME (1120
    /// warnings in one live session). One resolution point, at build, fixes
    /// the cache key, the natural size, and the paint lookup together.
    fn resolve_resource_url(&self, raw: &str) -> Option<Url> {
        let id = self.building_view.get()?;
        self.resolve_resource_url_in(id, raw)
    }

    /// Same, against a named view. Used where the build scope has already
    /// been cleared (display-list assembly runs after the layout build).
    fn resolve_resource_url_in(&self, id: EngineViewId, raw: &str) -> Option<Url> {
        match self.views.get(&id).and_then(|v| v.url.as_ref()) {
            Some(base) => base.join(raw).ok(),
            None => Url::parse(raw).ok(),
        }
    }

    /// Live value + caret for a control in a SPECIFIC view.
    pub fn edit_value_in(&self, id: EngineViewId, node_raw: usize) -> Option<(String, usize)> {
        self.views
            .get(&id)?
            .edit_states
            .get(&node_raw)
            .map(|s| (s.value(), s.caret_position()))
    }

    /// Make a view's NATIVE view the window's first responder so the OS
    /// routes keyboard events to it. Engine-side focus (focused_node) decides
    /// which element gets the keys; this decides whether the keys arrive at
    /// all. Two systems, both required.
    pub fn grab_keyboard(&self, id: EngineViewId) {
        if let Some(view) = self.views.get(&id) {
            let _ = <ViewHost as ViewHostTrait>::focus_view(&self.viewhost, view.viewhost_id);
        }
    }

    /// The DOM node currently holding focus in a view, if any.
    pub fn focused_node(&self, id: EngineViewId) -> Option<rustkit_dom::NodeId> {
        self.views.get(&id).and_then(|v| v.focused_node)
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
        // A committed navigation replaces the document; start at the top.
        view.scroll_offset = (0.0, 0.0);

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
        // A new document invalidates every per-node side table. NodeId is
        // PER-DOCUMENT (each Document restarts its counter at 1), so a
        // surviving entry keyed by raw id 4 would be read as the NEW page's
        // node 4: the previous page's typed text painted into a fresh
        // control, with first-focus seeding skipped because the key already
        // exists. The old doc comment claimed reload dropped this map; it
        // did not, and asserting a lifetime the code does not implement is
        // how a silent correctness bug hides in plain sight.
        // (Prometheus, #110 R1 must-fix.)
        view.edit_states.clear();
        view.focused_node = None;

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

        // This is a NEW document, and load_html deliberately fetches no
        // subresources — so nothing downstream will ever overwrite the
        // stylesheets a previous document left on this view. Clearing here
        // is what stops inline content from silently inheriting the last
        // navigated page's CSS.
        //
        // This is the second door onto the same leak as the one fixed in
        // load_subresources: that one carried stale CSS forward when the new
        // document had no <link>; this one carried it forward whenever the
        // new document arrived via load_html at all. Closing one and not the
        // other would leave the bug reachable by the shorter route.
        if !view.external_stylesheets.is_empty() {
            debug!(
                ?id,
                dropped = view.external_stylesheets.len(),
                "Clearing previous document's external stylesheets for inline load"
            );
            view.external_stylesheets.clear();
        }

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
        // A committed navigation replaces the document; start at the top.
        view.scroll_offset = (0.0, 0.0);

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
        // A new document invalidates every per-node side table. NodeId is
        // PER-DOCUMENT (each Document restarts its counter at 1), so a
        // surviving entry keyed by raw id 4 would be read as the NEW page's
        // node 4: the previous page's typed text painted into a fresh
        // control, with first-focus seeding skipped because the key already
        // exists. The old doc comment claimed reload dropped this map; it
        // did not, and asserting a lifetime the code does not implement is
        // how a silent correctness bug hides in plain sight.
        // (Prometheus, #110 R1 must-fix.)
        view.edit_states.clear();
        view.focused_node = None;

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
    /// Rebuild layout and repaint a view.
    ///
    /// Public so the shell can refresh after an edit changes a form
    /// control's value — the value lives in engine-side edit state, so
    /// nothing else would trigger a rebuild.
    pub fn relayout(&mut self, id: EngineViewId) -> Result<(), EngineError> {
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
            // Scope the focused node to THIS view for the duration of the
            // build; cleared immediately after so no later build inherits it.
            self.build_layout_for_view(id, &document, &external_stylesheets)
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
            let mut dl = DisplayList::build(&root_box);
            // Splice cached SVG documents in place of their Image commands.
            // Done once per layout (not per frame): the renderer only speaks
            // raster textures, so vector images become their own command
            // runs positioned in the image's dest_rect.
            // Normalize resource URLs to absolute, then splice SVGs.
            //
            // CSS `url(...)` is parsed without a base (the parser is a free
            // function with no document in scope), and `<img>` boxes carry
            // whatever the build produced. The loader caches under ABSOLUTE
            // urls, so any relative key here misses the cache and then fails
            // to parse in the paint path — `Invalid URL for image`, once per
            // image per frame. Rewriting here means the renderer only ever
            // sees keys that match what the loader stored.
            let mut expanded = Vec::with_capacity(dl.commands.len());
            for mut cmd in dl.commands.drain(..) {
                match &mut cmd {
                    rustkit_layout::DisplayCommand::Image { url, .. }
                    | rustkit_layout::DisplayCommand::BackgroundImage { url, .. } => {
                        if let Some(abs) = self.resolve_resource_url_in(id, url) {
                            *url = abs.to_string();
                        }
                    }
                    _ => {}
                }
                match &cmd {
                    rustkit_layout::DisplayCommand::Image { url, dest_rect, .. } => {
                        if let Some(svg) = self.svg_cache.get(url) {
                            expanded.extend(svg.render(
                                dest_rect.x,
                                dest_rect.y,
                                dest_rect.width,
                                dest_rect.height,
                            ));
                            continue;
                        }
                    }
                    _ => {}
                }
                expanded.push(cmd);
            }
            dl.commands = expanded;
            dl
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
        // Re-clamp: a relayout can shrink the document (or a navigation can
        // replace it) while the user is scrolled past the new maximum, which
        // would render a translate into empty space.
        view.scroll_offset = (
            view.scroll_offset.0.min(view.max_scroll_offset.0),
            view.scroll_offset.1.min(max_scroll_y),
        );

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

    /// Build a layout tree FOR A SPECIFIC VIEW.
    ///
    /// Owns the `building_view`/`building_focus` scoping so no caller can
    /// forget it: without them the view-agnostic recursive builder cannot
    /// find live edit state and SILENTLY falls back to the frozen DOM
    /// attribute — typed text vanishes with no error. Every layout build
    /// goes through here.
    fn build_layout_for_view(
        &self,
        id: EngineViewId,
        document: &Document,
        external_stylesheets: &[Stylesheet],
    ) -> LayoutBox {
        self.building_view.set(Some(id));
        self.building_focus
            .set(self.views.get(&id).and_then(|v| v.focused_node));
        let built = self.build_layout_from_document(document, external_stylesheets);
        self.building_focus.set(None);
        self.building_view.set(None);
        built
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

        // A trace describes ONE build. Keeping entries from the previous
        // page would let `hiwave_style` answer with a stale element that no
        // longer exists — the same class of lie as a gate reading a stale
        // snapshot.
        if let Some(trace) = self.style_trace.borrow_mut().as_mut() {
            trace.clear();
        }

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
            // Identity tracking starts at body, matching Chrome's capture: it
            // skips `html`, so `body` is the root of every selector path.
            let element_ids = Cell::new(0);
            let body_box = self.build_layout_from_parent_style_and_path(
                &body,
                &stylesheets,
                &css_vars,
                &[],
                html_style.as_ref(),
                &[],
                0,
                1,
                "body",
                &element_ids,
                false,
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
    /// Build the Chrome-compatible selector SEGMENT for one element, e.g.
    /// `div.hero:nth-of-type(1)`.
    ///
    /// This mirrors `getSelector()` in `tools/parity_oracle/capture_baseline.mjs`
    /// as it stood when `baselines/chrome-148/**/layout-rects.json` was
    /// captured. It is a JOIN KEY, not a display string: it must reproduce
    /// Chrome's output byte-for-byte or the geometry oracle silently fails to
    /// pair boxes — and a silent failure to pair reads as "no geometry error",
    /// which is the exact class of lying instrument this campaign exists to end.
    ///
    /// Two quirks of the capture script are reproduced DELIBERATELY, not fixed:
    ///
    /// 1. A multi-class element yields `div.card featured`: the raw `className`
    ///    is concatenated after a single dot, so the internal space survives.
    ///    The result is not a valid CSS selector — it is the key committed in
    ///    the baselines, and 572 baseline selectors depend on it.
    /// 2. `:nth-of-type(N)` is appended ONLY when the element has more than one
    ///    same-tag element sibling, so a unique child carries no index.
    ///
    /// Class whitespace is normalized (runs collapsed, ends trimmed) to match
    /// `el.className` for well-formed markup; all 886 class attributes in the
    /// corpus are already normal, so this is a robustness measure and not a
    /// behavioral difference on any current case.
    fn selector_segment(
        tag_lower: &str,
        attributes: &HashMap<String, String>,
        same_tag_index: usize,
        same_tag_total: usize,
        is_foreign: bool,
    ) -> String {
        let mut segment = String::from(tag_lower);

        // Chrome's capture guards on `typeof el.className === 'string'`. For an
        // SVG or MathML element `className` is an SVGAnimatedString object, so
        // the guard fails and the class is DROPPED: a classed `<svg>` is keyed
        // as plain `svg`. Reproducing that is not optional — shelf.html has a
        // classed inline svg, and getting this wrong lost the svg and both of
        // its children from the join.
        if let Some(class_attr) = attributes.get("class").filter(|_| !is_foreign) {
            let classes = class_attr.split_whitespace().collect::<Vec<_>>().join(" ");
            if !classes.is_empty() {
                segment.push('.');
                segment.push_str(&classes);
            }
        }

        if same_tag_total > 1 {
            segment.push_str(&format!(":nth-of-type({})", same_tag_index));
        }

        segment
    }

    /// The selector Chrome REPORTS for an element, given its structural path.
    ///
    /// Chrome's `getSelector()` short-circuits to `#id` before walking the tree,
    /// and stops the walk at `body`, so the body element reports `html > body`.
    /// Neither affects descendant paths — a child of an id'd element still
    /// reports the full tag/class path — which is why the reported selector and
    /// the path used to build children are computed separately.
    fn reported_selector(selector_path: &str, attributes: &HashMap<String, String>) -> String {
        match attributes.get("id") {
            Some(id) if !id.is_empty() => format!("#{}", id),
            _ if selector_path == "body" => "html > body".to_string(),
            _ => selector_path.to_string(),
        }
    }

    /// Whether this tag opens a foreign-content subtree. Everything at or below
    /// it is SVG/MathML, so `className` is not a string there.
    fn enters_foreign_content(tag_lower: &str) -> bool {
        matches!(tag_lower, "svg" | "math")
    }

    /// Extend a parent path with one child segment.
    ///
    /// Returns the empty string — meaning "identity not tracked" — when the
    /// parent is untracked or the child is not an element. Anonymous and text
    /// boxes must never inherit a path.
    fn child_selector_path(selector_path: &str, segment: Option<&str>) -> String {
        match (selector_path.is_empty(), segment) {
            (false, Some(segment)) => format!("{} > {}", selector_path, segment),
            _ => String::new(),
        }
    }

    /// Compute the selector segment for each child node, aligned by index with
    /// `children`. Non-element nodes yield `None` — they have no identity.
    ///
    /// Done in the parent because `:nth-of-type` needs the full same-tag
    /// sibling count, which a child cannot see from its own position.
    fn child_selector_segments(
        children: &[Rc<Node>],
        parent_is_foreign: bool,
    ) -> Vec<Option<String>> {
        let mut totals: HashMap<String, usize> = HashMap::new();
        for child in children {
            if let NodeType::Element { tag_name, .. } = &child.node_type {
                *totals.entry(tag_name.to_lowercase()).or_insert(0) += 1;
            }
        }

        let mut seen: HashMap<String, usize> = HashMap::new();
        children
            .iter()
            .map(|child| match &child.node_type {
                NodeType::Element {
                    tag_name,
                    attributes,
                    ..
                } => {
                    let tag_lower = tag_name.to_lowercase();
                    let index = {
                        let counter = seen.entry(tag_lower.clone()).or_insert(0);
                        *counter += 1;
                        *counter
                    };
                    let total = totals.get(&tag_lower).copied().unwrap_or(1);
                    let is_foreign = parent_is_foreign || Self::enters_foreign_content(&tag_lower);
                    Some(Self::selector_segment(
                        &tag_lower,
                        attributes,
                        index,
                        total,
                        is_foreign,
                    ))
                }
                _ => None,
            })
            .collect()
    }

    fn build_layout_from_node_with_styles(
        &self,
        node: &Rc<Node>,
        stylesheets: &[Stylesheet],
        css_vars: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
    ) -> LayoutBox {
        self.build_layout_from_parent_style_and_path(
            node,
            stylesheets,
            css_vars,
            ancestors,
            None,
            &[],
            0,
            1,
            "",
            &Cell::new(0),
            false,
        )
    }

    /// Build a layout box, additionally threading the element-identity context
    /// the geometry oracle needs.
    ///
    /// `selector_path` is this element's Chrome-style path built from tag /
    /// class / nth-of-type segments, e.g. `body > div.container`. It is `"body"`
    /// for the body element and `""` when identity is not being tracked. Note
    /// this is the path used to build DESCENDANT paths, which is not always the
    /// reported selector: an element with an `id` reports `#id`, but its
    /// children still hang off the full path, exactly as Chrome's capture does.
    ///
    /// `element_ids` is a document-order counter shared across one build.
    #[allow(clippy::too_many_arguments)]
    fn build_layout_from_parent_style_and_path(
        &self,
        node: &Rc<Node>,
        stylesheets: &[Stylesheet],
        css_vars: &HashMap<String, String>,
        ancestors: &[(String, Vec<String>, Option<String>)],
        parent_style: Option<&ComputedStyle>,
        siblings_before: &[(String, Vec<String>, Option<String>)],
        element_index: usize,
        sibling_count: usize,
        selector_path: &str,
        element_ids: &Cell<usize>,
        in_foreign_content: bool,
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
                    // SAME selection rule as discover_images, or the loader
                    // caches under one key and layout looks up another — the
                    // exact cache-miss shape #113 fixed for relative URLs.
                    let src = attributes
                        .get("srcset")
                        .and_then(|ss| Self::pick_from_srcset(ss))
                        .or_else(|| attributes.get("src").cloned())
                        .unwrap_or_default();

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
                    // Absolute from here on: the box, the cache lookup and
                    // the display command must all use the same key.
                    let src = self
                        .resolve_resource_url(&src)
                        .map(|u| u.to_string())
                        .unwrap_or(src);

                    let loaded = Url::parse(&src).ok().and_then(|parsed_url| {
                        if let Some(cached) = self.image_manager.get_cached(&parsed_url) {
                            Some(cached)
                        } else if parsed_url.scheme() == "data" {
                            self.image_manager.load_blocking(parsed_url).ok()
                        } else {
                            None
                        }
                    });

                    // Vector images: the SVG's own sizing (viewBox/width/height)
                    // is the natural size the raster cache can't provide.
                    let svg_size = Url::parse(&src).ok().and_then(|u| {
                        self.svg_cache.get(u.as_str()).map(|svg| {
                            svg.get_size(
                                explicit_width.unwrap_or(300.0),
                                explicit_height.unwrap_or(150.0),
                            )
                        })
                    });

                    let (natural_width, natural_height) = match (&loaded, svg_size) {
                        (Some(image), _) => {
                            (image.natural_width as f32, image.natural_height as f32)
                        }
                        (None, Some((w, h))) => (w, h),
                        // Image unavailable at layout time: fall back to the
                        // width=/height= attributes, then the placeholder size.
                        (None, None) => match (explicit_width, explicit_height) {
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
                    // type=hidden generates NO box (HTML §4.10.5.1.1). We were
                    // rendering Google's hidden CSRF/state fields as a row of
                    // visible hash-string boxes (live, 2026-08-07).
                    if input_type.eq_ignore_ascii_case("hidden") {
                        return LayoutBox::new(BoxType::Block, {
                            let mut st = ComputedStyle::new();
                            st.display = rustkit_css::Display::None;
                            st
                        });
                    }
                    // Read through live edit state when the user has typed
                    // into this field; fall back to the authored value.
                    // The DOM attribute is never rewritten (there is no
                    // set_attribute), so this override is what makes typed
                    // text visible.
                    let value = self
                        .edit_value(node.id.raw())
                        .map(|(v, _)| v)
                        .unwrap_or_else(|| attributes.get("value").cloned().unwrap_or_default());
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

                    // Stamp identity BEFORE returning. Form controls return
                    // early, so they never reach the general element path's
                    // node_id assignment — which meant a hit test on an input
                    // reported no node and focus could never resolve. The
                    // unit tests missed it by hand-building boxes; only the
                    // production path exercises this.
                    let mut b = LayoutBox::new(BoxType::FormControl(control), style);
                    b.node_id = Some(node.id.raw());
                    if self.building_focus.get() == Some(node.id) {
                        b.focused_caret = self.edit_value(node.id.raw()).map(|(_, c)| c);
                    }
                    return b;
                }

                if tag_lower == "button" {
                    // A <button> is a flow container in every real engine —
                    // icon buttons (<button><svg/></button>, eBay-class UIs)
                    // have element children and no text. Rendering those as
                    // an opaque FormControl leaf discarded the children and
                    // stamped a literal "Button" placeholder (2026-08-05
                    // live session: whole grids of them). Only text-only
                    // buttons keep the leaf-widget fast path.
                    let has_element_children = node
                        .children()
                        .iter()
                        .any(|c| matches!(c.node_type, NodeType::Element { .. }));

                    if !has_element_children {
                        let text = node.text_content();
                        let label = if text.trim().is_empty() {
                            // No text, no children: an empty button renders
                            // empty, not a placeholder word.
                            attributes.get("value").cloned().unwrap_or_default()
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
                    // Element children present: fall through to normal box
                    // construction so the children lay out inside the button.
                }

                if tag_lower == "textarea" {
                    // Same read-through as <input>; a textarea's authored
                    // value is its text content rather than an attribute.
                    let value = self
                        .edit_value(node.id.raw())
                        .map(|(v, _)| v)
                        .unwrap_or_else(|| node.text_content());
                    let placeholder = attributes.get("placeholder").cloned().unwrap_or_default();
                    let rows = attributes
                        .get("rows")
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(2);
                    let cols = attributes
                        .get("cols")
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(20);

                    let mut b = LayoutBox::new(
                        BoxType::FormControl(rustkit_layout::FormControlType::TextArea {
                            value,
                            placeholder,
                            rows,
                            cols,
                        }),
                        style,
                    );
                    b.node_id = Some(node.id.raw());
                    if self.building_focus.get() == Some(node.id) {
                        b.focused_caret = self.edit_value(node.id.raw()).map(|(_, c)| c);
                    }
                    return b;
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

                    let mut b = LayoutBox::new(
                        BoxType::FormControl(rustkit_layout::FormControlType::Select {
                            options,
                            selected_index,
                            size,
                        }),
                        style,
                    );
                    b.node_id = Some(node.id.raw());
                    if self.building_focus.get() == Some(node.id) {
                        b.focused_caret = self.edit_value(node.id.raw()).map(|(_, c)| c);
                    }
                    return b;
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

                // Attach element identity so the geometry oracle can join this
                // box to Chrome's selector-keyed rects. Only ELEMENT boxes reach
                // here; anonymous, text and pseudo-element boxes are built
                // elsewhere and correctly keep `identity: None`.
                if !selector_path.is_empty() {
                    let reported = Self::reported_selector(selector_path, attributes);
                    let next_id = element_ids.get() + 1;
                    element_ids.set(next_id);
                    layout_box.set_identity(ElementIdentity {
                        element_id: next_id,
                        tag: tag_lower.clone(),
                        selector: reported,
                    });
                }

                // Every element box remembers which DOM node it came from.
                // This is what lets a click resolve to an element (focus,
                // form editing, event dispatch) instead of just a rectangle.
                layout_box.node_id = Some(node.id.raw());

                // Carry caret position onto the box when this element is the
                // focused text control, so the painter can draw the caret and
                // focus ring without reaching back into engine state.
                if self.building_focus.get() == Some(node.id) {
                    layout_box.focused_caret = self.edit_value(node.id.raw()).map(|(_, c)| c);
                }

                // Links carry their RAW href so a hit test can navigate
                // without walking back into the DOM. Resolution against the
                // document base URL happens at click time, where the view —
                // and therefore the base — is unambiguous; this builder is
                // view-agnostic and must not guess.
                if tag_lower == "a" {
                    if let Some(href) = attributes.get("href") {
                        let href = href.trim();
                        // javascript: and empty hrefs are not navigations;
                        // leaving them None keeps the click a no-op rather
                        // than a load of a bogus URL.
                        if !href.is_empty() && !href.starts_with("javascript:") {
                            layout_box.link_href = Some(href.to_string());
                        }
                    }
                }

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
                // Selector segments are computed here, not in the child, because
                // `:nth-of-type` needs the full same-tag sibling count.
                let children_are_foreign =
                    in_foreign_content || Self::enters_foreign_content(&tag_lower);
                let child_segments =
                    Self::child_selector_segments(&child_nodes, children_are_foreign);
                for (child_index, child) in child_nodes.iter().enumerate() {
                    let child_path = Self::child_selector_path(
                        selector_path,
                        child_segments.get(child_index).and_then(|s| s.as_deref()),
                    );
                    let child_box = self.build_layout_from_parent_style_and_path(
                        child,
                        stylesheets,
                        css_vars,
                        &child_ancestors,
                        Some(&style),
                        &preceding_siblings,
                        preceding_siblings.len(),
                        child_element_count,
                        &child_path,
                        element_ids,
                        children_are_foreign,
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
                // UA default background lives HERE, not in the painter: paint
                // used to substitute WHITE whenever computed alpha was 0,
                // which cannot tell "author said nothing" from "author said
                // transparent" (#83). #83 put this in four per-tag arms
                // further down the match — arms this grouped one had shadowed
                // since before that merge, so the engine half of #83 was
                // unreachable from the day it landed. The compiler said so on
                // every build ("unreachable pattern"); nobody read it until
                // Pete asked what the warnings meant.
                if tag_name != "button" {
                    // input/select/textarea get Chrome's white field; buttons
                    // are ButtonFace-themed, not white — leave them to the
                    // themed paint path.
                    style.background_color = rustkit_css::Color::WHITE;
                }
                if tag_name == "textarea" {
                    style.font_family = "monospace".to_string();
                }
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

        // Provenance is recorded from INSIDE this loop rather than by a
        // separate pass, so "which rule won" is answered by the same code
        // that decided it. A recorder that walked the rules again could
        // disagree with the cascade, and a diagnostic tool that disagrees
        // with the engine is worse than no tool.
        let recording = self.style_trace.borrow().is_some();
        let mut records: Vec<DeclarationRecord> = Vec::new();
        let mut order = 0usize;

        // Apply matching rules in order
        for (rule, specificity, _) in matching_rules {
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
                if recording {
                    records.push(DeclarationRecord {
                        property: decl.property.clone(),
                        value: resolved_value,
                        selector: rule.selector.clone(),
                        specificity,
                        origin: "author",
                        important: decl.important,
                        order,
                    });
                    order += 1;
                }
            }
        }

        // Parse inline style attribute if present (highest specificity)
        if let Some(style_attr) = attributes.get("style") {
            self.apply_inline_style(&mut style, style_attr, css_vars);
            if recording {
                self.record_inline_style(style_attr, css_vars, &mut records, &mut order);
            }
        }

        if recording {
            let id = attributes.get("id").cloned();
            let classes = attributes
                .get("class")
                .map(|c| c.split_whitespace().map(str::to_string).collect())
                .unwrap_or_default();
            // The full supported set, not just the declared properties: the
            // fixture sets padding via the `padding` shorthand, and an agent
            // asking for the computed `padding-left` should get 16px rather
            // than a hole because no rule spelled that longhand.
            let computed = Self::COMPUTED_PROPERTIES
                .iter()
                .filter_map(|p| {
                    Self::computed_value_of(&style, p).map(|v| (p.to_string(), v))
                })
                .collect();
            if let Some(trace) = self.style_trace.borrow_mut().as_mut() {
                trace.push(StyleRecord {
                    tag: tag_name.to_lowercase(),
                    id,
                    classes,
                    declarations: records,
                    computed,
                });
            }
        }

        style
    }

    /// Record the inline `style=` declarations that `apply_inline_style` just
    /// applied. Split out rather than folded into that function so the
    /// applying path stays byte-identical when recording is off.
    fn record_inline_style(
        &self,
        style_attr: &str,
        css_vars: &HashMap<String, String>,
        records: &mut Vec<DeclarationRecord>,
        order: &mut usize,
    ) {
        for declaration in style_attr.split(';') {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                continue;
            }
            if let Some((property, value)) = declaration.split_once(':') {
                records.push(DeclarationRecord {
                    property: property.trim().to_lowercase(),
                    value: self.resolve_css_variables(value.trim(), css_vars),
                    selector: "style=".to_string(),
                    // An inline declaration outranks any selector; CSS gives
                    // it a specificity above (1,0,0) rather than a tuple.
                    specificity: (usize::MAX, 0, 0),
                    origin: "author-inline",
                    important: false,
                    order: *order,
                });
                *order += 1;
            }
        }
    }

    /// Read one computed property back off the style the cascade produced.
    ///
    /// Deliberately a SUBSET. Every arm here is a property whose computed
    /// form is unambiguous to serialize; anything not listed returns `None`
    /// and the exporter reports it as unsupported rather than guessing. A
    /// wrong computed value in a tool built to adjudicate computed values
    /// would be the worst possible failure, so the honest gap is the safer
    /// default.
    const COMPUTED_PROPERTIES: &'static [&'static str] = &[
        "width",
        "height",
        "padding-top",
        "padding-right",
        "padding-bottom",
        "padding-left",
        "margin-top",
        "margin-right",
        "margin-bottom",
        "margin-left",
        "font-size",
        "font-weight",
        "color",
        "background-color",
        "display",
    ];

    fn computed_value_of(style: &ComputedStyle, property: &str) -> Option<String> {
        fn len(l: &rustkit_css::Length) -> String {
            match l {
                rustkit_css::Length::Px(v) => format!("{v}px"),
                other => format!("{other:?}"),
            }
        }
        fn color(c: &rustkit_css::Color) -> String {
            format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, c.a)
        }
        Some(match property {
            "width" => len(&style.width),
            "height" => len(&style.height),
            "padding-top" => len(&style.padding_top),
            "padding-right" => len(&style.padding_right),
            "padding-bottom" => len(&style.padding_bottom),
            "padding-left" => len(&style.padding_left),
            "margin-top" => len(&style.margin_top),
            "margin-right" => len(&style.margin_right),
            "margin-bottom" => len(&style.margin_bottom),
            "margin-left" => len(&style.margin_left),
            "font-size" => len(&style.font_size),
            "font-weight" => style.font_weight.0.to_string(),
            "color" => color(&style.color),
            "background-color" => color(&style.background_color),
            "display" => format!("{:?}", style.display).to_lowercase(),
            _ => return None,
        })
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
    /// Choose one candidate from an `srcset` attribute.
    ///
    /// HTML §4.8.4.2. Deliberately a SUBSET: candidates are parsed and the
    /// widest `w` (or highest `x`) wins, which is the right answer on a
    /// retina display and a defensible one everywhere. Full selection needs
    /// the `sizes` attribute and viewport/DPR math — that is a separate unit
    /// and is NOT claimed here.
    ///
    /// Why it exists at all: `srcset` had ZERO support, so a page serving
    /// images only via srcset (increasingly common; the `src` is often a
    /// 1x1 placeholder or absent) rendered NO IMAGE AT ALL. A wrong-density
    /// image is a rendering difference; no image is a hole.
    fn pick_from_srcset(srcset: &str) -> Option<String> {
        // `w` and `x` descriptors are NOT comparable — one is a pixel width,
        // the other a device ratio. The first version scaled x by 1000 to
        // rank them together; a test then showed 2x and 2000w colliding
        // exactly, with the tie decided by document order. Inventing a
        // common scale for incomparable units is the bug, not the tie.
        //
        // So: partition. If ANY width candidate exists, width decides
        // (that is the descriptor authors reach for when the rendered size
        // varies); otherwise density decides. Mixed srcsets are invalid per
        // HTML §4.8.4.2 anyway.
        let mut widest: Option<(f32, String)> = None;
        let mut densest: Option<(f32, String)> = None;
        let mut bare: Option<String> = None;

        for cand in srcset.split(',') {
            let mut parts = cand.split_whitespace();
            let url = match parts.next() {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };
            match parts.next() {
                // No descriptor means 1x (HTML §4.8.4.2).
                None => {
                    if bare.is_none() {
                        bare = Some(url.to_string());
                    }
                    if densest.as_ref().map(|(d, _)| 1.0 > *d).unwrap_or(true) {
                        densest = Some((1.0, url.to_string()));
                    }
                }
                Some(d) if d.ends_with('w') => {
                    if let Ok(w) = d[..d.len() - 1].parse::<f32>() {
                        if widest.as_ref().map(|(b, _)| w > *b).unwrap_or(true) {
                            widest = Some((w, url.to_string()));
                        }
                    }
                }
                Some(d) if d.ends_with('x') => {
                    if let Ok(x) = d[..d.len() - 1].parse::<f32>() {
                        if densest.as_ref().map(|(b, _)| x > *b).unwrap_or(true) {
                            densest = Some((x, url.to_string()));
                        }
                    }
                }
                Some(_) => {}
            }
        }

        widest
            .map(|(_, u)| u)
            .or_else(|| densest.map(|(_, u)| u))
            .or(bare)
    }

    fn discover_images(&self, document: &Document, base_url: Option<&Url>) -> Vec<(String, Url)> {
        let mut images = Vec::new();

        // Find all <img> elements
        let img_elements = document.get_elements_by_tag_name("img");

        for img_el in img_elements {
            if let NodeType::Element { attributes, .. } = &img_el.node_type {
                // srcset wins when present (that is the point of it); src is
                // the fallback and is often a placeholder on srcset pages.
                let chosen = attributes
                    .get("srcset")
                    .and_then(|ss| Self::pick_from_srcset(ss))
                    .or_else(|| attributes.get("src").cloned());

                if let Some(src) = chosen {
                    // Resolve relative URL
                    let resolved = if let Some(base) = base_url {
                        base.join(&src).ok()
                    } else {
                        Url::parse(&src).ok()
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

        // Fetch concurrently, but keep DOCUMENT ORDER in the result: the
        // cascade depends on stylesheet order, so `buffered` (ordered) is
        // load-bearing here where images use `buffer_unordered`.
        use futures::stream::StreamExt;
        const MAX_CONCURRENT_CSS_LOADS: usize = 6;

        let loader = self.loader.clone();
        let fetched: Vec<Option<Stylesheet>> = futures::stream::iter(urls.into_iter().map(|url| {
            let loader = loader.clone();
            async move {
                info!(%url, "Loading external stylesheet");
                match loader.fetch(Request::get(url.clone())).await {
                    Ok(response) => {
                        if response.ok() {
                            match response.text().await {
                                Ok(css_text) => match Stylesheet::parse(&css_text) {
                                    Ok(stylesheet) => {
                                        debug!(rules = stylesheet.rules.len(), %url, "Parsed external stylesheet");
                                        Some(stylesheet)
                                    }
                                    Err(e) => {
                                        warn!(?e, %url, "Failed to parse external stylesheet");
                                        None
                                    }
                                },
                                Err(e) => {
                                    warn!(?e, %url, "Failed to read stylesheet body");
                                    None
                                }
                            }
                        } else {
                            warn!(status = %response.status, %url, "Failed to fetch stylesheet");
                            None
                        }
                    }
                    Err(e) => {
                        warn!(?e, %url, "Failed to fetch stylesheet");
                        None
                    }
                }
            }
        }))
        .buffered(MAX_CONCURRENT_CSS_LOADS)
        .collect()
        .await;

        Ok(fetched.into_iter().flatten().collect())
    }

    /// Load images asynchronously and store in cache.
    pub async fn load_images(&mut self, id: EngineViewId) -> Result<usize, EngineError> {
        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let Some(document) = &view.document else {
            return Ok(0);
        };

        let base_url = view.url.as_ref();
        let images = self.discover_images(document.as_ref(), base_url);

        let image_manager = self.image_manager.clone();

        // Fetch concurrently with bounded parallelism. The serial loop cost
        // 69 seconds on a Wikipedia article whose ~30 thumbnails each burned
        // a sequential round-trip failing (2026-08-05 live session);
        // buffer_unordered polls the futures on this thread, so no Send
        // bounds are required and the engine stays single-threaded.
        use futures::stream::StreamExt;
        const MAX_CONCURRENT_IMAGE_LOADS: usize = 8;

        let mut pending = Vec::new();
        let mut svg_urls = Vec::new();
        let mut loaded = 0;
        for (_src, url) in images {
            if image_manager.is_cached(&url) || self.svg_cache.contains_key(url.as_str()) {
                debug!(%url, "Image already cached");
                loaded += 1;
                continue;
            }
            // SVG is vector content: ImageManager's raster decode rejects it
            // ("Unknown image format", every Wikipedia logo in the live
            // session). Routed by URL extension; SVG served from
            // extensionless URLs still falls through to the raster lane
            // (content-type routing is the named follow-up).
            if url.path().to_ascii_lowercase().ends_with(".svg") {
                svg_urls.push(url);
            } else {
                pending.push(url);
            }
        }

        // Concurrent like the raster lane (Prometheus, #104 R1: SVG was left
        // serial while images were parallelized). Parsing happens inside the
        // futures; only the cache insert is serialized afterwards, because
        // &mut self cannot be held across them.
        {
            use futures::stream::StreamExt;
            let loader = self.loader.clone();
            let parsed: Vec<Option<(String, rustkit_svg::SvgDocument)>> =
                futures::stream::iter(svg_urls.into_iter().map(|url| {
                    let loader = loader.clone();
                    async move {
                        info!(%url, "Loading SVG image");
                        match loader.fetch(Request::get(url.clone())).await {
                            Ok(response) if response.ok() => match response.text().await {
                                Ok(xml) => match rustkit_svg::SvgDocument::parse(&xml) {
                                    Ok(doc) => Some((url.to_string(), doc)),
                                    Err(e) => {
                                        warn!(?e, %url, "Failed to parse SVG image");
                                        None
                                    }
                                },
                                Err(e) => {
                                    warn!(?e, %url, "Failed to read SVG body");
                                    None
                                }
                            },
                            Ok(response) => {
                                warn!(status = %response.status, %url, "Failed to fetch SVG image");
                                None
                            }
                            Err(e) => {
                                warn!(?e, %url, "Failed to fetch SVG image");
                                None
                            }
                        }
                    }
                }))
                .buffer_unordered(MAX_CONCURRENT_IMAGE_LOADS)
                .collect()
                .await;

            for (url, doc) in parsed.into_iter().flatten() {
                self.svg_cache.insert(url, doc);
                loaded += 1;
            }
        }

        let results: Vec<bool> = futures::stream::iter(pending.into_iter().map(|url| {
            let image_manager = image_manager.clone();
            async move {
                info!(%url, "Loading image via ImageManager");
                match image_manager.load(url.clone()).await {
                    Ok(image) => {
                        debug!(
                            %url,
                            width = image.natural_width,
                            height = image.natural_height,
                            "Image loaded and cached"
                        );
                        true
                    }
                    Err(e) => {
                        warn!(?e, %url, "Failed to load image");
                        false
                    }
                }
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_IMAGE_LOADS)
        .collect()
        .await;

        loaded += results.into_iter().filter(|ok| *ok).count();

        Ok(loaded)
    }

    /// Load all subresources (stylesheets, images) for a view.
    pub async fn load_subresources(&mut self, id: EngineViewId) -> Result<(), EngineError> {
        // Load external stylesheets.
        //
        // The assignment below is UNCONDITIONAL on purpose. It used to sit
        // inside `if !external_stylesheets.is_empty()`, which meant a
        // document with no <link> never cleared the field — so navigating
        // from a styled page to an unstyled one left the PREVIOUS document's
        // rules applying to the new one. That is a cross-document style leak:
        // the new page renders wrong and nothing logs anything.
        //
        // Found by Athena on hiwave-windows (her #54 -> #59) and reported
        // across the fleet; this tree is the one she ported the shape FROM,
        // so it had the defect first.
        let external_stylesheets = self.load_external_stylesheets(id).await?;
        let count = external_stylesheets.len();

        // Relayout is needed when we HAVE new sheets, and equally when we
        // just cleared sheets a previous document left behind — dropping
        // rules changes rendering exactly as much as adding them does.
        let had_previous = self
            .views
            .get(&id)
            .map(|v| !v.external_stylesheets.is_empty())
            .unwrap_or(false);

        if let Some(view) = self.views.get_mut(&id) {
            view.external_stylesheets = external_stylesheets;
        }

        if count > 0 {
            info!(count, "Loaded external stylesheets");
        } else if had_previous {
            info!("No external stylesheets on this document — cleared the previous document's");
        }

        if count > 0 || had_previous {
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
            match self.render(id) {
                Ok(()) => {
                    if self.render_failing.remove(&id) {
                        info!(?id, "View render recovered");
                    }
                }
                Err(e) => {
                    // Warn once per failure episode, not per frame: a wedged
                    // surface renders at event rate and would flood the log,
                    // but total silence is how a frozen screen goes
                    // undiagnosed for a whole session.
                    if self.render_failing.insert(id) {
                        warn!(?id, error = %e, "View render failing; frames are NOT being presented (will log again on recovery)");
                    } else {
                        trace!(?id, error = %e, "View render still failing");
                    }
                }
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

    /// Export the view's display list — the paint commands built from the
    /// layout tree — as JSON.
    ///
    /// This is the other half of `export_layout_json`. Layout answers "what
    /// box did the engine compute"; this answers "what did it then decide to
    /// paint, in what order". A pixel diff cannot separate the two, so every
    /// paint-stage bug this engine has shipped — the advance contract, the
    /// gradient axis routing, the SVG break — had to be diagnosed by reading
    /// `{:?}` off a trace log. Emitting the list in a walkable form is the
    /// difference between inspecting the boundary and inferring it.
    ///
    /// The command list is FLAT and ORDERED, exactly as the renderer consumes
    /// it. Paint order is load-bearing (later commands cover earlier ones) and
    /// the push/pop clip, stacking-context, and transform commands only make
    /// sense as a sequence, so nesting them into a tree would invent structure
    /// the renderer does not have. `index` is the position in that sequence.
    ///
    /// Commands the exporter has not modelled are emitted as
    /// `{"op": "...", "modelled": false, "debug": "..."}` rather than dropped
    /// or flattened into a lie: an agent can still read the value, and the
    /// `modelled` flag says plainly that the shape is not a stable contract.
    pub fn export_display_list_json(
        &self,
        id: EngineViewId,
        path: &str,
    ) -> Result<(), EngineError> {
        use rustkit_layout::DisplayCommand as Cmd;

        let view = self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let display_list = view.display_list.as_ref().ok_or_else(|| {
            EngineError::RenderError("No display list available".into())
        })?;

        fn color(c: &rustkit_css::Color) -> serde_json::Value {
            serde_json::json!({ "r": c.r, "g": c.g, "b": c.b, "a": c.a })
        }

        fn rect(r: &rustkit_layout::Rect) -> serde_json::Value {
            serde_json::json!({
                "x": r.x,
                "y": r.y,
                "width": r.width,
                "height": r.height
            })
        }

        fn radius(r: &rustkit_layout::BorderRadius) -> serde_json::Value {
            serde_json::json!({
                "top_left": r.top_left,
                "top_right": r.top_right,
                "bottom_right": r.bottom_right,
                "bottom_left": r.bottom_left
            })
        }

        fn stops(list: &[rustkit_css::ColorStop]) -> serde_json::Value {
            serde_json::Value::Array(
                list.iter()
                    .map(|s| {
                        serde_json::json!({
                            "color": color(&s.color),
                            "position": s.position.as_ref().map(|p| format!("{:?}", p))
                        })
                    })
                    .collect(),
            )
        }

        fn command_to_json(cmd: &Cmd) -> serde_json::Value {
            match cmd {
                Cmd::SolidColor(c, r) => serde_json::json!({
                    "op": "solid_color", "color": color(c), "rect": rect(r)
                }),
                Cmd::RoundedRect {
                    color: c,
                    rect: r,
                    radius: rad,
                } => serde_json::json!({
                    "op": "rounded_rect",
                    "color": color(c),
                    "rect": rect(r),
                    "radius": radius(rad)
                }),
                Cmd::Border {
                    color: c,
                    rect: r,
                    top,
                    right,
                    bottom,
                    left,
                } => serde_json::json!({
                    "op": "border",
                    "color": color(c),
                    "rect": rect(r),
                    "widths": {
                        "top": top, "right": right, "bottom": bottom, "left": left
                    }
                }),
                Cmd::Text {
                    text,
                    x,
                    y,
                    color: c,
                    font_size,
                    font_family,
                    font_weight,
                    font_style,
                    advances,
                    ascent,
                } => serde_json::json!({
                    "op": "text",
                    "text": text,
                    "x": x,
                    "y": y,
                    "color": color(c),
                    "font_size": font_size,
                    "font_family": font_family,
                    "font_weight": font_weight,
                    "font_style": font_style,
                    // The ADVANCE CONTRACT, made visible. `advances: null`
                    // means paint fell back to re-deriving its own metrics
                    // instead of using layout's — the exact condition behind
                    // the width-drift class of bugs, and previously only
                    // observable by reading a trace log.
                    "advances": advances,
                    "ascent": ascent
                }),
                Cmd::TextDecoration {
                    x,
                    y,
                    width,
                    thickness,
                    color: c,
                    style,
                } => serde_json::json!({
                    "op": "text_decoration",
                    "x": x,
                    "y": y,
                    "width": width,
                    "thickness": thickness,
                    "color": color(c),
                    "style": format!("{:?}", style)
                }),
                Cmd::Image {
                    url,
                    src_rect,
                    dest_rect,
                    object_fit,
                    opacity,
                } => serde_json::json!({
                    "op": "image",
                    "url": url,
                    "src_rect": src_rect.as_ref().map(rect),
                    "dest_rect": rect(dest_rect),
                    "object_fit": format!("{:?}", object_fit),
                    "opacity": opacity
                }),
                Cmd::BackgroundImage {
                    url,
                    rect: r,
                    size,
                    position,
                    repeat,
                } => serde_json::json!({
                    "op": "background_image",
                    "url": url,
                    "rect": rect(r),
                    "size": format!("{:?}", size),
                    "position": { "x": position.0, "y": position.1 },
                    "repeat": format!("{:?}", repeat)
                }),
                Cmd::BoxShadow {
                    offset_x,
                    offset_y,
                    blur_radius,
                    spread_radius,
                    color: c,
                    rect: r,
                    inset,
                } => serde_json::json!({
                    "op": "box_shadow",
                    "offset_x": offset_x,
                    "offset_y": offset_y,
                    "blur_radius": blur_radius,
                    "spread_radius": spread_radius,
                    "color": color(c),
                    "rect": rect(r),
                    "inset": inset
                }),
                Cmd::LinearGradient {
                    rect: r,
                    direction,
                    stops: s,
                    repeating,
                    border_radius,
                } => serde_json::json!({
                    "op": "linear_gradient",
                    "rect": rect(r),
                    "direction": format!("{:?}", direction),
                    "stops": stops(s),
                    "repeating": repeating,
                    "border_radius": radius(border_radius)
                }),
                Cmd::RadialGradient {
                    rect: r,
                    shape,
                    size,
                    center,
                    stops: s,
                    repeating,
                    border_radius,
                } => serde_json::json!({
                    "op": "radial_gradient",
                    "rect": rect(r),
                    "shape": format!("{:?}", shape),
                    "size": format!("{:?}", size),
                    "center": { "x": center.0, "y": center.1 },
                    "stops": stops(s),
                    "repeating": repeating,
                    "border_radius": radius(border_radius)
                }),
                Cmd::ConicGradient {
                    rect: r,
                    from_angle,
                    center,
                    stops: s,
                    repeating,
                    border_radius,
                } => serde_json::json!({
                    "op": "conic_gradient",
                    "rect": rect(r),
                    "from_angle": from_angle,
                    "center": { "x": center.0, "y": center.1 },
                    "stops": stops(s),
                    "repeating": repeating,
                    "border_radius": radius(border_radius)
                }),
                Cmd::PushClip(r) => serde_json::json!({
                    "op": "push_clip", "rect": rect(r)
                }),
                Cmd::PushClipRounded { rect: r, radius: rad } => serde_json::json!({
                    "op": "push_clip_rounded",
                    "rect": rect(r),
                    "border_radius": radius(rad)
                }),
                Cmd::PopClip => serde_json::json!({ "op": "pop_clip" }),
                Cmd::PushStackingContext { z_index, rect: r } => serde_json::json!({
                    "op": "push_stacking_context", "z_index": z_index, "rect": rect(r)
                }),
                Cmd::PopStackingContext => {
                    serde_json::json!({ "op": "pop_stacking_context" })
                }
                Cmd::PushTransform { matrix, origin } => serde_json::json!({
                    "op": "push_transform",
                    "matrix": matrix,
                    "origin": { "x": origin.0, "y": origin.1 }
                }),
                Cmd::PopTransform => serde_json::json!({ "op": "pop_transform" }),
                // Not yet modelled: form controls, carets, focus rings, and
                // the SVG primitives. Named and dumped rather than dropped.
                other => serde_json::json!({
                    "op": display_command_op_name(other),
                    "modelled": false,
                    "debug": format!("{:?}", other)
                }),
            }
        }

        /// The variant name, for commands the exporter does not model. Kept
        /// separate so an agent can still filter by op without parsing Debug.
        fn display_command_op_name(cmd: &Cmd) -> &'static str {
            match cmd {
                Cmd::TextInput { .. } => "text_input",
                Cmd::Button { .. } => "button",
                Cmd::FocusRing { .. } => "focus_ring",
                Cmd::Caret { .. } => "caret",
                Cmd::BackdropFilter { .. } => "backdrop_filter",
                Cmd::GradientText { .. } => "gradient_text",
                Cmd::FillRect { .. } => "fill_rect",
                Cmd::StrokeRect { .. } => "stroke_rect",
                Cmd::FillCircle { .. } => "fill_circle",
                Cmd::StrokeCircle { .. } => "stroke_circle",
                Cmd::FillEllipse { .. } => "fill_ellipse",
                Cmd::Line { .. } => "line",
                Cmd::Polyline { .. } => "polyline",
                Cmd::FillPolygon { .. } => "fill_polygon",
                Cmd::StrokePolygon { .. } => "stroke_polygon",
                _ => "unknown",
            }
        }

        let commands: Vec<serde_json::Value> = display_list
            .commands
            .iter()
            .enumerate()
            .map(|(index, cmd)| {
                let mut value = command_to_json(cmd);
                if let Some(object) = value.as_object_mut() {
                    object.insert("index".into(), serde_json::json!(index));
                }
                value
            })
            .collect();

        let (width, height) = self
            .compositor
            .get_surface_size(view.viewhost_id)
            .unwrap_or((0, 0));

        let wrapper = serde_json::json!({
            "version": 1,
            "viewport": { "width": width, "height": height },
            "count": commands.len(),
            "commands": commands
        });

        let json_str = serde_json::to_string_pretty(&wrapper).map_err(|e| {
            EngineError::RenderError(format!("JSON serialization failed: {}", e))
        })?;

        std::fs::write(path, json_str).map_err(|e| {
            EngineError::RenderError(format!("Failed to write display list file: {}", e))
        })?;

        info!(?id, path, count = display_list.commands.len(), "Display list exported");
        Ok(())
    }

    /// Export the cascade for the elements matching `selector`: the computed
    /// values, and for each declared property the rule that WON plus every
    /// rule it beat.
    ///
    /// Layout answers what box the engine computed and the display list
    /// answers what it painted; neither answers *why*. When a declaration is
    /// parsed, matched, and then silently overridden, both of those tools
    /// report the consequence and none of them report the cause — which is
    /// how seven dead behaviours in this engine were found by hand rather
    /// than by asking it.
    ///
    /// Requires [`Engine::set_style_recording(true)`] before the page was
    /// loaded, and says so rather than returning an empty result: a tool that
    /// answers "no rules" when it simply was not listening is worse than one
    /// that refuses.
    ///
    /// Two limits are reported in the payload rather than papered over:
    ///
    /// - **`origin` is only ever `author` or `author-inline`.** The UA sheet
    ///   is a hardcoded Rust `match` on tag name, not parsed rules, so a
    ///   UA-set property has no selector to cite. Properties with no author
    ///   declaration carry `"winner": null` and `"origin": "user-agent-or-initial"`.
    /// - **`!important` is recorded but not honoured by the cascade**, which
    ///   orders by specificity alone. An `important: true` declaration that
    ///   is not the winner is a real engine bug, and it is visible here.
    pub fn export_style_json(
        &self,
        id: EngineViewId,
        selector: &str,
        path: &str,
    ) -> Result<(), EngineError> {
        self.views.get(&id).ok_or(EngineError::ViewNotFound(id))?;

        let borrowed = self.style_trace.borrow();
        let trace = borrowed.as_ref().ok_or_else(|| {
            EngineError::RenderError(
                "style recording is off — call set_style_recording(true) before loading".into(),
            )
        })?;

        let query = SimpleSelector::parse(selector).ok_or_else(|| {
            EngineError::RenderError(format!(
                "hiwave_style understands simple selectors only \
                 (`tag`, `.class`, `#id`, `tag.class`); got {selector:?}"
            ))
        })?;

        let elements: Vec<serde_json::Value> = trace
            .iter()
            .filter(|record| query.matches(record))
            .map(|record| {
                // Winner = highest application order for that property, which
                // is by construction the declaration that wrote the field
                // last. Deriving it from the recorded order rather than
                // re-comparing specificity keeps this from being a second
                // opinion about what the cascade did.
                let mut properties: std::collections::BTreeMap<&str, Vec<&DeclarationRecord>> =
                    std::collections::BTreeMap::new();
                for decl in &record.declarations {
                    properties.entry(decl.property.as_str()).or_default().push(decl);
                }

                let mut declared: Vec<serde_json::Value> = properties
                    .into_iter()
                    .map(|(property, mut decls)| {
                        decls.sort_by_key(|d| d.order);
                        let winner = decls.last().copied();
                        let overridden: Vec<serde_json::Value> = decls
                            [..decls.len().saturating_sub(1)]
                            .iter()
                            .map(|d| Self::declaration_to_json(d))
                            .collect();
                        serde_json::json!({
                            "property": property,
                            "computed": Self::computed_value_of_recorded(record, property),
                            "winner": winner.map(Self::declaration_to_json),
                            "origin": winner.map(|w| w.origin),
                            "overridden": overridden,
                        })
                    })
                    .collect();

                // A property that no author rule set still has a computed
                // value, and "nothing declared this" is an ANSWER — it is how
                // an agent tells "the author's rule lost" apart from "the
                // author never wrote one". Emitting it with a null winner
                // says that; omitting the property entirely would leave the
                // caller unable to distinguish the two without guessing.
                let authored: std::collections::BTreeSet<&str> = record
                    .declarations
                    .iter()
                    .map(|d| d.property.as_str())
                    .collect();
                for property in Self::COMPUTED_PROPERTIES {
                    if authored.contains(property) {
                        continue;
                    }
                    declared.push(serde_json::json!({
                        "property": property,
                        "computed": Self::computed_value_of_recorded(record, property),
                        "winner": serde_json::Value::Null,
                        // Not a cop-out: the UA sheet is a hardcoded match on
                        // tag name rather than parsed rules, so there is no
                        // selector to cite, and the initial value is
                        // indistinguishable from it at this layer.
                        "origin": "user-agent-or-initial",
                        "overridden": [],
                    }));
                }

                let computed: serde_json::Map<String, serde_json::Value> = record
                    .computed
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();

                serde_json::json!({
                    "tag": record.tag,
                    "id": record.id,
                    "classes": record.classes,
                    "computed": computed,
                    "declared": declared,
                })
            })
            .collect();

        let wrapper = serde_json::json!({
            "version": 1,
            "selector": selector,
            "count": elements.len(),
            "elements": elements,
            "limits": {
                "origins": "author and author-inline only — the UA stylesheet is a \
                            hardcoded match on tag name, not parsed rules, so it has no \
                            selector to cite",
                "important": "recorded but NOT honoured by this cascade, which orders by \
                              specificity alone; an important declaration that is not the \
                              winner is an engine bug, not a reporting artefact",
                "computed_properties": Self::COMPUTED_PROPERTIES,
            }
        });

        let json_str = serde_json::to_string_pretty(&wrapper).map_err(|e| {
            EngineError::RenderError(format!("JSON serialization failed: {}", e))
        })?;
        std::fs::write(path, json_str).map_err(|e| {
            EngineError::RenderError(format!("Failed to write style file: {}", e))
        })?;

        info!(?id, path, selector, count = elements.len(), "Style cascade exported");
        Ok(())
    }

    fn declaration_to_json(d: &DeclarationRecord) -> serde_json::Value {
        let (a, b, c) = d.specificity;
        serde_json::json!({
            "value": d.value,
            "selector": d.selector,
            // usize::MAX marks an inline declaration, which CSS ranks above
            // every selector rather than giving it a tuple.
            "specificity": if a == usize::MAX {
                serde_json::Value::String("inline".into())
            } else {
                serde_json::json!([a, b, c])
            },
            "origin": d.origin,
            "important": d.important,
        })
    }

    /// The computed value for a declared property, or `null` when the
    /// property is a shorthand or otherwise outside the serializable subset.
    fn computed_value_of_recorded(record: &StyleRecord, property: &str) -> serde_json::Value {
        record
            .computed
            .iter()
            .find(|(k, _)| k == property)
            .map(|(_, v)| serde_json::Value::String(v.clone()))
            .unwrap_or(serde_json::Value::Null)
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

            // Scroll is applied at render time as a whole-page translate:
            // the display list stays in document coordinates and the GPU
            // shifts it by the clamped offset scroll_view() maintains.
            let scroll_offset = self
                .views
                .get(&id)
                .map(|v| v.scroll_offset)
                .unwrap_or((0.0, 0.0));

            // Render using display list if available, otherwise just clear to background
            {
                let _execute_span = tracing::info_span!("renderer_execute", cmd_count).entered();
                if let (Some(renderer), Some(display_list)) = (&mut self.renderer, display_list) {
                    if scroll_offset == (0.0, 0.0) {
                        renderer
                            .execute(&display_list.commands, &texture_view)
                            .map_err(|e| EngineError::RenderError(e.to_string()))?;
                    } else {
                        let mut scrolled: Vec<rustkit_layout::DisplayCommand> =
                            Vec::with_capacity(display_list.commands.len() + 2);
                        scrolled.push(rustkit_layout::DisplayCommand::PushTransform {
                            matrix: [1.0, 0.0, 0.0, 1.0, -scroll_offset.0, -scroll_offset.1],
                            origin: (0.0, 0.0),
                        });
                        scrolled.extend(display_list.commands.iter().cloned());
                        scrolled.push(rustkit_layout::DisplayCommand::PopTransform);
                        renderer
                            .execute(&scrolled, &texture_view)
                            .map_err(|e| EngineError::RenderError(e.to_string()))?;
                    }
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

/// Convert one layout box to its JSON form for `export_layout_json`.
///
/// Module-level rather than nested so it can be tested directly: the engine
/// tests that go through `build_layout_from_document` need a GPU compositor and
/// SKIP when none is present, which would make an identity test vacuous on any
/// machine without a GPU adapter.
fn layout_box_to_json(layout_box: &LayoutBox) -> serde_json::Value {
    // Element identity, when this box came from a DOM element. Absent
    // on anonymous and text boxes — the geometry oracle must SKIP
    // those rather than pair them positionally with Chrome elements.
    // Emitting a placeholder here would manufacture geometry failures.
    let mut value = layout_box_body_to_json(layout_box);
    if let (Some(identity), Some(object)) = (layout_box.identity(), value.as_object_mut()) {
        object.insert("element_id".into(), identity.element_id.into());
        object.insert("tag".into(), identity.tag.clone().into());
        object.insert("selector".into(), identity.selector.clone().into());
    }
    value
}

fn layout_box_body_to_json(layout_box: &LayoutBox) -> serde_json::Value {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_view_id_uniqueness() {
        let id1 = EngineViewId::new();
        let id2 = EngineViewId::new();
        assert_ne!(id1, id2);
    }

    fn record(tag: &str, id: Option<&str>, classes: &[&str]) -> StyleRecord {
        StyleRecord {
            tag: tag.to_string(),
            id: id.map(str::to_string),
            classes: classes.iter().map(|c| c.to_string()).collect(),
            declarations: Vec::new(),
            computed: Vec::new(),
        }
    }

    #[test]
    fn simple_selector_matches_tag_class_and_id() {
        let hero = record("div", Some("main"), &["hero", "wide"]);

        assert!(SimpleSelector::parse(".hero").unwrap().matches(&hero));
        assert!(SimpleSelector::parse("div").unwrap().matches(&hero));
        assert!(SimpleSelector::parse("#main").unwrap().matches(&hero));
        assert!(SimpleSelector::parse("div.hero").unwrap().matches(&hero));
        // Every class in the query must be present, not just one.
        assert!(SimpleSelector::parse(".hero.wide").unwrap().matches(&hero));
        assert!(!SimpleSelector::parse(".hero.narrow").unwrap().matches(&hero));
        assert!(!SimpleSelector::parse("span.hero").unwrap().matches(&hero));
        assert!(!SimpleSelector::parse("#other").unwrap().matches(&hero));
    }

    #[test]
    fn simple_selector_refuses_what_it_cannot_answer() {
        // Combinators, attribute and pseudo selectors need tree/state context
        // the style trace does not keep. Refusing them is the contract: an
        // approximate match would answer a different question invisibly.
        for unsupported in [
            "div p", "div > p", "div + p", "div ~ p", "a, b", "[type=text]",
            "a:hover", "*", "", "   ", ".", "#",
        ] {
            assert!(
                SimpleSelector::parse(unsupported).is_none(),
                "should have refused {unsupported:?}"
            );
        }
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

    /// A new document must not inherit the previous document's stylesheets.
    ///
    /// T-RED: with the `load_html` clear removed, this fails — the view still
    /// reports the stale sheet after loading unrelated inline content, which
    /// is a cross-document style leak (the new page renders under the old
    /// page's rules and nothing logs it).
    ///
    /// Reported across the fleet by Athena from hiwave-windows #54 -> #59.
    /// This tree is the one that shape was ported FROM, so it had the defect
    /// first; the reference tree being wrong is exactly why a port finding
    /// has to be checked upstream instead of assumed local.
    // REQUIRES: cargo test -p rustkit-engine --features headless
    //
    // `headless` is not a default feature, and I checked rather than assumed:
    // `cargo test --workspace` does NOT compile this test in — feature
    // unification from parity-capture does not reach this crate's own lib-test
    // target, so the workspace run reports it as "0 tests" with no warning.
    // Stated here because a gated test that nobody notices is skipped provides
    // the appearance of coverage and none of the substance, which is the same
    // defect class as everything else this engine keeps getting caught by.
    //
    // This is a live argument for the workspace test gate that CI still does
    // not have (CI builds one crate of 38 and runs no tests at all).
    #[cfg(feature = "headless")]
    #[test]
    fn load_html_does_not_inherit_the_previous_documents_stylesheets() {
        let mut engine = match EngineBuilder::new().javascript_enabled(false).build() {
            Ok(e) => e,
            Err(_) => return, // no GPU adapter in this environment; nothing to assert
        };
        let bounds = rustkit_viewhost::Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        let id = match engine.create_headless_view(bounds) {
            Ok(id) => id,
            Err(_) => return,
        };

        // Stand in for "a previous document brought stylesheets with it".
        // Asserting the setup took effect first, so a later green cannot come
        // from the field having been empty all along — that would make this
        // test pass for the wrong reason.
        engine
            .views
            .get_mut(&id)
            .expect("view exists")
            .external_stylesheets
            .push(Stylesheet::default());
        assert_eq!(
            engine.views[&id].external_stylesheets.len(),
            1,
            "setup failed: the stale sheet was never installed, so this test would be vacuous"
        );

        engine
            .load_html(id, "<html><body><p>unrelated</p></body></html>")
            .expect("inline load succeeds");

        assert!(
            engine.views[&id].external_stylesheets.is_empty(),
            "load_html left {} stylesheet(s) from the previous document on the view — \
             cross-document style leak",
            engine.views[&id].external_stylesheets.len()
        );
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(ResourceLoader::new(LoaderConfig::default()).expect("loader")),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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
            
            font_loader: Arc::new(FontLoader::new()),viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            loader: Arc::new(
                ResourceLoader::new(LoaderConfig::default()).expect("Failed to create loader"),
            ),
            image_manager: Arc::new(ImageManager::new()),
            event_tx,
            event_rx: Some(event_rx),
            style_trace: std::cell::RefCell::new(None),
            render_failing: std::collections::HashSet::new(),
            svg_cache: std::collections::HashMap::new(),
            building_focus: std::cell::Cell::new(None),
            building_view: std::cell::Cell::new(None),
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

#[cfg(test)]
mod element_identity_tests {
    use super::*;

    // ---- P0a-0: element identity for the geometry oracle ----
    //
    // These tests deliberately avoid `build_layout_from_document`, which needs a
    // GPU compositor and silently `return`s when none is available. A test that
    // skips on the machine running it is a gate that cannot go red, which is the
    // instrument failure this campaign exists to end. Everything below runs on
    // any machine.

    fn attrs(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// The join key must reproduce Chrome's capture byte-for-byte. Every string
    /// asserted here is copied verbatim out of
    /// `baselines/chrome-148/websuite/card-grid/layout-rects.json`.
    #[test]
    #[cfg(target_os = "macos")]
    fn ua_form_control_defaults_reach_computed_style() {
        // Gated to macOS because it constructs a real Engine (Compositor::new
        // wants a device) — which the macos-latest CI leg already does for the
        // whole parity swarm, so this is exercised in CI, not vacuous.
        //
        // Why it exists: #83's four per-tag UA arms sat behind a shadowing
        // grouped arm, unreachable from the day they merged, with every test
        // green — the compiler's "unreachable pattern" warning was the only
        // witness until Pete asked what the warnings meant.
        let engine = Engine::new(EngineConfig::default()).expect("engine");
        let empty = std::collections::HashMap::new();
        let vars = HashMap::new();
        let style_of = |tag: &str| {
            engine.compute_style_for_element(tag, &empty, &[], &vars, &[], &[], 0, 1, None)
        };

        let input = style_of("input");
        assert_eq!(input.background_color, rustkit_css::Color::WHITE,
            "input must carry the UA white field background");
        assert_eq!(input.display, rustkit_css::Display::InlineBlock);
        assert_eq!(style_of("select").background_color, rustkit_css::Color::WHITE);
        let textarea = style_of("textarea");
        assert_eq!(textarea.background_color, rustkit_css::Color::WHITE);
        assert_eq!(textarea.font_family, "monospace");

        // input=WHITE ∧ button≠WHITE kills BOTH ancestors of this code:
        // the grouped-arm-only version had no backgrounds anywhere, and the
        // dead-arm version can never fire. Buttons are ButtonFace-themed.
        assert_ne!(style_of("button").background_color, rustkit_css::Color::WHITE);
    }

    #[test]
    fn selector_segments_match_committed_chrome_baseline() {
        // `body > div.header:nth-of-type(1)` — two sibling divs, so indexed.
        assert_eq!(
            Engine::selector_segment("div", &attrs(&[("class", "header")]), 1, 2, false),
            "div.header:nth-of-type(1)"
        );
        // `... > h1` — the only h1 among its siblings, so NO nth-of-type.
        assert_eq!(Engine::selector_segment("h1", &attrs(&[]), 1, 1, false), "h1");
        // A unique tag that nonetheless carries a class.
        assert_eq!(
            Engine::selector_segment("div", &attrs(&[("class", "grid")]), 2, 2, false),
            "div.grid:nth-of-type(2)"
        );
    }

    /// Chrome's capture concatenated the RAW `className` after one dot, so a
    /// multi-class element yields `div.card-icon purple` — space and all. It is
    /// not valid CSS; it is the committed key, and 572 baseline selectors use
    /// this form. Emitting the "correct" `div.card-icon.purple` would join
    /// against nothing.
    #[test]
    fn multi_class_selector_keeps_the_baseline_space_form() {
        assert_eq!(
            Engine::selector_segment("div", &attrs(&[("class", "card-icon purple")]), 1, 1, false),
            "div.card-icon purple"
        );
    }

    #[test]
    fn nth_of_type_is_omitted_for_a_lone_sibling_and_present_otherwise() {
        assert_eq!(Engine::selector_segment("p", &attrs(&[]), 1, 1, false), "p");
        assert_eq!(
            Engine::selector_segment("p", &attrs(&[]), 2, 3, false),
            "p:nth-of-type(2)"
        );
    }

    /// An id short-circuits the whole path (`#versionBadge` in the about
    /// baseline), and body reports `html > body`.
    #[test]
    fn reported_selector_honors_id_short_circuit_and_body_root() {
        assert_eq!(
            Engine::reported_selector("body > span.badge", &attrs(&[("id", "versionBadge")])),
            "#versionBadge"
        );
        assert_eq!(
            Engine::reported_selector("body", &attrs(&[])),
            "html > body"
        );
        assert_eq!(
            Engine::reported_selector("body > div.header:nth-of-type(1)", &attrs(&[])),
            "body > div.header:nth-of-type(1)"
        );
        // An id on body still wins over the body special case.
        assert_eq!(
            Engine::reported_selector("body", &attrs(&[("id", "top")])),
            "#top"
        );
    }

    /// The `Option` on identity is load-bearing. Anonymous and text boxes have
    /// no originating element; if the export gave them a selector the oracle
    /// would pair them with real Chrome elements and report geometry failures
    /// that do not exist.
    #[test]
    fn export_omits_identity_for_anonymous_and_text_boxes() {
        use rustkit_css::ComputedStyle;

        let text = LayoutBox::new(BoxType::Text("hello".into()), ComputedStyle::new());
        let json = layout_box_to_json(&text);
        assert_eq!(json["type"], "text");
        assert!(
            json.get("selector").is_none() && json.get("element_id").is_none(),
            "text box leaked an identity into the export: {json}"
        );

        let anon = LayoutBox::new(BoxType::AnonymousBlock, ComputedStyle::new());
        let json = layout_box_to_json(&anon);
        assert_eq!(json["type"], "anonymous_block");
        assert!(
            json.get("selector").is_none() && json.get("element_id").is_none(),
            "anonymous box leaked an identity into the export: {json}"
        );
    }

    /// The other half: a box that DOES come from an element must carry all
    /// three fields, or the oracle has no join key at all.
    #[test]
    fn export_emits_identity_for_element_boxes() {
        use rustkit_css::ComputedStyle;

        let mut element = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        element.set_identity(ElementIdentity {
            element_id: 7,
            tag: "div".into(),
            selector: "body > div.header:nth-of-type(1)".into(),
        });

        let json = layout_box_to_json(&element);
        assert_eq!(json["element_id"], 7);
        assert_eq!(json["tag"], "div");
        assert_eq!(json["selector"], "body > div.header:nth-of-type(1)");
    }

    /// Image and form-control boxes take early-return paths in the export. They
    /// are still elements, so they must still be joinable — this is the case a
    /// naive "add the fields at the end" change silently misses.
    #[test]
    fn export_emits_identity_for_image_and_form_control_boxes() {
        use rustkit_css::ComputedStyle;

        let mut image = LayoutBox::new(
            BoxType::Image {
                url: String::new(),
                natural_width: 10.0,
                natural_height: 10.0,
            },
            ComputedStyle::new(),
        );
        image.set_identity(ElementIdentity {
            element_id: 3,
            tag: "img".into(),
            selector: "body > img".into(),
        });
        let json = layout_box_to_json(&image);
        assert_eq!(json["type"], "image");
        assert_eq!(
            json["selector"], "body > img",
            "image box lost its join key"
        );
        assert_eq!(json["element_id"], 3);
    }

    /// `set_identity` is the only way in, so `element_id` and `identity` can
    /// never disagree — a box either joins or is excluded, never half of each.
    #[test]
    fn identity_and_element_id_stay_in_lockstep() {
        use rustkit_css::ComputedStyle;

        let mut b = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        assert!(b.identity().is_none() && b.element_id().is_none());

        b.set_identity(ElementIdentity {
            element_id: 42,
            tag: "section".into(),
            selector: "body > section".into(),
        });
        assert_eq!(b.element_id(), Some(42));
        assert_eq!(b.identity().map(|i| i.element_id), Some(42));
    }

    /// The join key is only worth anything if it actually JOINS. This walks the
    /// real fixture DOMs with the same three helpers the layout builder uses
    /// (`child_selector_segments`, `child_selector_path`, `reported_selector`)
    /// and checks the selectors produced against the committed Chrome
    /// baselines. Needs no GPU, so it runs everywhere.
    ///
    /// Chrome's capture skips zero-size elements and head-ish tags, so RustKit
    /// legitimately produces selectors Chrome does not have. The direction that
    /// matters for the oracle is the other one: every Chrome element must be
    /// findable, or the geometry gate silently scores fewer boxes than it
    /// claims.
    #[test]
    fn every_chrome_baseline_selector_is_reproduced_on_the_real_corpus() {
        use rustkit_dom::NodeType;
        use std::path::PathBuf;

        fn walk(node: &Rc<Node>, path: &str, foreign: bool, out: &mut Vec<String>) {
            if path.is_empty() {
                return;
            }
            let mut children_foreign = foreign;
            if let NodeType::Element {
                tag_name,
                attributes,
                ..
            } = &node.node_type
            {
                out.push(Engine::reported_selector(path, attributes));
                children_foreign =
                    foreign || Engine::enters_foreign_content(&tag_name.to_lowercase());
            }
            let children = node.children();
            let segments = Engine::child_selector_segments(&children, children_foreign);
            for (i, child) in children.iter().enumerate() {
                let child_path =
                    Engine::child_selector_path(path, segments.get(i).and_then(|s| s.as_deref()));
                walk(child, &child_path, children_foreign, out);
            }
        }

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let cases = [
            ("websuite/cases/card-grid", "baselines/chrome-148/websuite/card-grid"),
            ("websuite/cases/article-typography", "baselines/chrome-148/websuite/article-typography"),
            ("websuite/cases/flex-positioning", "baselines/chrome-148/websuite/flex-positioning"),
            ("websuite/cases/css-selectors", "baselines/chrome-148/websuite/css-selectors"),
            ("websuite/cases/sticky-scroll", "baselines/chrome-148/websuite/sticky-scroll"),
            ("websuite/cases/image-gallery", "baselines/chrome-148/websuite/image-gallery"),
            ("websuite/cases/form-elements", "baselines/chrome-148/websuite/form-elements"),
            ("websuite/cases/gradient-backgrounds", "baselines/chrome-148/websuite/gradient-backgrounds"),
            ("websuite/micro/backgrounds", "baselines/chrome-148/micro/backgrounds"),
            ("websuite/micro/bg-pure", "baselines/chrome-148/micro/bg-pure"),
            ("websuite/micro/bg-solid", "baselines/chrome-148/micro/bg-solid"),
            ("websuite/micro/combinators", "baselines/chrome-148/micro/combinators"),
            ("websuite/micro/form-controls", "baselines/chrome-148/micro/form-controls"),
            ("websuite/micro/gpu-gradient-regression", "baselines/chrome-148/micro/gpu-gradient-regression"),
            ("websuite/micro/gradient-no-radius", "baselines/chrome-148/micro/gradient-no-radius"),
            ("websuite/micro/gradient-radius-only", "baselines/chrome-148/micro/gradient-radius-only"),
            ("websuite/micro/gradients", "baselines/chrome-148/micro/gradients"),
            ("websuite/micro/images-intrinsic", "baselines/chrome-148/micro/images-intrinsic"),
            ("websuite/micro/pseudo-classes", "baselines/chrome-148/micro/pseudo-classes"),
            ("websuite/micro/rounded-corners", "baselines/chrome-148/micro/rounded-corners"),
            ("websuite/micro/specificity", "baselines/chrome-148/micro/specificity"),
            ("crates/hiwave-app/src/ui|about.html", "baselines/chrome-148/builtins/about"),
            ("crates/hiwave-app/src/ui|new_tab.html", "baselines/chrome-148/builtins/new_tab"),
            ("crates/hiwave-app/src/ui|settings.html", "baselines/chrome-148/builtins/settings"),
            ("crates/hiwave-app/src/ui|shelf.html", "baselines/chrome-148/builtins/shelf"),
            ("crates/hiwave-app/src/ui|chrome_rustkit.html", "baselines/chrome-148/builtins/chrome_rustkit"),
        ];

        let mut checked = 0usize;
        let mut total_expected = 0usize;
        let mut missing: Vec<String> = Vec::new();

        for (case_dir, baseline_dir) in cases {
            let html_path = match case_dir.split_once('|') {
                Some((dir, file)) => root.join(dir).join(file),
                None => root.join(case_dir).join("index.html"),
            };
            let rects_path = root.join(baseline_dir).join("layout-rects.json");
            let (html, rects) = match (
                std::fs::read_to_string(&html_path),
                std::fs::read_to_string(&rects_path),
            ) {
                (Ok(h), Ok(r)) => (h, r),
                _ => continue, // corpus not checked out; nothing to assert
            };

            let document = Rc::new(Document::parse_html(&html).expect("fixture parses"));
            let body = document.body().expect("fixture has a body");
            let mut produced = Vec::new();
            walk(&body, "body", false, &mut produced);
            let produced: std::collections::HashSet<&str> =
                produced.iter().map(|s| s.as_str()).collect();

            let baseline: serde_json::Value = serde_json::from_str(&rects).expect("baseline parses");
            let elements = baseline["elements"].as_array().expect("elements array");
            assert!(!elements.is_empty(), "{case_dir}: empty baseline");

            for element in elements {
                let selector = element["selector"].as_str().expect("selector is a string");
                total_expected += 1;
                if !produced.contains(selector) {
                    missing.push(format!("{case_dir} :: {selector}"));
                }
            }
            checked += 1;
        }

        assert!(checked > 0, "no corpus cases were readable; test would be vacuous");
        assert!(
            missing.is_empty(),
            "{} of {} Chrome baseline selectors across {} cases could not be reproduced \
             by RustKit's generator, so the geometry oracle would silently skip them:\n{}",
            missing.len(),
            total_expected,
            checked,
            missing
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        eprintln!("join check: {total_expected} baseline selectors reproduced across {checked} cases");
    }
}

#[cfg(test)]
mod scroll_wiring_tests {
    use super::*;

    // ---- 2026-08-05 live-session fix: scroll wiring ----
    //
    // scroll_view/max_scroll_offset/PushTransform all existed with zero
    // production callers (the orphan-module class). These tests pin the
    // state machine the new wiring depends on. macOS-gated for the same
    // reason as ua_form_control_defaults_reach_computed_style: Engine::new
    // needs a real GPU device, which the macos CI leg has.

    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn engine_with_scrollable_view(max_y: f32) -> (Engine, EngineViewId) {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("headless view");
        let view = engine.views.get_mut(&id).expect("view");
        view.max_scroll_offset = (0.0, max_y);
        (engine, id)
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn scroll_clamps_and_reports_change() {
        let (mut engine, id) = engine_with_scrollable_view(1000.0);

        // Trackpad flick down arrives as negative delta_y (observed live:
        // PixelDelta y=-45..0 for a downward flick) and must ADVANCE the page.
        assert!(engine.scroll_view(id, 0.0, -45.0).unwrap());
        assert_eq!(engine.get_scroll_offset(id).unwrap(), (0.0, 45.0));

        // Scrolling above the top clamps to 0 and reports change=true only
        // while there is distance to travel.
        assert!(engine.scroll_view(id, 0.0, 100.0).unwrap());
        assert_eq!(engine.get_scroll_offset(id).unwrap(), (0.0, 0.0));
        assert!(!engine.scroll_view(id, 0.0, 100.0).unwrap(), "at top: no change");

        // Scrolling past the bottom clamps to max.
        assert!(engine.scroll_view(id, 0.0, -99999.0).unwrap());
        assert_eq!(engine.get_scroll_offset(id).unwrap(), (0.0, 1000.0));
        assert!(!engine.scroll_view(id, 0.0, -1.0).unwrap(), "at bottom: no change");
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn relayout_reclamps_offset_when_document_shrinks() {
        let (mut engine, id) = engine_with_scrollable_view(1000.0);
        engine.scroll_view(id, 0.0, -800.0).unwrap();
        assert_eq!(engine.get_scroll_offset(id).unwrap(), (0.0, 800.0));

        // Simulate the relayout path shrinking the document: the clamp added
        // alongside max_scroll_offset assignment must pull the offset back in
        // range, or render would translate into empty space.
        let view = engine.views.get_mut(&id).expect("view");
        view.max_scroll_offset = (0.0, 300.0);
        view.scroll_offset = (
            view.scroll_offset.0.min(view.max_scroll_offset.0),
            view.scroll_offset.1.min(view.max_scroll_offset.1),
        );
        assert_eq!(engine.get_scroll_offset(id).unwrap(), (0.0, 300.0));
    }
}

#[cfg(test)]
mod button_children_tests {
    use super::*;

    // ---- 2026-08-05 live-session fix: <button> is a flow container ----
    //
    // Icon buttons (element children, no text) were collapsed to an opaque
    // FormControl leaf stamped with the literal string "Button". These pin
    // the three shapes. macOS-gated: Engine::new needs a GPU device (the
    // macos CI leg runs these; see ua_form_control_defaults note).

    #[cfg(target_os = "macos")]
    fn build_button(children: Vec<Rc<Node>>) -> LayoutBox {
        let engine = Engine::new(EngineConfig::default()).expect("engine");
        let button = Node::new(
            rustkit_dom::NodeId::new(1),
            NodeType::Element {
                tag_name: "button".into(),
                namespace: String::new(),
                attributes: HashMap::new(),
            },
        );
        for c in children {
            button.append_child(c);
        }
        engine.build_layout_from_parent_style_and_path(
            &button,
            &[],
            &HashMap::new(),
            &[],
            None,
            &[],
            0,
            1,
            "button",
            &Cell::new(0),
            false,
        )
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn icon_button_keeps_its_element_children() {
        let svg = Node::new(
            rustkit_dom::NodeId::new(2),
            NodeType::Element {
                tag_name: "svg".into(),
                namespace: String::new(),
                attributes: HashMap::new(),
            },
        );
        let layout = build_button(vec![svg]);
        assert!(
            !matches!(layout.box_type, BoxType::FormControl(_)),
            "a button with element children must be a flow container, got FormControl leaf"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn text_button_stays_a_widget_with_its_text() {
        let text = Node::new(rustkit_dom::NodeId::new(2), NodeType::Text("Buy It Now".into()));
        let layout = build_button(vec![text]);
        match layout.box_type {
            BoxType::FormControl(rustkit_layout::FormControlType::Button { ref label, .. }) => {
                assert_eq!(label, "Buy It Now");
            }
            ref other => panic!("text-only button should stay a FormControl leaf, got {other:?}"),
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn empty_button_has_no_placeholder_word() {
        let layout = build_button(vec![]);
        match layout.box_type {
            BoxType::FormControl(rustkit_layout::FormControlType::Button { ref label, .. }) => {
                assert_eq!(label, "", "empty button must not be stamped with a literal 'Button'");
            }
            ref other => panic!("empty button should stay a FormControl leaf, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod svg_image_tests {
    use super::*;

    // ---- 2026-08-05 live-session fix: SVG in <img> ----
    //
    // rustkit-svg existed unwired (orphan-module class): every SVG <img>
    // failed ImageManager's raster decode with "Unknown image format".
    // Pins the layout half of the wire: a cached SVG document supplies the
    // natural size the raster cache cannot. macOS-gated per the GPU-device
    // rationale on the other Engine-constructing tests.

    #[test]
    #[cfg(target_os = "macos")]
    fn cached_svg_supplies_natural_size_to_img_layout() {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let svg = rustkit_svg::SvgDocument::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"></svg>"#)
            .expect("parse svg");
        engine
            .svg_cache
            .insert("https://example.com/logo.svg".to_string(), svg);

        let mut attrs = HashMap::new();
        attrs.insert("src".to_string(), "https://example.com/logo.svg".to_string());
        let img = Node::new(
            rustkit_dom::NodeId::new(1),
            NodeType::Element {
                tag_name: "img".into(),
                namespace: String::new(),
                attributes: attrs,
            },
        );

        let layout = engine.build_layout_from_parent_style_and_path(
            &img,
            &[],
            &HashMap::new(),
            &[],
            None,
            &[],
            0,
            1,
            "img",
            &Cell::new(0),
            false,
        );
        match layout.box_type {
            BoxType::Image {
                natural_width,
                natural_height,
                ..
            } => {
                assert_eq!((natural_width, natural_height), (40.0, 20.0),
                    "SVG natural size must come from the parsed document, not the 150x150 placeholder");
            }
            ref other => panic!("img should build an Image box, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod link_click_tests {
    use super::*;

    // ---- 2026-08-05 live-session fix: clicking links ----
    //
    // Layout carried no way to answer "what link is at this point", so a
    // click could never navigate. These pin the two properties that make
    // link_at_point trustworthy: nested content resolves to its enclosing
    // link, and the viewport->document translation honors scroll.

    fn link_box(href: &str, x: f32, y: f32, w: f32, h: f32) -> LayoutBox {
        let mut b = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        b.link_href = Some(href.to_string());
        b.dimensions.content = rustkit_layout::Rect::new(x, y, w, h);
        b
    }

    #[test]
    fn a_click_on_content_inside_a_link_resolves_to_that_link() {
        // <a href><img></a>: the image is the hit box and has no href of its
        // own; the enclosing link must supply it.
        let mut anchor = link_box("/deep", 0.0, 0.0, 200.0, 100.0);
        let mut img = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        img.dimensions.content = rustkit_layout::Rect::new(10.0, 10.0, 50.0, 50.0);
        anchor.children.push(img);

        let hit = anchor.hit_test(20.0, 20.0).expect("hit");
        assert_eq!(hit.link_href.as_deref(), Some("/deep"),
            "content nested in a link must resolve to the link's href");
    }

    #[test]
    fn the_nearest_link_wins_over_an_outer_one() {
        let mut outer = link_box("/outer", 0.0, 0.0, 200.0, 100.0);
        let inner = link_box("/inner", 10.0, 10.0, 50.0, 50.0);
        outer.children.push(inner);

        let hit = outer.hit_test(20.0, 20.0).expect("hit");
        assert_eq!(hit.link_href.as_deref(), Some("/inner"));
    }

    #[test]
    fn a_click_outside_any_link_resolves_to_nothing() {
        let mut root = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        root.dimensions.content = rustkit_layout::Rect::new(0.0, 0.0, 200.0, 100.0);
        root.children.push(link_box("/somewhere", 0.0, 0.0, 20.0, 20.0));

        let hit = root.hit_test(100.0, 80.0).expect("hit");
        assert_eq!(hit.link_href, None);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn link_at_point_translates_viewport_coords_by_scroll() {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("headless view");

        // A link 500px down the document.
        let mut root = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        root.dimensions.content = rustkit_layout::Rect::new(0.0, 0.0, 800.0, 2000.0);
        root.children.push(link_box("/target", 0.0, 500.0, 100.0, 20.0));

        {
            let view = engine.views.get_mut(&id).expect("view");
            view.layout = Some(root);
            view.url = Some(Url::parse("https://example.com/page").unwrap());
            view.max_scroll_offset = (0.0, 1400.0);
        }

        // Unscrolled: viewport y=505 is document y=505 — a hit, resolved
        // against the document base URL.
        assert_eq!(
            engine.link_at_point(id, 10.0, 505.0).as_deref(),
            Some("https://example.com/target")
        );

        // Scrolled down 500: the same link now sits at viewport y=5, and the
        // old viewport coordinate must MISS. Without the scroll translation
        // every click would be wrong by exactly the scroll offset.
        engine.scroll_view(id, 0.0, -500.0).unwrap();
        assert_eq!(
            engine.link_at_point(id, 10.0, 5.0).as_deref(),
            Some("https://example.com/target")
        );
        assert_eq!(engine.link_at_point(id, 10.0, 505.0), None);
    }
}

#[cfg(test)]
mod node_identity_tests {
    use super::*;

    // ---- click-to-focus: closing the "requires node_id tracking" TODO ----
    //
    // Layout boxes carried no DOM identity, so a hit test could locate a
    // rectangle but never the element it came from. That single gap is what
    // the mouse/keyboard handlers cite as the reason focus and event
    // dispatch were left unimplemented. These pin the plumbing.

    #[test]
    fn hit_test_reports_the_node_of_the_box_actually_under_the_cursor() {
        // node_id must NOT inherit from ancestors the way link_href does:
        // the caller wants the element under the cursor, not the nearest
        // interesting one above it.
        let mut parent = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        parent.dimensions.content = rustkit_layout::Rect::new(0.0, 0.0, 200.0, 100.0);
        parent.node_id = Some(1);

        let mut child = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        child.dimensions.content = rustkit_layout::Rect::new(10.0, 10.0, 50.0, 50.0);
        child.node_id = Some(2);
        parent.children.push(child);

        assert_eq!(parent.hit_test(20.0, 20.0).unwrap().node_id, Some(2), "child wins");
        assert_eq!(parent.hit_test(150.0, 80.0).unwrap().node_id, Some(1), "parent when child missed");
    }

    #[test]
    fn an_anonymous_box_reports_no_node() {
        // Text and anonymous boxes have no element; they must stay None
        // rather than borrowing a neighbour's identity.
        let mut b = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        b.dimensions.content = rustkit_layout::Rect::new(0.0, 0.0, 50.0, 50.0);
        assert_eq!(b.hit_test(10.0, 10.0).unwrap().node_id, None);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn clicking_a_form_control_focuses_it_and_clicking_away_clears_focus() {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("headless view");

        let html = r#"<html><body><input type="text" id="a"><div id="plain">x</div></body></html>"#;
        let doc = std::rc::Rc::new(
            rustkit_dom::Document::parse_html(html).expect("parse"),
        );
        // Find the input's real NodeId by walking the parsed document, so the
        // test cannot pass against a hand-invented id.
        fn find<'a>(n: &std::rc::Rc<Node>, tag: &str) -> Option<std::rc::Rc<Node>> {
            if let NodeType::Element { tag_name, .. } = &n.node_type {
                if tag_name.eq_ignore_ascii_case(tag) {
                    return Some(n.clone());
                }
            }
            n.children().iter().find_map(|c| find(c, tag))
        }
        let root = doc.root();
        let input = find(&root, "input").expect("input node");
        let div = find(&root, "div").expect("div node");

        let mut layout = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        layout.dimensions.content = rustkit_layout::Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut input_box = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        input_box.dimensions.content = rustkit_layout::Rect::new(0.0, 0.0, 100.0, 20.0);
        input_box.node_id = Some(input.id.raw());
        let mut div_box = LayoutBox::new(BoxType::Block, ComputedStyle::new());
        div_box.dimensions.content = rustkit_layout::Rect::new(0.0, 100.0, 100.0, 20.0);
        div_box.node_id = Some(div.id.raw());
        layout.children.push(input_box);
        layout.children.push(div_box);

        {
            let view = engine.views.get_mut(&id).expect("view");
            view.document = Some(doc);
            view.layout = Some(layout);
        }

        assert_eq!(engine.focus_at_point(id, 10.0, 10.0).as_deref(), Some("input"));
        assert_eq!(engine.focused_node(id), Some(input.id));

        // Clicking a non-focusable element clears focus, like clicking page
        // background — NOT "keeps the previous focus", which would leave keys
        // going to an element the user visibly clicked away from.
        assert_eq!(engine.focus_at_point(id, 10.0, 110.0), None);
        assert_eq!(engine.focused_node(id), None);
    }
}

#[cfg(test)]
mod form_typing_tests {
    use super::*;

    // ---- typing into web forms: routing keys to the orphaned text model ----
    //
    // rustkit-dom's TextEditState (insert/delete/caret/selection, ~2000
    // lines) had only test callers, and the engine's key handler was
    // cfg(windows). These tests go through the PRODUCTION layout path on
    // purpose: an earlier version of this change stamped node_id only on the
    // general element branch, and form controls return before reaching it —
    // so hit testing an <input> reported no node and focus silently could
    // never work. Hand-built layout boxes passed anyway. Only building from
    // real HTML catches it.

    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn engine_with_html(html: &str) -> (Engine, EngineViewId) {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("headless view");
        let doc = std::rc::Rc::new(Document::parse_html(html).expect("parse"));
        let layout = engine.build_layout_for_view(id, &doc, &[]);
        let view = engine.views.get_mut(&id).expect("view");
        view.document = Some(doc);
        view.layout = Some(layout);
        (engine, id)
    }

    /// Walk a layout tree collecting every box that carries a node id.
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn boxes_with_nodes(b: &LayoutBox, out: &mut Vec<(usize, BoxType)>) {
        if let Some(n) = b.node_id {
            out.push((n, b.box_type.clone()));
        }
        for c in &b.children {
            boxes_with_nodes(c, out);
        }
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn a_form_control_box_built_from_real_html_carries_its_node_id() {
        // THE REGRESSION TEST for the bug above: form controls take an early
        // return, so they need their own identity stamp.
        let (engine, id) = engine_with_html(r#"<html><body><input type="text"></body></html>"#);
        let layout = engine.views.get(&id).unwrap().layout.as_ref().unwrap();
        let mut found = Vec::new();
        boxes_with_nodes(layout, &mut found);
        assert!(
            found
                .iter()
                .any(|(_, bt)| matches!(bt, BoxType::FormControl(_))),
            "the <input>'s FormControl box must carry a node_id; without it a \
             hit test finds a rectangle with no element and focus cannot resolve"
        );
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn typing_into_a_focused_input_changes_what_layout_renders() {
        let (mut engine, id) =
            engine_with_html(r#"<html><body><input type="text" value="ab"></body></html>"#);

        // Focus the input by NodeId taken from the built layout, not invented.
        let node_raw = {
            let layout = engine.views.get(&id).unwrap().layout.as_ref().unwrap();
            let mut found = Vec::new();
            boxes_with_nodes(layout, &mut found);
            found
                .iter()
                .find(|(_, bt)| matches!(bt, BoxType::FormControl(_)))
                .expect("form control box")
                .0
        };
        engine.views.get_mut(&id).unwrap().focused_node =
            Some(rustkit_dom::NodeId::new(node_raw));
        let view = engine.views.get_mut(&id).unwrap();
        view.edit_states.insert(
            node_raw,
            rustkit_dom::forms::TextEditState::with_value("ab"),
        );
        view.edit_states.get(&node_raw).unwrap().move_to_end(false);

        // 'c' (no modifiers) must insert.
        assert!(engine.handle_text_key(id, 0, "c", false, false, false));
        assert_eq!(engine.edit_value_in(id, node_raw).unwrap().0, "abc");

        // Backspace (VK 0x08) must delete.
        assert!(engine.handle_text_key(id, 0x08, "", false, false, false));
        assert_eq!(engine.edit_value_in(id, node_raw).unwrap().0, "ab");

        // And the change must reach LAYOUT — the DOM attribute still says
        // "ab" forever (there is no set_attribute), so if layout did not read
        // through edit state the typed text would be invisible.
        engine
            .views
            .get(&id)
            .unwrap()
            .edit_states
            .get(&node_raw)
            .unwrap()
            .insert_text("XY");
        let doc = engine.views.get(&id).unwrap().document.clone().unwrap();
        let relaid = engine.build_layout_for_view(id, &doc, &[]);

        fn find_input_value(b: &LayoutBox) -> Option<String> {
            if let BoxType::FormControl(rustkit_layout::FormControlType::TextInput {
                value, ..
            }) = &b.box_type
            {
                return Some(value.clone());
            }
            b.children.iter().find_map(find_input_value)
        }
        assert_eq!(
            find_input_value(&relaid).as_deref(),
            Some("abXY"),
            "layout must read through live edit state, not the frozen DOM attribute"
        );
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn keys_go_nowhere_when_nothing_is_focused() {
        // The property that makes it safe to route window-level keys here:
        // with no focus, handle_text_key must decline so the caller can fall
        // back to scrolling.
        let (mut engine, id) =
            engine_with_html(r#"<html><body><input type="text"></body></html>"#);
        assert!(!engine.handle_text_key(id, 0, "c", false, false, false));
    }
}

#[cfg(test)]
mod form_submit_tests {
    use super::*;

    // ---- Enter in a form field submits it ----
    //
    // Typing is only useful if something happens on Enter. All fixtures are
    // built from real HTML through the production layout/DOM path, per the
    // node_id lesson: hand-built structures pass while the real path is
    // broken.

    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn engine_with(html: &str, url: &str) -> (Engine, EngineViewId, std::rc::Rc<Document>) {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("view");
        let doc = std::rc::Rc::new(Document::parse_html(html).expect("parse"));
        let view = engine.views.get_mut(&id).expect("view");
        view.document = Some(doc.clone());
        view.url = Some(Url::parse(url).unwrap());
        (engine, id, doc)
    }

    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn find_by_tag(n: &std::rc::Rc<Node>, tag: &str) -> Option<std::rc::Rc<Node>> {
        if let NodeType::Element { tag_name, .. } = &n.node_type {
            if tag_name.eq_ignore_ascii_case(tag) {
                return Some(n.clone());
            }
        }
        n.children().iter().find_map(|c| find_by_tag(c, tag))
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn submitting_carries_what_the_user_typed_not_the_authored_value() {
        let (mut engine, id, doc) = engine_with(
            r#"<html><body><form action="/search"><input name="q" value="old"></form></body></html>"#,
            "https://example.com/page",
        );
        let input = find_by_tag(&doc.root(), "input").expect("input");
        engine.views.get_mut(&id).unwrap().focused_node = Some(input.id);
        engine.views.get_mut(&id).unwrap().edit_states.insert(
            input.id.raw(),
            rustkit_dom::forms::TextEditState::with_value("typed"),
        );

        let sub = engine.form_submission_for_focus(id).expect("submission");
        assert!(
            sub.url.contains("q=typed"),
            "submission must carry live edit state, got {}",
            sub.url
        );
        assert!(!sub.url.contains("old"), "authored value must not win");
        assert!(sub.url.starts_with("https://example.com/search"),
            "action must resolve against the document URL, got {}", sub.url);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn unnamed_disabled_and_unchecked_controls_do_not_submit() {
        // HTML §4.10 successful-controls rules. Each of these silently
        // corrupts a query string if it leaks in.
        let (mut engine, id, doc) = engine_with(
            r#"<html><body><form action="/s">
                 <input name="kept" value="1">
                 <input value="no-name">
                 <input name="off" value="2" disabled>
                 <input type="checkbox" name="box" value="3">
                 <input type="submit" name="btn" value="Go">
               </form></body></html>"#,
            "https://example.com/",
        );
        let input = find_by_tag(&doc.root(), "input").expect("input");
        engine.views.get_mut(&id).unwrap().focused_node = Some(input.id);

        let sub = engine.form_submission_for_focus(id).expect("submission");
        assert!(sub.url.contains("kept=1"));
        for forbidden in ["no-name", "off=", "box=", "btn="] {
            assert!(
                !sub.url.contains(forbidden),
                "{forbidden} must not be submitted; got {}",
                sub.url
            );
        }
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn a_post_form_is_declined_rather_than_downgraded_to_get() {
        // Quietly turning a POST into a GET would send form data in a URL —
        // worse than not submitting. Declining is the honest behavior until
        // the loader accepts a body.
        let (mut engine, id, doc) = engine_with(
            r#"<html><body><form action="/s" method="post"><input name="q" value="x"></form></body></html>"#,
            "https://example.com/",
        );
        let input = find_by_tag(&doc.root(), "input").expect("input");
        engine.views.get_mut(&id).unwrap().focused_node = Some(input.id);
        assert!(engine.form_submission_for_focus(id).is_none());
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn a_field_outside_any_form_submits_nothing() {
        let (mut engine, id, doc) = engine_with(
            r#"<html><body><input name="loose" value="x"></body></html>"#,
            "https://example.com/",
        );
        let input = find_by_tag(&doc.root(), "input").expect("input");
        engine.views.get_mut(&id).unwrap().focused_node = Some(input.id);
        assert!(engine.form_submission_for_focus(id).is_none());
    }
}

#[cfg(test)]
mod edit_state_lifecycle_tests {
    use super::*;

    // ---- the side table's lifetime is part of the side table ----
    //
    // Prometheus's #110 R1 must-fix. NodeId is PER-DOCUMENT: every Document
    // restarts its counter at 1. An edit_states entry surviving a navigation
    // is therefore read as the NEW page's node with the same raw id — the
    // previous page's typed text painted into a fresh control, with
    // first-focus seeding skipped because the key already exists. The
    // original code carried a doc comment claiming reload dropped the map;
    // it did not.

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn typed_text_does_not_survive_a_navigation_into_the_next_page() {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("view");

        // Page one: type into a field.
        let doc1 = std::rc::Rc::new(
            Document::parse_html(r#"<html><body><input name="q"></body></html>"#).expect("parse"),
        );
        fn first_input(n: &std::rc::Rc<Node>) -> Option<std::rc::Rc<Node>> {
            if let NodeType::Element { tag_name, .. } = &n.node_type {
                if tag_name.eq_ignore_ascii_case("input") {
                    return Some(n.clone());
                }
            }
            n.children().iter().find_map(first_input)
        }
        let input1 = first_input(&doc1.root()).expect("input");
        {
            let view = engine.views.get_mut(&id).expect("view");
            view.document = Some(doc1.clone());
            view.focused_node = Some(input1.id);
            view.edit_states.insert(
                input1.id.raw(),
                rustkit_dom::forms::TextEditState::with_value("secret"),
            );
        }
        assert_eq!(
            engine.edit_value_in(id, input1.id.raw()).unwrap().0,
            "secret"
        );

        // Navigate through the REAL path. load_html shares the document
        // replacement code with load_url and needs no network, so this
        // exercises production rather than re-implementing it in the test —
        // the distinction that let the node_id bug pass a green suite.
        engine
            .load_html(id, r#"<html><body><input name="other"></body></html>"#)
            .expect("load_html");

        // The next document's first input reuses the same raw NodeId. If the
        // map survived, this reads back "secret" — the previous page's typed
        // text, in a control the user has never touched.
        let doc2 = engine.views.get(&id).unwrap().document.clone().unwrap();
        let input2 = first_input(&doc2.root()).expect("input");
        assert_eq!(
            input2.id.raw(),
            input1.id.raw(),
            "precondition: NodeId is per-document, so the ids DO collide — \
             that collision is exactly why the map must be cleared"
        );
        assert_eq!(
            engine.edit_value_in(id, input2.id.raw()),
            None,
            "the new page's control must have no inherited value"
        );
        assert_eq!(engine.focused_node(id), None, "focus must not survive either");
    }
}

#[cfg(test)]
mod relative_url_tests {
    use super::*;

    // ---- relative resource URLs (live session, 2026-08-06) ----
    //
    // Wikipedia painted no images and emitted 1120 `Invalid URL for image`
    // warnings in one session. The loader resolves and caches under the
    // ABSOLUTE url; layout and paint used the raw attribute. Cached under
    // https://www.wikipedia.org/portal/img/logo.png, looked up under
    // portal/img/logo.png — a miss every time, plus a parse failure per
    // image PER FRAME.

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn a_relative_image_src_reaches_the_display_list_absolute() {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("view");

        // Exactly Wikipedia's shape, including the CSS background that takes
        // a separate path through the parser (which has no base in scope).
        engine
            .load_html(
                id,
                r#"<html><body>
                     <img src="portal/wikipedia.org/assets/img/Wikipedia-logo-v2.png">
                     <div style="background-image: url(portal/wikipedia.org/assets/img/sprite.svg); width:10px; height:10px"></div>
                   </body></html>"#,
            )
            .expect("load_html");

        // load_html sets the base to about:blank, which is correct for inline
        // content and useless here. Put the view in the state a real
        // navigation leaves it in — document plus document URL — then rebuild.
        {
            let view = engine.views.get_mut(&id).expect("view");
            view.url = Some(Url::parse("https://www.wikipedia.org/").unwrap());
        }
        engine.relayout(id).expect("relayout");

        let dl = engine
            .views
            .get(&id)
            .unwrap()
            .display_list
            .as_ref()
            .expect("display list");

        let urls: Vec<&str> = dl
            .commands
            .iter()
            .filter_map(|c| match c {
                rustkit_layout::DisplayCommand::Image { url, .. }
                | rustkit_layout::DisplayCommand::BackgroundImage { url, .. } => {
                    Some(url.as_str())
                }
                _ => None,
            })
            .collect();

        assert!(!urls.is_empty(), "precondition: the page must emit image commands");
        for u in &urls {
            assert!(
                Url::parse(u).is_ok(),
                "every image URL reaching paint must parse; got {u:?} — a relative \
                 key here is both a cache miss and a per-frame warning"
            );
            assert!(
                u.starts_with("https://www.wikipedia.org/portal/"),
                "must resolve against the document base, got {u:?}"
            );
        }
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn an_absolute_src_is_left_alone() {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("view");
        {
            let view = engine.views.get_mut(&id).expect("view");
            view.url = Some(Url::parse("https://example.com/a/b").unwrap());
        }
        assert_eq!(
            engine
                .resolve_resource_url_in(id, "https://cdn.example.net/x.png")
                .map(|u| u.to_string())
                .as_deref(),
            Some("https://cdn.example.net/x.png")
        );
        // Root-relative resolves against the ORIGIN, not the directory.
        assert_eq!(
            engine
                .resolve_resource_url_in(id, "/x.png")
                .map(|u| u.to_string())
                .as_deref(),
            Some("https://example.com/x.png")
        );
    }
}

#[cfg(test)]
mod srcset_tests {
    use super::*;

    // ---- srcset had ZERO support (live finding, 2026-08-08) ----
    //
    // A page serving images only via srcset rendered NO IMAGE AT ALL — the
    // src is often a placeholder or absent on such pages. A wrong-density
    // pick is a rendering difference; no pick is a hole.

    #[test]
    fn widest_w_candidate_wins() {
        let picked = Engine::pick_from_srcset(
            "small.jpg 400w, medium.jpg 800w, large.jpg 1600w",
        );
        assert_eq!(picked.as_deref(), Some("large.jpg"));
    }

    #[test]
    fn density_candidates_are_ranked_among_themselves() {
        let picked = Engine::pick_from_srcset("a.png, b.png 2x, c.png 3x");
        assert_eq!(picked.as_deref(), Some("c.png"));
    }

    #[test]
    fn a_bare_candidate_is_one_x_not_zero() {
        // A no-descriptor candidate means 1x. Treating it as weight 0 would
        // make a single-candidate srcset resolve to nothing, which is the
        // no-image hole this whole change exists to close.
        assert_eq!(
            Engine::pick_from_srcset("only.png").as_deref(),
            Some("only.png")
        );
    }

    #[test]
    fn density_never_outranks_width_by_scale_accident() {
        // 2x and 2000w are on different scales. Without normalisation a
        // naive max() picks the 2x candidate over a far larger w one.
        let picked = Engine::pick_from_srcset("dense.png 2x, wide.png 2000w");
        assert_eq!(picked.as_deref(), Some("wide.png"));
    }

    #[test]
    fn malformed_input_yields_none_rather_than_a_bogus_url() {
        assert_eq!(Engine::pick_from_srcset(""), None);
        assert_eq!(Engine::pick_from_srcset("   "), None);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "headless"))]
    fn an_img_with_only_srcset_still_gets_a_layout_box_with_that_url() {
        // The end-to-end property: BOTH the loader's discovery and the
        // layout box must choose the SAME candidate, or the loader caches
        // under one key while layout looks up another — the cache-miss
        // shape #113 fixed for relative URLs, one attribute over.
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, 800, 600))
            .expect("view");
        engine
            .load_html(
                id,
                r#"<html><body><img srcset="a.png 400w, b.png 1200w"></body></html>"#,
            )
            .expect("load_html");
        {
            let view = engine.views.get_mut(&id).expect("view");
            view.url = Some(Url::parse("https://example.com/page").unwrap());
        }
        engine.relayout(id).expect("relayout");

        fn find_image_url(b: &LayoutBox) -> Option<String> {
            if let BoxType::Image { url, .. } = &b.box_type {
                return Some(url.clone());
            }
            b.children.iter().find_map(find_image_url)
        }
        let layout = engine.views.get(&id).unwrap().layout.as_ref().unwrap();
        let url = find_image_url(layout).expect("img must produce an Image box");
        assert_eq!(
            url, "https://example.com/b.png",
            "layout must resolve the WIDEST srcset candidate, absolutely"
        );
    }
}
