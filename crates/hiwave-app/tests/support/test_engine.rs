//! TestEngine - Headless test wrapper around RustKit engine.

use rustkit_engine::{Engine, EngineBuilder, EngineViewId};
use rustkit_viewhost::Bounds;

use super::test_frame::TestFrame;

/// Headless test engine wrapper.
///
/// Uses RustKit's headless mode to render without requiring a window.
/// Perfect for automated testing and CI environments.
pub struct TestEngine {
    engine: Engine,
    view_id: EngineViewId,
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

        let bounds = Bounds {
            x: 0,
            y: 0,
            width,
            height,
        };

        // Use headless mode for testing (no window required!)
        let view_id = engine
            .create_headless_view(bounds)
            .expect("Failed to create headless test view");

        Self {
            engine,
            view_id,
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
