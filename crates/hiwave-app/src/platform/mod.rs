//! Platform abstraction layer for cross-platform reliability
//!
//! This module provides a unified interface for platform-specific operations,
//! reducing code duplication and ensuring consistent behavior across macOS,
//! Windows, and Linux.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

// Re-export menu IDs for each platform
#[cfg(target_os = "macos")]
pub use macos::menu_ids;

use muda::Menu;
use std::path::{Path, PathBuf};
use tao::window::Window;

/// Result type for platform operations
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Platform-specific error types
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some variants reserved for future use
pub enum PlatformError {
    /// Menu initialization failed
    MenuInitFailed(String),
    /// Failed to open external URL
    OpenExternalFailed(String),
    /// Failed to open file
    OpenFileFailed(String),
    /// File not found
    FileNotFound(PathBuf),
    /// Failed to show file in folder
    ShowInFolderFailed(String),
    /// Command execution failed
    CommandFailed(String),
    /// Platform-specific error
    Other(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformError::MenuInitFailed(msg) => write!(f, "Menu initialization failed: {}", msg),
            PlatformError::OpenExternalFailed(msg) => write!(f, "Failed to open external: {}", msg),
            PlatformError::OpenFileFailed(msg) => write!(f, "Failed to open file: {}", msg),
            PlatformError::FileNotFound(path) => write!(f, "File not found: {:?}", path),
            PlatformError::ShowInFolderFailed(msg) => write!(f, "Failed to show in folder: {}", msg),
            PlatformError::CommandFailed(msg) => write!(f, "Command failed: {}", msg),
            PlatformError::Other(msg) => write!(f, "Platform error: {}", msg),
        }
    }
}

impl std::error::Error for PlatformError {}

/// WebView engine type for the current platform
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // WebView2 used on Windows
pub enum WebViewEngine {
    /// WebKit (macOS native, Linux GTK)
    WebKit,
    /// WebView2 (Windows, Chromium-based)
    WebView2,
}

impl WebViewEngine {
    /// Returns true if this engine tends to be aggressive with new window requests
    #[allow(dead_code)] // Used for popup filtering in later phases
    pub fn is_aggressive_popup_opener(&self) -> bool {
        matches!(self, WebViewEngine::WebKit)
    }
}

/// Platform-specific capabilities
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used for feature detection in later phases
pub struct PlatformCapabilities {
    /// Whether a native menu is required for clipboard operations (macOS)
    pub native_menu_required_for_clipboard: bool,
    /// Whether the platform supports selecting a file in its parent folder
    pub supports_file_selection_in_folder: bool,
    /// The WebView engine used on this platform
    pub webview_engine: WebViewEngine,
    /// Default download directory
    pub default_download_dir: PathBuf,
    /// Platform name for logging/debugging
    pub platform_name: &'static str,
}

/// Trait for platform-specific operations
///
/// Implementations provide unified interfaces for operations that differ
/// across macOS, Windows, and Linux.
pub trait PlatformManager: Send + Sync {
    /// Initialize the native menu for the window
    ///
    /// On macOS, this is required for clipboard shortcuts (Cmd+C/V) to work.
    /// On Windows/Linux, this adds the menu bar to the window.
    fn initialize_menu(&self, window: &Window, menu: &Menu) -> PlatformResult<()>;

    /// Open a URL in the system's default browser
    fn open_external(&self, url: &str) -> PlatformResult<()>;

    /// Open a file with the system's default application
    fn open_file(&self, path: &Path) -> PlatformResult<()>;

    /// Show a file in the system's file manager (Finder/Explorer/Nautilus)
    ///
    /// On macOS and Windows, this selects the file in the folder.
    /// On Linux, this opens the parent directory (file selection not supported).
    fn show_in_folder(&self, path: &Path) -> PlatformResult<()>;

    /// Get the platform's capabilities
    fn capabilities(&self) -> &PlatformCapabilities;

    /// Determine if a new window request should be opened as a tab
    ///
    /// Platform-specific heuristics for popup filtering. macOS WebKit is
    /// particularly aggressive and needs extensive filtering.
    #[allow(dead_code)] // Used for popup handling in later phases
    fn should_open_as_tab(&self, url: &str, initiating_url: Option<&str>) -> bool;

    /// Get the platform name for logging
    fn platform_name(&self) -> &'static str {
        self.capabilities().platform_name
    }
}

/// Get the platform manager for the current operating system
pub fn get_platform_manager() -> Box<dyn PlatformManager> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOSPlatform::new())
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsPlatform::new())
    }

    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxPlatform::new())
    }
}

