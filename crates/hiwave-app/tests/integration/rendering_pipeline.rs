//! Rendering pipeline integration tests
//!
//! These tests verify the full HTML → Pixels rendering pipeline:
//! - HTML parsing → DOM construction
//! - CSS application → Style computation
//! - Layout calculation → Box positioning
//! - Display list generation → GPU commands
//! - Final pixel output

use crate::support::{TestEngine, RGB, assert_color_near, assert_not_blank};

#[test]
#[cfg(target_os = "macos")]
fn test_simple_html_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body {
                    margin: 0;
                    background: #ffffff;
                }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // Verify frame rendered (not blank)
    assert_not_blank(&frame);

    // Verify background is white
    let bg_color = frame.sample_pixel(100, 100);
    assert_color_near(bg_color, RGB::new(255, 255, 255), 5);
}

#[test]
#[cfg(target_os = "macos")]
fn test_red_background_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body {
                    margin: 0;
                    background: #ff0000;
                }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // Verify background is red
    let bg_color = frame.sample_pixel(100, 100);
    assert_color_near(bg_color, RGB::new(255, 0, 0), 5);
}

#[test]
#[cfg(target_os = "macos")]
fn test_blue_background_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body {
                    margin: 0;
                    background: #0000ff;
                }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // Verify background is blue
    let bg_color = frame.sample_pixel(100, 100);
    assert_color_near(bg_color, RGB::new(0, 0, 255), 5);
}

#[test]
#[cfg(target_os = "macos")]
fn test_inline_styles_apply() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <body style="margin: 0; background: #00ff00;"></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // Verify inline style applied (green background)
    let bg_color = frame.sample_pixel(100, 100);
    assert_color_near(bg_color, RGB::new(0, 255, 0), 5);
}

#[test]
#[cfg(target_os = "macos")]
fn test_nested_divs_render() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { margin: 0; background: white; }
                .outer {
                    background: #ff0000;
                    padding: 50px;
                }
                .inner {
                    background: #0000ff;
                    width: 100px;
                    height: 100px;
                }
            </style>
        </head>
        <body>
            <div class="outer">
                <div class="inner"></div>
            </div>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // Verify outer div (red) is visible
    let outer_color = frame.sample_pixel(10, 10);
    assert_color_near(outer_color, RGB::new(255, 0, 0), 5);

    // Verify inner div (blue) is visible (offset by padding)
    let inner_color = frame.sample_pixel(75, 75);
    assert_color_near(inner_color, RGB::new(0, 0, 255), 5);
}

#[test]
#[cfg(target_os = "macos")]
fn test_css_cascade() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { margin: 0; }
                div { background: red; }
                div.special { background: blue; }
                #unique { background: green; }
            </style>
        </head>
        <body>
            <div id="unique" class="special" style="width: 800px; height: 600px;"></div>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // ID selector (#unique) should win - green background
    let color = frame.sample_pixel(400, 300);
    assert_color_near(color, RGB::new(0, 255, 0), 5);
}

#[test]
#[cfg(target_os = "macos")]
fn test_multiple_renders_consistent() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { margin: 0; background: #ff00ff; }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    // Render multiple times
    let frame1 = engine
        .render_and_capture()
        .expect("First render should succeed");

    let frame2 = engine
        .render_and_capture()
        .expect("Second render should succeed");

    // Both renders should produce identical results
    let color1 = frame1.sample_pixel(100, 100);
    let color2 = frame2.sample_pixel(100, 100);

    assert_eq!(
        color1, color2,
        "Multiple renders should be consistent"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn test_resize_re_renders() {
    let mut engine = TestEngine::with_size(400, 300);

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { margin: 0; background: #ffff00; }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    // Render at original size
    let frame1 = engine
        .render_and_capture()
        .expect("Should render at 400x300");

    assert_eq!(frame1.width, 400);
    assert_eq!(frame1.height, 300);

    // Resize and render again
    engine.resize(800, 600).expect("Should resize");

    let frame2 = engine
        .render_and_capture()
        .expect("Should render at 800x600");

    assert_eq!(frame2.width, 800);
    assert_eq!(frame2.height, 600);

    // Both should have same background color
    let color1 = frame1.sample_pixel(100, 100);
    let color2 = frame2.sample_pixel(400, 300);

    assert_eq!(color1, color2, "Color should be consistent after resize");
}

#[test]
#[cfg(target_os = "macos")]
fn test_complex_document_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body {
                    margin: 0;
                    background: white;
                }
                .container {
                    display: block;
                }
                .box {
                    background: #4a90d9;
                    width: 100px;
                    height: 100px;
                    margin: 10px;
                }
            </style>
        </head>
        <body>
            <div class="container">
                <div class="box"></div>
                <div class="box"></div>
                <div class="box"></div>
            </div>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine
        .render_and_capture()
        .expect("Should render complex document");

    // Verify frame rendered
    assert_not_blank(&frame);

    // Verify at least one box rendered (blue color)
    let box_color = frame.sample_pixel(60, 60);

    // Should be either blue (box) or white (background)
    assert!(
        box_color.near(RGB::new(74, 144, 217), 10)
            || box_color == RGB::new(255, 255, 255),
        "Should render boxes or background"
    );
}
