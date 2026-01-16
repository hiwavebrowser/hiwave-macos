//! Integration tests for HiWave browser
//!
//! These tests verify end-to-end behavior of the RustKit engine and HiWave application.
//!
//! ## Test Categories
//!
//! - `engine_lifecycle`: Engine creation, view management, resource cleanup
//! - `rendering_pipeline`: Full HTML → GPU rendering validation
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all integration tests
//! cargo test --package hiwave-app --test integration
//!
//! # Run specific category
//! cargo test --package hiwave-app --test integration engine_lifecycle
//!
//! # Run with output
//! cargo test --package hiwave-app --test integration -- --nocapture
//! ```

mod engine_lifecycle;
mod rendering_pipeline;
