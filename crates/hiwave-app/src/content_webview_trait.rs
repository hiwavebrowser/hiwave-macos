//! Content WebView trait for unified interface

use wry::Rect;

/// Trait for content webview operations
pub trait ContentWebViewOps {
    fn load_url(&self, url: &str) -> Result<(), String>;
    fn load_html(&self, html: &str) -> Result<(), String>;
    fn evaluate_script(&self, script: &str) -> Result<(), String>;
    fn set_bounds(&self, rect: Rect) -> Result<(), String>;
}

// Implement for WRY WebView
impl ContentWebViewOps for wry::WebView {
    fn load_url(&self, url: &str) -> Result<(), String> {
        self.load_url(url).map_err(|e| format!("{}", e))
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        self.load_html(html).map_err(|e| format!("{}", e))
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        self.evaluate_script(script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        self.set_bounds(rect).map_err(|e| format!("{}", e))
    }
}

// Implement for RustKitView
#[cfg(target_os = "macos")]
impl ContentWebViewOps for super::webview_rustkit::RustKitView {
    fn load_url(&self, url: &str) -> Result<(), String> {
        self.load_url(url)
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        self.load_html(html)
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        self.evaluate_script(script)
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        self.set_bounds(rect)
    }
}

