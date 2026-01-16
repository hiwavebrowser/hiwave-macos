//! Test support utilities for HiWave integration tests
//!
//! This module provides helpers for writing integration tests:
//! - TestEngine: Headless engine wrapper
//! - TestFrame: Frame capture and pixel verification
//! - Assertions: Custom test assertions

mod test_engine;
mod test_frame;
mod assertions;

pub use test_engine::TestEngine;
pub use test_frame::{TestFrame, RGB};
pub use assertions::*;
