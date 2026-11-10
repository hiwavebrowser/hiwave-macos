//! Application state management
//!
//! This module contains the central state for the HiWave application,
//! including the browser shell, ad blocker, password vault, and configuration.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use url::Url;
use hiwave_core::types::{TabId, WorkspaceId};
use hiwave_core::HiWaveResult;
use hiwave_shell::{BrowserShell, ShellSnapshot};
use hiwave_shield::AdBlocker;
use hiwave_vault::Vault;
use hiwave_analytics::Analytics;
use crate::import::ImportResult;

/// Write `data` to `path` so that a crash or a full disk leaves the OLD file, never a truncated
/// one: write `<name>.<pid>.tmp` next to it, sync it, rename it over the real file. The temp file
/// is removed when the write, the sync or the rename fails.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()
    })();
    if let Err(err) = written {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

/// Read a small JSON state file for a loader that falls back to defaults. Returns the value (if
/// there is a usable one) and whether saving over this file must be REFUSED for the run.
/// Missing file: `(None, false)`. Content that does not parse (not UTF-8 included): the file is
/// kept aside (`keep_aside`) so the next save does not destroy it: `(None, false)`, or
/// `(None, true)` when it could not be moved either. An I/O error (access denied, a sharing
/// violation, a cloud placeholder) says nothing about the content: the file is left alone and
/// saving is refused, `(None, true)`.
fn load_json_keep_aside<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> (Option<T>, bool) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return (None, false),
        Err(err) => {
            tracing::warn!("{what} could not be read ({err}); using defaults and not saving it this run");
            return (None, true);
        }
    };
    let parsed = std::str::from_utf8(&bytes)
        .map_err(|err| err.to_string())
        .and_then(|text| serde_json::from_str::<T>(text).map_err(|err| err.to_string()));
    match parsed {
        Ok(value) => {
            sweep_stale_temps(path);
            (Some(value), false)
        }
        Err(err) => match keep_aside(path) {
            Ok(kept) => {
                tracing::warn!("{what} is not valid ({err}); kept as {} and using defaults", kept.display());
                (None, false)
            }
            Err(rename_err) => {
                tracing::warn!(
                    "{what} is not valid ({err}) and could not be moved aside ({rename_err}); using defaults and not saving it this run"
                );
                (None, true)
            }
        },
    }
}

/// Rename an unusable file to `<name>.corrupt`, then `.corrupt-1`, `.corrupt-2`, ... (never over
/// an earlier one), and return where it went. The caller then starts empty without the next save
/// destroying the only copy.
pub(crate) fn keep_aside(path: &Path) -> std::io::Result<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut target = path.with_file_name(format!("{name}.corrupt"));
    let mut n = 1;
    while target.exists() {
        target = path.with_file_name(format!("{name}.corrupt-{n}"));
        n += 1;
    }
    fs::rename(path, &target)?;
    Ok(target)
}

/// Remove `<name>.<pid>.tmp` files a crashed run left next to `path`. Call it only AFTER `path`
/// has been read successfully: a complete temp file from a crash between the sync and the rename
/// may be the best copy left when the real file is missing or unreadable. A file touched in the
/// last minute is left alone (another instance may be about to rename it).
pub(crate) fn sweep_stale_temps(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().map(|n| n.to_string_lossy().into_owned())) else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("{name}.");
    for entry in entries.filter_map(|e| e.ok()) {
        let file = entry.file_name().to_string_lossy().into_owned();
        if !(file.starts_with(&prefix) && file.ends_with(".tmp")) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > std::time::Duration::from_secs(60));
        if stale && entry.path().is_file() {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Cross-platform path wrapper that serializes with forward slashes
///
/// This type ensures paths are stored in a portable format (forward slashes)
/// regardless of the operating system. When deserialized, paths are converted
/// back to the native format.
#[derive(Debug, Clone)]
pub struct PortablePath(PathBuf);

#[allow(dead_code)] // Methods used in tests and future phases
impl PortablePath {
    /// Create a new PortablePath from a PathBuf
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// Create a new PortablePath from a string
    pub fn from_str(s: &str) -> Self {
        Self(PathBuf::from(s))
    }

    /// Get a reference to the underlying Path
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Get the underlying PathBuf
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Check if the path exists
    pub fn exists(&self) -> bool {
        self.0.exists()
    }

    /// Get the file name component
    pub fn file_name(&self) -> Option<&std::ffi::OsStr> {
        self.0.file_name()
    }

    /// Convert to a string representation (may fail for non-UTF8 paths)
    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string_lossy()
    }
}

impl From<PathBuf> for PortablePath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

impl From<&Path> for PortablePath {
    fn from(path: &Path) -> Self {
        Self(path.to_path_buf())
    }
}

impl From<String> for PortablePath {
    fn from(s: String) -> Self {
        Self(PathBuf::from(s))
    }
}

impl AsRef<Path> for PortablePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Serialize for PortablePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Always serialize with forward slashes for cross-platform compatibility
        let normalized = self
            .0
            .to_str()
            .ok_or_else(|| serde::ser::Error::custom("Path contains invalid UTF-8"))?
            .replace('\\', "/");
        serializer.serialize_str(&normalized)
    }
}

impl<'de> Deserialize<'de> for PortablePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        // PathBuf::from handles platform-specific separator conversion
        Ok(PortablePath(PathBuf::from(s)))
    }
}

/// User-defined blocklist for popup domains
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserBlocklist {
    /// Set of blocked domains
    pub domains: HashSet<String>,
}

impl UserBlocklist {
    /// Load blocklist from file
    pub fn load(path: &PathBuf) -> Self {
        Self::load_checked(path).0
    }

    /// Like `load`, and whether saving over the file must be refused (see `load_json_keep_aside`).
    pub(crate) fn load_checked(path: &PathBuf) -> (Self, bool) {
        let (value, blocked) = load_json_keep_aside(path, "The blocklist");
        (value.unwrap_or_default(), blocked)
    }

    /// Save blocklist to file
    pub fn save(&self, path: &PathBuf) -> HiWaveResult<()> {
        let data = serde_json::to_string_pretty(self).map_err(|e| {
            hiwave_core::HiWaveError::Config(format!("Failed to serialize blocklist: {}", e))
        })?;
        write_atomic(path, data.as_bytes())?;
        Ok(())
    }

    /// Add a domain to the blocklist
    pub fn add_domain(&mut self, domain: String) -> bool {
        self.domains.insert(domain)
    }

    /// Remove a domain from the blocklist
    pub fn remove_domain(&mut self, domain: &str) -> bool {
        self.domains.remove(domain)
    }