/// Common URL filtering logic shared across platforms
///
/// Returns true if the URL should definitely be blocked as a popup.
/// This is the comprehensive blocklist applied on all platforms.
#[allow(dead_code)] // Used by platform implementations
pub fn is_definitely_popup(url: &str) -> bool {
    let url_lower = url.to_lowercase();

    // Block about:blank and about:srcdoc - typically JS-initiated popup/iframe placeholders
    if url_lower == "about:blank" || url_lower == "about:srcdoc" {
        return true;
    }

    // Block data: URLs - inline content, not real navigation
    if url_lower.starts_with("data:") {
        return true;
    }

    // Block javascript: URLs
    if url_lower.starts_with("javascript:") {
        return true;
    }

    // Comprehensive blocklist of ad/tracking/iframe patterns
    // NOTE: All patterns MUST be lowercase since we compare against url.to_lowercase()
    let blocked_patterns = [
        // Google ad infrastructure
        "googlesyndication.com",
        "doubleclick.net",
        "googleadservices.com",
        "syndicatedsearch.goog",
        "safeframe.googlesyndication.com",
        "adtrafficquality.google",
        "/sodar/",
        "/recaptcha/api2/aframe",
        // Facebook tracking
        "facebook.com/tr",
        "connect.facebook.net",
        // Snapchat tracking
        "tr.snapchat.com",
        // Criteo ad tracking
        "criteo.com/syncframe",
        "criteo.com/sync",
        // Pinterest tracking
        "ct.pinterest.com",
        // eBay internal ads
        "epnt.ebay.com/placement",
        // BlueCava fingerprinting/sync
        "sync.graph.bluecava.com",
        "bluecava.com",
        // Attribution tracking
        "pixall.esm1.net",
        "/attribution/iframe",
        // Ad delivery networks
        "cdn.flashtalking.com",
        "flashtalking.com",
        // Common tracking/analytics
        "devicebind.",
        "/signin/sub/tt.html",
        "/cm/i?",
        // Iframe containers and service workers
        "/iframe.html",
        "/container.html",
        "sw_iframe.html",
        "/service_worker/",
        // Analytics beacons
        "/beacon",
        "/pixel",
        "/track",
        // Ad hub/feed domains
        "pghub.io",
        "tapad.com",
        "feed.pghub.io",
        // Sovrn/Lijit ad sync
        "lijit.com",
        "pxdrop.",
        "dtscout.com",
        // Google internal frames
        "/_/bscframe",
        "/rotatecookiespage",
        "ogs.google.com/widget",
        // Ad tech bot detection
        "robots.txt?upapi",
        "?upapi=true",
        // Payment/checkout iframes (internal frames, not user navigation)
        "m.stripe.network/inner.html",
        "js.stripe.com/v3/controller",
        // Google widgets/popups
        "/widget/hovercard",
        "/widget/app",
        "/widget/account",
        "/static/proxy.html",
        "clients6.google.com",
        // Generic SDK/embed patterns (blocks all embedded widget iframes)
        "/sdk/",
        "?zoid=",           // zoid cross-domain iframe framework
        "&zoid=",
        "placement-api.",   // payment widget APIs (Afterpay, Klarna, etc.)
        "-ecdn.com",        // e-commerce CDN embeds (Salsify, etc.)
        // Shopify/e-commerce tracking sandboxes
        "/web-pixels",      // Shopify web pixel tracking iframes
        "/sandbox/",        // generic sandbox iframes (analytics, tracking)
        // Chat widget/storage sync iframes
        "gorgias.chat",     // Gorgias chat widget
        "-storage-sync",    // storage sync iframes (various widgets)
        "chat-storage",     // chat storage sync patterns
        // Embedded media (iframes)
        "/embed/",          // YouTube, Vimeo, etc. embedded players
        "/embed?",          // embed with query params
        // Generic patterns
        "popup",
        "clicktrack",
        "adsystem",
        "adserver",
    ];

    for pattern in blocked_patterns {
        if url_lower.contains(pattern) {
            return true;
        }
    }

    // Check for tracker redirects
    if is_tracker_redirect(&url_lower) {
        return true;
    }

    false
}

/// Heuristic detection of tracker/ad redirect URLs
///
/// These URLs typically:
/// 1. Have paths like /tag, /pixel, /sync, /track
/// 2. Include referrer/page info in query params
/// 3. Contain ad-tech identifiers (bp_id, gdpr, initiator=js)
#[allow(dead_code)] // Used by is_definitely_popup
pub fn is_tracker_redirect(url: &str) -> bool {
    // Must have a query string to be a tracker redirect
    if !url.contains('?') {
        return false;
    }

    // Suspicious path patterns (tracking endpoints)
    let tracker_paths = [
        "/tag?", "/tag/",
        "/pixel?", "/pixel/",
        "/sync?", "/sync/",
        "/usersync",
        "/collect?",
        "/event?",
        "/impression?",
        "/click?",
        "/redirect?",
        "/redir?",
        "/bounce?",
        "/jump?",
        "/out?",
        "/go?",
    ];

    let has_tracker_path = tracker_paths.iter().any(|p| url.contains(p));

    // Ad-tech query parameters (strong signals)
    let adtech_params = [
        "bp_id=",
        "initiator=js",
        "gdpr=",
        "liveramp",
        "page_url=",
        "referrer_url=",
        "mediavine",
        "adserver",
        "adunit",
        "prebid",
        "rubicon",
        "pubmatic",
        "appnexus",
        "openx",
        "casale",
        "tradedesk",
        "amazon-adsystem",
    ];

    let adtech_count = adtech_params.iter().filter(|p| url.contains(*p)).count();

    // Strong heuristic: tracker path + multiple ad-tech params
    if has_tracker_path && adtech_count >= 2 {
        return true;
    }

    // Very strong signal: page_url or referrer_url with initiator=js
    if (url.contains("page_url=") || url.contains("referrer_url="))
        && url.contains("initiator=js")
    {
        return true;
    }

    false
}

