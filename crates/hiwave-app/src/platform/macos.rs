//! macOS platform implementation
//!
//! WebKit on macOS is aggressive with new window requests, so we use
//! comprehensive filtering. The native menu is required for clipboard
//! shortcuts (Cmd+C/V) to work properly.

use super::{
    is_definitely_popup, is_legitimate_navigation, PlatformCapabilities, PlatformError,
    PlatformManager, PlatformResult, WebViewEngine,
};
use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    Menu, MenuItem, PredefinedMenuItem, Submenu,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use tao::window::Window;
use tracing::debug;

/// Menu item IDs for handling keyboard shortcuts
pub mod menu_ids {
    pub const NEW_TAB: &str = "new_tab";
    pub const CLOSE_TAB: &str = "close_tab";
    pub const RELOAD: &str = "reload";
    pub const FIND: &str = "find";
    pub const COMMAND_PALETTE: &str = "command_palette";
    pub const HISTORY: &str = "history";
    pub const TOGGLE_SIDEBAR: &str = "toggle_sidebar";
    pub const GO_BACK: &str = "go_back";
    pub const GO_FORWARD: &str = "go_forward";
    pub const FOCUS_URL: &str = "focus_url";
    pub const TOGGLE_INSPECTOR: &str = "toggle_inspector";
}

/// macOS platform manager
pub struct MacOSPlatform {
    capabilities: PlatformCapabilities,
}