    /// Check if a URL should be blocked based on its domain
    #[allow(dead_code)]
    pub fn should_block(&self, url: &str) -> bool {
        if let Ok(parsed) = Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                // Check exact domain match
                if self.domains.contains(host) {
                    return true;
                }
                // Check if any blocked domain is a suffix (e.g., block example.com also blocks sub.example.com)
                for blocked in &self.domains {
                    if host.ends_with(blocked)
                        && (host == blocked.as_str() || host.ends_with(&format!(".{}", blocked)))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Extract domain from a URL
    pub fn extract_domain(url: &str) -> Option<String> {
        Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
    }
}

/// Focus mode configuration (persisted to settings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusModeConfig {
    /// Whether auto-focus is enabled
    pub auto_enabled: bool,
    
    /// Seconds before auto-entering focus mode (default: 60)
    pub activation_delay_secs: u32,
    
    /// Show progress bar at top of screen
    pub show_progress_bar: bool,
    
    /// Hide cursor after inactivity
    pub hide_cursor: bool,
    
    /// Seconds before cursor hides (default: 3)
    pub cursor_hide_delay_secs: u32,
    
    /// Exit focus mode after inactivity (0 = disabled)
    pub inactivity_exit_mins: u32,
    
    /// Keyboard shortcut identifier (default: "F11")
    pub shortcut: String,
    
    /// Domains that never auto-focus
    pub blocklist: Vec<String>,
}

impl Default for FocusModeConfig {
    fn default() -> Self {
        Self {
            auto_enabled: true,
            activation_delay_secs: 60,
            show_progress_bar: true,
            hide_cursor: true,
            cursor_hide_delay_secs: 3,
            inactivity_exit_mins: 0,
            shortcut: "F11".to_string(),
            blocklist: vec![
                "youtube.com".to_string(),
                "netflix.com".to_string(),
                "twitch.tv".to_string(),
                "vimeo.com".to_string(),
                "dailymotion.com".to_string(),
                "hulu.com".to_string(),
                "disneyplus.com".to_string(),
                "primevideo.com".to_string(),
            ],
        }
    }
}

impl FocusModeConfig {
    /// Check if a URL's domain is in the blocklist
    pub fn is_blocklisted(&self, url: &str) -> bool {
        if let Ok(parsed) = Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                for blocked in &self.blocklist {
                    if host == blocked || host.ends_with(&format!(".{}", blocked)) {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    /// Add a domain to the blocklist
    pub fn add_to_blocklist(&mut self, domain: String) -> bool {
        let domain = domain.trim().to_lowercase();
        if domain.is_empty() || self.blocklist.contains(&domain) {
            return false;
        }
        self.blocklist.push(domain);
        true
    }
    
    /// Remove a domain from the blocklist
    pub fn remove_from_blocklist(&mut self, domain: &str) -> bool {
        let domain = domain.trim().to_lowercase();
        if let Some(idx) = self.blocklist.iter().position(|d| d == &domain) {
            self.blocklist.remove(idx);
            return true;
        }
        false
    }
}

/// Focus mode runtime state (not persisted)
pub struct FocusModeState {
    /// Currently in focus mode
    pub active: bool,
    
    /// When focus mode was entered (for session duration tracking)
    pub entered_at: Option<Instant>,
    
    /// When current page was loaded (for auto-trigger timing)
    pub page_loaded_at: Option<Instant>,
    
    /// Last significant scroll timestamp (resets auto-trigger timer)
    pub last_scroll_at: Option<Instant>,
    
    /// Last user interaction timestamp (for inactivity exit)
    pub last_interaction_at: Option<Instant>,
    
    /// Whether peek UI is currently visible (mouse at top)
    pub peek_visible: bool,
    
    /// Current scroll progress (0.0 - 1.0)
    pub scroll_progress: f32,
    
    /// Whether media is currently playing in the content
    pub media_playing: bool,
    
    /// Current page URL (for blocklist checking)
    pub current_url: Option<String>,

    /// Whether auto-trigger has already fired for this page load
    /// (prevents re-triggering after user exits)
    pub auto_triggered_for_page: bool,
}

impl FocusModeState {
    pub fn new() -> Self {
        Self {
            active: false,
            entered_at: None,
            page_loaded_at: None,
            last_scroll_at: None,
            last_interaction_at: None,
            peek_visible: false,
            scroll_progress: 0.0,
            media_playing: false,
            current_url: None,
            auto_triggered_for_page: false,
        }
    }
    
    /// Enter focus mode
    pub fn enter(&mut self) {
        if self.active {
            return;
        }
        self.active = true;
        self.entered_at = Some(Instant::now());
        self.last_interaction_at = Some(Instant::now());
        self.peek_visible = false;
        tracing::info!("Entered focus mode");
    }
    
    /// Exit focus mode
    pub fn exit(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        self.entered_at = None;
        self.peek_visible = false;
        // Prevent auto-trigger from re-activating on same page after user exits
        self.auto_triggered_for_page = true;
        tracing::info!("Exited focus mode");
    }
    
    /// Toggle focus mode
    pub fn toggle(&mut self) {
        if self.active {
            self.exit();
        } else {
            self.enter();
        }
    }
    
    /// Show peek UI (mouse at top)
    pub fn show_peek(&mut self) {
        if self.active && !self.peek_visible {
            self.peek_visible = true;
            self.record_interaction();
        }
    }
    
    /// Hide peek UI
    pub fn hide_peek(&mut self) {
        self.peek_visible = false;
    }
    
    /// Record user interaction (for inactivity tracking)
    pub fn record_interaction(&mut self) {
        self.last_interaction_at = Some(Instant::now());
    }
    
    /// Record a significant scroll (resets auto-trigger timer)
    pub fn record_scroll(&mut self) {
        self.last_scroll_at = Some(Instant::now());
        self.record_interaction();
    }
    
    /// Record page navigation
    pub fn record_navigation(&mut self, url: &str) {
        // Exit focus mode on navigation
        if self.active {
            self.exit();
        }
        self.current_url = Some(url.to_string());
        self.page_loaded_at = Some(Instant::now());
        self.last_scroll_at = None;
        self.scroll_progress = 0.0;
        self.media_playing = false;
        // Reset auto-trigger flag for new page
        self.auto_triggered_for_page = false;
    }
    
    /// Update scroll progress
    pub fn update_scroll_progress(&mut self, progress: f32) {
        self.scroll_progress = progress.clamp(0.0, 1.0);
    }
    
    /// Update media playing state
    pub fn set_media_playing(&mut self, playing: bool) {
        self.media_playing = playing;
    }
    
    /// Check if auto-focus should trigger based on config and current state
    pub fn should_auto_enter(&self, config: &FocusModeConfig) -> bool {
        // Already active
        if self.active {
            return false;
        }

        // Already auto-triggered for this page (prevents re-trigger after user exits)
        if self.auto_triggered_for_page {
            return false;
        }

        // Auto-focus disabled
        if !config.auto_enabled {
            return false;
        }
        
        // Media is playing
        if self.media_playing {
            return false;
        }
        
        // Check if URL is blocklisted
        if let Some(ref url) = self.current_url {
            if config.is_blocklisted(url) {
                return false;
            }
        }
        
        // Check if enough time has passed since page load
        let Some(page_loaded) = self.page_loaded_at else {
            return false;
        };
        
        let elapsed = page_loaded.elapsed().as_secs() as u32;
        if elapsed < config.activation_delay_secs {
            return false;
        }
        
        // Check if there was recent scroll activity (user still exploring)
        if let Some(last_scroll) = self.last_scroll_at {
            // If scrolled within last 10 seconds, don't auto-enter
            if last_scroll.elapsed().as_secs() < 10 {
                return false;
            }
        }
        
        true
    }
    
    /// Check if should exit due to inactivity
    #[allow(dead_code)]
    pub fn should_inactivity_exit(&self, config: &FocusModeConfig) -> bool {
        if !self.active || config.inactivity_exit_mins == 0 {
            return false;
        }

        if let Some(last_interaction) = self.last_interaction_at {
            let inactivity_secs = last_interaction.elapsed().as_secs();
            let threshold_secs = (config.inactivity_exit_mins as u64) * 60;
            return inactivity_secs >= threshold_secs;
        }

        false
    }
    
    /// Get duration in focus mode (seconds)
    pub fn focus_duration_secs(&self) -> Option<u64> {
        self.entered_at.map(|t| t.elapsed().as_secs())
    }
}

impl Default for FocusModeState {
    fn default() -> Self {
        Self::new()
    }
}

// Legacy FocusMode alias for compatibility during transition
pub type FocusMode = FocusModeState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub status: DownloadStatus,
    /// Path to the downloaded file, stored in portable format (forward slashes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PortablePath>,
    pub progress: Option<f32>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
}

#[allow(dead_code)] // Method used in tests and future IPC updates
impl DownloadItem {
    /// Get the path as a string for display or IPC
    pub fn path_string(&self) -> Option<String> {
        self.path.as_ref().map(|p| p.to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitRecord {
    pub id: u64,
    pub url: String,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub timestamp: u64,
    pub count: u32,
}

/// Central application state
pub struct AppState {
    /// Browser shell (tabs, workspaces, command palette)
    pub shell: BrowserShell,

    /// Ad blocker
    pub shield: AdBlocker,

    /// Password vault
    pub vault: Vault,

    /// Analytics tracking
    pub analytics: Analytics,

    /// Configuration
    pub config: AppConfig,

    /// User-configurable settings
    pub user_settings: UserSettings,

    /// Focus mode state (runtime, not persisted)
    pub focus_mode: FocusMode,

    /// Focus mode configuration (persisted)
    pub focus_mode_config: FocusModeConfig,

    /// Shelved tabs by workspace name
    pub shelves: HashMap<String, Vec<ShelfItem>>,

    /// Cellar storage for aged-out shelf items (recoverable)
    pub cellar: Vec<CellarItem>,

    /// Download history (both in-progress and completed)
    pub downloads: Vec<DownloadItem>,

    /// Visit history entries
    pub visit_history: Vec<VisitRecord>,

    /// Monotonic download identifier generator
    pub next_download_id: u64,

    /// Monotonic visit identifier generator
    pub next_visit_id: u64,

    /// User-defined blocked domains (for popup blocking)
    pub user_blocklist: UserBlocklist,

    /// Recent popup requests for flood protection (URL, timestamp)
    /// This is runtime-only state, not persisted
    #[allow(dead_code)]
    pub recent_popups: Vec<(String, Instant)>,

    /// The tab a tab shortcut last asked the chrome to activate, while
    /// that `activate_tab` is in flight (M1d): the next shortcut steps
    /// from it, so two fast presses reach two tabs. Runtime only.
    pub tab_select_pending: Option<crate::tab_shortcuts::PendingSelect>,

    /// Set when the saved session could not be read and could not be moved aside: saving is
    /// refused for the rest of the run, so the unreadable file is never overwritten.
    workspace_state_save_blocked: bool,

    /// Small state files whose saves are refused for this run because the file could not be
    /// read or moved aside (`settings.json`, `focus_mode.json`, `user-blocklist.json`,
    /// `visit_history.json`): overwriting it would destroy what may be the user's only copy.
    blocked_state_files: std::collections::HashSet<&'static str>,

    /// Zoom levels per tab (tab_id -> zoom_level, default 1.0)
    pub tab_zoom_levels: HashMap<String, f32>,

    /// Audio playing state per tab (tab_id -> is_playing)
    pub tab_audio_playing: HashMap<String, bool>,

    /// Muted state per tab (tab_id -> is_muted)
    pub tab_audio_muted: HashMap<String, bool>,

    /// Session start time (for analytics)
    pub session_start_time: Instant,

    /// Autofill: the pending user-requested fill, if any. Runtime only.
    pub autofill: crate::ipc::autofill::AutofillGate,
}

impl AppState {
    /// Create a new application state
    pub fn new(config: AppConfig) -> HiWaveResult<Self> {
        // Create data directory if it doesn't exist
        if let Some(parent) = config.data_dir.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::create_dir_all(&config.data_dir).ok();
        std::fs::create_dir_all(&config.download_dir).ok();

        // Initialize components
        let shell = BrowserShell::new();
        let shield = AdBlocker::with_filter_lists();
        let focus_mode = FocusMode::new();

        let vault_path = config.data_dir.join("vault.db");
        let vault = Vault::new(&vault_path)?;

        // Initialize analytics
        let analytics_path = config.data_dir.join("analytics.db");
        let analytics = Analytics::new(analytics_path)?;

        // Load user settings
        let settings_path = config.data_dir.join("settings.json");
        let mut blocked_state_files = std::collections::HashSet::new();
        let (user_settings, blocked) = UserSettings::load_checked(&settings_path);
        if blocked {
            blocked_state_files.insert("settings.json");
        }
        tracing::info!("Loaded user settings (theme: {})", user_settings.theme);

        // Load focus mode config
        let focus_config_path = config.data_dir.join("focus_mode.json");
        let (focus_mode_config, blocked) = Self::load_focus_config(&focus_config_path);
        if blocked {
            blocked_state_files.insert("focus_mode.json");
        }

        // Load user blocklist
        let blocklist_path = config.data_dir.join("user-blocklist.json");
        let (user_blocklist, blocked) = UserBlocklist::load_checked(&blocklist_path);
        if blocked {
            blocked_state_files.insert("user-blocklist.json");
        }
        if !user_blocklist.domains.is_empty() {
            tracing::info!(
                "Loaded {} user-blocked domains",
                user_blocklist.domains.len()
            );
        }

        Ok(Self {
            shell,
            shield,
            vault,
            analytics,
            config,
            user_settings,
            focus_mode,
            focus_mode_config,
            shelves: HashMap::new(),
            cellar: Vec::new(),
            downloads: Vec::new(),
            visit_history: Vec::new(),
            next_download_id: 1,
            next_visit_id: 1,
            user_blocklist,
            recent_popups: Vec::new(),
            tab_select_pending: None,
            workspace_state_save_blocked: false,
            blocked_state_files,
            tab_zoom_levels: HashMap::new(),
            tab_audio_playing: HashMap::new(),
            tab_audio_muted: HashMap::new(),
            session_start_time: Instant::now(),
            autofill: Default::default(),
        })
    }

    pub fn current_timestamp_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }

    /// Get the path to the user blocklist file
    pub fn blocklist_path(&self) -> PathBuf {
        self.config.data_dir.join("user-blocklist.json")
    }

    /// Refuse a save over a small state file that could not be read or moved aside at load.
    fn refuse_if_blocked(&self, file: &'static str) -> HiWaveResult<()> {
        if self.blocked_state_files.contains(file) {
            return Err(hiwave_core::HiWaveError::Config(format!(
                "Not saving {file}: the existing file could not be read or moved aside"
            )));
        }
        Ok(())
    }

    /// Add a domain to the user blocklist and save
    pub fn block_domain(&mut self, domain: String) -> HiWaveResult<bool> {
        self.refuse_if_blocked("user-blocklist.json")?;
        let added = self.user_blocklist.add_domain(domain.clone());
        if added {
            tracing::info!("Added domain to user blocklist: {}", domain);
            self.user_blocklist.save(&self.blocklist_path())?;
        }
        Ok(added)
    }

    /// Remove a domain from the user blocklist and save
    pub fn unblock_domain(&mut self, domain: &str) -> HiWaveResult<bool> {
        self.refuse_if_blocked("user-blocklist.json")?;
        let removed = self.user_blocklist.remove_domain(domain);
        if removed {
            tracing::info!("Removed domain from user blocklist: {}", domain);
            self.user_blocklist.save(&self.blocklist_path())?;
        }
        Ok(removed)
    }

    /// Get the path to the settings file
    pub fn settings_path(&self) -> PathBuf {
        self.config.data_dir.join("settings.json")
    }

    /// Get a clone of current user settings
    pub fn get_settings(&self) -> UserSettings {
        self.user_settings.clone()
    }

    /// Save current user settings to disk
    pub fn save_settings(&self) -> HiWaveResult<()> {
        self.refuse_if_blocked("settings.json")?;
        self.user_settings.save(&self.settings_path())
    }

    /// Update user settings and save to disk
    pub fn update_settings(&mut self, settings: UserSettings) -> HiWaveResult<()> {
        self.user_settings = settings;
        self.save_settings()
    }

    /// Get the path to the focus mode config file
    pub fn focus_config_path(&self) -> PathBuf {
        self.config.data_dir.join("focus_mode.json")
    }

    /// Load focus mode config from file
    fn load_focus_config(path: &std::path::Path) -> (FocusModeConfig, bool) {
        let (value, blocked) = load_json_keep_aside(path, "The focus mode config");
        (value.unwrap_or_default(), blocked)
    }

    /// Save focus mode config to disk
    pub fn save_focus_config(&self) -> HiWaveResult<()> {
        self.refuse_if_blocked("focus_mode.json")?;
        let path = self.focus_config_path();
        let json = serde_json::to_string_pretty(&self.focus_mode_config)
            .map_err(|e| hiwave_core::HiWaveError::Config(format!("Failed to serialize focus config: {}", e)))?;
        write_atomic(&path, json.as_bytes())?;
        Ok(())
    }

    /// Update focus mode config and save to disk
    pub fn update_focus_config(&mut self, config: FocusModeConfig) -> HiWaveResult<()> {
        self.focus_mode_config = config;
        self.save_focus_config()
    }

    /// Generate an export of workspaces and optionally settings
    pub fn export_data(&self, include_settings: bool, include_workspaces: bool) -> HiWaveExport {
        let mut export = HiWaveExport::new();

        if include_settings {
            export.settings = Some(self.user_settings.clone());
        }

        if include_workspaces {
            // Export workspaces with their tabs
            for workspace in self.shell.list_workspaces() {
                let tabs: Vec<TabExport> = self
                    .shell
                    .list_tabs(Some(workspace.id))
                    .iter()
                    .map(|tab| TabExport {
                        url: tab.url.to_string(),
                        title: tab.title.clone(),
                        locked: tab.locked,
                    })
                    .collect();

                export.workspaces.push(WorkspaceExport {
                    name: workspace.name.clone(),
                    tabs,
                });
            }

            // Export shelf items
            export.shelf = self.shelves.clone();
        }

        export
    }

    /// Import data from an export, merging or replacing workspaces
    pub fn import_data(&mut self, data: &HiWaveExport, replace: bool) -> HiWaveResult<ImportResult> {
        let mut workspaces_created = 0;
        let mut tabs_created = 0;
        let mut errors = Vec::new();

        // Import settings if present
        if let Some(ref settings) = data.settings {
            self.user_settings = settings.clone();
            if let Err(e) = self.save_settings() {
                errors.push(format!("Failed to save settings: {}", e));
            }
        }

        // Import workspaces
        for ws_export in &data.workspaces {
            // Check if workspace already exists
            let existing = self
                .shell
                .list_workspaces()
                .iter()
                .find(|ws| ws.name == ws_export.name)
                .map(|ws| ws.id);

            let workspace_id = if let Some(id) = existing {
                if replace {
                    // Close all existing tabs in this workspace
                    let tabs: Vec<_> = self.shell.list_tabs(Some(id)).iter().map(|t| t.id).collect();
                    for tab_id in tabs {
                        let _ = self.shell.close_tab(tab_id);
                    }
                }
                id
            } else {
                // Create new workspace
                let ws_id = self.shell.create_workspace(ws_export.name.clone());
                workspaces_created += 1;
                ws_id
            };

            // Import tabs
            for tab_export in &ws_export.tabs {
                let url = match url::Url::parse(&tab_export.url) {
                    Ok(u) => u,
                    Err(e) => {
                        errors.push(format!("Invalid URL '{}': {}", tab_export.url, e));
                        continue;
                    }
                };

                let tab = hiwave_core::types::TabInfo {
                    id: hiwave_core::types::TabId::new(),
                    url,
                    title: tab_export.title.clone(),
                    favicon: None,
                    workspace_id,
                    suspended: true,
                    loading: false,
                    locked: tab_export.locked,
                    last_visited: Some(Self::current_timestamp_secs()),
                };

                if self.shell.create_tab(tab).is_ok() {
                    tabs_created += 1;
                }
            }
        }

        // Import shelf items
        for (workspace_name, items) in &data.shelf {
            self.shelves
                .entry(workspace_name.clone())
                .or_default()
                .extend(items.clone());
        }

        // Save state
        if let Err(e) = self.save_workspace_state() {
            errors.push(format!("Failed to save workspace state: {}", e));
        }

        Ok(ImportResult {
            success: errors.is_empty(),
            workspaces_created,
            tabs_created,
            errors,
        })
    }

    /// Clear all shelf items
    pub fn clear_shelf(&mut self) {
        self.shelves.clear();
    }

    /// Move a shelf item to the cellar (for recovery)
    #[allow(dead_code)]
    pub fn shelve_to_cellar(&mut self, workspace: &str, shelf_item: &ShelfItem) {
        let cellar_item = CellarItem {
            id: format!("cellar-{}", Self::current_timestamp_secs()),
            url: shelf_item.url.clone(),
            title: shelf_item.title.clone(),
            workspace: workspace.to_string(),
            originally_shelved: shelf_item.added_at,
            cellared_at: Self::current_timestamp_secs(),
        };
        self.cellar.push(cellar_item);

        // Keep cellar from growing unbounded (max 100 items)
        while self.cellar.len() > 100 {
            self.cellar.remove(0);
        }
    }

    /// Restore a cellar item back to the shelf
    pub fn restore_from_cellar(&mut self, cellar_id: &str) -> Option<ShelfItem> {
        let idx = self.cellar.iter().position(|item| item.id == cellar_id)?;
        let cellar_item = self.cellar.remove(idx);

        let shelf_item = ShelfItem {
            id: format!("shelf-{}", Self::current_timestamp_secs()),
            url: cellar_item.url,
            title: cellar_item.title,
            workspace: cellar_item.workspace.clone(),
            added_at: Self::current_timestamp_secs(),
        };

        // Add back to shelf
        self.shelves
            .entry(cellar_item.workspace)
            .or_default()
            .push(shelf_item.clone());

        Some(shelf_item)
    }

    /// Get all cellar items
    pub fn list_cellar(&self) -> &Vec<CellarItem> {
        &self.cellar
    }

    /// Clear the cellar
    pub fn clear_cellar(&mut self) {
        self.cellar.clear();
    }

    /// Check if a popup request should be allowed (flood protection)
    /// Returns true if allowed, false if blocked due to flood
    #[allow(dead_code)]
    pub fn should_allow_popup(&mut self, url: &str) -> bool {
        use std::time::Duration;

        let now = Instant::now();
        let flood_window = Duration::from_secs(2);
        let duplicate_window = Duration::from_millis(500);
        let max_popups_per_window = 5;

        // Clean up old entries
        self.recent_popups
            .retain(|(_, timestamp)| now.duration_since(*timestamp) < flood_window);

        // Check for duplicate URL in short window (same URL requested twice quickly)
        let is_duplicate = self.recent_popups.iter().any(|(popup_url, timestamp)| {
            popup_url == url && now.duration_since(*timestamp) < duplicate_window
        });

        if is_duplicate {
            tracing::warn!(
                "Popup blocked (duplicate): {} - same URL requested within 500ms",
                crate::page_url_for_log(url)
            );
            return false;
        }

        // Check for flood (too many popups in time window)
        if self.recent_popups.len() >= max_popups_per_window {
            tracing::warn!(
                "Popup blocked (flood): {} - {} popups in last 2 seconds",
                crate::page_url_for_log(url),
                self.recent_popups.len()
            );
            return false;
        }

        // Record this popup request
        self.recent_popups.push((url.to_string(), now));
        true
    }

    /// Get the zoom level for a tab (defaults to 1.0 if not set)
    pub fn get_zoom_level(&self, tab_id: &str) -> f32 {
        *self.tab_zoom_levels.get(tab_id).unwrap_or(&1.0)
    }

    /// Set the zoom level for a tab (clamped to 0.5 - 3.0)
    pub fn set_zoom_level(&mut self, tab_id: &str, level: f32) -> f32 {
        let clamped = level.clamp(0.5, 3.0);
        self.tab_zoom_levels.insert(tab_id.to_string(), clamped);
        clamped
    }

    /// Zoom in for a tab (step = 0.1, max 3.0)
    pub fn zoom_in(&mut self, tab_id: &str) -> f32 {
        let current = self.get_zoom_level(tab_id);
        self.set_zoom_level(tab_id, current + 0.1)
    }

    /// Zoom out for a tab (step = 0.1, min 0.5)
    pub fn zoom_out(&mut self, tab_id: &str) -> f32 {
        let current = self.get_zoom_level(tab_id);
        self.set_zoom_level(tab_id, current - 0.1)
    }

    /// Reset zoom to 100% for a tab
    pub fn reset_zoom(&mut self, tab_id: &str) -> f32 {
        self.set_zoom_level(tab_id, 1.0)
    }

    /// Remove zoom level entry when a tab is closed
    #[allow(dead_code)]
    pub fn remove_tab_zoom(&mut self, tab_id: &str) {
        self.tab_zoom_levels.remove(tab_id);
    }

    /// Check if a tab is playing audio
    pub fn is_tab_playing_audio(&self, tab_id: &str) -> bool {
        *self.tab_audio_playing.get(tab_id).unwrap_or(&false)
    }

    /// Set audio playing state for a tab
    pub fn set_tab_audio_playing(&mut self, tab_id: &str, playing: bool) {
        self.tab_audio_playing.insert(tab_id.to_string(), playing);
    }

    /// Check if a tab is muted
    pub fn is_tab_muted(&self, tab_id: &str) -> bool {
        *self.tab_audio_muted.get(tab_id).unwrap_or(&false)
    }

    /// Set muted state for a tab
    pub fn set_tab_muted(&mut self, tab_id: &str, muted: bool) {
        self.tab_audio_muted.insert(tab_id.to_string(), muted);
    }

    /// Toggle mute state for a tab
    pub fn toggle_tab_mute(&mut self, tab_id: &str) -> bool {
        let currently_muted = self.is_tab_muted(tab_id);
        self.set_tab_muted(tab_id, !currently_muted);
        !currently_muted
    }

    /// Remove audio state entries when a tab is closed
    #[allow(dead_code)]
    pub fn remove_tab_audio_state(&mut self, tab_id: &str) {
        self.tab_audio_playing.remove(tab_id);
        self.tab_audio_muted.remove(tab_id);
    }

    /// Create with default configuration
    pub fn with_defaults() -> HiWaveResult<Self> {
        let mut state = Self::new(AppConfig::default())?;
        if let Err(err) = state.load_workspace_state() {
            tracing::warn!("Failed to load workspace state: {}", err);
        }
        if let Err(err) = state.load_visit_history() {
            tracing::warn!("Failed to load visit history: {}", err);
        }
        Ok(state)
    }

    pub fn load_workspace_state(&mut self) -> HiWaveResult<bool> {
        let path = self.workspace_state_path();
        // Only NotFound means "no session yet". Any other error (access denied, a sharing
        // violation, a cloud placeholder) says nothing about the file's content: keep it, and do
        // not save over it this run.
        match fs::metadata(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => {
                self.workspace_state_save_blocked = true;
                tracing::warn!("The saved session could not be inspected ({}); not saving it this run", err);
                return Err(err.into());
            }
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                // An I/O failure is not corruption: leave the file where it is.
                self.workspace_state_save_blocked = true;
                tracing::warn!("The saved session could not be read ({}); not saving it this run", err);
                return Err(err.into());
            }
        };
        match self.read_workspace_state(&bytes) {
            Ok(()) => {
                // Read fine: now a crashed run's leftover temp files are no longer anyone's
                // best copy.
                sweep_stale_temps(&path);
                self.ensure_download_dir();
                Ok(true)
            }
            Err(err) => {
                // The content cannot be used (not UTF-8, truncated, invalid): it must not be
                // overwritten by the empty state the first save would write.
                match keep_aside(&path) {
                    Ok(kept) => tracing::warn!(
                        "The saved session could not be read; kept as {} and starting empty",
                        kept.display()
                    ),
                    Err(rename_err) => {
                        self.workspace_state_save_blocked = true;
                        tracing::warn!(
                            "The saved session could not be read and could not be moved aside ({}); not saving it this run",
                            rename_err
                        );
                    }
                }
                Err(err)
            }
        }
    }

    fn read_workspace_state(&mut self, bytes: &[u8]) -> HiWaveResult<()> {
        let contents = std::str::from_utf8(bytes).map_err(|err| {
            hiwave_core::HiWaveError::Config(format!("Invalid workspace state: {}", err))
        })?;
        if let Ok(snapshot) = serde_json::from_str::<WorkspaceStateSnapshot>(contents) {
            self.shell.load_snapshot(snapshot.shell)?;
            self.shelves = snapshot.shelves;
            self.cellar = snapshot.cellar;
            self.downloads = snapshot.downloads;
            self.next_download_id = self
                .downloads
                .iter()
                .map(|item| item.id)
                .max()
                .map(|max| max + 1)
                .unwrap_or(1);
            self.sync_shelf_keys();
        } else {
            let snapshot: ShellSnapshot = serde_json::from_str(contents).map_err(|err| {
                hiwave_core::HiWaveError::Config(format!("Invalid workspace state: {}", err))
            })?;
            self.shell.load_snapshot(snapshot)?;
            self.shelves.clear();
            self.cellar.clear();
            self.downloads.clear();
            self.next_download_id = 1;
            self.sync_shelf_keys();
        }
        Ok(())
    }

    pub fn save_workspace_state(&self) -> HiWaveResult<()> {
        if self.workspace_state_save_blocked {
            return Err(hiwave_core::HiWaveError::Config(
                "Not saving: the saved session could not be read or moved aside".to_string(),
            ));
        }
        let path = self.workspace_state_path();
        let snapshot = WorkspaceStateSnapshot {
            shell: self.shell.snapshot(),
            shelves: self.shelves.clone(),
            cellar: self.cellar.clone(),
            downloads: self.downloads.clone(),
        };
        let data = serde_json::to_string_pretty(&snapshot).map_err(|err| {
            hiwave_core::HiWaveError::Config(format!("Failed to save workspace state: {}", err))
        })?;
        write_atomic(&path, data.as_bytes())?;
        Ok(())
    }

    fn workspace_state_path(&self) -> PathBuf {
        self.config.data_dir.join("workspace_state.json")
    }

    fn ensure_download_dir(&self) {
        if let Err(err) = fs::create_dir_all(&self.config.download_dir) {
            tracing::warn!("Failed to create download directory: {}", err);
        }
    }

    #[allow(dead_code)]
    pub fn prepare_download_path(&self, url: &str) -> PathBuf {
        self.ensure_download_dir();
        let file_name = Self::suggested_download_name(url, self.next_download_id);
        Self::unique_file_path(&self.config.download_dir, &file_name)
    }

    #[allow(dead_code)]
    pub fn register_download(&mut self, url: &str, target_path: &Path) {
        let id = self.next_download_id;
        self.next_download_id = self.next_download_id.saturating_add(1);
        let file_name = target_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("download-{}", id));
        self.downloads.push(DownloadItem {
            id,
            url: url.to_string(),
            file_name,
            status: DownloadStatus::InProgress,
            path: Some(PortablePath::from(target_path)),
            progress: None,
            started_at: Self::current_timestamp_secs(),
            finished_at: None,
            error: None,
        });
    }

    #[allow(dead_code)]
    pub fn finalize_download(&mut self, url: &str, saved_path: Option<PathBuf>, success: bool) {
        // Convert saved_path to string for comparison
        let saved_path_str = saved_path
            .as_ref()
            .and_then(|p| p.to_str().map(|s| s.to_string()));

        if let Some(item) = self
            .downloads
            .iter_mut()
            .rev()
            .filter(|item| item.status == DownloadStatus::InProgress)
            .find(|item| {
                // Match by path if available, otherwise by URL
                if let Some(path_value) = &saved_path_str {
                    if let Some(item_path) = &item.path {
                        return item_path.to_string_lossy() == *path_value;
                    }
                }
                item.url == url
            })
        {
            item.status = if success {
                DownloadStatus::Completed
            } else {
                DownloadStatus::Failed
            };
            item.finished_at = Some(Self::current_timestamp_secs());
            if let Some(buffered_path) = saved_path {
                item.path = Some(PortablePath::from(buffered_path.clone()));
                if let Some(name) = buffered_path.file_name() {
                    item.file_name = name.to_string_lossy().into_owned();
                }
            }
            item.error = if success {
                None
            } else {
                Some("Download failed".to_string())
            };
        }
    }

    pub fn downloads_snapshot(&self) -> Vec<DownloadItem> {
        self.downloads.clone()
    }

    pub fn active_download_count(&self) -> usize {
        self.downloads
            .iter()
            .filter(|item| item.status == DownloadStatus::InProgress)
            .count()
    }

    pub fn clear_download_history(&mut self) {
        self.downloads.clear();
        self.next_download_id = 1;
    }

    fn visit_history_path(&self) -> PathBuf {
        self.config.data_dir.join("visit_history.json")
    }

    pub fn load_visit_history(&mut self) -> HiWaveResult<bool> {
        let path = self.visit_history_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => {
                // An I/O failure says nothing about the content: leave the file, refuse saves.
                self.blocked_state_files.insert("visit_history.json");
                tracing::warn!("The visit history could not be read ({}); not saving it this run", err);
                return Err(err.into());
            }
        };
        let parsed = std::str::from_utf8(&bytes)
            .map_err(|err| err.to_string())
            .and_then(|text| serde_json::from_str::<VisitHistorySnapshot>(text).map_err(|err| err.to_string()));
        match parsed {
            Ok(snapshot) => {
                self.visit_history = snapshot.records;
                self.next_visit_id = snapshot.next_id.max(1);
                sweep_stale_temps(&path);
                Ok(true)
            }
            Err(err) => {
                // Keep the unreadable history aside so the next save does not erase it.
                match keep_aside(&path) {
                    Ok(kept) => tracing::warn!(
                        "Invalid visit history data ({}); kept as {} and starting empty",
                        err,
                        kept.display()
                    ),
                    Err(rename_err) => {
                        self.blocked_state_files.insert("visit_history.json");
                        tracing::warn!(
                            "Invalid visit history data ({}) and it could not be moved aside ({}); not saving it this run",
                            err,
                            rename_err
                        );
                    }
                }
                Ok(false)
            }
        }
    }

    pub fn save_visit_history(&self) -> HiWaveResult<()> {
        self.refuse_if_blocked("visit_history.json")?;
        let snapshot = VisitHistorySnapshot {
            records: self.visit_history.clone(),
            next_id: self.next_visit_id,
        };
        let data = serde_json::to_string_pretty(&snapshot).map_err(|err| {
            hiwave_core::HiWaveError::Config(format!("Failed to save visit history: {}", err))
        })?;
        write_atomic(&self.visit_history_path(), data.as_bytes())?;
        Ok(())
    }

    /// Return a sorted snapshot of visit history (newest first)
    pub fn visit_history_snapshot(&self) -> Vec<VisitRecord> {
        let mut snapshot = self.visit_history.clone();
        snapshot.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        snapshot
    }

    fn sanitize_optional_text(value: Option<String>) -> Option<String> {
        value.and_then(|text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub fn record_visit(
        &mut self,
        url: &str,
        title: Option<String>,
        workspace: Option<String>,
    ) -> HiWaveResult<()> {
        let timestamp = Self::current_timestamp_secs();
        let normalized_title = Self::sanitize_optional_text(title);
        let normalized_workspace = Self::sanitize_optional_text(workspace);

        if let Some(record) = self
            .visit_history
            .iter_mut()
            .find(|entry| entry.url == url)
        {
            record.count = record.count.saturating_add(1);
            record.timestamp = timestamp;
            if let Some(title) = normalized_title.clone() {
                record.title = Some(title);
            }
            if normalized_workspace.is_some() {
                record.workspace = normalized_workspace.clone();
            }
        } else {
            let entry = VisitRecord {
                id: self.next_visit_id,
                url: url.to_string(),
                title: normalized_title.clone(),
                workspace: normalized_workspace.clone(),
                timestamp,
                count: 1,
            };
            self.next_visit_id = self.next_visit_id.saturating_add(1);
            self.visit_history.push(entry);
        }

        self.visit_history
            .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        self.save_visit_history()
    }

    pub fn clear_visit_history(&mut self) -> HiWaveResult<()> {
        self.visit_history.clear();
        self.next_visit_id = 1;
        self.save_visit_history()
    }

    #[allow(dead_code)]
    fn suggested_download_name(url: &str, idx: u64) -> String {
        if let Ok(parsed) = Url::parse(url) {
            if let Some(name) = parsed
                .path_segments()
                .and_then(|segments| segments.filter(|seg| !seg.is_empty()).last())
            {
                return name.to_string();
            }
            if let Some(host) = parsed.host_str() {
                return host.to_string();
            }
        }
        format!("download-{}", idx)
    }

    #[allow(dead_code)]
    fn unique_file_path(dir: &Path, file_name: &str) -> PathBuf {
        let mut candidate = dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
        let (base, extension) = match file_name.rfind('.') {
            Some(idx) if idx > 0 => (file_name[..idx].to_string(), file_name[idx..].to_string()),
            _ => (file_name.to_string(), String::new()),
        };
        let mut counter = 1;
        loop {
            let next_name = if extension.is_empty() {
                format!("{}-{}", base, counter)
            } else {
                format!("{}-{}{}", base, counter, extension)
            };
            candidate = dir.join(&next_name);
            if !candidate.exists() {
                return candidate;
            }
            counter += 1;
        }
    }

    pub fn shelf_items_for_active_workspace(&self) -> Vec<ShelfItem> {
        let name = self.shell.get_active_workspace().map(|ws| ws.name.clone());
        if let Some(name) = name {
            self.shelf_items_for_workspace(&name)
        } else {
            Vec::new()
        }
    }

    pub fn shelf_items_for_workspace(&self, workspace_name: &str) -> Vec<ShelfItem> {
        self.shelves
            .get(workspace_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn shelf_items_all(&self) -> Vec<ShelfItem> {
        let mut items: Vec<ShelfItem> = self
            .shelves
            .values()
            .flat_map(|list| list.iter().cloned())
            .collect();
        items.sort_by_key(|item| item.added_at);
        items
    }

    pub fn shelf_count_all(&self) -> usize {
        self.shelves.values().map(|list| list.len()).sum()
    }

    pub fn add_to_shelf(&mut self, tab_id: TabId) -> HiWaveResult<Option<ShelfItem>> {
        let tab = match self.shell.get_tab(tab_id) {
            Some(tab) => tab.clone(),
            None => return Ok(None),
        };
        let workspace_name = self
            .shell
            .get_workspace(tab.workspace_id)
            .map(|ws| ws.name.clone())
            .unwrap_or_else(|| "Default".to_string());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let item = ShelfItem {
            id: format!("shelf-{}-{}", tab.id.0, timestamp),
            url: tab.url.to_string(),
            title: tab.title.clone(),
            workspace: workspace_name.clone(),
            added_at: timestamp,
        };
        self.shelves
            .entry(workspace_name.clone())
            .or_default()
            .push(item.clone());
        let _ = self.shell.close_tab(tab_id);

        // Track shelf addition
        if let Ok(parsed_url) = url::Url::parse(&item.url) {
            if let Some(domain) = parsed_url.host_str() {
                let workspace_id = tab.workspace_id.0.to_string();
                let _ = self.analytics.track_tab_to_shelf(domain, Some(&workspace_id));
            }
        }

        Ok(Some(item))
    }

    pub fn restore_from_shelf(
        &mut self,
        id: &str,
        target_workspace_id: Option<WorkspaceId>,
    ) -> HiWaveResult<Option<RestoredShelfItem>> {
        let mut found: Option<ShelfItem> = None;
        let mut source_workspace = None;

        for (workspace, items) in self.shelves.iter_mut() {
            if let Some(index) = items.iter().position(|item| item.id == id) {
                found = Some(items.remove(index));
                source_workspace = Some(workspace.clone());
                break;
            }
        }

        let item = match found {
            Some(item) => item,
            None => return Ok(None),
        };

        let workspace_id = if let Some(ws_id) = target_workspace_id {
            Some(ws_id)
        } else {
            let target_workspace = source_workspace.unwrap_or_else(|| item.workspace.clone());
            self.shell
                .list_workspaces()
                .iter()
                .find(|ws| ws.name == target_workspace)
                .map(|ws| ws.id)
                .or_else(|| self.shell.get_active_workspace().map(|ws| ws.id))
        };

        let workspace_id = match workspace_id {
            Some(id) => id,
            None => return Ok(None),
        };

        let url = url::Url::parse(&item.url)?;
        let tab = hiwave_core::types::TabInfo {
            id: hiwave_core::types::TabId::new(),
            url: url.clone(),
            title: item.title.clone(),
            favicon: None,
            workspace_id,
            suspended: false,
            loading: true,
            locked: false,
            last_visited: Some(Self::current_timestamp_secs()),
        };
        let tab_id = tab.id;
        let _ = self.shell.create_tab(tab);
        let _ = self.shell.set_active_workspace(workspace_id);
        let _ = self.shell.set_active_tab(tab_id);

        // Track shelf restoration
        if let Some(domain) = url.host_str() {
            let workspace_id_str = workspace_id.0.to_string();
            let _ = self.analytics.track_tab_from_shelf(domain, Some(&workspace_id_str));
        }

        Ok(Some(RestoredShelfItem {
            url: url.to_string(),
            workspace_id,
        }))
    }

    pub fn delete_from_shelf(&mut self, id: &str) -> bool {
        for items in self.shelves.values_mut() {
            if let Some(index) = items.iter().position(|item| item.id == id) {
                items.remove(index);
                return true;
            }
        }
        false
    }

    pub fn ensure_workspace_shelf(&mut self, workspace_name: &str) {
        self.shelves.entry(workspace_name.to_string()).or_default();
    }

    pub fn remove_workspace_shelf(&mut self, workspace_name: &str) {
        self.shelves.remove(workspace_name);
    }

    fn sync_shelf_keys(&mut self) {
        let mut workspace_names: Vec<String> = self
            .shell
            .list_workspaces()
            .iter()
            .map(|ws| ws.name.clone())
            .collect();
        workspace_names.sort();

        self.shelves
            .retain(|name, _| workspace_names.contains(name));
        for name in workspace_names {
            self.shelves.entry(name).or_default();
        }
    }

    pub fn lock_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        self.shell.lock_tab(tab_id)
    }

    pub fn unlock_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        self.shell.unlock_tab(tab_id)
    }

    pub fn touch_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        self.shell.touch_tab(tab_id)
    }

    pub fn get_stale_locks(&self, threshold_days: u64) -> Vec<hiwave_core::types::TabInfo> {
        self.shell.stale_locks(threshold_days)
    }

    pub fn workspace_locked_count(&self, workspace_id: WorkspaceId) -> usize {
        self.shell.workspace_locked_count(workspace_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShelfItem {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub workspace: String,
    pub added_at: u64,
}

/// An item that has aged out of the shelf (stored in the cellar for recovery)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellarItem {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    /// Which workspace this originally came from
    pub workspace: String,
    /// When the item was originally added to the shelf
    pub originally_shelved: u64,
    /// When the item moved to cellar (aged out)
    pub cellared_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkspaceStateSnapshot {
    shell: ShellSnapshot,
    shelves: HashMap<String, Vec<ShelfItem>>,
    #[serde(default)]
    cellar: Vec<CellarItem>,
    #[serde(default)]
    downloads: Vec<DownloadItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VisitHistorySnapshot {
    records: Vec<VisitRecord>,
    next_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoredShelfItem {
    pub url: String,
    pub workspace_id: WorkspaceId,
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Data directory for persistent storage
    #[serde(skip)]
    pub data_dir: PathBuf,

    /// Directory where downloads are stored
    #[serde(skip)]
    pub download_dir: PathBuf,

    /// Default search engine
    pub search_engine: String,

    /// Default homepage
    pub homepage: String,

    /// Enable ad blocking
    pub shield_enabled: bool,

    /// Focus mode default duration (minutes)
    pub focus_duration_minutes: u32,

    /// Focus mode break duration (minutes)
    pub focus_break_minutes: u32,

    /// Maximum tabs per workspace
    pub max_tabs_per_workspace: usize,

    /// Maximum workspaces
    pub max_workspaces: usize,

    /// Tab decay timeout (seconds before tab is considered stale)
    pub tab_decay_timeout_secs: u64,

    /// Auto-suspend timeout for workspaces (seconds)
    pub workspace_suspend_timeout_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        // Get data directory
        let data_dir = dirs_or_default();
        let download_dir = data_dir.join("downloads");

        Self {
            data_dir,
            download_dir,
            search_engine: "https://duckduckgo.com/?q=".to_string(),
            homepage: "hiwave://newtab".to_string(),
            shield_enabled: true,
            focus_duration_minutes: 25,
            focus_break_minutes: 5,
            max_tabs_per_workspace: 100,
            max_workspaces: 10,
            tab_decay_timeout_secs: 7 * 24 * 60 * 60, // 7 days
            workspace_suspend_timeout_secs: 300,      // 5 minutes
        }
    }
}

/// Get data directory, with fallback
fn dirs_or_default() -> PathBuf {
    // Try standard data directory
    if let Some(data_dir) = dirs::data_local_dir() {
        return data_dir.join("hiwave");
    }

    // Fallback to current directory
    PathBuf::from(".hiwave")
}

/// User-configurable settings (separate from AppConfig which has system paths)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    // General
    /// Theme: "dark", "light", or "system"
    pub theme: String,
    /// Default mode: "essentials", "balanced", or "zen"
    pub default_mode: String,
    /// Sidebar open by default on startup
    pub sidebar_default_open: bool,
    /// Auto-hide sidebar after N seconds (None = disabled)
    pub sidebar_auto_hide_secs: Option<u32>,
    /// Sidebar width in pixels (default: 220, min: 48, max: 400)
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
    /// Auto-hide URL bar after N seconds (None = disabled)
    pub url_bar_auto_hide_secs: Option<u32>,

    // Tab Behavior
    /// Days before tabs start visually fading (default: 7)
    pub tab_decay_days: u32,
    /// Days before tabs auto-move to shelf in Zen mode (None = disabled)
    pub auto_shelf_days: Option<u32>,
    /// Custom new tab URL (empty = default new tab page)
    pub new_tab_url: String,

    // Shield
    /// Shield (ad/tracker blocking) enabled by default
    pub shield_enabled: bool,

    // Focus Mode
    /// Default focus mode duration in minutes
    pub focus_duration_minutes: u32,
    /// Default focus mode break duration in minutes
    pub focus_break_minutes: u32,

    // Import Display
    /// How to display imported workspace sources: "abbreviation" (BRV:) or "symbol"
    pub import_prefix_style: String,

    // Advanced
    /// Enable developer tools
    pub devtools_enabled: bool,
    /// Workspace save interval in seconds
    pub workspace_save_interval_secs: u32,
}

// Serde default function for sidebar width
fn default_sidebar_width() -> u32 {
    220
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            // General
            theme: "dark".to_string(),
            default_mode: "balanced".to_string(),
            sidebar_default_open: true,
            sidebar_auto_hide_secs: None,
            sidebar_width: 220,
            url_bar_auto_hide_secs: None,

            // Tab Behavior
            tab_decay_days: 7,
            auto_shelf_days: None,
            new_tab_url: String::new(),

            // Shield
            shield_enabled: true,

            // Focus Mode
            focus_duration_minutes: 25,
            focus_break_minutes: 5,

            // Import Display
            import_prefix_style: "abbreviation".to_string(),

            // Advanced
            devtools_enabled: false,
            workspace_save_interval_secs: 30,
        }
    }
}

impl UserSettings {
    /// Load settings from the given path, returning defaults if not found
    pub fn load(path: &Path) -> Self {
        Self::load_checked(path).0
    }

    /// Like `load`, and whether saving over the file must be refused (see `load_json_keep_aside`).
    pub(crate) fn load_checked(path: &Path) -> (Self, bool) {
        let (value, blocked) = load_json_keep_aside(path, "The settings file");
        (value.unwrap_or_default(), blocked)
    }

    /// Save settings to the given path
    pub fn save(&self, path: &Path) -> HiWaveResult<()> {
        let data = serde_json::to_string_pretty(self).map_err(|e| {
            hiwave_core::HiWaveError::Config(format!("Failed to serialize settings: {}", e))
        })?;
        write_atomic(path, data.as_bytes())?;
        Ok(())
    }
}

/// Export format for backing up workspaces and settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiWaveExport {
    /// Export format version
    pub version: String,
    /// Unix timestamp when exported
    pub exported_at: u64,
    /// User settings (if included)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<UserSettings>,
    /// Workspaces with their tabs
    pub workspaces: Vec<WorkspaceExport>,
    /// Shelf items by workspace name
    pub shelf: HashMap<String, Vec<ShelfItem>>,
}

/// Workspace data for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceExport {
    pub name: String,
    pub tabs: Vec<TabExport>,
}

/// Tab data for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabExport {
    pub url: String,
    pub title: Option<String>,
    pub locked: bool,
}

impl HiWaveExport {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            version: "1.0".to_string(),
            exported_at: now,
            settings: None,
            workspaces: Vec::new(),
            shelf: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portable_path_serializes_with_forward_slashes() {
        // Create a path with backslashes (simulating Windows)
        let path = PortablePath::new(PathBuf::from("downloads/subdir/test.pdf"));
        let json = serde_json::to_string(&path).unwrap();

        // Should serialize with forward slashes
        assert_eq!(json, "\"downloads/subdir/test.pdf\"");
    }

