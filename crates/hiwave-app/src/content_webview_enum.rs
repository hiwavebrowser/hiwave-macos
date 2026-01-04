//! Unified content webview enum for macOS

#[cfg(target_os = "macos")]
use super::webview_rustkit::RustKitView;
use super::content_webview_trait::ContentWebViewOps;
use std::sync::Arc;
use wry::Rect;

/// Unified content webview type
#[cfg(target_os = "macos")]
pub enum ContentWebView {
    RustKit(Arc<RustKitView>),
    Wry(Arc<wry::WebView>),
}

#[cfg(not(target_os = "macos"))]
pub type ContentWebView = Arc<wry::WebView>;

#[cfg(target_os = "macos")]
impl ContentWebViewOps for ContentWebView {
    fn load_url(&self, url: &str) -> Result<(), String> {
        match self {
            ContentWebView::RustKit(v) => v.wry_load_url(url),
            ContentWebView::Wry(v) => v.load_url(url).map_err(|e| e.to_string()),
        }
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        match self {
            ContentWebView::RustKit(v) => v.wry_load_html(html),
            ContentWebView::Wry(v) => v.load_html(html).map_err(|e| e.to_string()),
        }
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        match self {
            ContentWebView::RustKit(v) => v.wry_evaluate_script(script),
            ContentWebView::Wry(v) => v.evaluate_script(script).map_err(|e| e.to_string()),
        }
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        match self {
            ContentWebView::RustKit(v) => v.wry_set_bounds(rect),
            ContentWebView::Wry(v) => v.set_bounds(rect).map_err(|e| e.to_string()),
        }
    }
}

#[cfg(target_os = "macos")]
impl ContentWebView {
    /// Process events (for RustKit)
    pub fn process_events(&self) {
        match self {
            ContentWebView::RustKit(v) => {
                v.process_events();
            }
            ContentWebView::Wry(_) => {
                // WRY handles events internally
            }
        }
    }

    /// Render (for RustKit)
    pub fn render(&self) {
        match self {
            ContentWebView::RustKit(v) => {
                v.render();
            }
            ContentWebView::Wry(_) => {
                // WRY handles rendering internally
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl ContentWebViewOps for ContentWebView {
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

