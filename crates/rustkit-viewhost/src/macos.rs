//! macOS ViewHost implementation using NSView and Cocoa
//!
//! This module provides the macOS-specific implementation of ViewHost,
//! using NSView for rendering surfaces and TAO window handles.

use crate::{Bounds, ViewHostError, ViewId};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{NSView, NSWindow},
    base::{id, nil},
};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

/// macOS-specific view state
#[cfg(target_os = "macos")]
struct MacOSViewState {
    id: ViewId,
    view: id, // NSView
    bounds: Bounds,
    dpi: u32,
    visible: bool,
    focused: bool,
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
                unsafe { handle.ns_view.as_ptr() as id }
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

        // Create a new NSView for our content
        let view: id = unsafe {
            use objc::runtime::Class;
            let view_class = Class::get("NSView").expect("NSView class not found");
            let view: id = msg_send![view_class, alloc];
            let frame = cocoa::foundation::NSRect::new(
                cocoa::foundation::NSPoint::new(bounds.x as f64, bounds.y as f64),
                cocoa::foundation::NSSize::new(bounds.width as f64, bounds.height as f64),
            );
            msg_send![view, initWithFrame: frame]
        };

        if view == nil {
            return Err(ViewHostError::WindowCreation(
                "Failed to create NSView".to_string(),
            ));
        }

        // Configure the view
        unsafe {
            // Enable layer-backed rendering for Metal
            let wants_layer: bool = true;
            let _: () = msg_send![view, setWantsLayer: wants_layer];

            // Set up Metal layer
            let layer_class = objc::runtime::Class::get("CAMetalLayer").ok_or_else(|| {
                ViewHostError::WindowCreation("CAMetalLayer not available".to_string())
            })?;
            let layer: id = msg_send![layer_class, layer];
            let _: () = msg_send![view, setLayer: layer];
        }

        // Add view to content view
        unsafe {
            let _: () = msg_send![content_view, addSubview: view];
        }

        // Get DPI (backing scale factor)
        let dpi = unsafe {
            let scale: f64 = msg_send![ns_window, backingScaleFactor];
            (scale * 96.0) as u32
        };

        let state = Arc::new(Mutex::new(MacOSViewState {
            id: view_id,
            view,
            bounds,
            dpi,
            visible: true,
            focused: false,
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
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;
        Ok(state.lock().unwrap().view)
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
            return Err(ViewHostError::ViewNotFound(view_id));
        }

        // Create raw window handle
        let handle = RawWindowHandle::AppKit(
            raw_window_handle::AppKitWindowHandle::new(unsafe { view as *mut std::ffi::c_void })
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
            let frame = cocoa::foundation::NSRect::new(
                cocoa::foundation::NSPoint::new(bounds.x as f64, bounds.y as f64),
                cocoa::foundation::NSSize::new(bounds.width as f64, bounds.height as f64),
            );
            let _: () = msg_send![state.view, setFrame: frame];
        }

        debug!(?view_id, ?bounds, "View bounds updated");
        Ok(())
    }

    /// Get view bounds
    pub fn get_bounds(&self, view_id: ViewId) -> Result<Bounds, ViewHostError> {
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;
        Ok(state.lock().unwrap().bounds)
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
        let views = self.views.read().unwrap();
        let state = views
            .get(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;
        Ok(state.lock().unwrap().dpi)
    }

    /// Destroy a view
    pub fn destroy_view(&self, view_id: ViewId) -> Result<(), ViewHostError> {
        let views = self.views.write().unwrap();
        let state = views
            .remove(&view_id)
            .ok_or(ViewHostError::ViewNotFound(view_id))?;

        let view = state.lock().unwrap().view;

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

