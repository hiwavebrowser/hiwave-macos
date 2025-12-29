//! Convert imported bookmarks to HiWave workspaces
//!
//! Conversion rules:
//! - Each top-level folder becomes a workspace
//! - Subfolders become separate workspaces with parent name prefix
//! - Bookmarks become tabs in the workspace
//! - Workspace names are prefixed with browser abbreviation (CHR/FF)

use super::{Browser, ImportedBookmark, ImportResult};

/// Maximum workspace name length
const MAX_NAME_LENGTH: usize = 30;

/// Configuration for the import conversion
pub struct ConversionConfig {
    /// Browser prefix (CHR or FF)
    pub prefix: String,
    /// Whether to create separate workspaces for subfolders
    pub flatten_subfolders: bool,
    /// Maximum folder depth before flattening
    pub max_depth: usize,
}

impl ConversionConfig {
    pub fn for_browser(browser: Browser) -> Self {
        Self {
            prefix: browser.prefix().to_string(),
            flatten_subfolders: false,
            max_depth: 2,
        }
    }
}

/// A workspace to be created during import
#[derive(Debug, Clone)]
pub struct ImportWorkspace {
    pub name: String,
    pub tabs: Vec<ImportTab>,
}

/// A tab to be created during import
#[derive(Debug, Clone)]
pub struct ImportTab {
    pub title: String,
    pub url: String,
}

/// Result of converting bookmarks to workspaces
pub struct ConversionResult {
    pub workspaces: Vec<ImportWorkspace>,
    #[allow(dead_code)]
    pub stats: ImportResult,
}

/// Convert imported bookmarks to HiWave workspaces
pub fn convert_to_workspaces(
    bookmarks: Vec<ImportedBookmark>,
    config: &ConversionConfig,
) -> ConversionResult {
    let mut workspaces = Vec::new();
    let mut workspace_count = 0;
    let mut tab_count = 0;

    for bookmark in bookmarks {
        if bookmark.is_folder {
            // Top-level folder becomes a workspace
            let sub_workspaces = process_folder(
                &bookmark,
                &config.prefix,
                "",
                0,
                config,
            );

            for ws in sub_workspaces {
                tab_count += ws.tabs.len();
                workspace_count += 1;
                workspaces.push(ws);
            }
        } else if let Some(url) = &bookmark.url {
            // Standalone bookmark at root level - create a workspace for it
            let ws_name = format_workspace_name(&config.prefix, "Imported", "");
            workspaces.push(ImportWorkspace {
                name: ws_name,
                tabs: vec![ImportTab {
                    title: bookmark.title.clone(),
                    url: url.clone(),
                }],
            });
            workspace_count += 1;
            tab_count += 1;
        }
    }

    ConversionResult {
        workspaces,
        stats: ImportResult::success(workspace_count, tab_count),
    }
}

/// Process a folder and its contents
fn process_folder(
    folder: &ImportedBookmark,
    prefix: &str,
    parent_path: &str,
    depth: usize,
    config: &ConversionConfig,
) -> Vec<ImportWorkspace> {
    let folder_name = &folder.title;
    let current_path = if parent_path.is_empty() {
        folder_name.clone()
    } else {
        format!("{}-{}", parent_path, folder_name)
    };

    let mut workspaces = Vec::new();
    let mut direct_tabs = Vec::new();

    for child in &folder.children {
        if child.is_folder {
            if depth < config.max_depth && !config.flatten_subfolders {
                // Create separate workspace for subfolder
                let sub_workspaces = process_folder(
                    child,
                    prefix,
                    &current_path,
                    depth + 1,
                    config,
                );
                workspaces.extend(sub_workspaces);
            } else {
                // Flatten: add subfolder contents as tabs to current workspace
                let flattened_tabs = flatten_folder(child);
                direct_tabs.extend(flattened_tabs);
            }
        } else if let Some(url) = &child.url {
            // Direct bookmark in this folder
            direct_tabs.push(ImportTab {
                title: child.title.clone(),
                url: url.clone(),
            });
        }
    }

    // Create workspace for this folder if it has any direct tabs
    if !direct_tabs.is_empty() {
        let ws_name = format_workspace_name(prefix, folder_name, parent_path);
        workspaces.insert(0, ImportWorkspace {
            name: ws_name,
            tabs: direct_tabs,
        });
    }

    workspaces
}

