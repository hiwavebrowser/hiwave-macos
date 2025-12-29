//! HiWave Core Library
//!
//! This crate provides shared types, errors, and configuration for HiWave.

pub mod config;
pub mod error;
pub mod types;

pub use config::BrowserConfig;
pub use error::{HiWaveError, HiWaveResult};
