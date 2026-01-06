//! TestEngine - Headless test wrapper around RustKit engine.

use rustkit_engine::{Engine, EngineBuilder, EngineViewId};
use rustkit_viewhost::Bounds;

use super::test_frame::TestFrame;

/// Error returned when GPU is not available.
#[derive(Debug)]
pub struct NoGpuError;

impl std::fmt::Display for NoGpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "No GPU adapter available for testing")
    }
}

impl std::error::Error for NoGpuError {}

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
    /// 
    /// Returns `None` if no GPU is available (e.g., in CI without GPU).
    pub fn try_new() -> Result<Self, NoGpuError> {
        Self::try_with_size(800, 600)
    }
    
    /// Create a new test engine with default size (800x600).
    /// 
    /// Panics if no GPU is available.
    pub fn new() -> Self {
        Self::try_new().expect("Failed to create test engine (no GPU?)")
    }

    /// Create a new test engine with specified size.
    /// 
    /// Returns `None` if no GPU is available.
    pub fn try_with_size(width: u32, height: u32) -> Result<Self, NoGpuError> {
        let engine_result = EngineBuilder::new()
            .user_agent("TestEngine/1.0")
            .javascript_enabled(false)
            .build();
        
        let mut engine = match engine_result {
            Ok(e) => e,
            Err(e) => {
                // Check if this is a GPU error
                let err_str = format!("{:?}", e);
                if err_str.contains("GPU") || err_str.contains("adapter") {
                    return Err(NoGpuError);
                }
                panic!("Failed to create test engine: {:?}", e);
            }
        };

        let bounds = Bounds {
            x: 0,
            y: 0,
            width,
            height,
        };

        // Use headless mode for testing (no window required!)
        let view_result = engine.create_headless_view(bounds);
        
        let view_id = match view_result {
            Ok(id) => id,
            Err(e) => {
                let err_str = format!("{:?}", e);
                if err_str.contains("GPU") || err_str.contains("adapter") {
                    return Err(NoGpuError);
                }
                panic!("Failed to create headless view: {:?}", e);
            }
        };

        Ok(Self {
            engine,
            view_id,
            width,
            height,
        })
    }

    /// Create a new test engine with specified size.
    /// 
    /// Panics if no GPU is available.
    pub fn with_size(width: u32, height: u32) -> Self {
        Self::try_with_size(width, height).expect("Failed to create test engine (no GPU?)")
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
        match TestEngine::try_new() {
            Ok(_engine) => {
                // If we got here, engine created successfully
            }
            Err(_) => {
                eprintln!("Skipping test: No GPU available");
            }
        }
    }

    #[test]
    fn test_engine_custom_size() {
        match TestEngine::try_with_size(1024, 768) {
            Ok(_engine) => {
                // Successfully created with custom size
            }
            Err(_) => {
                eprintln!("Skipping test: No GPU available");
            }
        }
    }
}