/// Flatten a folder's contents into a list of tabs
fn flatten_folder(folder: &ImportedBookmark) -> Vec<ImportTab> {
    let mut tabs = Vec::new();

    for child in &folder.children {
        if child.is_folder {
            tabs.extend(flatten_folder(child));
        } else if let Some(url) = &child.url {
            tabs.push(ImportTab {
                title: child.title.clone(),
                url: url.clone(),
            });
        }
    }

    tabs
}

/// Generic browser folder names to skip (these don't add value)
const SKIP_FOLDER_NAMES: &[&str] = &[
    "Bookmarks", "Bookmarks bar", "Bookmark Bar", "Other bookmarks",
    "Other", "Menu", "Toolbar", "Mobile", "Synced", "Favorites",
];

/// Check if a folder name should be skipped
fn should_skip_folder_name(name: &str) -> bool {
    SKIP_FOLDER_NAMES.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// Format a workspace name with prefix and path
fn format_workspace_name(prefix: &str, name: &str, parent_path: &str) -> String {
    // Filter out generic folder names from parent path
    let filtered_parent: Vec<&str> = parent_path
        .split('-')
        .filter(|s| !should_skip_folder_name(s))
        .collect();

    // If the name itself is a generic folder, just use prefix
    let display_name = if should_skip_folder_name(name) {
        if filtered_parent.is_empty() {
            "Imported".to_string()
        } else {
            filtered_parent.join("-")
        }
    } else if filtered_parent.is_empty() {
        name.to_string()
    } else {
        // Abbreviate parent path if too long
        let parent_short = abbreviate_path(&filtered_parent.join("-"), 8);
        format!("{}-{}", parent_short, name)
    };

    let full_name = format!("{}: {}", prefix, display_name);

    // Truncate if too long
    if full_name.len() > MAX_NAME_LENGTH {
        format!("{}...", &full_name[..MAX_NAME_LENGTH - 3])
    } else {
        full_name
    }
}

/// Abbreviate a path by taking first letters of each segment
fn abbreviate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    // Take first 3 chars of each segment
    let abbreviated: String = path
        .split('-')
        .map(|s| {
            if s.len() > 3 {
                &s[..3]
            } else {
                s
            }
        })
        .collect::<Vec<_>>()
        .join("-");

    if abbreviated.len() > max_len {
        format!("{}...", &abbreviated[..max_len - 3])
    } else {
        abbreviated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_workspace_name() {
        assert_eq!(
            format_workspace_name("CHR", "Work", ""),
            "CHR: Work"
        );
        assert_eq!(
            format_workspace_name("FF", "Projects", "Work"),
            "FF: Work-Projects"
        );
    }

    #[test]
    fn test_abbreviate_path() {
        assert_eq!(abbreviate_path("Work", 10), "Work");
        assert_eq!(abbreviate_path("Shopping-Clothes", 10), "Sho-Clo...");
    }

    #[test]
    fn test_convert_simple_bookmarks() {
        let bookmarks = vec![
            ImportedBookmark::folder("Work".to_string(), vec![
                ImportedBookmark::bookmark("GitHub".to_string(), "https://github.com".to_string()),
                ImportedBookmark::bookmark("Google".to_string(), "https://google.com".to_string()),
            ]),
        ];

        let config = ConversionConfig::for_browser(Browser::Chrome);
        let result = convert_to_workspaces(bookmarks, &config);

        assert_eq!(result.stats.workspaces_created, 1);
        assert_eq!(result.stats.tabs_created, 2);
        assert_eq!(result.workspaces.len(), 1);
        assert_eq!(result.workspaces[0].name, "CHR: Work");
    }
}
