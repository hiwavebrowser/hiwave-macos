//! macOS ViewHost implementation using NSView and Cocoa
//!
//! This module provides the macOS-specific implementation of ViewHost,
//! using NSView for rendering surfaces and TAO window handles.

use crate::{Bounds, ViewHostError, ViewId};
use raw_window_handle::RawWindowHandle;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tracing::{debug, info, warn};

#[cfg(target_os = "macos")]
use cocoa::{
    base::{id, nil},
};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

/// macOS-specific view state
#[cfg(target_os = "macos")]
struct MacOSViewState {
    _id: ViewId,
    view: id, // NSView
    bounds: Bounds,
    dpi: u32,
    visible: bool,
    _focused: bool,
}

/// macOS ViewHost implementation
#[cfg(target_os = "macos")]
pub struct MacOSViewHost {
    views: RwLock<HashMap<ViewId, Arc<Mutex<MacOSViewState>>>>,
}

#[cfg(target_os = "macos")]
impl MacOSViewHost {
    pub fn new() -> Self {
        Self {
            views: RwLock::new(HashMap::new()),
        }
    }

    /// Convert top-left origin bounds to Cocoa's bottom-left origin coordinate system.
    ///
    /// HiWave/Wry uses top-left origin (y=0 at top, increasing downward).
    /// Cocoa uses bottom-left origin (y=0 at bottom, increasing upward).
    ///
    /// Formula: y_cocoa = parent_height - bounds.y - bounds.height
    fn convert_to_cocoa_frame(bounds: Bounds, parent_height: f64) -> cocoa::foundation::NSRect {
        let y_cocoa = parent_height - bounds.y as f64 - bounds.height as f64;
        cocoa::foundation::NSRect::new(
            cocoa::foundation::NSPoint::new(bounds.x as f64, y_cocoa),
            cocoa::foundation::NSSize::new(bounds.width as f64, bounds.height as f64),
        )
    }

    /// Create a view from a TAO window handle
    pub fn create_view_from_window(
        &self,
        window_handle: RawWindowHandle,
        bounds: Bounds,
    ) -> Result<ViewId, ViewHostError> {
        let view_id = ViewId::new();
        debug!(?view_id, ?bounds, "Creating macOS view");

        // Extract NSWindow from raw window handle
        // In raw-window-handle 0.6, AppKitHandle contains ns_view, not ns_window
        // We need to get the window from the view
        let ns_view = match window_handle {
            RawWindowHandle::AppKit(handle) => {
                handle.ns_view.as_ptr() as id
            }
            _ => {
                return Err(ViewHostError::InvalidParent);
            }
        };
        
        if ns_view == nil {
            return Err(ViewHostError::InvalidParent);
        }
        
        // Get the window from the view
        let ns_window: id = unsafe { msg_send![ns_view, window] };

        if ns_window == nil {
            return Err(ViewHostError::InvalidParent);
        }

        // Get the content view of the window
        let content_view: id = unsafe { msg_send![ns_window, contentView] };
        if content_view == nil {
            return Err(ViewHostError::WindowCreation(
                "Window has no content view".to_string(),
            ));
        }

        // Get the content view's frame to get parent height for coordinate conversion
        let parent_frame: cocoa::foundation::NSRect = unsafe { msg_send![content_view, frame] };
        let parent_height = parent_frame.size.height;

        debug!(parent_height, "Got parent content view height");

        // Create a new NSView for our content
        // Convert from top-left origin (HiWave/Wry) to bottom-left origin (Cocoa)
        let frame = Self::convert_to_cocoa_frame(bounds, parent_height);
        debug!(?bounds, cocoa_y = frame.origin.y, "Converted bounds to Cocoa coordinates");

        let view: id = unsafe {
            use objc::runtime::Class;
            let view_class = Class::get("NSView").expect("NSView class not found");
            let view: id = msg_send![view_class, alloc];
            msg_send![view, initWithFrame: frame]
        };

        if view == nil {
            return Err(ViewHostError::WindowCreation(
                "Failed to create NSView".to_string(),
            ));
        }

        // Configure the view for layer-backed rendering
        // NOTE: Don't manually create CAMetalLayer - let wgpu manage it
        // wgpu will create and configure its own Metal layer when the surface is created
        unsafe {
            // Enable layer-backed rendering (required for wgpu)
            let wants_layer: bool = true;
            let _: () = msg_send![view, setWantsLayer: wants_layer];
        }

        // Add view to content view
        unsafe {
            let _: () = msg_send![content_view, addSubview: view];
            debug!(?view_id, "Added RustKit view as subview");
        }

        // Get DPI (backing scale factor)
        let dpi = unsafe {
            let scale: f64 = msg_send![ns_window, backingScaleFactor];
            (scale * 96.0) as u32
        };

        let state = Arc::new(Mutex::new(MacOSViewState {
            _id: view_id,
            view,
            bounds,
            dpi,
            visible: true,
            _focused: false,
        }));

        {
            let mut views = self.views.write().unwrap();
            views.insert(view_id, state);
        }

        info!(?view_id, dpi, "macOS view created");
        Ok(view_id)
    }

    /// Get the NSView for a view ID
    pub fn get_view(&self, view_id: ViewId) -> Result<id, ViewHostError> {
        let state_arc = {
            let views = self.views.read().unwrap();
            views
                .get(&view_id)
                .ok_or(ViewHostError::ViewNotFound(view_id))?
                .clone() // Clone the Arc to extend lifetime
        }; // views lock is released here
        let view = state_arc.lock().unwrap().view;
        Ok(view)
    }