    #[test]
    fn test_portable_path_deserializes_correctly() {
        // Deserialize a path with forward slashes
        let json = "\"downloads/subdir/test.pdf\"";
        let path: PortablePath = serde_json::from_str(json).unwrap();

        // Should be a valid path
        assert!(path.to_string_lossy().contains("downloads"));
        assert!(path.to_string_lossy().contains("test.pdf"));
    }

    #[test]
    fn test_portable_path_roundtrip() {
        let original = PortablePath::new(PathBuf::from("some/nested/path/file.txt"));

        // Serialize
        let json = serde_json::to_string(&original).unwrap();

        // Deserialize
        let restored: PortablePath = serde_json::from_str(&json).unwrap();

        // Should match (comparing string representations)
        assert_eq!(
            original.to_string_lossy().replace('\\', "/"),
            restored.to_string_lossy().replace('\\', "/")
        );
    }

    #[test]
    fn test_download_item_with_portable_path() {
        let item = DownloadItem {
            id: 1,
            url: "https://example.com/file.pdf".to_string(),
            file_name: "file.pdf".to_string(),
            status: DownloadStatus::Completed,
            path: Some(PortablePath::new(PathBuf::from("downloads/file.pdf"))),
            progress: Some(1.0),
            started_at: 1000,
            finished_at: Some(2000),
            error: None,
        };

        // Serialize
        let json = serde_json::to_string(&item).unwrap();

        // Path should be serialized with forward slashes
        assert!(json.contains("\"downloads/file.pdf\""));

        // Deserialize
        let restored: DownloadItem = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, 1);
        assert!(restored.path.is_some());
    }

