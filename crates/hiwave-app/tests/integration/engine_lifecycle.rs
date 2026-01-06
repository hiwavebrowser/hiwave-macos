//! Engine lifecycle integration tests
//!
//! These tests verify that the RustKit engine:
//! - Initializes correctly with GPU support
//! - Creates and manages views properly
//! - Cleans up resources without leaks

use crate::support::TestEngine;

#[test]
#[cfg(target_os = "macos")]
fn test_engine_creates_successfully() {
    // Create engine with default settings
    let _engine = TestEngine::new();

    // If we reached here, engine created without panic
    // This verifies GPU initialization worked
}

#[test]
#[cfg(target_os = "macos")]
fn test_engine_creates_with_custom_size() {
    let _engine = TestEngine::with_size(1024, 768);

    // Successfully created with custom viewport size
}

#[test]
#[cfg(target_os = "macos")]
fn test_multiple_engines_can_coexist() {
    // Create multiple engines to ensure they don't interfere
    let _engine1 = TestEngine::new();
    let _engine2 = TestEngine::new();
    let _engine3 = TestEngine::new();

    // All three should be able to exist simultaneously
}

#[test]
#[cfg(target_os = "macos")]
fn test_view_resize() {
    let mut engine = TestEngine::new();

    // Resize to larger
    engine.resize(1920, 1080).expect("Should resize to 1920x1080");

    // Resize to smaller
    engine.resize(320, 240).expect("Should resize to 320x240");

    // Resize back to default
    engine.resize(800, 600).expect("Should resize to 800x600");
}

#[test]
#[cfg(target_os = "macos")]
fn test_engine_cleanup_on_drop() {
    // Create engine in a scope
    {
        let _engine = TestEngine::new();
        // Engine exists here
    }
    // Engine dropped here, should clean up resources

    // Create another engine to verify cleanup didn't break anything
    let _engine2 = TestEngine::new();
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_simple_html() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head><title>Test</title></head>
        <body><h1>Hello World</h1></body>
        </html>"#;

    engine.load_html(html).expect("Should load simple HTML");
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_empty_html() {
    let mut engine = TestEngine::new();

    // Empty document
    let html = r#"<!DOCTYPE html><html><body></body></html>"#;

    engine.load_html(html).expect("Should load empty HTML");
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_malformed_html() {
    let mut engine = TestEngine::new();

    // Malformed HTML should still load (HTML is error-tolerant)
    let html = r#"<html><body><p>Unclosed paragraph<div>Nested wrong</p></div>"#;

    engine.load_html(html).expect("Should load malformed HTML");
}
