//! Common types used throughout HiWave

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

/// Unique identifier for a tab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

/// Unique identifier for a workspace
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub u64);

/// Unique identifier for a DOM node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Browser tab metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: TabId,
    pub url: Url,
    pub title: Option<String>,
    pub favicon: Option<Url>,
    pub workspace_id: WorkspaceId,
    pub suspended: bool,
    pub loading: bool,
    pub locked: bool,
    pub last_visited: Option<u64>,
}

/// Workspace metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
    pub name: String,
    pub tabs: Vec<TabId>,
    pub active_tab: Option<TabId>,
    pub suspended: bool,
}

/// Navigation request
#[derive(Debug, Clone)]
pub struct NavigationRequest {
    pub url: Url,
    pub referrer: Option<Url>,
    pub headers: HashMap<String, String>,
}

/// Focus session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSession {
    pub id: u64,
    pub workspace_id: WorkspaceId,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub duration_minutes: u32,
    pub completed: bool,
}

/// Browsing analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowsingStats {
    pub domain: String,
    pub time_spent_seconds: u64,
    pub visit_count: u32,
    pub last_visit: u64,
}

impl TabId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl WorkspaceId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl NodeId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for TabId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}