    /// Get the raw window handle for a view
    pub fn get_raw_window_handle(&self, view_id: ViewId) -> Result<RawWindowHandle, ViewHostError> {
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;
        let view = state.lock().unwrap().view;

        // Get the window from the view
        let window: id = unsafe { msg_send![view, window] };
        if window == nil {
            warn!(?view_id, "View has no window attached");
            return Err(ViewHostError::ViewNotFound(view_id));
        }

        // Verify view state
        unsafe {
            let is_hidden: bool = msg_send![view, isHidden];
            let superview: id = msg_send![view, superview];
            let has_superview = superview != nil;
            let frame: cocoa::foundation::NSRect = msg_send![view, frame];
            info!(
                ?view_id,
                is_hidden,
                has_superview,
                frame_x = frame.origin.x,
                frame_y = frame.origin.y,
                frame_w = frame.size.width,
                frame_h = frame.size.height,
                "Getting raw window handle - view state"
            );
        }

        // Create raw window handle
        // AppKitWindowHandle::new() expects NonNull<c_void>
        use std::ptr::NonNull;
        let handle = RawWindowHandle::AppKit(
            raw_window_handle::AppKitWindowHandle::new(
                NonNull::new(view as *mut std::ffi::c_void)
                    .expect("View pointer is null")
            )
        );

        Ok(handle)
    }

    /// Set view bounds
    pub fn set_bounds(&self, view_id: ViewId, bounds: Bounds) -> Result<(), ViewHostError> {
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;

        let mut state = state.lock().unwrap();
        state.bounds = bounds;

        unsafe {
            // Get the superview to determine parent height for coordinate conversion
            let superview: id = msg_send![state.view, superview];
            let parent_height = if superview != nil {
                let parent_frame: cocoa::foundation::NSRect = msg_send![superview, frame];
                parent_frame.size.height
            } else {
                // Fallback: try to get window content view height
                let window: id = msg_send![state.view, window];
                if window != nil {
                    let content_view: id = msg_send![window, contentView];
                    if content_view != nil {
                        let content_frame: cocoa::foundation::NSRect = msg_send![content_view, frame];
                        content_frame.size.height
                    } else {
                        bounds.height as f64 + bounds.y as f64 // Fallback
                    }
                } else {
                    bounds.height as f64 + bounds.y as f64 // Fallback
                }
            };

            // Convert from top-left origin to Cocoa's bottom-left origin
            let frame = Self::convert_to_cocoa_frame(bounds, parent_height);
            let _: () = msg_send![state.view, setFrame: frame];
        }

        debug!(?view_id, ?bounds, "View bounds updated");
        Ok(())
    }

    /// Get view bounds
    pub fn get_bounds(&self, view_id: ViewId) -> Result<Bounds, ViewHostError> {
        let state_arc = {
            let views = self.views.read().unwrap();
            views
                .get(&view_id)
                .ok_or(ViewHostError::ViewNotFound(view_id))?
                .clone() // Clone the Arc to extend lifetime
        }; // views lock is released here
        let bounds = state_arc.lock().unwrap().bounds;
        Ok(bounds)
    }

    /// Set view visibility
    pub fn set_visible(&self, view_id: ViewId, visible: bool) -> Result<(), ViewHostError> {
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;

        let mut state = state.lock().unwrap();
        state.visible = visible;

        unsafe {
            let hidden: bool = !visible;
            let _: () = msg_send![state.view, setHidden: hidden];
        }

        debug!(?view_id, visible, "View visibility changed");
        Ok(())
    }

    /// Focus a view
    pub fn focus(&self, view_id: ViewId) -> Result<(), ViewHostError> {
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;

        let state = state.lock().unwrap();

        unsafe {
            let window: id = msg_send![state.view, window];
            if window != nil {
                let _: () = msg_send![window, makeFirstResponder: state.view];
            }
        }

        debug!(?view_id, "View focused");
        Ok(())
    }

    /// Get DPI for a view
    pub fn get_dpi(&self, view_id: ViewId) -> Result<u32, ViewHostError> {
        let state_arc = {
            let views = self.views.read().unwrap();
            views
                .get(&view_id)
                .ok_or(ViewHostError::ViewNotFound(view_id))?
                .clone() // Clone the Arc to extend lifetime
        }; // views lock is released here
        let dpi = state_arc.lock().unwrap().dpi;
        Ok(dpi)
    }

    /// Destroy a view
    pub fn destroy_view(&self, view_id: ViewId) -> Result<(), ViewHostError> {
        let state_arc = {
            let mut views = self.views.write().unwrap();
            views
                .remove(&view_id)
                .ok_or(ViewHostError::ViewNotFound(view_id))?
        }; // views lock is released here
        
        let view = state_arc.lock().unwrap().view;

        unsafe {
            let _: () = msg_send![view, removeFromSuperview];
        }

        debug!(?view_id, "View destroyed");
        Ok(())
    }

    /// Pump macOS event loop (stub for now)
    pub fn pump_messages(&self) -> bool {
        // TODO: Implement proper event loop pumping
        // For now, this is a no-op as TAO handles the event loop
        true
    }
}

#[cfg(not(target_os = "macos"))]
pub struct MacOSViewHost;

#[cfg(not(target_os = "macos"))]
impl MacOSViewHost {
    pub fn new() -> Self {
        Self
    }
}

