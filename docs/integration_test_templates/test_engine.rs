//! TestEngine Helper - Headless test wrapper around RustKit engine
//!
//! This helper provides a convenient API for integration testing without
//! requiring a full GUI window. It handles engine lifecycle, event pumping,
//! and frame capture.
//!
//! Example usage:
//! ```rust
//! let mut engine = TestEngine::new();
//! engine.load_html("<h1>Test</h1>").unwrap();
//! let frame = engine.render_and_capture().unwrap();
//! assert!(!frame.is_blank());
//! ```

use rustkit_engine::{Engine, EngineBuilder, EngineEvent, EngineViewId};
use rustkit_viewhost::Bounds;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Test window handle that works without a real GUI window.
pub struct TestWindow {
    // Platform-specific implementation
    #[cfg(target_os = "macos")]
    _handle: (),
}

impl TestWindow {
    pub fn create() -> Self {
        // Create offscreen window or mock handle
        Self {
            #[cfg(target_os = "macos")]
            _handle: (),
        }
    }

    pub fn raw_handle(&self) -> raw_window_handle::RawWindowHandle {
        // Return appropriate handle for platform
        todo!("Implement platform-specific window handle")
    }
}

/// Headless test engine wrapper.
pub struct TestEngine {
    engine: Engine,
    view_id: EngineViewId,
    event_queue: Vec<EngineEvent>,
    window: TestWindow,
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
            .javascript_enabled(false) // Disable JS initially
            .build()
            .expect("Failed to create test engine");

        let window = TestWindow::create();
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
            event_queue: Vec::new(),
            window,
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

    /// Load a URL (file:// or http:// when supported).
    pub fn load_url(&mut self, url: &str) -> Result<(), String> {
        self.engine
            .load_url(self.view_id, url)
            .map_err(|e| format!("Failed to load URL: {:?}", e))
    }

    /// Render the current view and return a frame.
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
        let temp_path = format!("/tmp/test_frame_{}_{}.ppm",
            std::process::id(),
            self.view_id
        );

        self.engine
            .capture_frame(self.view_id, &temp_path)
            .map_err(|e| format!("Failed to capture frame: {:?}", e))?;

