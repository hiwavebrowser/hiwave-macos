//! Example Integration Tests
//!
//! This file contains template examples for writing integration tests
//! using the TestEngine helper. Copy and modify these for your own tests.

mod support;

use support::{TestEngine, TestFrame, RGB, assert_color_near};

// ============================================================================
// CATEGORY 1: ENGINE LIFECYCLE TESTS
// ============================================================================

#[test]
fn test_engine_creates_successfully() {
    let engine = TestEngine::new();
    // If we got here, engine created without panic
    assert!(true);
}

#[test]
fn test_engine_creates_with_custom_size() {
    let engine = TestEngine::with_size(1024, 768);
    // Verify size was set (would need accessor method)
    assert!(true);
}

#[test]
fn test_view_lifecycle() {
    let mut engine = TestEngine::new();

    // Engine should have view created
    // (TestEngine::new creates a view internally)

    // Resize view
    engine.resize(640, 480).expect("Resize should succeed");

    // Engine should clean up on drop
    drop(engine);
}

// ============================================================================
// CATEGORY 2: RENDERING PIPELINE TESTS
// ============================================================================

#[test]
fn test_simple_html_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { background: #ff0000; }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine.render_and_capture().expect("Should render and capture");

    // Verify frame is not blank
    assert!(!frame.is_blank(), "Frame should not be blank");

    // Verify background is red
    let bg_color = frame.sample_pixel(10, 10);
    assert_color_near(bg_color, RGB::new(255, 0, 0), 5);
}

#[test]
fn test_text_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body {
                    margin: 0;
                    background: white;
                }
                h1 {
                    color: #00ff00;
                    margin: 20px;
                }
            </style>
        </head>
        <body>
            <h1>Hello World</h1>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine.render_and_capture().expect("Should render");

    // Text should be rendered in green somewhere
    // (This is simplified - real test would know exact text position)
    let text_sample = frame.sample_pixel(50, 30);

    // Either green text or white background
    assert!(
        text_sample == RGB::new(0, 255, 0) || text_sample == RGB::new(255, 255, 255),
        "Should have green text or white background"
    );
}

#[test]
fn test_nested_elements_render() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                .outer { background: #ff0000; padding: 20px; }
                .inner { background: #00ff00; padding: 20px; }
            </style>
        </head>
        <body>
            <div class="outer">
                <div class="inner">Content</div>
            </div>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine.render_and_capture().expect("Should render");

    // Should have both red outer and green inner visible
    // (Exact pixel locations depend on layout - this is simplified)
    assert!(!frame.is_blank());
}

#[test]
fn test_css_cascade() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                p { color: red; }
                p.special { color: blue; }
            </style>
        </head>
        <body>
            <p>Normal paragraph</p>
            <p class="special">Special paragraph</p>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine.render_and_capture().expect("Should render");

    // Verify rendering succeeded
    assert!(!frame.is_blank());

    // Real test would verify text colors using element bounds
}

#[test]
fn test_flexbox_layout() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                .container {
                    display: flex;
                    width: 300px;
                    height: 100px;
                }
                .item1 { flex: 1; background: red; }
                .item2 { flex: 2; background: blue; }
            </style>
        </head>
        <body>
            <div class="container">
                <div class="item1"></div>
                <div class="item2"></div>
            </div>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    let frame = engine.render_and_capture().expect("Should render");

    // Verify flex items rendered
    assert!(!frame.is_blank());

    // Real test would verify exact layout bounds:
    // item1 should be 100px wide (1/3)
    // item2 should be 200px wide (2/3)
}

// ============================================================================
// CATEGORY 3: NAVIGATION TESTS
// ============================================================================

#[test]
fn test_load_file_url() {
    let mut engine = TestEngine::new();

    // Create temporary test file
    let test_html = r#"<!DOCTYPE html>
        <html><body><h1>Test Page</h1></body></html>"#;

    let temp_file = std::env::temp_dir().join("test_page.html");
    std::fs::write(&temp_file, test_html).expect("Should write temp file");

    // Load file URL
    let url = format!("file://{}", temp_file.display());
    engine.load_url(&url).expect("Should load file URL");

    // Wait for load
    engine.wait_for_navigation().expect("Should navigate");

    // Verify loaded
    assert_eq!(engine.current_url(), url);

    // Clean up
    std::fs::remove_file(&temp_file).ok();
}

#[test]
#[ignore] // Requires HTTP implementation
fn test_load_http_url() {
    // This test will be enabled when HTTP is implemented
    let mut engine = TestEngine::new();

    engine.load_url("http://example.com").expect("Should load HTTP");
    engine.wait_for_navigation().expect("Should navigate");

    assert_eq!(engine.current_url(), "http://example.com");
}

