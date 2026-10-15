//! The presentation half of the engine: the view host, the compositor
//! (surfaces and headless textures) and the renderer.
//!
//! Threads plan M3, first PR (the pure move). Before this module the
//! `Engine` held `viewhost`, `compositor` and `renderer` beside the page
//! state (documents, bindings, layout, display lists), and `render`
//! reached into both. Now everything that touches a window, a GPU surface
//! or the renderer lives here, and the page side hands it what to draw:
//! a display list, a scroll offset and a frame generation. The engine
//! still calls it on the same thread, in the same order, with the same
//! arguments as before; nothing about a frame changes. The second PR
//! makes the hand-over a `FrameCommit` and gives the presenter its own
//! latest-commit-per-view store, which is what a page thread (M4) needs.
//!
//! Bodies are moved from `lib.rs` as they were, so the diff of this PR is
//! read as a move. Where a body read a `ViewState` field, the field is now
//! an argument.

use std::collections::HashSet;

use rustkit_compositor::Compositor;
use rustkit_image::ImageManager;
use rustkit_layout::DisplayList;
use rustkit_renderer::{RenderStats, Renderer};
use rustkit_viewhost::{Bounds, ViewHost, ViewHostTrait, ViewId};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use rustkit_viewhost::WindowHandle;
use tracing::{info, trace, warn};
use url::Url;

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

use super::{EngineError, EngineViewId, PresentedFrame};

/// What one frame is drawn from. The page side fills it from its
/// `ViewState`; the presenter reads nothing else of the view.
pub(crate) struct Frame<'a> {
    pub viewhost_id: ViewId,
    /// A headless view draws to its texture and presents nothing.
    pub is_headless: bool,
    pub display_list: Option<&'a DisplayList>,
    /// `ViewState::frame_generation` of `display_list`.
    pub generation: u64,
    pub scroll_offset: (f32, f32),
}

/// The window thread's half of the engine: every NSView/HWND, every GPU
/// surface and the one renderer. See the module doc.
pub(crate) struct Presenter {
    viewhost: ViewHost,
    compositor: Compositor,
    renderer: Option<Renderer>,
    /// Views whose most recent render attempt failed. A view entering this
    /// set logs one `warn!`; leaving it logs recovery. Without this, a
    /// persistently failing render (e.g. a wedged surface) freezes the
    /// screen while the log stays silent.
    render_failing: HashSet<EngineViewId>,
    /// `EngineConfig::background_color`: what a view shows before its
    /// first frame.
    background_color: [f64; 4],
    /// Images handed to the renderer so far (tests: one upload per image,
    /// however often the frame is drawn).
    #[cfg(test)]
    pub(crate) images_uploaded: usize,
}

impl Presenter {
    /// The view host, the compositor and the renderer, as `Engine::new`
    /// built them.
    pub(crate) fn new(background_color: [f64; 4]) -> Result<Self, EngineError> {
        // Initialize ViewHost
        let viewhost = ViewHost::new();

        // Initialize Compositor (unit tests serialise device creation; see
        // `test_compositor`)
        #[cfg(test)]
        let compositor = super::test_compositor();
        #[cfg(not(test))]
        let compositor = Compositor::new();
        let compositor = compositor.map_err(|e| EngineError::RenderError(e.to_string()))?;

        // Initialize Renderer
        let renderer = Renderer::new(
            compositor.device_arc(),
            compositor.queue_arc(),
            compositor.surface_format(),
        )
        .map_err(|e| EngineError::RenderError(e.to_string()))?;

        info!(
            adapter = ?compositor.adapter_info().name,
            "Engine initialized with GPU renderer"
        );

        Ok(Self {
            viewhost,
            compositor,
            renderer: Some(renderer),
            render_failing: HashSet::new(),
            background_color,
            #[cfg(test)]
            images_uploaded: 0,
        })
    }

    /// A presenter with a compositor and no renderer: what the layout-only
    /// test engines built by hand (`renderer: None`).
    #[cfg(test)]
    pub(crate) fn layout_only(compositor: Compositor) -> Self {
        Self {
            viewhost: ViewHost::new(),
            compositor,
            renderer: None,
            render_failing: HashSet::new(),
            background_color: super::EngineConfig::default().background_color,
            images_uploaded: 0,
        }
    }

