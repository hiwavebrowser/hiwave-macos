# Integration Test Templates

This directory contains templates and examples for writing integration tests for HiWave.

## Quick Start

### 1. Set Up Test Directory Structure

Create the following structure in `crates/hiwave-app/tests/`:

```
tests/
├── integration/
│   ├── mod.rs                     # Main integration test module
│   ├── engine_lifecycle.rs        # Engine lifecycle tests
│   ├── rendering_pipeline.rs      # Rendering tests
│   ├── navigation.rs              # Navigation tests
│   └── ... (other test categories)
└── support/
    ├── mod.rs                     # Test utilities module
    ├── test_engine.rs             # TestEngine helper
    ├── test_frame.rs              # TestFrame helper
    └── assertions.rs              # Custom assertions
```

### 2. Copy Template Files

```bash
# From repo root
cd crates/hiwave-app/tests

# Create directories
mkdir -p integration support

# Copy templates
cp ../../docs/integration_test_templates/test_engine.rs support/
cp ../../docs/integration_test_templates/example_tests.rs integration/engine_lifecycle.rs
```

### 3. Create Module Files

**`tests/support/mod.rs`:**
```rust
//! Test support utilities

mod test_engine;
mod test_frame;
mod assertions;

pub use test_engine::{TestEngine, TestWindow, TestElement};
pub use test_frame::{TestFrame, RGB, FrameDiff};
pub use assertions::*;
```

**`tests/integration/mod.rs`:**
```rust
//! Integration tests for HiWave

mod engine_lifecycle;
mod rendering_pipeline;
// Add more modules as you implement them
```

### 4. Update Cargo.toml

Add to `crates/hiwave-app/Cargo.toml`:

```toml
[dev-dependencies]
http = "1.0"
tempfile = "3.13"
```

### 5. Run Tests

```bash
# Run all integration tests
cargo test --package hiwave-app --test integration

# Run specific category
cargo test --package hiwave-app --test integration engine_lifecycle

# Run with output
cargo test --package hiwave-app --test integration -- --nocapture

# Run ignored tests (HTTP, JS)
cargo test --package hiwave-app --test integration -- --ignored
```

## Writing Your First Test

1. **Choose a category** from the Integration Test Plan
2. **Create a test file** in `tests/integration/`
3. **Copy example template** from `example_tests.rs`
4. **Implement test logic** using `TestEngine`

### Example: Simple Rendering Test

```rust
use crate::support::{TestEngine, RGB, assert_color_near};

#[test]
fn test_red_background_renders() {
    let mut engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head>
            <style>
                body { background: #ff0000; margin: 0; }
            </style>
        </head>
        <body></body>
        </html>"#;

    engine.load_html(html).expect("Should load HTML");

    let frame = engine
        .render_and_capture()
        .expect("Should render and capture");

    // Verify background is red
    let color = frame.sample_pixel(100, 100);
    assert_color_near(color, RGB::new(255, 0, 0), 5);
}
```

## Test Writing Guidelines

### DO:
- ✅ Use descriptive test names: `test_flexbox_layout_calculates_correct_widths`
- ✅ Add comments explaining complex assertions
- ✅ Use helper functions to reduce duplication
- ✅ Test one thing per test
- ✅ Clean up resources (temp files, etc.)

### DON'T:
- ❌ Use hardcoded paths (use `tempfile` or `std::env::temp_dir()`)
- ❌ Test multiple unrelated things in one test
- ❌ Rely on timing (use event polling instead)
- ❌ Ignore test failures (fix them!)

## Common Patterns

### Pattern 1: Load HTML and Verify Rendering

```rust
#[test]
fn test_something_renders() {
    let mut engine = TestEngine::new();

    engine.load_html("<html>...</html>").unwrap();

    let frame = engine.render_and_capture().unwrap();

    assert!(!frame.is_blank());
    // Add more specific assertions
}
```

### Pattern 2: Test Navigation Flow

```rust
#[test]
fn test_navigation() {
    let mut engine = TestEngine::new();

    engine.load_html("<h1>Page 1</h1>").unwrap();
    engine.wait_for_navigation().unwrap();

    engine.load_html("<h1>Page 2</h1>").unwrap();
    engine.wait_for_navigation().unwrap();

    engine.go_back().unwrap();
    // Verify we're on page 1
}
```

### Pattern 3: Test User Interaction

```rust
#[test]
fn test_click() {
    let mut engine = TestEngine::new();

    engine.load_html("... with button ...").unwrap();

    // Click at button position
    engine.send_mouse_click(100, 50).unwrap();

    // Verify effect of click
}
```

### Pattern 4: Compare Before/After Frames

```rust
#[test]
fn test_interaction_changes_rendering() {
    let mut engine = TestEngine::new();

    engine.load_html("...").unwrap();

    let before = engine.render_and_capture().unwrap();

    // Perform action
    engine.send_mouse_click(100, 50).unwrap();

    let after = engine.render_and_capture().unwrap();

    // Verify difference
    let diff = before.compare(&after, 5);
    assert!(diff.diff_pixels > 0);
}
```

### Pattern 5: Performance Testing

```rust
#[test]
fn test_renders_fast() {
    use std::time::Instant;

    let mut engine = TestEngine::new();
    engine.load_html("...").unwrap();

    let start = Instant::now();
    engine.render().unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 50);
}
```

## Debugging Failed Tests

### 1. Save Frame on Failure

```rust
#[test]
fn test_something() {
    let mut engine = TestEngine::new();
    engine.load_html("...").unwrap();

    let frame = engine.render_and_capture().unwrap();

    if frame.is_blank() {
        // Save frame for debugging
        std::fs::copy(
            "/tmp/test_frame_*.ppm",
            "/tmp/debug_frame.ppm"
        ).ok();
        panic!("Frame was blank - see /tmp/debug_frame.ppm");
    }
}
```

### 2. Use --nocapture for println! Debugging

```bash
cargo test test_something -- --nocapture
```

### 3. Use RUST_LOG for Tracing

```bash
RUST_LOG=debug cargo test test_something
```

### 4. Run Single Test

```bash
cargo test test_something --package hiwave-app
```

## CI/CD Integration

Tests run automatically in CI on:
- Every push to main
- Every pull request
- Nightly builds

### Skipping GPU Tests in CI

For tests that require GPU:

```rust
#[test]
#[cfg(feature = "gpu-tests")]
fn test_requires_gpu() {
    // This test only runs when gpu-tests feature is enabled
}
```

Run locally with:
```bash
cargo test --features gpu-tests
```

## Resources

- [Integration Test Plan](../INTEGRATION_TEST_PLAN.md) - Full implementation plan
- [RustKit Architecture](../ARCHITECTURE.md) - Understanding the engine
- [Parser Corpus Tests](../../crates/rustkit-html/tests/corpus_tests.rs) - Example of comprehensive testing

## Questions?

- Check existing tests for examples
- See the Integration Test Plan for detailed scenarios
- Ask in team chat or create an issue

---

**Next Steps:**
1. Review the [Integration Test Plan](../INTEGRATION_TEST_PLAN.md)
2. Set up your test directory structure
3. Copy and modify example tests
4. Run tests and verify they work
5. Start implementing Phase 1 tests (Engine Lifecycle)
