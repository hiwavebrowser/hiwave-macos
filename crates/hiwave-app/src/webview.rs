//! WebView abstraction for HiWave
//!
//! This module provides an abstraction layer over WRY's WebView,
//! allowing for future replacement with a custom rendering engine.

use url::Url;
use hiwave_core::HiWaveResult;

/// Trait for web content rendering abstraction
///
/// This trait defines the interface for rendering web content,
/// allowing the browser to swap between WebView (WRY) and
/// a future custom rendering engine.
#[allow(dead_code)] // Reserved for Phase 2 custom rendering
pub trait IWebContent {
    /// Navigate to a URL
    fn navigate(&mut self, url: &Url) -> HiWaveResult<()>;

    /// Execute JavaScript in the page context
    fn execute_script(&self, script: &str) -> HiWaveResult<String>;

    /// Get the current URL
    fn get_url(&self) -> Option<Url>;

    /// Check if navigation back is possible
    fn can_go_back(&self) -> bool;

    /// Check if navigation forward is possible
    fn can_go_forward(&self) -> bool;

    /// Navigate back in history
    fn go_back(&mut self) -> HiWaveResult<()>;

    /// Navigate forward in history
    fn go_forward(&mut self) -> HiWaveResult<()>;

    /// Reload the current page
    fn reload(&mut self) -> HiWaveResult<()>;
}

// Note: The actual WRY WebView implementation will be added in Phase 2
// when we need more sophisticated control over the WebView.
// For now, we use WRY's WebView directly in main.rs.
