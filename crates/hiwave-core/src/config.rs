//! Browser configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Browser configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// User data directory
    pub data_dir: PathBuf,

    /// Cache directory
    pub cache_dir: PathBuf,

    /// Enable ad blocking
    pub ad_blocking_enabled: bool,

    /// Enable JavaScript
    pub javascript_enabled: bool,

    /// User agent string
    pub user_agent: String,

    /// Maximum number of concurrent connections
    pub max_connections: usize,

    /// Workspace settings
    pub workspaces: WorkspaceConfig,

    /// Focus mode settings
    pub focus_mode: FocusModeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Maximum number of workspaces
    pub max_workspaces: usize,

    /// Auto-suspend inactive workspaces
    pub auto_suspend: bool,

    /// Suspend timeout in seconds
    pub suspend_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusModeConfig {
    /// Enable focus mode
    pub enabled: bool,

    /// Pomodoro duration in minutes
    pub pomodoro_duration: u32,

    /// Break duration in minutes
    pub break_duration: u32,

    /// Block distracting sites during focus
    pub block_distractions: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            data_dir: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("hiwave"),
            cache_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("hiwave"),
            ad_blocking_enabled: true,
            javascript_enabled: true,
            user_agent: format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) HiWave/{} (KHTML, like Gecko)",
                env!("CARGO_PKG_VERSION")
            ),
            max_connections: 6,
            workspaces: WorkspaceConfig::default(),
            focus_mode: FocusModeConfig::default(),
        }
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            max_workspaces: 10,
            auto_suspend: true,
            suspend_timeout: 300, // 5 minutes
        }
    }
}

impl Default for FocusModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pomodoro_duration: 25,
            break_duration: 5,
            block_distractions: true,
        }
    }
}

// Helper to get directories
mod dirs {
    use std::path::PathBuf;

    pub fn data_dir() -> Option<PathBuf> {
        if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home_dir().map(|h| h.join("Library").join("Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| home_dir().map(|h| h.join(".local").join("share")))
        }
    }

    pub fn cache_dir() -> Option<PathBuf> {
        if cfg!(target_os = "windows") {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home_dir().map(|h| h.join("Library").join("Caches"))
        } else {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| home_dir().map(|h| h.join(".cache")))
        }
    }

    fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