    #[test]
    fn test_download_item_path_string() {
        let item = DownloadItem {
            id: 1,
            url: "https://example.com/file.pdf".to_string(),
            file_name: "file.pdf".to_string(),
            status: DownloadStatus::Completed,
            path: Some(PortablePath::new(PathBuf::from("downloads/file.pdf"))),
            progress: Some(1.0),
            started_at: 1000,
            finished_at: Some(2000),
            error: None,
        };

        let path_str = item.path_string();
        assert!(path_str.is_some());
        assert!(path_str.unwrap().contains("downloads"));
    }

    #[test]
    fn test_portable_path_from_implementations() {
        // From PathBuf
        let p1: PortablePath = PathBuf::from("test/path").into();
        assert!(p1.to_string_lossy().contains("test"));

        // From &Path
        let path = Path::new("test/path");
        let p2: PortablePath = path.into();
        assert!(p2.to_string_lossy().contains("test"));

        // From String
        let p3: PortablePath = String::from("test/path").into();
        assert!(p3.to_string_lossy().contains("test"));
    }

    #[test]
    fn test_download_status_serialization() {
        assert_eq!(
            serde_json::to_string(&DownloadStatus::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Failed).unwrap(),
            "\"failed\""
        );
    }
}

/// M4: the session file must survive a crash, a full disk and an unreadable state. These
/// pin the behaviour; they do not name the fix's internals (see `workspace_state_internals`).
#[cfg(test)]
mod workspace_state_file_tests {
    use super::*;

