//! TestEngine - Headless test wrapper around RustKit engine.

use rustkit_engine::{Engine, EngineBuilder, EngineViewId};
use rustkit_viewhost::Bounds;

use super::test_frame::TestFrame;

#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{NSBackingStoreType, NSWindowStyleMask},
    base::{id, nil, NO},
    foundation::{NSPoint, NSRect, NSSize},
};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

/// Test window wrapper.
///
/// Creates a real but invisible NSWindow for headless testing.
/// The window is never shown but provides a valid GPU rendering surface.
#[cfg(target_os = "macos")]
pub struct TestWindow {
    ns_window: id,
}

#[cfg(target_os = "macos")]
impl TestWindow {
    /// Create a real offscreen test window.
    ///
    /// This creates an actual NSWindow with a valid NSView, but sets it to invisible.
    /// This allows GPU rendering to work without showing anything on screen.
    pub fn create(width: u32, height: u32) -> Self {
        unsafe {
            // Get NSWindow class
            let ns_window_class = cocoa::appkit::NSWindow::class();

            // Allocate and initialize NSWindow
            let ns_window: id = msg_send![ns_window_class, alloc];
            let ns_window: id = msg_send![ns_window,
                initWithContentRect: NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(width as f64, height as f64)
                )
                styleMask: NSWindowStyleMask::NSBorderlessWindowMask
                backing: NSBackingStoreType::NSBackingStoreBuffered
                defer: NO
            ];

            // Make it invisible
            let _: () = msg_send![ns_window, setIsVisible: NO];

            // Set title for debugging
            let title = cocoa::foundation::NSString::alloc(nil);
            let title = cocoa::foundation::NSString::init_str(title, "TestWindow");
            let _: () = msg_send![ns_window, setTitle: title];

            Self { ns_window }
        }
    }

    /// Get the raw window handle for the test window.
    pub fn raw_handle(&self) -> raw_window_handle::RawWindowHandle {
        use raw_window_handle::{AppKitWindowHandle, RawWindowHandle};
        use std::ptr::NonNull;

        unsafe {
            // Get the content view from the window
            let content_view: id = msg_send![self.ns_window, contentView];

            // Create handle with the real NSView pointer
            let ns_view = NonNull::new(content_view as *mut std::ffi::c_void)
                .expect("Content view should not be null");

            let handle = AppKitWindowHandle::new(ns_view);
            RawWindowHandle::AppKit(handle)
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            // Close and release the window
            let _: () = msg_send![self.ns_window, close];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub struct TestWindow {
    _marker: std::marker::PhantomData<()>,
}

#[cfg(not(target_os = "macos"))]
impl TestWindow {
    pub fn create(_width: u32, _height: u32) -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    pub fn raw_handle(&self) -> raw_window_handle::RawWindowHandle {
        // Platform-specific implementation needed
        panic!("TestWindow only supports macOS currently")
    }
}

/// Headless test engine wrapper.
pub struct TestEngine {
    engine: Engine,
    view_id: EngineViewId,
    _window: TestWindow,
    width: u32,
    height: u32,
}

impl TestEngine {
    /// Create a new test engine with default size (800x600).
    pub fn new() -> Self {
        Self::with_size(800, 600)
    }

    /// Create a new test engine with specified size.
    pub fn with_size(width: u32, height: u32) -> Self {
        let mut engine = EngineBuilder::new()
            .user_agent("TestEngine/1.0")
            .javascript_enabled(false)
            .build()
            .expect("Failed to create test engine");

        let window = TestWindow::create(width, height);
        let bounds = Bounds {
            x: 0,
            y: 0,
            width,
            height,
        };

        let view_id = engine
            .create_view(window.raw_handle(), bounds)
            .expect("Failed to create test view");

        Self {
            engine,
            view_id,
            _window: window,
            width,
            height,
        }
    }

    /// Load HTML content into the engine.
    pub fn load_html(&mut self, html: &str) -> Result<(), String> {
        self.engine
            .load_html(self.view_id, html)
            .map_err(|e| format!("Failed to load HTML: {:?}", e))
    }

    /// Render the current view.
    pub fn render(&mut self) -> Result<(), String> {
        self.engine
            .render_view(self.view_id)
            .map_err(|e| format!("Failed to render: {:?}", e))
    }

    /// Render and capture frame to a TestFrame.
    pub fn render_and_capture(&mut self) -> Result<TestFrame, String> {
        // Render first
        self.render()?;

        // Capture to temporary file
        let temp_path = format!(
            "/tmp/test_frame_{}_{}.ppm",
            std::process::id(),
            self.view_id.raw()
        );

        self.engine
            .capture_frame(self.view_id, &temp_path)
            .map_err(|e| format!("Failed to capture frame: {:?}", e))?;

        // Load frame from file
        let frame = TestFrame::load(&temp_path)?;

        // Clean up temp file
        std::fs::remove_file(&temp_path).ok();

        Ok(frame)
    }

    /// Resize the view.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.width = width;
        self.height = height;

        let bounds = Bounds {
            x: 0,
            y: 0,
            width,
            height,
        };

        self.engine
            .resize_view(self.view_id, bounds)
            .map_err(|e| format!("Failed to resize: {:?}", e))
    }

    /// Get the view ID.
    pub fn view_id(&self) -> EngineViewId {
        self.view_id
    }

    /// Get engine reference (for advanced testing).
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        // Clean up view
        let _ = self.engine.destroy_view(self.view_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creates() {
        let _engine = TestEngine::new();
        // If we got here, engine created successfully
    }

    #[test]
    fn test_engine_custom_size() {
        let _engine = TestEngine::with_size(1024, 768);
        // Successfully created with custom size
    }
}
