//! Chrome WebView enum for unified interface (RustKit + WRY support)
//!
//! Similar to ContentWebView, this provides a unified interface for the chrome
//! UI that can use either RustKit or WRY (WebKit) as the rendering backend.

#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
use super::webview_rustkit_chrome::RustKitChromeView;
use std::sync::Arc;
use wry::Rect;

/// Trait for chrome webview operations
pub trait ChromeWebViewOps {
    fn evaluate_script(&self, script: &str) -> Result<(), String>;
    fn set_bounds(&self, rect: Rect) -> Result<(), String>;
}

/// Unified chrome webview type (RustKit + WRY support)
#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
#[allow(dead_code)]
pub enum ChromeWebView {
    RustKit(RustKitChromeView),
    Wry(wry::WebView),
}

/// WebKit fallback mode - just wrap WRY WebView
#[cfg(all(target_os = "macos", feature = "webview-fallback"))]
pub type ChromeWebView = wry::WebView;

#[cfg(not(target_os = "macos"))]
pub type ChromeWebView = wry::WebView;

#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
impl ChromeWebViewOps for ChromeWebView {
    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        match self {
            ChromeWebView::RustKit(_v) => {
                // TODO: Implement JS bridge for RustKit
                // For now, silently succeed
                let _ = script;
                Ok(())
            }
            ChromeWebView::Wry(v) => v.evaluate_script(script).map_err(|e| e.to_string()),
        }
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        match self {
            ChromeWebView::RustKit(v) => v.wry_set_bounds(rect),
            ChromeWebView::Wry(v) => v.set_bounds(rect).map_err(|e| e.to_string()),
        }
    }
}

#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
impl ChromeWebView {
    /// Process events (for RustKit)
    #[allow(dead_code)]
    pub fn process_events(&self) {
        match self {
            ChromeWebView::RustKit(v) => {
                v.process_events();
            }
            ChromeWebView::Wry(_) => {
                // WRY handles events internally
            }
        }
    }

    /// Render (for RustKit)
    #[allow(dead_code)]
    pub fn render(&self) {
        match self {
            ChromeWebView::RustKit(v) => {
                v.render();
            }
            ChromeWebView::Wry(_) => {
                // WRY handles rendering internally
            }
        }
    }
}

// WebKit fallback mode implementation
#[cfg(all(target_os = "macos", feature = "webview-fallback"))]
impl ChromeWebViewOps for ChromeWebView {
    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        wry::WebView::set_bounds(self, rect).map_err(|e| format!("{}", e))
    }
}

// Non-macOS implementation
#[cfg(not(target_os = "macos"))]
impl ChromeWebViewOps for ChromeWebView {
    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        wry::WebView::set_bounds(self, rect).map_err(|e| format!("{}", e))
    }
}

// Implement for Arc<ChromeWebView>
#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
impl ChromeWebViewOps for Arc<ChromeWebView> {
    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        (**self).evaluate_script(script)
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        (**self).set_bounds(rect)
    }
}

#[cfg(all(target_os = "macos", feature = "webview-fallback"))]
impl ChromeWebViewOps for Arc<ChromeWebView> {
    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        wry::WebView::set_bounds(self, rect).map_err(|e| format!("{}", e))
    }
}

#[cfg(not(target_os = "macos"))]
impl ChromeWebViewOps for Arc<ChromeWebView> {
    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        wry::WebView::set_bounds(self, rect).map_err(|e| format!("{}", e))
    }
}

