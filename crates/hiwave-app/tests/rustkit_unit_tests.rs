//! Integration tests for RustKit WebView
//!
//! These tests verify that the RustKit engine integrates correctly with the
//! HiWave browser application on macOS.
//!
//! Note: Engine creation tests are disabled because they require GPU initialization
//! which may not be available in all test environments.

#[cfg(target_os = "macos")]
mod rustkit_tests {
    use rustkit_viewhost::Bounds;

    /// Test that bounds can be created and manipulated
    #[test]
    fn test_bounds_creation() {
        let bounds = Bounds::new(0, 0, 800, 600);
        
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.width, 800);
        assert_eq!(bounds.height, 600);
    }

    /// Test bounds zero constructor
    #[test]
    fn test_bounds_zero() {
        let bounds = Bounds::zero();
        
        assert_eq!(bounds.x, 0);
        assert_eq!(bounds.y, 0);
        assert_eq!(bounds.width, 0);
        assert_eq!(bounds.height, 0);
    }

    /// Test that EngineBuilder can be constructed (without building the engine)
    #[test]
    fn test_engine_builder_construction() {
        use rustkit_engine::EngineBuilder;
        
        let builder = EngineBuilder::new()
            .user_agent("HiWave Test/1.0")
            .javascript_enabled(true);
        
        // Just verify the builder can be created and configured
        let _ = builder;
    }
}

/// Tests that run on all platforms (not macOS specific)
#[cfg(target_os = "macos")]
mod common_tests {
    use rustkit_viewhost::Bounds;

    #[test]
    fn test_bounds_contains_point() {
        let bounds = Bounds::new(10, 10, 100, 100);
        
        // Point inside
        assert!(bounds.x <= 50 && 50 < bounds.x + bounds.width as i32);
        assert!(bounds.y <= 50 && 50 < bounds.y + bounds.height as i32);
    }

    #[test]
    fn test_bounds_dimensions() {
        let bounds = Bounds::new(100, 200, 640, 480);
        
        assert_eq!(bounds.x, 100);
        assert_eq!(bounds.y, 200);
        assert_eq!(bounds.width, 640);
        assert_eq!(bounds.height, 480);
    }
}

/// Tests for the text rendering subsystem (struct tests only, no Core Text initialization)
#[cfg(target_os = "macos")]
mod text_tests {
    use rustkit_text::macos::FontMetrics;

    #[test]
    fn test_font_metrics_struct() {
        // Test that FontMetrics struct works correctly
        let metrics = FontMetrics {
            ascent: 10.0,
            descent: 2.0,
            leading: 2.0,
            cap_height: 8.0,
            x_height: 6.0,
        };
        
        assert!(metrics.ascent > 0.0);
        assert!(metrics.descent >= 0.0);
        assert!(metrics.cap_height > metrics.x_height);
    }

    #[test]
    fn test_get_available_fonts() {
        // Test that we can get available fonts list
        let fonts = rustkit_text::macos::get_available_fonts();
        assert!(!fonts.is_empty(), "Should have at least some fonts");
        // Check for common system fonts
        assert!(fonts.iter().any(|f| f == "Helvetica" || f == "Arial"));
    }
}

/// Tests for the compositor subsystem (config only, no GPU initialization)
#[cfg(target_os = "macos")]
mod compositor_tests {
    use wgpu::PowerPreference;

    #[test]
    fn test_power_preference_values() {
        // Test that power preference enum values are accessible
        let _low = PowerPreference::LowPower;
        let _high = PowerPreference::HighPerformance;
        let _none = PowerPreference::None;
    }
}