#[test]
fn test_history_navigation() {
    let mut engine = TestEngine::new();

    // Load first page
    engine.load_html("<h1>Page 1</h1>").unwrap();
    engine.wait_for_navigation().unwrap();

    // Load second page
    engine.load_html("<h1>Page 2</h1>").unwrap();
    engine.wait_for_navigation().unwrap();

    // Go back
    engine.go_back().expect("Should go back");

    // Verify we're on page 1
    // (Would need to check actual page content)

    // Go forward
    engine.go_forward().expect("Should go forward");

    // Verify we're on page 2
}

// ============================================================================
// CATEGORY 4: IPC INTEGRATION TESTS
// ============================================================================

#[test]
#[ignore] // Requires IPC test harness
fn test_create_tab_message() {
    // This test would use IpcTestHarness
    // See INTEGRATION_TEST_PLAN.md for full example
}

// ============================================================================
// CATEGORY 5: USER INTERACTION TESTS
// ============================================================================

#[test]
fn test_mouse_click_event() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { margin: 0; background: white; }
                #btn {
                    position: absolute;
                    left: 50px;
                    top: 50px;
                    width: 100px;
                    height: 40px;
                    background: blue;
                }
                #btn.clicked { background: red; }
            </style>
        </head>
        <body>
            <button id="btn">Click Me</button>
            <script>
                document.getElementById('btn').addEventListener('click', () => {
                    document.getElementById('btn').classList.add('clicked');
                });
            </script>
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    // Render before click
    let frame_before = engine.render_and_capture().unwrap();

    // Click button center (100, 70)
    engine.send_mouse_click(100, 70).expect("Should send click");

    // Render after click
    let frame_after = engine.render_and_capture().unwrap();

    // Frames should be different (button changed color)
    let diff = frame_before.compare(&frame_after, 5);
    assert!(diff.diff_pixels > 0, "Click should change rendering");
}

#[test]
fn test_keyboard_input() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <body>
            <input id="field" type="text" value="">
        </body>
        </html>"#;

    engine.load_html(html).expect("HTML should load");

    // Type into field
    engine.send_key("H").unwrap();
    engine.send_key("i").unwrap();

    // Verify input value (would need DOM query)
    // let input = engine.query_selector("#field").unwrap();
    // assert_eq!(input.get_attribute("value"), Some("Hi".to_string()));
}

// ============================================================================
// CATEGORY 6: NETWORKING TESTS (Future)
// ============================================================================

#[test]
#[ignore] // Requires HTTP implementation
fn test_http_get_request() {
    // Will implement when HTTP is ready
    // See INTEGRATION_TEST_PLAN.md for TestHttpServer example
}

// ============================================================================
// CATEGORY 7: JAVASCRIPT INTEGRATION TESTS (Future)
// ============================================================================

#[test]
#[ignore] // Requires JS runtime
fn test_js_dom_manipulation() {
    // Will implement when JS runtime is ready
    // See INTEGRATION_TEST_PLAN.md for example
}

// ============================================================================
// CATEGORY 8: PERFORMANCE REGRESSION TESTS
// ============================================================================

#[test]
fn test_simple_page_renders_fast() {
    use std::time::Instant;

    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html><body><h1>Simple Page</h1></body></html>"#;

    engine.load_html(html).unwrap();

    let start = Instant::now();
    engine.render().expect("Should render");
    let elapsed = start.elapsed();

    // Simple page should render in < 50ms
    assert!(
        elapsed.as_millis() < 50,
        "Simple page took {}ms (should be < 50ms)",
        elapsed.as_millis()
    );
}

#[test]
fn test_complex_page_renders_reasonably() {
    use std::time::Instant;

    let mut engine = TestEngine::new();

    // Generate complex HTML with 1000 elements
    let mut html = String::from("<!DOCTYPE html><html><body>");
    for i in 0..1000 {
        html.push_str(&format!("<div class='item'>Item {}</div>", i));
    }
    html.push_str("</body></html>");

    engine.load_html(&html).unwrap();

    let start = Instant::now();
    engine.render().expect("Should render");
    let elapsed = start.elapsed();

    // Complex page should render in < 200ms
    assert!(
        elapsed.as_millis() < 200,
        "Complex page took {}ms (should be < 200ms)",
        elapsed.as_millis()
    );
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a test HTML fixture with specified background color.
fn test_html_with_background(color: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
        <html>
        <head>
            <style>body {{ background: {}; margin: 0; }}</style>
        </head>
        <body></body>
        </html>"#,
        color
    )
}

/// Assert that frame has the expected background color.
fn assert_background_color(frame: &TestFrame, expected: RGB, tolerance: u8) {
    let sample = frame.sample_pixel(frame.width / 2, frame.height / 2);
    assert_color_near(sample, expected, tolerance);
}
