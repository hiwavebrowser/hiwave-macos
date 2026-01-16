//! CSS style computation tests.

use crate::{TestError, TestResult, TestSummary};
use rustkit_css::Stylesheet;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tracing::debug;

/// Style test runner.
pub struct StyleTestRunner;

impl StyleTestRunner {
    pub fn new() -> Self {
        Self
    }

    /// Run all style tests in a directory.
    pub fn run_all(&self, dir: &Path) -> Result<TestSummary, TestError> {
        let mut summary = TestSummary::new();

        if !dir.exists() {
            return Ok(summary);
        }

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "css" || ext == "html")
            })
        {
            let result = self.run_file(entry.path())?;
            summary.add(result);
        }

        Ok(summary)
    }

    /// Run a single style test.
    pub fn run_file(&self, path: &Path) -> Result<TestResult, TestError> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        debug!(?path, "Running style test");
        let start = Instant::now();

        // Read content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return Ok(TestResult::error(&name, 0, e.to_string())),
        };

        // Extract CSS (from <style> or .css file)
        let css = if path.extension().is_some_and(|e| e == "html") {
            extract_style_from_html(&content)
        } else {
            content
        };

        // Parse CSS
        let stylesheet = match Stylesheet::parse(&css) {
            Ok(s) => s,
            Err(e) => {
                let duration = start.elapsed().as_millis() as u64;
                return Ok(TestResult::error(&name, duration, e.to_string()));
            }
        };

        let duration = start.elapsed().as_millis() as u64;

        // Check for expected output
        let expected_path = path.with_extension("expected");
        if !expected_path.exists() {
            // Just verify parsing succeeded
            return Ok(TestResult::pass(&name, duration));
        }

        // Compare computed styles against expected
        let expected = match fs::read_to_string(&expected_path) {
            Ok(e) => e,
            Err(e) => return Ok(TestResult::error(&name, duration, e.to_string())),
        };

        // For now, just check rule count
        let actual = format!("rules: {}", stylesheet.rule_count());

        if expected.trim() == actual.trim() {
            Ok(TestResult::pass(&name, duration))
        } else {
            Ok(TestResult::fail_with_diff(
                &name, duration, expected, actual,
            ))
        }
    }
}

impl Default for StyleTestRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract CSS from <style> tags in HTML.
fn extract_style_from_html(html: &str) -> String {
    let mut css = String::new();

    // Simple extraction - find <style>...</style> blocks
    let mut in_style = false;
    for line in html.lines() {
        if line.contains("<style") {
            in_style = true;
            continue;
        }
        if line.contains("</style>") {
            in_style = false;
            continue;
        }
        if in_style {
            css.push_str(line);
            css.push('\n');
        }
    }

    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_style() {
        let html = r#"
            <html>
            <head>
            <style>
            body { color: red; }
            </style>
            </head>
            </html>
        "#;

        let css = extract_style_from_html(html);
        assert!(css.contains("body"));
        assert!(css.contains("color: red"));
    }
}
