//! Firefox bookmark import
//!
//! Firefox stores bookmarks in places.sqlite, but also keeps JSON backups in:
//! ~/Library/Application Support/Firefox/Profiles/<profile>/bookmarkbackups/
//!
//! We use the JSON backups for simpler, safer parsing.

use std::fs;
use std::path::{Path, PathBuf};
use serde::Deserialize;

use super::{Browser, BrowserProfile, ImportedBookmark};

/// Firefox bookmark JSON backup structure
#[derive(Debug, Deserialize)]
struct FirefoxBookmarkNode {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    uri: Option<String>,
    #[serde(rename = "type", default)]
    type_code: Option<String>,
    #[serde(rename = "typeCode", default)]
    type_code_num: Option<u32>,
    #[serde(default)]
    children: Option<Vec<FirefoxBookmarkNode>>,
    #[serde(default)]
    root: Option<String>,
}

/// Find Firefox profiles on the system
pub fn find_profiles() -> Vec<BrowserProfile> {
    let mut profiles = Vec::new();

    // macOS Firefox location
    #[cfg(target_os = "macos")]
    let firefox_base = dirs::home_dir()
        .map(|h| h.join("Library/Application Support/Firefox/Profiles"));

    // Windows Firefox location
    #[cfg(target_os = "windows")]
    let firefox_base = dirs::data_dir()
        .map(|d| d.join("Mozilla/Firefox/Profiles"));

    // Linux Firefox location
    #[cfg(target_os = "linux")]
    let firefox_base = dirs::home_dir()
        .map(|h| h.join(".mozilla/firefox"));

    if let Some(base) = firefox_base {
        if base.exists() {
            if let Ok(entries) = fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Check if this profile has bookmarks
                        let has_bookmarks = path.join("places.sqlite").exists()
                            || path.join("bookmarkbackups").exists();

                        if has_bookmarks {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string();

                            // Extract profile name from "random.profile-name" format
                            let display_name = if let Some(dot_pos) = name.find('.') {
                                name[dot_pos + 1..].to_string()
                            } else {
                                name.clone()
                            };

                            profiles.push(BrowserProfile {
                                name: display_name,
                                path,
                                browser: Browser::Firefox,
                            });
                        }
                    }
                }
            }
        }
    }

    profiles
}

/// Parse Firefox bookmarks from a profile directory
/// Uses JSON backup files for simpler parsing
pub fn parse_bookmarks(profile_path: &Path) -> Result<Vec<ImportedBookmark>, String> {
    let backups_dir = profile_path.join("bookmarkbackups");

    if !backups_dir.exists() {
        return Err("No bookmark backups found. Firefox may not have created any yet.".to_string());
    }

    // Find the most recent backup file
    let backup_file = find_latest_backup(&backups_dir)?;

    let content = fs::read_to_string(&backup_file)
        .map_err(|e| format!("Failed to read bookmark backup: {}", e))?;

    // Firefox backup might be compressed (jsonlz4) or plain JSON
    // For now, we only support plain JSON backups
    let root: FirefoxBookmarkNode = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse bookmarks JSON: {}", e))?;

    let mut result = Vec::new();

    // Process root children (menu, toolbar, unfiled, mobile)
    if let Some(children) = root.children {
        for child in children {
            let root_name = child.root.as_deref().or(child.title.as_deref());

            match root_name {
                Some("toolbarFolder") | Some("Bookmarks Toolbar") => {
                    let bookmarks = convert_node(&child);
                    if !bookmarks.children.is_empty() {
                        result.push(ImportedBookmark::folder(
                            "Toolbar".to_string(),
                            bookmarks.children,
                        ));
                    }
                }
                Some("bookmarksMenuFolder") | Some("Bookmarks Menu") => {
                    let bookmarks = convert_node(&child);
                    if !bookmarks.children.is_empty() {
                        result.push(ImportedBookmark::folder(
                            "Menu".to_string(),
                            bookmarks.children,
                        ));
                    }
                }
                Some("unfiledBookmarksFolder") | Some("Other Bookmarks") => {
                    let bookmarks = convert_node(&child);
                    if !bookmarks.children.is_empty() {
                        result.push(ImportedBookmark::folder(
                            "Other".to_string(),
                            bookmarks.children,
                        ));
                    }
                }
                Some("mobileFolder") | Some("Mobile Bookmarks") => {
                    let bookmarks = convert_node(&child);
                    if !bookmarks.children.is_empty() {
                        result.push(ImportedBookmark::folder(
                            "Mobile".to_string(),
                            bookmarks.children,
                        ));
                    }
                }
                _ => {
                    // Unknown root, still process it
                    let bookmarks = convert_node(&child);
                    if !bookmarks.children.is_empty() || bookmarks.url.is_some() {
                        result.push(bookmarks);
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Find the most recent JSON backup file
fn find_latest_backup(backups_dir: &Path) -> Result<PathBuf, String> {
    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;

    if let Ok(entries) = fs::read_dir(backups_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Look for .json files (not .jsonlz4 compressed files)
            if name.ends_with(".json") && name.starts_with("bookmarks-") {
                if let Ok(metadata) = path.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if latest.is_none() || modified > latest.as_ref().unwrap().1 {
                            latest = Some((path, modified));
                        }
                    }
                }
            }
        }
    }

    latest
        .map(|(path, _)| path)
        .ok_or_else(|| "No JSON bookmark backup files found. Firefox may use compressed .jsonlz4 format.".to_string())
}

/// Convert a Firefox bookmark node to our generic structure
fn convert_node(node: &FirefoxBookmarkNode) -> ImportedBookmark {
    let title = node.title.clone().unwrap_or_else(|| "Untitled".to_string());

    // Check if this is a folder or bookmark
    // typeCode: 1 = bookmark, 2 = folder, 3 = separator
    let is_folder = node.type_code_num == Some(2)
        || node.type_code.as_deref() == Some("text/x-moz-place-container")
        || node.children.is_some();

    if is_folder {
        let children = node.children
            .as_ref()
            .map(|c| c.iter()
                .filter(|n| n.type_code_num != Some(3)) // Skip separators
                .map(convert_node)
                .collect())
            .unwrap_or_default();

        ImportedBookmark::folder(title, children)
    } else if let Some(url) = &node.uri {
        ImportedBookmark::bookmark(title, url.clone())
    } else {
        // Unknown type, treat as empty folder
        ImportedBookmark::folder(title, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_firefox_bookmark_json() {
        let json = r#"{
            "title": "",
            "children": [
                {
                    "title": "Bookmarks Toolbar",
                    "root": "toolbarFolder",
                    "typeCode": 2,
                    "children": [
                        {
                            "title": "Mozilla",
                            "uri": "https://mozilla.org",
                            "typeCode": 1
                        }
                    ]
                },
                {
                    "title": "Bookmarks Menu",
                    "root": "bookmarksMenuFolder",
                    "typeCode": 2,
                    "children": []
                }
            ]
        }"#;

        let root: FirefoxBookmarkNode = serde_json::from_str(json).unwrap();
        assert!(root.children.is_some());
        assert_eq!(root.children.as_ref().unwrap().len(), 2);
    }
}
