//! HiWave Integration Tests
//!
//! Comprehensive end-to-end tests for the RustKit browser engine.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all integration tests
//! cargo test --package hiwave-app --test integration_tests
//!
//! # Run specific test category
//! cargo test --package hiwave-app --test integration_tests engine_lifecycle
//! cargo test --package hiwave-app --test integration_tests rendering_pipeline
//!
//! # Run with output (see println! and frame captures)
//! cargo test --package hiwave-app --test integration_tests -- --nocapture
//!
//! # Run single test
//! cargo test --package hiwave-app --test integration_tests test_simple_html_renders
//! ```
//!
//! ## Test Categories
//!
//! - **engine_lifecycle**: Engine initialization, view management, cleanup
//! - **rendering_pipeline**: Full HTML → Pixels rendering validation
//!
//! ## Architecture
//!
//! Tests use the `TestEngine` helper which wraps the RustKit engine in a
//! headless environment suitable for automated testing. Frame captures are
//! saved to `/tmp/test_frame_*.ppm` and automatically cleaned up.
//!
//! ## Adding New Tests
//!
//! 1. Add test function to appropriate module in `integration/`
//! 2. Use `TestEngine` for engine setup
//! 3. Use `assert_color_near` for pixel verification
//! 4. Clean up resources (automatic via Drop)
//!
//! See `/docs/INTEGRATION_TEST_PLAN.md` for detailed guidance.

// Test support utilities
mod support;

// Test modules
mod integration;
