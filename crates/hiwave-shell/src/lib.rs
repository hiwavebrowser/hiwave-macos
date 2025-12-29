//! Browser UI shell - tabs, workspaces, command palette

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use hiwave_core::{
    types::{TabId, TabInfo, WorkspaceId, WorkspaceInfo},
    HiWaveResult,
};

pub struct BrowserShell {
    tabs: HashMap<TabId, TabInfo>,
    workspaces: HashMap<WorkspaceId, WorkspaceInfo>,
    active_workspace: Option<WorkspaceId>,
    command_palette: CommandPalette,
}

impl BrowserShell {
    pub fn new() -> Self {
        log::info!("Initializing browser shell");

        let mut shell = Self {
            tabs: HashMap::new(),
            workspaces: HashMap::new(),
            active_workspace: None,
            command_palette: CommandPalette::new(),
        };

        // Create default workspace
        let default_ws = shell.create_workspace("Default".to_string());
        shell.set_active_workspace(default_ws).ok();

        shell
    }

    fn current_timestamp_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }

    // Workspace management
    pub fn create_workspace(&mut self, name: String) -> WorkspaceId {
        let id = WorkspaceId::new();

        let workspace = WorkspaceInfo {
            id,
            name,
            tabs: Vec::new(),
            active_tab: None,
            suspended: false,
        };

        log::info!("Created workspace {:?}: {}", id, workspace.name);
        self.workspaces.insert(id, workspace);
        id
    }

    pub fn delete_workspace(&mut self, id: WorkspaceId) -> HiWaveResult<()> {
        if self.workspaces.len() <= 1 {
            return Err(hiwave_core::HiWaveError::Config(
                "Cannot delete the last workspace".to_string(),
            ));
        }

        if let Some(workspace) = self.workspaces.remove(&id) {
            log::info!("Deleted workspace {:?}: {}", id, workspace.name);

            // Close all tabs in this workspace
            for tab_id in workspace.tabs {
                self.tabs.remove(&tab_id);
            }

            // Switch to another workspace if this was active
            if self.active_workspace == Some(id) {
                if let Some(&new_active) = self.workspaces.keys().next() {
                    self.set_active_workspace(new_active)?;
                }
            }
        }

        Ok(())
    }

    pub fn rename_workspace(&mut self, id: WorkspaceId, new_name: String) -> HiWaveResult<()> {
        if let Some(workspace) = self.workspaces.get_mut(&id) {
            log::info!(
                "Renaming workspace {:?} from '{}' to '{}'",
                id,
                workspace.name,
                new_name
            );
            workspace.name = new_name;
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::Config(
                "Workspace not found".to_string(),
            ))
        }
    }

    pub fn set_active_workspace(&mut self, id: WorkspaceId) -> HiWaveResult<()> {
        if !self.workspaces.contains_key(&id) {
            return Err(hiwave_core::HiWaveError::Config(
                "Workspace not found".to_string(),
            ));
        }

        log::info!("Switching to workspace {:?}", id);

        // Suspend previous workspace
        if let Some(prev_id) = self.active_workspace {
            if let Some(prev_ws) = self.workspaces.get_mut(&prev_id) {
                prev_ws.suspended = true;
            }
        }

        // Activate new workspace
        if let Some(workspace) = self.workspaces.get_mut(&id) {
            workspace.suspended = false;
        }

        self.active_workspace = Some(id);
        Ok(())
    }

    pub fn get_active_workspace(&self) -> Option<&WorkspaceInfo> {
        self.active_workspace
            .and_then(|id| self.workspaces.get(&id))
    }

    pub fn get_workspace(&self, id: WorkspaceId) -> Option<&WorkspaceInfo> {
        self.workspaces.get(&id)
    }

    pub fn list_workspaces(&self) -> Vec<&WorkspaceInfo> {
        self.workspaces.values().collect()
    }

    // Tab management
    pub fn create_tab(&mut self, tab: TabInfo) -> HiWaveResult<TabId> {
        let tab_id = tab.id;
        let workspace_id = tab.workspace_id;

        log::info!("Creating tab {:?} in workspace {:?}", tab_id, workspace_id);

        self.tabs.insert(tab_id, tab);

        // Add to workspace
        if let Some(workspace) = self.workspaces.get_mut(&workspace_id) {
            workspace.tabs.push(tab_id);
            if workspace.active_tab.is_none() {
                workspace.active_tab = Some(tab_id);
            }
        }

        Ok(tab_id)
    }

    pub fn close_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        if let Some(tab) = self.tabs.remove(&tab_id) {
            log::info!("Closing tab {:?}", tab_id);

            // Remove from workspace
            if let Some(workspace) = self.workspaces.get_mut(&tab.workspace_id) {
                workspace.tabs.retain(|&id| id != tab_id);

                // Update active tab if necessary
                if workspace.active_tab == Some(tab_id) {
                    workspace.active_tab = workspace.tabs.first().copied();
                }
            }
        }

        Ok(())
    }

    pub fn set_active_tab(&mut self, tab_id: TabId) -> HiWaveResult<Option<&TabInfo>> {
        // Find which workspace this tab belongs to and update last_visited
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or_else(|| hiwave_core::HiWaveError::Config("Tab not found".to_string()))?;

        let workspace_id = tab.workspace_id;

        // Touch last_visited when activating a tab
        tab.last_visited = Some(Self::current_timestamp_secs());

        // Update the workspace's active tab
        if let Some(workspace) = self.workspaces.get_mut(&workspace_id) {
            log::info!(
                "Setting active tab {:?} in workspace {:?}",
                tab_id,
                workspace_id
            );
            workspace.active_tab = Some(tab_id);
        }

        Ok(self.tabs.get(&tab_id))
    }

    pub fn lock_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.locked = true;
            tab.last_visited = Some(Self::current_timestamp_secs());
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::Config("Tab not found".to_string()))
        }
    }

    pub fn unlock_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.locked = false;
            tab.last_visited = Some(Self::current_timestamp_secs());
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::Config("Tab not found".to_string()))
        }
    }

    pub fn touch_tab(&mut self, tab_id: TabId) -> HiWaveResult<()> {
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.last_visited = Some(Self::current_timestamp_secs());
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::Config("Tab not found".to_string()))
        }
    }

    pub fn stale_locks(&self, threshold_days: u64) -> Vec<TabInfo> {
        let cutoff_secs = threshold_days.saturating_mul(86400);
        let now = Self::current_timestamp_secs();
        self.tabs
            .values()
            .filter(|tab| tab.locked)
            .filter(|tab| {
                tab.last_visited
                    .map(|last| now.saturating_sub(last) >= cutoff_secs)
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Calculate decay level for a tab (0 = fresh, 1-4 = progressively decayed, 5 = expired)
    /// Based on percentage of decay_days elapsed since last visit
    pub fn calculate_decay_level(last_visited: Option<u64>, decay_days: u32) -> u8 {
        let now = Self::current_timestamp_secs();
        let decay_secs = (decay_days as u64).saturating_mul(86400);

        if decay_secs == 0 {
            return 0;
        }

        let elapsed = match last_visited {
            Some(last) => now.saturating_sub(last),
            None => decay_secs, // Treat tabs without last_visited as fully decayed
        };

        // Calculate decay level (0-5) based on percentage of decay threshold
        let percentage = (elapsed as f64 / decay_secs as f64).min(1.0);

        match percentage {
            p if p <= 0.0 => 0,    // Fresh
            p if p < 0.2 => 1,     // Slightly decayed
            p if p < 0.4 => 2,     // Moderately decayed
            p if p < 0.6 => 3,     // Significantly decayed
            p if p < 0.8 => 4,     // Heavily decayed
            _ => 5,                // Expired (>= 80% of threshold)
        }
    }

    /// Get all tabs with their decay levels for a workspace
    pub fn tabs_with_decay(&self, workspace_id: Option<WorkspaceId>, decay_days: u32) -> Vec<(TabInfo, u8)> {
        self.tabs
            .values()
            .filter(|tab| workspace_id.is_none() || tab.workspace_id == workspace_id.unwrap())
            .map(|tab| {
                let decay_level = Self::calculate_decay_level(tab.last_visited, decay_days);
                (tab.clone(), decay_level)
            })
            .collect()
    }

    /// Get tabs that have reached maximum decay (level 5) and are candidates for auto-shelving
    pub fn expired_tabs(&self, workspace_id: Option<WorkspaceId>, decay_days: u32) -> Vec<TabInfo> {
        self.tabs
            .values()
            .filter(|tab| workspace_id.is_none() || tab.workspace_id == workspace_id.unwrap())
            .filter(|tab| !tab.locked) // Don't expire locked tabs
            .filter(|tab| Self::calculate_decay_level(tab.last_visited, decay_days) >= 5)
            .cloned()
            .collect()
    }

    pub fn workspace_locked_count(&self, workspace_id: WorkspaceId) -> usize {
        self.tabs
            .values()
            .filter(|tab| tab.workspace_id == workspace_id && tab.locked)
            .count()
    }

    pub fn move_tab_to_workspace(
        &mut self,
        tab_id: TabId,
        target_workspace: WorkspaceId,
    ) -> HiWaveResult<()> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or_else(|| hiwave_core::HiWaveError::Config("Tab not found".to_string()))?;

        if tab.workspace_id == target_workspace {
            return Ok(());
        }

        if !self.workspaces.contains_key(&target_workspace) {
            return Err(hiwave_core::HiWaveError::Config(
                "Target workspace not found".to_string(),
            ));
        }

        let previous_workspace = tab.workspace_id;
        tab.workspace_id = target_workspace;

        if let Some(workspace) = self.workspaces.get_mut(&previous_workspace) {
            workspace.tabs.retain(|&id| id != tab_id);
            if workspace.active_tab == Some(tab_id) {
                workspace.active_tab = workspace.tabs.first().copied();
            }
        }

        if let Some(workspace) = self.workspaces.get_mut(&target_workspace) {
            workspace.tabs.push(tab_id);
            workspace.active_tab = Some(tab_id);
        }

        Ok(())
    }

    pub fn get_active_tab(&self) -> Option<&TabInfo> {
        self.active_workspace
            .and_then(|ws_id| self.workspaces.get(&ws_id))
            .and_then(|ws| ws.active_tab)
            .and_then(|tab_id| self.tabs.get(&tab_id))
    }

    pub fn get_tab(&self, id: TabId) -> Option<&TabInfo> {
        self.tabs.get(&id)
    }

    pub fn get_tab_mut(&mut self, id: TabId) -> Option<&mut TabInfo> {
        self.tabs.get_mut(&id)
    }

    /// Update a tab's URL
    pub fn update_tab_url(&mut self, id: TabId, url: url::Url) -> HiWaveResult<()> {
        if let Some(tab) = self.tabs.get_mut(&id) {
            log::info!("Updating tab {:?} URL to: {}", id, url);
            tab.url = url;
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::Config("Tab not found".to_string()))
        }
    }

    pub fn list_tabs(&self, workspace_id: Option<WorkspaceId>) -> Vec<&TabInfo> {
        if let Some(ws_id) = workspace_id {
            self.tabs
                .values()
                .filter(|tab| tab.workspace_id == ws_id)
                .collect()
        } else {
            self.tabs.values().collect()
        }
    }

    pub fn snapshot(&self) -> ShellSnapshot {
        let mut workspaces: Vec<&WorkspaceInfo> = self.workspaces.values().collect();
        workspaces.sort_by_key(|ws| ws.id.0);

        let active_workspace_index = self
            .active_workspace
            .and_then(|active_id| workspaces.iter().position(|ws| ws.id == active_id));

        let workspace_snapshots = workspaces
            .iter()
            .map(|ws| {
                let mut tabs = Vec::new();
                for tab_id in &ws.tabs {
                    if let Some(tab) = self.tabs.get(tab_id) {
                        tabs.push(TabSnapshot {
                            url: tab.url.to_string(),
                            title: tab.title.clone(),
                            locked: tab.locked,
                            last_visited: tab.last_visited,
                        });
                    }
                }

                let active_tab_index = ws
                    .active_tab
                    .and_then(|tab_id| ws.tabs.iter().position(|id| *id == tab_id));

                WorkspaceSnapshot {
                    name: ws.name.clone(),
                    tabs,
                    active_tab_index,
                }
            })
            .collect();

        ShellSnapshot {
            workspaces: workspace_snapshots,
            active_workspace_index,
        }
    }

    pub fn load_snapshot(&mut self, snapshot: ShellSnapshot) -> HiWaveResult<()> {
        self.tabs.clear();
        self.workspaces.clear();
        self.active_workspace = None;

        let mut workspace_ids = Vec::new();

        for workspace_snapshot in snapshot.workspaces {
            let workspace_id = WorkspaceId::new();
            workspace_ids.push(workspace_id);

            let mut workspace = WorkspaceInfo {
                id: workspace_id,
                name: workspace_snapshot.name,
                tabs: Vec::new(),
                active_tab: None,
                suspended: false,
            };

            for tab_snapshot in workspace_snapshot.tabs {
                let tab_id = TabId::new();
                let url = url::Url::parse(&tab_snapshot.url)
                    .unwrap_or_else(|_| url::Url::parse("about:blank").unwrap());
                let tab_info = TabInfo {
                    id: tab_id,
                    url,
                    title: tab_snapshot.title,
                    favicon: None,
                    workspace_id,
                    suspended: false,
                    loading: false,
                    locked: tab_snapshot.locked,
                    last_visited: tab_snapshot.last_visited,
                };
                self.tabs.insert(tab_id, tab_info);
                workspace.tabs.push(tab_id);
            }

            if let Some(index) = workspace_snapshot.active_tab_index {
                if index < workspace.tabs.len() {
                    workspace.active_tab = Some(workspace.tabs[index]);
                }
            }

            if workspace.active_tab.is_none() {
                workspace.active_tab = workspace.tabs.first().copied();
            }

            self.workspaces.insert(workspace_id, workspace);
        }

        if let Some(index) = snapshot.active_workspace_index {
            if index < workspace_ids.len() {
                self.active_workspace = Some(workspace_ids[index]);
            }
        }

        if self.active_workspace.is_none() {
            self.active_workspace = workspace_ids.first().copied();
        }

        if self.workspaces.is_empty() {
            let default_ws = self.create_workspace("Default".to_string());
            self.set_active_workspace(default_ws).ok();
        }

        Ok(())
    }

    // Command palette
    pub fn search_commands(&self, query: &str) -> Vec<Command> {
        self.command_palette.search(query)
    }

    pub fn register_command(&mut self, command: Command) {
        self.command_palette.register(command);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub locked: bool,
    pub last_visited: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub name: String,
    pub tabs: Vec<TabSnapshot>,
    pub active_tab_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSnapshot {
    pub workspaces: Vec<WorkspaceSnapshot>,
    pub active_workspace_index: Option<usize>,
}

impl Default for BrowserShell {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub description: String,
    pub shortcut: Option<String>,
    pub category: CommandCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandCategory {
    Navigation,
    Workspace,
    Tab,
    Focus,
    Settings,
}

pub struct CommandPalette {
    commands: Vec<Command>,
    matcher: SkimMatcherV2,
}

impl CommandPalette {
    fn new() -> Self {
        let mut palette = Self {
            commands: Vec::new(),
            matcher: SkimMatcherV2::default(),
        };

        // Register default commands
        palette.register_defaults();
        palette
    }

    fn register_defaults(&mut self) {
        let defaults = vec![
            Command {
                id: "new_tab".to_string(),
                name: "New Tab".to_string(),
                description: "Open a new tab".to_string(),
                shortcut: Some("Ctrl+T".to_string()),
                category: CommandCategory::Tab,
            },
            Command {
                id: "close_tab".to_string(),
                name: "Close Tab".to_string(),
                description: "Close the current tab".to_string(),
                shortcut: Some("Ctrl+W".to_string()),
                category: CommandCategory::Tab,
            },
            Command {
                id: "new_workspace".to_string(),
                name: "New Workspace".to_string(),
                description: "Create a new workspace".to_string(),
                shortcut: Some("Ctrl+Shift+N".to_string()),
                category: CommandCategory::Workspace,
            },
            Command {
                id: "focus_mode".to_string(),
                name: "Toggle Focus Mode".to_string(),
                description: "Enable or disable focus mode".to_string(),
                shortcut: Some("Ctrl+Shift+F".to_string()),
                category: CommandCategory::Focus,
            },
        ];

        for command in defaults {
            self.register(command);
        }
    }

    fn register(&mut self, command: Command) {
        self.commands.push(command);
    }

    fn search(&self, query: &str) -> Vec<Command> {
        let mut results: Vec<(i64, Command)> = self
            .commands
            .iter()
            .filter_map(|cmd| {
                self.matcher
                    .fuzzy_match(&cmd.name, query)
                    .map(|score| (score, cmd.clone()))
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.0.cmp(&a.0));

        results.into_iter().map(|(_, cmd)| cmd).take(10).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_workspace() {
        let mut shell = BrowserShell::new();
        let ws = shell.create_workspace("Test".to_string());

        let workspace = shell.get_workspace(ws).unwrap();
        assert_eq!(workspace.name, "Test");
    }

    #[test]
    fn test_command_search() {
        let shell = BrowserShell::new();
        let results = shell.search_commands("new");

        assert!(!results.is_empty());
        assert!(results.iter().any(|c| c.id == "new_tab"));
    }
}
