//! The census's node-side tools (the prevalence crawler, the refused-host list, the known-property
//! list) have their own `node --test` files in `docs/census/tools/`. They run here so that one
//! command, `cargo test -p css-census`, runs every census test. They need node and the Playwright
//! Chromium in `tools/parity_oracle`, as the census itself does, and no network: the browser tests
//! map every host to a local server.

use std::path::PathBuf;
use std::process::Command;

fn tools_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../docs/census/tools")
}

#[test]
fn node_tool_tests_pass() {
    let dir = tools_dir().canonicalize().expect("docs/census/tools exists");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read docs/census/tools")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.to_string_lossy().ends_with(".test.mjs"))
        .collect();
    files.sort();
    let names: Vec<String> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for want in [
        "css_known_properties.test.mjs",
        "css_property_names.test.mjs",
        "never_load.test.mjs",
        "prevalence_visit.test.mjs",
    ] {
        assert!(names.iter().any(|n| n == want), "{want} missing from {names:?}");
    }
    let out = Command::new("node")
        .arg("--test")
        .args(&files)
        .current_dir(&dir)
        .output()
        .expect("node is on PATH (the census needs it too)");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "node --test failed:\n{text}");
    assert!(text.contains("# fail 0") || text.contains("ℹ fail 0"), "no fail count in:\n{text}");
}