/// Check if a URL is a legitimate navigation target
#[allow(dead_code)] // Used by platform implementations
pub fn is_legitimate_navigation(url: &str) -> bool {
    // Must be http or https
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }

    // Common legitimate domains
    let legitimate_patterns = [
        "github.com",
        "google.com",
        "microsoft.com",
        "apple.com",
        "amazon.com",
        "wikipedia.org",
        "stackoverflow.com",
    ];

    let url_lower = url.to_lowercase();
    for pattern in legitimate_patterns {
        if url_lower.contains(pattern) {
            return true;
        }
    }

    // Default: allow if it looks like a real URL
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_definitely_popup_blocks_special_urls() {
        // Block about: URLs
        assert!(is_definitely_popup("about:blank"));
        assert!(is_definitely_popup("about:srcdoc"));

        // Block javascript: URLs
        assert!(is_definitely_popup("javascript:void(0)"));
        assert!(is_definitely_popup("javascript:alert('test')"));

        // Block data: URLs
        assert!(is_definitely_popup("data:text/html,<h1>test</h1>"));
        assert!(is_definitely_popup("data:application/json,{}"));
    }

    #[test]
    fn test_is_definitely_popup_blocks_ad_networks() {
        // Google ad infrastructure
        assert!(is_definitely_popup("https://googlesyndication.com/safeframe"));
        assert!(is_definitely_popup("https://doubleclick.net/track"));
        assert!(is_definitely_popup("https://googleadservices.com/pagead"));

        // Facebook tracking
        assert!(is_definitely_popup("https://facebook.com/tr?id=123"));
        assert!(is_definitely_popup("https://connect.facebook.net/en_US/sdk.js"));

        // Other ad networks
        assert!(is_definitely_popup("https://criteo.com/sync"));
        assert!(is_definitely_popup("https://tapad.com/pixel"));
    }

    #[test]
    fn test_is_definitely_popup_blocks_tracking_patterns() {
        // Tracking endpoints
        assert!(is_definitely_popup("https://example.com/beacon?id=123"));
        assert!(is_definitely_popup("https://example.com/pixel.gif"));
        assert!(is_definitely_popup("https://example.com/track?user=456"));

        // Iframe patterns
        assert!(is_definitely_popup("https://example.com/iframe.html"));
        assert!(is_definitely_popup("https://example.com/container.html"));
    }

    #[test]
    fn test_is_definitely_popup_allows_legitimate() {
        // Normal URLs should not be blocked
        assert!(!is_definitely_popup("https://example.com"));
        assert!(!is_definitely_popup("https://github.com/pureflow"));
        assert!(!is_definitely_popup("https://google.com/search?q=test"));
        assert!(!is_definitely_popup("https://www.amazon.com/products"));
    }

    #[test]
    fn test_is_tracker_redirect() {
        // Should detect tracker redirects
        assert!(is_tracker_redirect("https://example.com/tag?bp_id=123&initiator=js&gdpr=1"));
        assert!(is_tracker_redirect("https://example.com/pixel?page_url=test&initiator=js"));

        // Should not flag normal URLs
        assert!(!is_tracker_redirect("https://example.com/page"));
        assert!(!is_tracker_redirect("https://example.com/search?q=test"));
    }

    #[test]
    fn test_is_legitimate_navigation() {
        // Legitimate
        assert!(is_legitimate_navigation("https://github.com/pureflow"));
        assert!(is_legitimate_navigation("https://google.com/search?q=test"));
        assert!(is_legitimate_navigation("http://localhost:3000"));

        // Not legitimate (wrong protocol)
        assert!(!is_legitimate_navigation("javascript:void(0)"));
        assert!(!is_legitimate_navigation("data:text/html,test"));
        assert!(!is_legitimate_navigation("ftp://example.com"));
    }

    #[test]
    fn test_webview_engine_popup_aggressiveness() {
        assert!(WebViewEngine::WebKit.is_aggressive_popup_opener());
        assert!(!WebViewEngine::WebView2.is_aggressive_popup_opener());
    }
}
