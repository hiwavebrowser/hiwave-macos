//! Content WebView abstraction
//!
//! This module provides a unified interface for content rendering,
//! using RustKit as the default engine on macOS.

#[cfg(target_os = "macos")]
use super::webview_rustkit::RustKitView;
use rustkit_viewhost::Bounds;
use std::sync::Arc;
use tao::window::Window;
use url::Url;

#[cfg(target_os = "macos")]
pub type ContentWebView = RustKitView;

#[cfg(not(target_os = "macos"))]
pub type ContentWebView = wry::WebView;

/// Builder for content webview
pub struct ContentWebViewBuilder {
    bounds: Bounds,
    initial_html: Option<String>,
    initial_url: Option<String>,
}

impl ContentWebViewBuilder {
    pub fn new() -> Self {
        Self {
            bounds: Bounds::zero(),
            initial_html: None,
            initial_url: None,
        }
    }

    pub fn with_bounds(mut self, bounds: Bounds) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_html(mut self, html: &str) -> Self {
        self.initial_html = Some(html.to_string());
        self
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.initial_url = Some(url.to_string());
        self
    }

    #[cfg(target_os = "macos")]
    pub fn build(self, window: &Window) -> Result<ContentWebView, String> {
        use super::webview_rustkit::RustKitView;
        
        let view = RustKitView::new(window, self.bounds)
            .map_err(|e| format!("Failed to create RustKit view: {}", e))?;

        // Load initial content
        if let Some(html) = self.initial_html {
            view.load_html(&html)
                .map_err(|e| format!("Failed to load HTML: {}", e))?;
        } else if let Some(url) = self.initial_url {
            if let Ok(parsed_url) = Url::parse(&url) {
                view.navigate(&parsed_url)
                    .map_err(|e| format!("Failed to navigate: {}", e))?;
            }
        }

        Ok(view)
    }

    #[cfg(not(target_os = "macos"))]
    pub fn build(self, window: &Window) -> Result<ContentWebView, String> {
        use wry::WebViewBuilder;
        
        let mut builder = WebViewBuilder::new();
        
        if let Some(html) = self.initial_html {
            builder = builder.with_html(&html);
        }
        
        builder = builder.with_bounds(wry::Rect {
            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(
                self.bounds.x as f64,
                self.bounds.y as f64,
            )),
            size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                self.bounds.width as f64,
                self.bounds.height as f64,
            )),
        });

        builder
            .build_as_child(window)
            .map_err(|e| format!("Failed to create WebView: {}", e))
    }
}

impl Default for ContentWebViewBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl ContentWebView {
    /// Load a URL
    pub fn load_url(&self, url: &str) -> Result<(), String> {
        let parsed = Url::parse(url)
            .map_err(|e| format!("Invalid URL: {}", e))?;
        self.navigate(&parsed)
            .map_err(|e| format!("Navigation failed: {}", e))?;
        Ok(())
    }

    /// Process events (call in event loop)
    pub fn process_events(&self) {
        self.process_events();
    }

    /// Render the view (call in event loop)
    pub fn render(&self) {
        self.render();
    }

    /// Set bounds
    pub fn set_bounds(&self, bounds: Bounds) -> Result<(), String> {
        self.set_bounds(bounds)
            .map_err(|e| format!("Failed to set bounds: {}", e))?;
        Ok(())
    }
}

