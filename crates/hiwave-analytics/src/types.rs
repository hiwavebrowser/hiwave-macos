use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event types that can be tracked
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalyticsEvent {
    // Shield events
    TrackerBlocked { domain: String },
    AdBlocked { domain: String },
    PopupBlocked { domain: String },

    // Browsing events
    PageVisit { domain: String },

    // Tab events
    TabOpened,
    TabClosed { duration_secs: i64 },

    // Shelf events
    TabToShelf { domain: String },
    TabFromShelf { domain: String },

    // Workspace events
    WorkspaceSwitch { from: String, to: String },

    // Focus mode events
    FocusStart { domain: String },
    FocusEnd { domain: String, duration_secs: i64 },

    // Session events
    SessionStart,
    SessionEnd { duration_secs: i64 },
}

/// Details stored with an event (serialized to JSON in database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDetails {
    pub duration_secs: Option<i64>,
    pub from_workspace: Option<String>,
    pub to_workspace: Option<String>,
    pub count: Option<i64>,
}

/// Daily aggregated statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyStats {
    pub date: String, // YYYY-MM-DD format
    pub trackers_blocked: i64,
    pub ads_blocked: i64,
    pub popups_blocked: i64,
    pub pages_visited: i64,
    pub tabs_opened: i64,
    pub tabs_closed: i64,
    pub browsing_time: i64,
    pub focus_time: i64,
    pub workspace_switches: i64,
    pub time_saved: i64,      // Estimated seconds saved from blocking
    pub bandwidth_saved: i64, // Estimated bytes saved from blocking
}

/// Per-domain statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainStats {
    pub domain: String,
    pub visit_count: i64,
    pub total_time: i64,
    pub trackers_blocked: i64,
    pub last_visit: DateTime<Utc>,
}

/// Per-workspace statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStats {
    pub workspace_id: String,
    pub total_time: i64,
    pub tab_count: i64,
    pub visit_count: i64,
}

/// Report settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSettings {
    pub enabled: bool,
    pub retention_days: i32,
    pub weekly_report: bool,
    pub report_day: String, // "Monday", "Tuesday", etc.
}

impl Default for ReportSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 90,
            weekly_report: true,
            report_day: "Monday".to_string(),
        }
    }
}

/// A complete report for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub start_date: String,
    pub end_date: String,
    pub total_stats: DailyStats,
    pub daily_breakdown: Vec<DailyStats>,
    pub top_domains: Vec<DomainStats>,
    pub workspace_breakdown: Vec<WorkspaceStats>,
}