    fn fresh(name: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!("hiwave_wsfile_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut config = AppConfig::default();
        config.data_dir = dir.clone();
        config.download_dir = dir.join("downloads");
        AppState::new(config).expect("state")
    }

    fn file(s: &AppState) -> PathBuf {
        s.config.data_dir.join("workspace_state.json")
    }

    fn tmp_files(s: &AppState) -> Vec<String> {
        std::fs::read_dir(&s.config.data_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn a_save_leaves_the_file_and_no_temp_file_and_loads_back() {
        let mut s = fresh("roundtrip");
        s.shell.create_workspace("Work".to_string());
        s.save_workspace_state().unwrap();
        assert!(file(&s).exists());
        assert!(tmp_files(&s).is_empty(), "no temp file is left behind: {:?}", tmp_files(&s));
        let mut again = AppState::new(s.config.clone()).unwrap();
        assert!(again.load_workspace_state().unwrap());
        assert_eq!(again.shell.list_workspaces().len(), 2);
    }

    #[test]
    fn an_empty_session_file_is_kept_aside_not_overwritten() {
        let mut s = fresh("empty");
        std::fs::write(file(&s), "").unwrap();
        assert!(s.load_workspace_state().is_err());
        assert!(!file(&s).exists(), "moved out of the way");
        assert!(s.config.data_dir.join("workspace_state.json.corrupt").exists());
        s.save_workspace_state().unwrap();
        assert!(file(&s).exists(), "the first save writes a fresh file");
        assert_eq!(std::fs::read_to_string(s.config.data_dir.join("workspace_state.json.corrupt")).unwrap(), "");
    }

    #[test]
    fn a_corrupt_session_file_keeps_its_bytes_and_each_one_gets_its_own_name() {
        let mut s = fresh("corrupt");
        std::fs::write(file(&s), "{ this is not json").unwrap();
        assert!(s.load_workspace_state().is_err());
        let first = s.config.data_dir.join("workspace_state.json.corrupt");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "{ this is not json");
        std::fs::write(file(&s), "[1, 2").unwrap();
        assert!(s.load_workspace_state().is_err());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "{ this is not json");
        assert_eq!(
            std::fs::read_to_string(s.config.data_dir.join("workspace_state.json.corrupt-1")).unwrap(),
            "[1, 2"
        );
    }

    #[test]
    fn a_file_that_is_not_utf8_is_kept_aside_too() {
        let mut s = fresh("binary");
        std::fs::write(file(&s), [0xff, 0xfe, 0x00, 0x9f]).unwrap();
        assert!(s.load_workspace_state().is_err());
        assert_eq!(
            std::fs::read(s.config.data_dir.join("workspace_state.json.corrupt")).unwrap(),
            vec![0xff, 0xfe, 0x00, 0x9f]
        );
    }

    #[test]
    fn a_truncated_real_snapshot_is_kept_aside_with_its_bytes() {
        let mut s = fresh("truncated");
        s.shell.create_workspace("Work".to_string());
        s.save_workspace_state().unwrap();
        let whole = std::fs::read(file(&s)).unwrap();
        let cut = whole[..whole.len() / 2].to_vec();
        std::fs::write(file(&s), &cut).unwrap();
        let mut again = AppState::new(s.config.clone()).unwrap();
        assert!(again.load_workspace_state().is_err());
        assert_eq!(std::fs::read(s.config.data_dir.join("workspace_state.json.corrupt")).unwrap(), cut);
    }

    #[test]
    fn a_missing_file_is_not_an_error_and_a_good_one_is_not_moved() {
        let mut s = fresh("missing");
        assert!(!s.load_workspace_state().unwrap());
        s.save_workspace_state().unwrap();
        let before = std::fs::read(file(&s)).unwrap();
        assert!(s.load_workspace_state().unwrap());
        assert_eq!(std::fs::read(file(&s)).unwrap(), before, "a readable file is left as it was");
        assert!(!s.config.data_dir.join("workspace_state.json.corrupt").exists());
    }

    #[test]
    fn the_legacy_shell_only_snapshot_still_loads() {
        let mut s = fresh("legacy");
        s.shell.create_workspace("Legacy".to_string());
        std::fs::write(file(&s), serde_json::to_string(&s.shell.snapshot()).unwrap()).unwrap();
        let mut again = AppState::new(s.config.clone()).unwrap();
        assert!(again.load_workspace_state().unwrap());
        assert_eq!(again.shell.list_workspaces().len(), 2);
        assert!(!s.config.data_dir.join("workspace_state.json.corrupt").exists());
    }

    #[test]
    fn a_read_error_keeps_the_file_and_blocks_saving() {
        // A directory where the file should be: metadata succeeds, reading fails with an I/O
        // error that says nothing about the content. Nothing may be moved or overwritten.
        let mut s = fresh("read_error");
        std::fs::create_dir_all(file(&s)).unwrap();
        assert!(s.load_workspace_state().is_err());
        assert!(file(&s).is_dir(), "the session path is left where it was");
        assert!(!s.config.data_dir.join("workspace_state.json.corrupt").exists(), "not treated as corruption");
        assert!(s.workspace_state_save_blocked, "the read error itself blocked saving");
        assert!(s.save_workspace_state().is_err(), "and nothing is saved over it");
    }

    #[test]
    fn a_save_that_cannot_create_its_temp_file_leaves_the_good_file() {
        let mut s = fresh("failed_save");
        s.shell.create_workspace("Kept".to_string());
        s.save_workspace_state().unwrap();
        let good = std::fs::read(file(&s)).unwrap();
        // The temp file cannot be created: a directory sits at its name.
        let tmp = s.config.data_dir.join(format!("workspace_state.json.{}.tmp", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        s.shell.create_workspace("Lost".to_string());
        assert!(s.save_workspace_state().is_err());
        assert_eq!(std::fs::read(file(&s)).unwrap(), good, "the old file is untouched");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn a_save_whose_rename_fails_removes_its_temp_file() {
        // A directory where the session file should be: the temp file is written and synced,
        // then the rename over a directory fails. The temp file must not be left behind.
        let mut s = fresh("rename_cleanup");
        std::fs::create_dir_all(file(&s)).unwrap();
        assert!(s.save_workspace_state().is_err());
        assert!(file(&s).is_dir(), "what was at the path is untouched");
        assert!(tmp_files(&s).is_empty(), "no temp file left after the failed rename: {:?}", tmp_files(&s));
    }

    #[cfg(windows)]
    #[test]
    fn when_the_rename_aside_fails_saving_is_refused_and_the_file_survives() {
        use std::os::windows::fs::OpenOptionsExt;
        let mut s = fresh("rename_fails");
        std::fs::write(file(&s), "{ truncated").unwrap();
        // FILE_SHARE_READ only: readable (so the content is judged corrupt) but not renamable.
        let hold = std::fs::OpenOptions::new().read(true).share_mode(1).open(file(&s)).unwrap();
        assert!(s.load_workspace_state().is_err());
        assert!(file(&s).exists(), "the rename failed, the file is where it was");
        assert!(!s.config.data_dir.join("workspace_state.json.corrupt").exists());
        assert!(s.workspace_state_save_blocked, "the failed rename itself blocked saving");
        assert!(s.save_workspace_state().is_err(), "and nothing may overwrite it");
        drop(hold);
        assert_eq!(std::fs::read_to_string(file(&s)).unwrap(), "{ truncated");
    }

    /// The unix twin of `when_the_rename_aside_fails_saving_is_refused_and_the_file_survives`:
    /// a read-only data directory (mode 0555) lets the corrupt file be read but not renamed.
    /// Not run on the box that wrote it (Windows); it runs on macOS and Linux.
    #[cfg(unix)]
    #[test]
    fn when_the_rename_aside_fails_on_unix_saving_is_refused_and_the_file_survives() {
        use std::os::unix::fs::PermissionsExt;
        let mut s = fresh("rename_fails_unix");
        std::fs::write(file(&s), "{ truncated").unwrap();
        let dir = s.config.data_dir.clone();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = s.load_workspace_state();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Running as root ignores the mode and the rename works; then there is nothing to pin.
        if file(&s).exists() {
            assert!(result.is_err());
            assert!(s.workspace_state_save_blocked, "the failed rename itself blocked saving");
            assert!(s.save_workspace_state().is_err(), "and nothing may overwrite it");
            assert_eq!(std::fs::read_to_string(file(&s)).unwrap(), "{ truncated");
        }
    }

    fn old_tmp(s: &AppState, name: &str) -> PathBuf {
        let p = s.config.data_dir.join(name);
        std::fs::write(&p, "{}").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::OpenOptions::new().write(true).open(&p).unwrap().set_modified(old).unwrap();
        p
    }

    #[test]
    fn a_stale_temp_file_is_swept_only_after_the_session_file_read_fine() {
        let mut s = fresh("sweep_ok");
        s.save_workspace_state().unwrap();
        let stale = old_tmp(&s, "workspace_state.json.4242.tmp");
        let recent = s.config.data_dir.join("workspace_state.json.4343.tmp");
        std::fs::write(&recent, "{}").unwrap();
        let other = old_tmp(&s, "something_else.4242.tmp");
        let mut again = AppState::new(s.config.clone()).unwrap();
        assert!(again.load_workspace_state().unwrap());
        assert!(!stale.exists(), "swept after a good read");
        assert!(recent.exists(), "a recent one may belong to a live instance");
        assert!(other.exists(), "another file's temp is not ours to remove");
    }

    #[test]
    fn a_temp_file_is_kept_when_the_session_file_is_missing_or_unreadable() {
        // A crash between the sync and the rename leaves a COMPLETE temp file and, the first
        // time, no session file at all: that temp may be the best copy left.
        let mut missing = fresh("sweep_missing");
        let tmp = old_tmp(&missing, "workspace_state.json.4242.tmp");
        assert!(!missing.load_workspace_state().unwrap());
        assert!(tmp.exists(), "no session file: the temp file stays");

        let mut corrupt = fresh("sweep_corrupt");
        std::fs::write(corrupt.config.data_dir.join("workspace_state.json"), "{ truncated").unwrap();
        let tmp = old_tmp(&corrupt, "workspace_state.json.4242.tmp");
        assert!(corrupt.load_workspace_state().is_err());
        assert!(tmp.exists(), "unreadable session file: the temp file stays");
    }
}

/// M4b item 3: the four other small state files (`settings.json`, `focus_mode.json`,
/// `user-blocklist.json`, `visit_history.json`) were plain `fs::write` with loaders that silently
/// fell back to defaults, so one bad byte erased the user's settings on the next save.
#[cfg(test)]
mod other_state_files_tests {
    use super::*;

    fn config(name: &str) -> AppConfig {
        let dir = std::env::temp_dir().join(format!("hiwave_otherfiles_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = AppConfig::default();
        config.data_dir = dir.clone();
        config.download_dir = dir.join("downloads");
        config
    }

    fn tmp_files(config: &AppConfig) -> Vec<String> {
        std::fs::read_dir(&config.data_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    /// Each file with unparsable content is kept aside, and the app starts with defaults.
    fn assert_kept_aside(config: &AppConfig, file: &str) {
        let kept = config.data_dir.join(format!("{file}.corrupt"));
        assert_eq!(std::fs::read(&kept).unwrap(), b"{ not json", "{file}: the bytes are kept");
        assert!(!config.data_dir.join(file).exists(), "{file}: moved out of the way, not overwritten");
    }

    #[test]
    fn an_unparsable_settings_file_is_kept_aside() {
        let config = config("settings");
        std::fs::write(config.data_dir.join("settings.json"), "{ not json").unwrap();
        let s = AppState::new(config.clone()).unwrap();
        assert_eq!(s.user_settings.theme, UserSettings::default().theme);
        assert_kept_aside(&config, "settings.json");
    }

    #[test]
    fn an_unparsable_focus_mode_file_is_kept_aside() {
        let config = config("focus");
        std::fs::write(config.data_dir.join("focus_mode.json"), "{ not json").unwrap();
        let _ = AppState::new(config.clone()).unwrap();
        assert_kept_aside(&config, "focus_mode.json");
    }

    #[test]
    fn an_unparsable_blocklist_file_is_kept_aside() {
        let config = config("blocklist");
        std::fs::write(config.data_dir.join("user-blocklist.json"), "{ not json").unwrap();
        let s = AppState::new(config.clone()).unwrap();
        assert!(s.user_blocklist.domains.is_empty());
        assert_kept_aside(&config, "user-blocklist.json");
    }

    #[test]
    fn an_unparsable_visit_history_file_is_kept_aside() {
        let config = config("history");
        std::fs::write(config.data_dir.join("visit_history.json"), "{ not json").unwrap();
        let mut s = AppState::new(config.clone()).unwrap();
        assert!(!s.load_visit_history().unwrap());
        assert_kept_aside(&config, "visit_history.json");
    }

    #[test]
    fn a_non_utf8_visit_history_is_kept_aside_not_overwritten() {
        // read_to_string reports invalid UTF-8 as an I/O error, so the history used to leave
        // through `?` without being kept aside, and the next save overwrote it (Prometheus R1 on
        // #1018, nit 1).
        let config = config("history_binary");
        std::fs::write(config.data_dir.join("visit_history.json"), [0xff, 0xfe, 0x00, 0x9f]).unwrap();
        let mut s = AppState::new(config.clone()).unwrap();
        assert!(!s.load_visit_history().unwrap(), "starts empty without an error");
        assert_eq!(
            std::fs::read(config.data_dir.join("visit_history.json.corrupt")).unwrap(),
            vec![0xff, 0xfe, 0x00, 0x9f],
            "the bytes are kept"
        );
        assert!(!config.data_dir.join("visit_history.json").exists(), "moved out of the way");
    }

    #[test]
    fn the_four_saves_leave_the_file_and_no_temp_file() {
        let config = config("saves");
        let mut s = AppState::new(config.clone()).unwrap();
        s.user_blocklist.add_domain("blocked.test".to_string());
        s.user_blocklist.save(&s.blocklist_path()).unwrap();
        s.save_settings().unwrap();
        s.save_focus_config().unwrap();
        s.save_visit_history().unwrap();
        for file in ["user-blocklist.json", "settings.json", "focus_mode.json", "visit_history.json"] {
            assert!(config.data_dir.join(file).exists(), "{file} was written");
        }
        assert!(tmp_files(&config).is_empty(), "no temp file left: {:?}", tmp_files(&config));
        // And they read back.
        let again = AppState::new(config.clone()).unwrap();
        assert!(again.user_blocklist.domains.contains("blocked.test"));
    }

    #[test]
    fn a_save_that_cannot_create_its_temp_file_leaves_the_good_settings() {
        let config = config("settings_tmp");
        let s = AppState::new(config.clone()).unwrap();
        s.save_settings().unwrap();
        let good = std::fs::read(config.data_dir.join("settings.json")).unwrap();
        let tmp = config.data_dir.join(format!("settings.json.{}.tmp", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(s.save_settings().is_err());
        assert_eq!(std::fs::read(config.data_dir.join("settings.json")).unwrap(), good);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// Nit 2: a read I/O error on a small file must refuse saves for the run (not just load
    /// defaults and then overwrite). A directory sitting where the file should be makes the read
    /// fail with an I/O error on every OS.
    #[test]
    fn a_read_error_on_each_small_file_refuses_its_saves() {
        for file in ["settings.json", "focus_mode.json", "user-blocklist.json", "visit_history.json"] {
            let config = config(&format!("blocked_{file}"));
            std::fs::create_dir_all(config.data_dir.join(file)).unwrap();
            let mut s = AppState::new(config.clone()).unwrap();
            if file == "visit_history.json" {
                assert!(s.load_visit_history().is_err(), "{file}: the read error is reported");
            }
            assert!(s.blocked_state_files.contains(file), "{file}: saving is refused for the run");
            let refused = match file {
                "settings.json" => s.save_settings(),
                "focus_mode.json" => s.save_focus_config(),
                "user-blocklist.json" => s.block_domain("x.test".to_string()).map(|_| ()),
                _ => s.save_visit_history(),
            };
            assert!(refused.is_err(), "{file}: the save is refused");
            assert!(config.data_dir.join(file).is_dir(), "{file}: what was there is untouched");
            assert!(!config.data_dir.join(format!("{file}.corrupt")).exists(), "{file}: not treated as corruption");
        }
    }

    #[test]
    fn a_good_small_file_is_not_blocked() {
        let config = config("not_blocked");
        let mut s = AppState::new(config.clone()).unwrap();
        s.save_settings().unwrap();
        s.save_focus_config().unwrap();
        assert!(s.block_domain("fine.test".to_string()).unwrap());
        s.save_visit_history().unwrap();
        assert!(s.blocked_state_files.is_empty());
    }

    /// Nit 2 again, on a real unreadable file (unix: mode 000). The data directory stays
    /// writable, so a save WOULD replace the file; only the block keeps it.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_small_file_is_not_overwritten_by_a_later_save() {
        use std::os::unix::fs::PermissionsExt;
        let config = config("unix_unreadable");
        let path = config.data_dir.join("settings.json");
        std::fs::write(&path, r#"{"theme":"light"}"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let s = AppState::new(config.clone()).unwrap();
        // Running as root can read it anyway; then there is nothing to pin.
        if s.blocked_state_files.contains("settings.json") {
            assert!(s.save_settings().is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"theme":"light"}"#);
        }
    }
}
