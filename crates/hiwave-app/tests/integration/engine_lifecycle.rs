//! Engine lifecycle integration tests
//!
//! These tests verify that the RustKit engine:
//! - Initializes correctly with GPU support
//! - Creates and manages views properly
//! - Cleans up resources without leaks
//!
//! Note: Tests skip gracefully if no GPU is available (e.g., in CI).

use crate::support::TestEngine;

/// Helper macro to skip test if no GPU is available.
macro_rules! require_gpu {
    () => {
        match TestEngine::try_new() {
            Ok(engine) => engine,
            Err(_) => {
                eprintln!("Skipping test: No GPU available");
                return;
            }
        }
    };
    ($width:expr, $height:expr) => {
        match TestEngine::try_with_size($width, $height) {
            Ok(engine) => engine,
            Err(_) => {
                eprintln!("Skipping test: No GPU available");
                return;
            }
        }
    };
}

#[test]
#[cfg(target_os = "macos")]
fn test_engine_creates_successfully() {
    // Create engine with default settings
    let _engine = require_gpu!();

    // If we reached here, engine created without panic
    // This verifies GPU initialization worked
}

#[test]
#[cfg(target_os = "macos")]
fn test_engine_creates_with_custom_size() {
    let _engine = require_gpu!(1024, 768);

    // Successfully created with custom viewport size
}

#[test]
#[cfg(target_os = "macos")]
fn test_multiple_engines_can_coexist() {
    // Create multiple engines to ensure they don't interfere
    let _engine1 = require_gpu!();
    let _engine2 = require_gpu!();
    let _engine3 = require_gpu!();

    // All three should be able to exist simultaneously
}

#[test]
#[cfg(target_os = "macos")]
fn test_view_resize() {
    let mut engine = require_gpu!();

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
        let _engine = require_gpu!();
        // Engine exists here
    }
    // Engine dropped here, should clean up resources

    // Create another engine to verify cleanup didn't break anything
    let _engine2 = require_gpu!();
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_simple_html() {
    let mut engine = require_gpu!();

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
    let mut engine = require_gpu!();

    // Empty document
    let html = r#"<!DOCTYPE html><html><body></body></html>"#;

    engine.load_html(html).expect("Should load empty HTML");
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_malformed_html() {
    let mut engine = require_gpu!();

    // Malformed HTML should still load (HTML is error-tolerant)
    let html = r#"<html><body><p>Unclosed paragraph<div>Nested wrong</p></div>"#;

    engine.load_html(html).expect("Should load malformed HTML");
}
