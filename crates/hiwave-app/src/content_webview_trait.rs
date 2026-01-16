//! Content WebView trait for unified interface

use std::sync::Arc;
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
        wry::WebView::load_url(self, url).map_err(|e| format!("{}", e))
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        wry::WebView::load_html(self, html).map_err(|e| format!("{}", e))
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        wry::WebView::set_bounds(self, rect).map_err(|e| format!("{}", e))
    }
}

// Implement for Arc<wry::WebView>
impl ContentWebViewOps for Arc<wry::WebView> {
    fn load_url(&self, url: &str) -> Result<(), String> {
        wry::WebView::load_url(self, url).map_err(|e| format!("{}", e))
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        wry::WebView::load_html(self, html).map_err(|e| format!("{}", e))
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, script).map_err(|e| format!("{}", e))
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        wry::WebView::set_bounds(self, rect).map_err(|e| format!("{}", e))
    }
}

// Implement for RustKitView
#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
impl ContentWebViewOps for super::webview_rustkit::RustKitView {
    fn load_url(&self, url: &str) -> Result<(), String> {
        self.wry_load_url(url)
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        self.wry_load_html(html)
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        self.wry_evaluate_script(script)
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        self.wry_set_bounds(rect)
    }
}

// Implement for Arc<RustKitView>
#[cfg(all(target_os = "macos", feature = "rustkit", not(feature = "webview-fallback")))]
impl ContentWebViewOps for Arc<super::webview_rustkit::RustKitView> {
    fn load_url(&self, url: &str) -> Result<(), String> {
        self.wry_load_url(url)
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        self.wry_load_html(html)
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        self.wry_evaluate_script(script)
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        self.wry_set_bounds(rect)
    }
}

// Implement for Arc<ContentWebView>
#[cfg(target_os = "macos")]
impl ContentWebViewOps for Arc<super::content_webview_enum::ContentWebView> {
    fn load_url(&self, url: &str) -> Result<(), String> {
        (**self).load_url(url)
    }

    fn load_html(&self, html: &str) -> Result<(), String> {
        (**self).load_html(html)
    }

    fn evaluate_script(&self, script: &str) -> Result<(), String> {
        (**self).evaluate_script(script)
    }

    fn set_bounds(&self, rect: Rect) -> Result<(), String> {
        (**self).set_bounds(rect)
    }
}

