//! Chrome bookmark import
//!
//! Chrome stores bookmarks in a JSON file at:
//! ~/Library/Application Support/Google/Chrome/<Profile>/Bookmarks

use std::fs;
use std::path::Path;
use serde::Deserialize;

use super::{Browser, BrowserProfile, ImportedBookmark};

/// Chrome bookmark JSON structure
#[derive(Debug, Deserialize)]
struct ChromeBookmarksFile {
    roots: ChromeRoots,
}

#[derive(Debug, Deserialize)]
struct ChromeRoots {
    bookmark_bar: ChromeBookmarkNode,
    other: ChromeBookmarkNode,
    #[serde(default)]
    synced: Option<ChromeBookmarkNode>,
}

#[derive(Debug, Deserialize)]
struct ChromeBookmarkNode {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    children: Option<Vec<ChromeBookmarkNode>>,
}

/// Find Chrome profiles on the system
pub fn find_profiles() -> Vec<BrowserProfile> {
    let mut profiles = Vec::new();

    // macOS Chrome location
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let chrome_base = home.join("Library/Application Support/Google/Chrome");

            if chrome_base.exists() {
                // Check for Default profile
                let default_profile = chrome_base.join("Default");
                if default_profile.join("Bookmarks").exists() {
                    profiles.push(BrowserProfile {
                        name: "Default".to_string(),
                        path: default_profile,
                        browser: Browser::Chrome,
                    });
                }

                // Check for numbered profiles (Profile 1, Profile 2, etc.)
                if let Ok(entries) = fs::read_dir(&chrome_base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            if name.starts_with("Profile ") && path.join("Bookmarks").exists() {
                                profiles.push(BrowserProfile {
                                    name: name.to_string(),
                                    path,
                                    browser: Browser::Chrome,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Windows Chrome location
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = dirs::data_local_dir() {
            let chrome_base = local_app_data.join("Google/Chrome/User Data");

            if chrome_base.exists() {
                let default_profile = chrome_base.join("Default");
                if default_profile.join("Bookmarks").exists() {
                    profiles.push(BrowserProfile {
                        name: "Default".to_string(),
                        path: default_profile,
                        browser: Browser::Chrome,
                    });
                }

                if let Ok(entries) = fs::read_dir(&chrome_base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            if name.starts_with("Profile ") && path.join("Bookmarks").exists() {
                                profiles.push(BrowserProfile {
                                    name: name.to_string(),
                                    path,
                                    browser: Browser::Chrome,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Linux Chrome location
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            let chrome_base = home.join(".config/google-chrome");

            if chrome_base.exists() {
                let default_profile = chrome_base.join("Default");
                if default_profile.join("Bookmarks").exists() {
                    profiles.push(BrowserProfile {
                        name: "Default".to_string(),
                        path: default_profile,
                        browser: Browser::Chrome,
                    });
                }

                if let Ok(entries) = fs::read_dir(&chrome_base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            if name.starts_with("Profile ") && path.join("Bookmarks").exists() {
                                profiles.push(BrowserProfile {
                                    name: name.to_string(),
                                    path,
                                    browser: Browser::Chrome,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    profiles
}

/// Find Brave profiles on the system (uses same format as Chrome)
pub fn find_brave_profiles() -> Vec<BrowserProfile> {
    let mut profiles = Vec::new();

    // macOS Brave location
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let brave_base = home.join("Library/Application Support/BraveSoftware/Brave-Browser");

            if brave_base.exists() {
                // Check for Default profile
                let default_profile = brave_base.join("Default");
                if default_profile.join("Bookmarks").exists() {
                    profiles.push(BrowserProfile {
                        name: "Default".to_string(),
                        path: default_profile,
                        browser: Browser::Brave,
                    });
                }

                // Check for numbered profiles
                if let Ok(entries) = fs::read_dir(&brave_base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            if name.starts_with("Profile ") && path.join("Bookmarks").exists() {
                                profiles.push(BrowserProfile {
                                    name: name.to_string(),
                                    path,
                                    browser: Browser::Brave,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Windows Brave location
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = dirs::data_local_dir() {
            let brave_base = local_app_data.join("BraveSoftware/Brave-Browser/User Data");

            if brave_base.exists() {
                let default_profile = brave_base.join("Default");
                if default_profile.join("Bookmarks").exists() {
                    profiles.push(BrowserProfile {
                        name: "Default".to_string(),
                        path: default_profile,
                        browser: Browser::Brave,
                    });
                }

                if let Ok(entries) = fs::read_dir(&brave_base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            if name.starts_with("Profile ") && path.join("Bookmarks").exists() {
                                profiles.push(BrowserProfile {
                                    name: name.to_string(),
                                    path,
                                    browser: Browser::Brave,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Linux Brave location
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            let brave_base = home.join(".config/BraveSoftware/Brave-Browser");

            if brave_base.exists() {
                let default_profile = brave_base.join("Default");
                if default_profile.join("Bookmarks").exists() {
                    profiles.push(BrowserProfile {
                        name: "Default".to_string(),
                        path: default_profile,
                        browser: Browser::Brave,
                    });
                }

                if let Ok(entries) = fs::read_dir(&brave_base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            if name.starts_with("Profile ") && path.join("Bookmarks").exists() {
                                profiles.push(BrowserProfile {
                                    name: name.to_string(),
                                    path,
                                    browser: Browser::Brave,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    profiles
}

/// Parse Chrome bookmarks from a profile directory
pub fn parse_bookmarks(profile_path: &Path) -> Result<Vec<ImportedBookmark>, String> {
    let bookmarks_file = profile_path.join("Bookmarks");

    if !bookmarks_file.exists() {
        return Err(format!("Bookmarks file not found at {:?}", bookmarks_file));
    }

    let content = fs::read_to_string(&bookmarks_file)
        .map_err(|e| format!("Failed to read bookmarks file: {}", e))?;

    let chrome_bookmarks: ChromeBookmarksFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse bookmarks JSON: {}", e))?;

    let mut result = Vec::new();

    // Convert bookmark bar
    let bar_bookmarks = convert_node(&chrome_bookmarks.roots.bookmark_bar);
    if !bar_bookmarks.children.is_empty() || bar_bookmarks.url.is_some() {
        result.push(ImportedBookmark::folder(
            "Bookmarks".to_string(),
            bar_bookmarks.children,
        ));
    }

    // Convert other bookmarks
    let other_bookmarks = convert_node(&chrome_bookmarks.roots.other);
    if !other_bookmarks.children.is_empty() || other_bookmarks.url.is_some() {
        result.push(ImportedBookmark::folder(
            "Other".to_string(),
            other_bookmarks.children,
        ));
    }

    // Convert synced bookmarks if present
    if let Some(synced) = &chrome_bookmarks.roots.synced {
        let synced_bookmarks = convert_node(synced);
        if !synced_bookmarks.children.is_empty() || synced_bookmarks.url.is_some() {
            result.push(ImportedBookmark::folder(
                "Mobile".to_string(),
                synced_bookmarks.children,
            ));
        }
    }

    Ok(result)
}

/// Convert a Chrome bookmark node to our generic structure
fn convert_node(node: &ChromeBookmarkNode) -> ImportedBookmark {
    match node.node_type.as_str() {
        "folder" => {
            let children = node.children
                .as_ref()
                .map(|c| c.iter().map(convert_node).collect())
                .unwrap_or_default();

            ImportedBookmark::folder(node.name.clone(), children)
        }
        "url" => {
            ImportedBookmark::bookmark(
                node.name.clone(),
                node.url.clone().unwrap_or_default(),
            )
        }
        _ => ImportedBookmark::folder(node.name.clone(), Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_chrome_bookmark_json() {
        let json = r#"{
            "roots": {
                "bookmark_bar": {
                    "name": "Bookmarks bar",
                    "type": "folder",
                    "children": [
                        {
                            "name": "Google",
                            "type": "url",
                            "url": "https://www.google.com"
                        },
                        {
                            "name": "Work",
                            "type": "folder",
                            "children": [
                                {
                                    "name": "GitHub",
                                    "type": "url",
                                    "url": "https://github.com"
                                }
                            ]
                        }
                    ]
                },
                "other": {
                    "name": "Other bookmarks",
                    "type": "folder",
                    "children": []
                }
            }
        }"#;

        let chrome_bookmarks: ChromeBookmarksFile = serde_json::from_str(json).unwrap();
        assert_eq!(chrome_bookmarks.roots.bookmark_bar.name, "Bookmarks bar");
        assert_eq!(chrome_bookmarks.roots.bookmark_bar.children.as_ref().unwrap().len(), 2);
    }
}
