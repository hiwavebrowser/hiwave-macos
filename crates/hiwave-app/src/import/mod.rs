//! Browser import functionality
//!
//! This module provides import capabilities for Chrome and Firefox:
//! - Bookmarks → Workspaces with tabs
//! - Passwords → Flow Vault credentials (CSV import)

pub mod chrome;
pub mod firefox;
pub mod converter;

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Supported browsers for import
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Browser {
    Chrome,
    Firefox,
    Brave,
}

impl Browser {
    pub fn prefix(&self) -> &'static str {
        match self {
            Browser::Chrome => "CHR",
            Browser::Firefox => "FF",
            Browser::Brave => "BRV",
        }
    }
}

/// A browser profile that can be imported from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserProfile {
    pub name: String,
    pub path: PathBuf,
    pub browser: Browser,
}

/// Result of an import operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub success: bool,
    pub workspaces_created: usize,
    pub tabs_created: usize,
    pub errors: Vec<String>,
}

impl ImportResult {
    pub fn success(workspaces: usize, tabs: usize) -> Self {
        Self {
            success: true,
            workspaces_created: workspaces,
            tabs_created: tabs,
            errors: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            workspaces_created: 0,
            tabs_created: 0,
            errors: vec![error],
        }
    }
}

/// A generic bookmark structure used during import
#[derive(Debug, Clone)]
pub struct ImportedBookmark {
    pub title: String,
    pub url: Option<String>,
    pub is_folder: bool,
    pub children: Vec<ImportedBookmark>,
}

impl ImportedBookmark {
    pub fn folder(title: String, children: Vec<ImportedBookmark>) -> Self {
        Self {
            title,
            url: None,
            is_folder: true,
            children,
        }
    }

    pub fn bookmark(title: String, url: String) -> Self {
        Self {
            title,
            url: Some(url),
            is_folder: false,
            children: Vec::new(),
        }
    }
}

/// Get available browser profiles on the system
pub fn get_browser_profiles(browser: Browser) -> Vec<BrowserProfile> {
    match browser {
        Browser::Chrome => chrome::find_profiles(),
        Browser::Firefox => firefox::find_profiles(),
        Browser::Brave => chrome::find_brave_profiles(),
    }
}

/// Import bookmarks from a browser profile
pub fn import_bookmarks(profile: &BrowserProfile) -> Result<Vec<ImportedBookmark>, String> {
    match profile.browser {
        Browser::Chrome | Browser::Brave => chrome::parse_bookmarks(&profile.path),
        Browser::Firefox => firefox::parse_bookmarks(&profile.path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_prefix() {
        assert_eq!(Browser::Chrome.prefix(), "CHR");
        assert_eq!(Browser::Firefox.prefix(), "FF");
    }

    #[test]
    fn test_import_result() {
        let result = ImportResult::success(5, 47);
        assert!(result.success);
        assert_eq!(result.workspaces_created, 5);
        assert_eq!(result.tabs_created, 47);
    }
}