impl MacOSPlatform {
    pub fn new() -> Self {
        let default_download_dir = dirs::download_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Downloads")
        });

        Self {
            capabilities: PlatformCapabilities {
                native_menu_required_for_clipboard: true,
                supports_file_selection_in_folder: true,
                webview_engine: WebViewEngine::WebKit,
                default_download_dir,
                platform_name: "macOS",
            },
        }
    }

    /// Create the standard macOS application menu
    fn create_app_menu(&self, menu: &Menu) -> PlatformResult<()> {
        let app_menu = Submenu::new("HiWave", true);

        // Standard macOS app menu items
        app_menu
            .append(&PredefinedMenuItem::about(Some("About HiWave"), None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add About: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::services(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Services: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::hide(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Hide: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::hide_others(None))
            .map_err(|e| {
                PlatformError::MenuInitFailed(format!("Failed to add Hide Others: {}", e))
            })?;

        app_menu
            .append(&PredefinedMenuItem::show_all(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Show All: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        app_menu
            .append(&PredefinedMenuItem::quit(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Quit: {}", e)))?;

        menu.append(&app_menu)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to append app menu: {}", e)))?;

        Ok(())
    }

    /// Create the Edit menu with clipboard operations
    fn create_edit_menu(&self, menu: &Menu) -> PlatformResult<()> {
        let edit_menu = Submenu::new("Edit", true);

        edit_menu
            .append(&PredefinedMenuItem::undo(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Undo: {}", e)))?;

        edit_menu
            .append(&PredefinedMenuItem::redo(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Redo: {}", e)))?;

        edit_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        edit_menu
            .append(&PredefinedMenuItem::cut(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Cut: {}", e)))?;

        edit_menu
            .append(&PredefinedMenuItem::copy(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Copy: {}", e)))?;

        edit_menu
            .append(&PredefinedMenuItem::paste(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Paste: {}", e)))?;

        edit_menu
            .append(&PredefinedMenuItem::select_all(None))
            .map_err(|e| {
                PlatformError::MenuInitFailed(format!("Failed to add Select All: {}", e))
            })?;

        edit_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        // Find in page (Cmd+F)
        let find_item = MenuItem::with_id(
            menu_ids::FIND,
            "Find...",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyF)),
        );
        edit_menu
            .append(&find_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Find: {}", e)))?;

        menu.append(&edit_menu)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to append edit menu: {}", e)))?;

        Ok(())
    }

    /// Create the File menu with tab operations
    fn create_file_menu(&self, menu: &Menu) -> PlatformResult<()> {
        let file_menu = Submenu::new("File", true);

        // New Tab (Cmd+T)
        let new_tab_item = MenuItem::with_id(
            menu_ids::NEW_TAB,
            "New Tab",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyT)),
        );
        file_menu
            .append(&new_tab_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add New Tab: {}", e)))?;

        // Close Tab (Cmd+W)
        let close_tab_item = MenuItem::with_id(
            menu_ids::CLOSE_TAB,
            "Close Tab",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyW)),
        );
        file_menu
            .append(&close_tab_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Close Tab: {}", e)))?;

        file_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        // Close Window (Cmd+Shift+W) - predefined
        file_menu
            .append(&PredefinedMenuItem::close_window(None))
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Close Window: {}", e)))?;

        menu.append(&file_menu)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to append file menu: {}", e)))?;

        Ok(())
    }

    /// Create the View menu with navigation and display options
    fn create_view_menu(&self, menu: &Menu) -> PlatformResult<()> {
        let view_menu = Submenu::new("View", true);

        // Reload (Cmd+R)
        let reload_item = MenuItem::with_id(
            menu_ids::RELOAD,
            "Reload Page",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyR)),
        );
        view_menu
            .append(&reload_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Reload: {}", e)))?;

        view_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        // Command Palette (Cmd+K)
        let cmd_palette_item = MenuItem::with_id(
            menu_ids::COMMAND_PALETTE,
            "Command Palette",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyK)),
        );
        view_menu
            .append(&cmd_palette_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Command Palette: {}", e)))?;

        // History (Cmd+Y - using Y instead of H to avoid conflict with Hide)
        let history_item = MenuItem::with_id(
            menu_ids::HISTORY,
            "History",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyY)),
        );
        view_menu
            .append(&history_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add History: {}", e)))?;

        view_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        // Toggle Sidebar (Cmd+B)
        let sidebar_item = MenuItem::with_id(
            menu_ids::TOGGLE_SIDEBAR,
            "Toggle Sidebar",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyB)),
        );
        view_menu
            .append(&sidebar_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Toggle Sidebar: {}", e)))?;

        // Focus URL Bar (Cmd+L)
        let focus_url_item = MenuItem::with_id(
            menu_ids::FOCUS_URL,
            "Focus Address Bar",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::KeyL)),
        );
        view_menu
            .append(&focus_url_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Focus URL: {}", e)))?;

        view_menu
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add separator: {}", e)))?;

        // Developer Tools / Inspector (Cmd+Opt+I)
        let inspector_item = MenuItem::with_id(
            menu_ids::TOGGLE_INSPECTOR,
            "Developer Tools",
            true,
            Some(Accelerator::new(Some(Modifiers::META | Modifiers::ALT), Code::KeyI)),
        );
        view_menu
            .append(&inspector_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Developer Tools: {}", e)))?;

        menu.append(&view_menu)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to append view menu: {}", e)))?;

        Ok(())
    }

    /// Create the History menu with navigation
    fn create_history_menu(&self, menu: &Menu) -> PlatformResult<()> {
        let history_menu = Submenu::new("History", true);

        // Back (Cmd+[)
        let back_item = MenuItem::with_id(
            menu_ids::GO_BACK,
            "Back",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::BracketLeft)),
        );
        history_menu
            .append(&back_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Back: {}", e)))?;

        // Forward (Cmd+])
        let forward_item = MenuItem::with_id(
            menu_ids::GO_FORWARD,
            "Forward",
            true,
            Some(Accelerator::new(Some(Modifiers::META), Code::BracketRight)),
        );
        history_menu
            .append(&forward_item)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to add Forward: {}", e)))?;

        menu.append(&history_menu)
            .map_err(|e| PlatformError::MenuInitFailed(format!("Failed to append history menu: {}", e)))?;

        Ok(())
    }
}

impl Default for MacOSPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformManager for MacOSPlatform {
    fn initialize_menu(&self, _window: &Window, menu: &Menu) -> PlatformResult<()> {
        // Create standard macOS menus in proper order
        self.create_app_menu(menu)?;
        self.create_file_menu(menu)?;
        self.create_edit_menu(menu)?;
        self.create_view_menu(menu)?;
        self.create_history_menu(menu)?;

        // Initialize the menu for the NSApp
        menu.init_for_nsapp();

        debug!("macOS menu initialized with keyboard shortcuts");
        Ok(())
    }

    fn open_external(&self, url: &str) -> PlatformResult<()> {
        debug!("Opening external URL: {}", url);

        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| PlatformError::OpenExternalFailed(format!("{}: {}", url, e)))?;

        Ok(())
    }

    fn open_file(&self, path: &Path) -> PlatformResult<()> {
        if !path.exists() {
            return Err(PlatformError::FileNotFound(path.to_path_buf()));
        }

        debug!("Opening file: {:?}", path);

        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| PlatformError::OpenFileFailed(format!("{:?}: {}", path, e)))?;

        Ok(())
    }

    fn show_in_folder(&self, path: &Path) -> PlatformResult<()> {
        if !path.exists() {
            return Err(PlatformError::FileNotFound(path.to_path_buf()));
        }

        debug!("Showing in Finder: {:?}", path);

        // Use -R flag to reveal (select) the file in Finder
        Command::new("open")
            .args(["-R", path.to_str().unwrap_or("")])
            .spawn()
            .map_err(|e| PlatformError::ShowInFolderFailed(format!("{:?}: {}", path, e)))?;

        Ok(())
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    fn should_open_as_tab(&self, url: &str, _initiating_url: Option<&str>) -> bool {
        // macOS WebKit is aggressive - use comprehensive filtering

        // Quick rejection for obvious popups
        if is_definitely_popup(url) {
            debug!("Blocking popup (definitely): {}", url);
            return false;
        }

        // Block about: and data: URLs
        if url == "about:blank" || url == "about:srcdoc" || url.starts_with("data:") {
            debug!("Blocking about/data URL: {}", url);
            return false;
        }

        // Block javascript: URLs
        if url.starts_with("javascript:") {
            debug!("Blocking javascript URL: {}", url);
            return false;
        }

        // Known ad/tracking patterns (macOS-specific aggressive filtering)
        let blocked_patterns = [
            // Google ad infrastructure
            "googlesyndication.com",
            "doubleclick.net",
            "googleadservices.com",
            "/sodar/",
            "/recaptcha/api2/aframe",
            // Facebook tracking
            "facebook.com/tr",
            "connect.facebook.net",
            // Common tracking
            "/beacon",
            "/pixel",
            "/track",
            "/_/bscframe",
            // Ad tech
            "criteo.com",
            "flashtalking.com",
            "tapad.com",
        ];

        let url_lower = url.to_lowercase();
        for pattern in blocked_patterns {
            if url_lower.contains(pattern) {
                debug!("Blocking URL matching pattern '{}': {}", pattern, url);
                return false;
            }
        }

        // Check for tracker redirects
        if is_tracker_redirect(&url_lower) {
            debug!("Blocking tracker redirect: {}", url);
            return false;
        }

        // Check for streaming popups
        if is_streaming_popup(&url_lower) {
            debug!("Blocking streaming popup: {}", url);
            return false;
        }

        // Allow if it looks legitimate
        if is_legitimate_navigation(url) {
            return true;
        }

        // Default: allow (let Shield handle further filtering)
        true
    }
}

/// Detect streaming site popup URLs
#[allow(dead_code)] // Used for popup filtering in later phases
fn is_streaming_popup(url: &str) -> bool {
    let popup_domains = [
        "ucast.pro",
        "loijtoottuleringv.info",
        "nicatethebene.info",
    ];

    if popup_domains.iter().any(|d| url.contains(d)) {
        return true;
    }

    let fingerprint_patterns = [
        "/fp?x-kpsdk",
        "gql.twitch.tv/",
        "passport.twitch.tv/",
    ];

    fingerprint_patterns.iter().any(|p| url.contains(p))
}

/// Detect tracker/ad redirect URLs
#[allow(dead_code)] // Used for popup filtering in later phases
fn is_tracker_redirect(url: &str) -> bool {
    if !url.contains('?') {
        return false;
    }

    let tracker_paths = [
        "/tag?", "/pixel?", "/sync?", "/usersync",
        "/collect?", "/event?", "/impression?",
        "/click?", "/redirect?", "/bounce?",
    ];

    let has_tracker_path = tracker_paths.iter().any(|p| url.contains(p));

    let adtech_params = [
        "bp_id=", "initiator=js", "gdpr=", "page_url=",
        "referrer_url=", "mediavine", "prebid", "rubicon",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_capabilities() {
        let platform = MacOSPlatform::new();
        let caps = platform.capabilities();

        assert!(caps.native_menu_required_for_clipboard);
        assert!(caps.supports_file_selection_in_folder);
        assert_eq!(caps.webview_engine, WebViewEngine::WebKit);
        assert_eq!(caps.platform_name, "macOS");
    }

    #[test]
    fn test_should_open_as_tab_blocks_popups() {
        let platform = MacOSPlatform::new();

        // Should block
        assert!(!platform.should_open_as_tab("about:blank", None));
        assert!(!platform.should_open_as_tab("javascript:void(0)", None));
        assert!(!platform.should_open_as_tab("data:text/html,<h1>test</h1>", None));
        assert!(!platform.should_open_as_tab("https://doubleclick.net/ad", None));
    }

    #[test]
    fn test_should_open_as_tab_allows_legitimate() {
        let platform = MacOSPlatform::new();

        // Should allow
        assert!(platform.should_open_as_tab("https://github.com/pureflow", None));
        assert!(platform.should_open_as_tab("https://example.com/page", None));
    }

    #[test]
    fn test_is_tracker_redirect() {
        assert!(is_tracker_redirect("https://example.com/tag?bp_id=123&initiator=js"));
        assert!(is_tracker_redirect("https://example.com/pixel?page_url=test&initiator=js"));
        assert!(!is_tracker_redirect("https://example.com/page"));
    }

    #[test]
    fn test_is_streaming_popup() {
        assert!(is_streaming_popup("https://ucast.pro/popup"));
        assert!(is_streaming_popup("https://site.com/fp?x-kpsdk-token"));
        assert!(!is_streaming_popup("https://twitch.tv/stream"));
    }
}