        // Load frame from file
        TestFrame::load(&temp_path)
    }

    /// Get the current URL.
    pub fn current_url(&self) -> String {
        // Get from engine state
        todo!("Implement current_url")
    }

    /// Get history length.
    pub fn history_length(&self) -> usize {
        // Get from engine navigation state
        todo!("Implement history_length")
    }

    /// Navigate back in history.
    pub fn go_back(&mut self) -> Result<(), String> {
        self.engine
            .go_back(self.view_id)
            .map_err(|e| format!("Failed to go back: {:?}", e))
    }

    /// Navigate forward in history.
    pub fn go_forward(&mut self) -> Result<(), String> {
        self.engine
            .go_forward(self.view_id)
            .map_err(|e| format!("Failed to go forward: {:?}", e))
    }

    /// Query for an element by selector.
    pub fn query_selector(&self, selector: &str) -> Option<TestElement> {
        // Query DOM tree
        todo!("Implement query_selector: {}", selector)
    }

    /// Get layout bounds for an element.
    pub fn get_element_bounds(&self, selector: &str) -> Option<Bounds> {
        // Get from layout tree
        todo!("Implement get_element_bounds: {}", selector)
    }

    /// Send a mouse click event.
    pub fn send_mouse_click(&mut self, x: i32, y: i32) -> Result<(), String> {
        use rustkit_core::InputEvent;

        // Send mouse down
        let down_event = InputEvent::MouseDown {
            x,
            y,
            button: rustkit_core::MouseButton::Left,
        };
        self.engine
            .send_event(self.view_id, down_event)
            .map_err(|e| format!("Failed to send mouse down: {:?}", e))?;

        // Send mouse up
        let up_event = InputEvent::MouseUp {
            x,
            y,
            button: rustkit_core::MouseButton::Left,
        };
        self.engine
            .send_event(self.view_id, up_event)
            .map_err(|e| format!("Failed to send mouse up: {:?}", e))
    }

    /// Send a keyboard event.
    pub fn send_key(&mut self, key: &str) -> Result<(), String> {
        use rustkit_core::InputEvent;

        let key_event = InputEvent::KeyDown {
            key: key.to_string(),
            modifiers: 0,
        };
        self.engine
            .send_event(self.view_id, key_event)
            .map_err(|e| format!("Failed to send key: {:?}", e))
    }

    /// Wait for navigation to complete.
    pub fn wait_for_navigation(&mut self) -> Result<(), String> {
        self.wait_for_navigation_timeout(Duration::from_secs(5))
    }

    /// Wait for navigation with timeout.
    pub fn wait_for_navigation_timeout(&mut self, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();

        while start.elapsed() < timeout {
            // Poll events
            self.pump_events()?;

            // Check for PageLoaded event
            if self.event_queue.iter().any(|e| matches!(e, EngineEvent::PageLoaded { .. })) {
                return Ok(());
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        Err("Navigation timeout".to_string())
    }

    /// Pump engine events.
    fn pump_events(&mut self) -> Result<(), String> {
        // Get events from engine
        todo!("Implement event pumping")
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
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        // Clean up view and engine
        let _ = self.engine.destroy_view(self.view_id);
    }
}

/// Test element wrapper for DOM queries.
pub struct TestElement {
    id: String,
}

impl TestElement {
    pub fn text_content(&self) -> String {
        todo!("Get text content")
    }

    pub fn parent_id(&self) -> Option<String> {
        todo!("Get parent element ID")
    }

    pub fn get_attribute(&self, name: &str) -> Option<String> {
        todo!("Get attribute: {}", name)
    }
}

/// Captured frame for testing.
pub struct TestFrame {
    pub width: u32,
    pub height: u32,
    pixels: Vec<RGB>,
}

impl TestFrame {
    /// Load frame from PPM file.
    pub fn load(path: &str) -> Result<Self, String> {
        let content = std::fs::read(path)
            .map_err(|e| format!("Failed to read frame: {}", e))?;

        // Parse PPM format
        // Format: P6\nWIDTH HEIGHT\n255\n<binary RGB data>
        let header_end = content
            .windows(2)
            .position(|w| w == b"\n\n" || w == b"\r\n")
            .ok_or("Invalid PPM format")?;

        let header = std::str::from_utf8(&content[..header_end])
            .map_err(|_| "Invalid UTF-8 in PPM header")?;

        let lines: Vec<&str> = header.lines().collect();
        if lines.len() < 3 {
            return Err("Invalid PPM header".to_string());
        }

        // Parse dimensions from second line (skip P6 magic)
        let dims: Vec<&str> = lines[1].split_whitespace().collect();
        if dims.len() != 2 {
            return Err("Invalid PPM dimensions".to_string());
        }

        let width: u32 = dims[0].parse().map_err(|_| "Invalid width")?;
        let height: u32 = dims[1].parse().map_err(|_| "Invalid height")?;

        // Read pixel data (skip header and max value line)
        let pixel_data = &content[header_end + 2..];
        let mut pixels = Vec::new();

        for chunk in pixel_data.chunks(3) {
            if chunk.len() == 3 {
                pixels.push(RGB {
                    r: chunk[0],
                    g: chunk[1],
                    b: chunk[2],
                });
            }
        }

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Sample a pixel at (x, y).
    pub fn sample_pixel(&self, x: u32, y: u32) -> RGB {
        let idx = (y * self.width + x) as usize;
        self.pixels[idx]
    }

    /// Check if frame is blank (all same color).
    pub fn is_blank(&self) -> bool {
        if self.pixels.is_empty() {
            return true;
        }

        let first = self.pixels[0];
        self.pixels.iter().all(|p| *p == first)
    }

    /// Compare this frame with another.
    pub fn compare(&self, other: &TestFrame, tolerance: u8) -> FrameDiff {
        if self.width != other.width || self.height != other.height {
            return FrameDiff {
                size_mismatch: true,
                diff_pixels: 0,
                total_pixels: 0,
                diff_percent: 0.0,
            };
        }

        let total = self.pixels.len();
        let mut diff_count = 0;

        for (a, b) in self.pixels.iter().zip(other.pixels.iter()) {
            if !a.near(*b, tolerance) {
                diff_count += 1;
            }
        }

        FrameDiff {
            size_mismatch: false,
            diff_pixels: diff_count,
            total_pixels: total,
            diff_percent: (diff_count as f64 / total as f64) * 100.0,
        }
    }
}

/// RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGB {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn near(self, other: RGB, tolerance: u8) -> bool {
        let dr = (self.r as i32 - other.r as i32).abs();
        let dg = (self.g as i32 - other.g as i32).abs();
        let db = (self.b as i32 - other.b as i32).abs();

        dr <= tolerance as i32 && dg <= tolerance as i32 && db <= tolerance as i32
    }
}

/// Frame comparison result.
pub struct FrameDiff {
    pub size_mismatch: bool,
    pub diff_pixels: usize,
    pub total_pixels: usize,
    pub diff_percent: f64,
}

/// Assert that a color matches expected within tolerance.
pub fn assert_color_near(actual: RGB, expected: RGB, tolerance: u8) {
    assert!(
        actual.near(expected, tolerance),
        "Color mismatch: expected {:?}, got {:?} (tolerance: {})",
        expected,
        actual,
        tolerance
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_near() {
        let color1 = RGB::new(100, 150, 200);
        let color2 = RGB::new(102, 148, 201);

        assert!(color1.near(color2, 5));
        assert!(!color1.near(color2, 1));
    }

    #[test]
    fn test_frame_is_blank() {
        let frame = TestFrame {
            width: 10,
            height: 10,
            pixels: vec![RGB::new(255, 0, 0); 100],
        };

        assert!(frame.is_blank());
    }
}