    // ----- views and surfaces -------------------------------------------

    /// A host view (NSView / HWND child) in `parent`.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) fn create_host_view(
        &self,
        parent: WindowHandle,
        bounds: Bounds,
    ) -> Result<ViewId, EngineError> {
        <ViewHost as ViewHostTrait>::create_view(&self.viewhost, parent, bounds)
            .map_err(|e| EngineError::ViewError(e.to_string()))
    }

    /// The GPU surface of a host view.
    #[cfg(target_os = "windows")]
    pub(crate) fn attach_surface(&mut self, viewhost_id: ViewId, bounds: Bounds) -> Result<(), EngineError> {
        // Create compositor surface
        let hwnd = <ViewHost as ViewHostTrait>::get_hwnd(&self.viewhost, viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))?;

        unsafe {
            self.compositor
                .create_surface_for_hwnd(viewhost_id, hwnd, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;
        }
        Ok(())
    }

    /// The GPU surface of a host view.
    #[cfg(target_os = "macos")]
    pub(crate) fn attach_surface(&mut self, viewhost_id: ViewId, bounds: Bounds) -> Result<(), EngineError> {
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
        Ok(())
    }

    /// A headless texture in place of a surface.
    #[cfg(any(test, feature = "headless"))]
    pub(crate) fn create_headless_texture(&mut self, viewhost_id: ViewId, bounds: Bounds) -> Result<(), EngineError> {
        self.compositor
            .create_headless_texture(viewhost_id, bounds.width, bounds.height)
            .map_err(|e| EngineError::RenderError(e.to_string()))
    }

    /// The background colour, before the view's first frame.
    pub(crate) fn clear_to_background(&self, viewhost_id: ViewId) -> Result<(), EngineError> {
        self.compositor
            .render_solid_color(viewhost_id, self.background_color)
            .map_err(|e| EngineError::RenderError(e.to_string()))
    }

    /// Drop a view's surface and host view, and forget its render state.
    pub(crate) fn destroy_view(&mut self, id: EngineViewId, viewhost_id: ViewId) {
        self.render_failing.remove(&id);

        // Destroy compositor surface
        let _ = self.compositor.destroy_surface(viewhost_id);

        // Destroy viewhost view
        let _ = <ViewHost as ViewHostTrait>::destroy_view(&self.viewhost, viewhost_id);
    }

    /// The surface half of a resize: the viewhost's bounds and the GPU
    /// surface (or the headless texture) take the new size.
    pub(crate) fn apply_bounds(
        &mut self,
        viewhost_id: ViewId,
        is_headless: bool,
        bounds: Bounds,
    ) -> Result<(), EngineError> {
        if is_headless {
            // Headless view: recreate headless texture with new size
            // First destroy old texture
            self.compositor.destroy_headless_texture(viewhost_id).ok(); // Ignore errors if it doesn't exist

            // Create new texture with new size
            self.compositor
                .create_headless_texture(viewhost_id, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;
        } else {
            // Regular view: resize viewhost and surface
            self.viewhost
                .set_bounds(viewhost_id, bounds)
                .map_err(|e| EngineError::ViewError(e.to_string()))?;

            self.compositor
                .resize_surface(viewhost_id, bounds.width, bounds.height)
                .map_err(|e| EngineError::RenderError(e.to_string()))?;
        }
        Ok(())
    }

    /// The host view's bounds (a headless view has none here; the page
    /// side keeps those).
    pub(crate) fn host_bounds(&self, viewhost_id: ViewId) -> Result<Bounds, EngineError> {
        self.viewhost
            .get_bounds(viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))
    }

    /// The surface's (or headless texture's) size in device pixels.
    pub(crate) fn surface_size(&self, viewhost_id: ViewId) -> Result<(u32, u32), EngineError> {
        self.compositor
            .get_surface_size(viewhost_id)
            .map_err(|e| EngineError::RenderError(e.to_string()))
    }

    /// Give the host view keyboard focus (the window system's half of
    /// focus; see `Engine::grab_keyboard`).
    pub(crate) fn grab_keyboard(&self, viewhost_id: ViewId) {
        let _ = <ViewHost as ViewHostTrait>::focus_view(&self.viewhost, viewhost_id);
    }

    pub(crate) fn focus(&self, viewhost_id: ViewId) -> Result<(), EngineError> {
        self.viewhost
            .focus(viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))
    }

    pub(crate) fn set_visible(&self, viewhost_id: ViewId, visible: bool) -> Result<(), EngineError> {
        self.viewhost
            .set_visible(viewhost_id, visible)
            .map_err(|e| EngineError::ViewError(e.to_string()))
    }

    /// Get the native window handle (HWND) for a view.
    #[cfg(windows)]
    pub(crate) fn hwnd(&self, viewhost_id: ViewId) -> Result<HWND, EngineError> {
        self.viewhost
            .get_hwnd(viewhost_id)
            .map_err(|e| EngineError::ViewError(e.to_string()))
    }

    /// Get GPU info.
    pub(crate) fn gpu_info(&self) -> String {
        format!("{:?}", self.compositor.adapter_info())
    }

    /// Batch sizes and stack depths of the last frame (shell diagnostics /
    /// screenshot harness).
    pub(crate) fn render_stats(&self) -> RenderStats {
        self.renderer
            .as_ref()
            .map(|r| r.get_render_stats())
            .unwrap_or_default()
    }

    // ----- drawing -------------------------------------------------------

    /// Bookkeeping around one `present`: warn once per failure episode,
    /// not per frame (a wedged surface renders at event rate and would
    /// flood the log, but total silence is how a frozen screen goes
    /// undiagnosed for a whole session), and log the recovery.
    pub(crate) fn note_render_result(&mut self, id: EngineViewId, result: &Result<(), EngineError>) {
        match result {
            Ok(()) => {
                if self.render_failing.remove(&id) {
                    info!(?id, "View render recovered");
                }
            }
            Err(e) => {
                if self.render_failing.insert(id) {
                    warn!(?id, error = %e, "View render failing; frames are NOT being presented (will log again on recovery)");
                } else {
                    trace!(?id, error = %e, "View render still failing");
                }
            }
        }
    }

    /// Draw one frame: upload the images its display list names, execute
    /// the list (scrolled, for a window view) and present. Returns what
    /// the frame was drawn from, for `Engine::frame_is_current`; `None`
    /// only from the no-renderer fallback of a window view, which drew the
    /// background and recorded nothing before this module existed (there is
    /// always a renderer, so that arm is dead; kept as it was).
    ///
    /// Was the body of `Engine::render` after its view reads.
    pub(crate) fn present(
        &mut self,
        id: EngineViewId,
        frame: Frame<'_>,
        images: &ImageManager,
    ) -> Result<Option<PresentedFrame>, EngineError> {
        let Frame {
            viewhost_id,
            is_headless,
            display_list,
            generation,
            scroll_offset,
        } = frame;
        let cmd_count = display_list.map(|dl| dl.commands.len()).unwrap_or(0);

        trace!(
            ?id,
            has_display_list = display_list.is_some(),
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

        // What this frame is drawn from, kept when it has been presented.
        let mut missing_images = Vec::new();

        // Upload images from cache to renderer before drawing
        if let Some(display_list) = display_list {
            missing_images = self.upload_display_list_images(images, &display_list.commands);
        }
        let presented = PresentedFrame {
            generation,
            scroll_offset,
            surface: (surface_width, surface_height),
            missing_images,
            at: std::time::Instant::now(),
        };

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
            if let Some(renderer) = &mut self.renderer {
                // A headless view is drawn unscrolled, as it always was.
                Self::execute(renderer, display_list, (0.0, 0.0), &texture_view)?;
            } else {
                // Fallback to compositor solid color
                self.compositor
                    .render_solid_color(viewhost_id, self.background_color)
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
            // the display list stays in document coordinates and the
            // renderer shifts it by the clamped offset scroll_view()
            // maintains (`Renderer::set_scroll_offset`), so a scrolled
            // frame draws the page's own list, not a copy of it.

            // Render using display list if available, otherwise just clear to background
            {
                let _execute_span = tracing::info_span!("renderer_execute", cmd_count).entered();
                if let Some(renderer) = &mut self.renderer {
                    Self::execute(renderer, display_list, scroll_offset, &texture_view)?;
                } else {
                    // Fallback to compositor solid color (shouldn't normally happen)
                    drop(output); // Release the texture
                    self.compositor
                        .render_solid_color(viewhost_id, self.background_color)
                        .map_err(|e| EngineError::RenderError(e.to_string()))?;
                    return Ok(None);
                }
            }

            // Present surface texture
            self.compositor.present(output);
        }

        Ok(Some(presented))
    }

    /// Execute `display_list` (or nothing, which clears to the background)
    /// at `scroll_offset` into `target`: the one draw both kinds of view
    /// share. The list is borrowed, never copied.
    fn execute(
        renderer: &mut Renderer,
        display_list: Option<&DisplayList>,
        scroll_offset: (f32, f32),
        target: &wgpu::TextureView,
    ) -> Result<(), EngineError> {
        let commands = display_list.map(|dl| dl.commands.as_slice()).unwrap_or(&[]);
        renderer.set_scroll_offset(scroll_offset);
        let drawn = renderer
            .execute(commands, target)
            .map_err(|e| EngineError::RenderError(e.to_string()));
        renderer.set_scroll_offset((0.0, 0.0));
        drawn
    }

    /// Draw `display_list` at `scroll_offset` into a headless view's
    /// texture through the window path's draw (`execute`), and write it to
    /// `path` as PPM when one is given. A headless view's own frames are
    /// drawn unscrolled; this is how a test sees a scrolled window frame.
    #[cfg(test)]
    pub(crate) fn draw_scrolled(
        &mut self,
        viewhost_id: ViewId,
        display_list: &DisplayList,
        scroll_offset: (f32, f32),
        path: Option<&str>,
    ) -> Result<(), EngineError> {
        let (width, height) = self.surface_size(viewhost_id)?;
        let renderer = self.renderer.as_mut().expect("renderer");
        renderer.set_viewport_size(width, height);
        match path {
            None => {
                let target = self
                    .compositor
                    .get_headless_texture_view(viewhost_id)
                    .map_err(|e| EngineError::RenderError(e.to_string()))?;
                Self::execute(renderer, Some(display_list), scroll_offset, &target)
            }
            Some(path) => {
                renderer.set_scroll_offset(scroll_offset);
                let captured = self
                    .compositor
                    .capture_frame_with_renderer(viewhost_id, path, renderer, &display_list.commands)
                    .map_err(|e| EngineError::RenderError(e.to_string()));
                renderer.set_scroll_offset((0.0, 0.0));
                captured
            }
        }
    }

    /// The renderer, for tests that read its caches.
    #[cfg(test)]
    pub(crate) fn renderer_mut(&mut self) -> Option<&mut Renderer> {
        self.renderer.as_mut()
    }

    /// Draw `commands` at `viewport` into a headless view's texture and
    /// write it to `path` as PPM: what the paint tests did by reaching
    /// into the renderer and the compositor.
    #[cfg(test)]
    pub(crate) fn capture_commands(
        &mut self,
        viewhost_id: ViewId,
        viewport: (u32, u32),
        path: &str,
        commands: &[rustkit_layout::DisplayCommand],
    ) -> Result<(), EngineError> {
        let renderer = self.renderer.as_mut().expect("renderer");
        renderer.set_viewport_size(viewport.0, viewport.1);
        self.compositor
            .capture_frame_with_renderer(viewhost_id, path, renderer, commands)
            .map_err(|e| EngineError::RenderError(e.to_string()))
    }

    /// Upload images referenced in display commands to the renderer's texture cache.
    ///
    /// This scans the display list for BackgroundImage and Image commands and ensures
    /// any cached images are uploaded to the GPU before rendering.
    /// For data: URLs, images are loaded synchronously on-demand.
    ///
    /// Returns the network images the list names that are not in the cache
    /// yet: the frame is drawn without them.
    fn upload_display_list_images(
        &mut self,
        images: &ImageManager,
        commands: &[rustkit_layout::DisplayCommand],
    ) -> Vec<Url> {
        use std::time::Duration;

        let mut missing = Vec::new();
        // Early exit if no renderer
        let Some(renderer) = &mut self.renderer else {
            return missing;
        };

        // Collect unique image URLs from display list
        // (url, image, whether it is uploaded as a mask texture)
        let mut urls_to_upload: Vec<(String, std::sync::Arc<rustkit_image::LoadedImage>, bool)> =
            Vec::new();
        let mut urls_seen = HashSet::new();

        for cmd in commands {
            // Extract URL from BackgroundImage, Image and MaskedColor commands
            let (url, mask) = match cmd {
                rustkit_layout::DisplayCommand::BackgroundImage { url, .. } => (url, false),
                rustkit_layout::DisplayCommand::Image { url, .. } => (url, false),
                rustkit_layout::DisplayCommand::MaskedColor { url, .. } => (url, true),
                _ => continue,
            };

            if !urls_seen.insert((url.clone(), mask)) {
                continue; // Already processed
            }

            // Skip if already in renderer
            if if mask { renderer.has_mask_image(url) } else { renderer.has_image(url) } {
                continue;
            }

            // Try to parse as URL
            let Ok(parsed_url) = url::Url::parse(url) else {
                tracing::warn!(%url, "Invalid URL for image");
                continue;
            };

            // Try to get from cache or load data: URLs synchronously
            let image = if let Some(cached) = images.get_cached(&parsed_url) {
                Some(cached)
            } else if parsed_url.scheme() == "data" {
                // For data: URLs, load synchronously since they don't require network
                match images.load_blocking(parsed_url.clone()) {
                    Ok(img) => Some(img),
                    Err(e) => {
                        tracing::warn!(?e, %url, "Failed to decode data URL image");
                        None
                    }
                }
            } else {
                // Image not cached and not a data: URL - it will render when loaded
                missing.push(parsed_url.clone());
                None
            };

            if let Some(img) = image {
                urls_to_upload.push((url.clone(), img, mask));
            }
        }

        // Now upload all collected images
        let mut uploaded_count = 0;
        for (url_str, image, mask) in urls_to_upload {
            let frame = image.current_frame(Duration::ZERO);
            let uploaded = if mask {
                renderer.upload_mask_image(&url_str, frame.width(), frame.height(), frame.data())
            } else {
                renderer.upload_image(&url_str, frame.width(), frame.height(), frame.data())
            };
            if let Err(e) = uploaded {
                tracing::warn!(?e, %url_str, "Failed to upload image to renderer");
            } else {
                uploaded_count += 1;
                tracing::debug!(%url_str, "Uploaded image to renderer");
            }
        }
        #[cfg(test)]
        {
            self.images_uploaded += uploaded_count;
        }
        let _ = uploaded_count;
        missing
    }

    /// Capture a frame from a view to a PPM file.
    ///
    /// This renders the current display list to an offscreen texture and saves it.
    /// This is useful for deterministic testing and visual debugging.
    /// The output is a PPM file (simple portable format).
    pub(crate) fn capture_frame(
        &mut self,
        viewhost_id: ViewId,
        display_list: Option<&DisplayList>,
        path: &str,
    ) -> Result<(), EngineError> {
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
        match (display_list, &mut self.renderer) {
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

    /// Render a view's current display list to `output_path` as PNG (plus a
    /// JSON sidecar) and return the capture metadata.
    #[cfg(windows)]
    pub(crate) fn capture_screenshot(
        &mut self,
        bounds: Bounds,
        display_list: Option<&DisplayList>,
        output_path: &std::path::Path,
    ) -> Result<super::ScreenshotMetadata, EngineError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(EngineError::RenderError(format!(
                "Cannot capture screenshot of zero-sized view: {}x{}",
                bounds.width, bounds.height
            )));
        }

        if let Some(renderer) = &mut self.renderer {
            renderer.set_viewport_size(bounds.width, bounds.height);
            let commands = display_list
                .map(|dl| dl.commands.as_slice())
                .unwrap_or(&[]);
            renderer
                .execute_and_capture(commands, output_path)
                .map_err(|e| EngineError::RenderError(e.to_string()))
        } else {
            Err(EngineError::RenderError("No renderer available".to_string()))
        }
    }
}

