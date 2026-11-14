//! Paint order of overlapping positioned layers with `z-index`, `position: fixed` among them, vs Chrome
//! for Testing 148.0.7778.216 (`docs/diagnostics/2026-11-14/fixed_zindex_cases.mjs`, output
//! `fixed_zindex_cft148.json`). Two boxes overlap: A (red) comes first in the document, B (blue) later;
//! the one drawn last at the overlap is on top. bing.com's later state painted its fixed background-photo
//! layers over its fixed header, covering the nav row and the search box (batch 75).

use super::*;
use rustkit_layout::DisplayCommand;
use std::collections::BTreeMap;

const CASES: &str = include_str!("../../../docs/diagnostics/2026-11-14/fixed_zindex_cases.json");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-14/fixed_zindex_cft148.json");

/// Which box is drawn last of the two: "a" (red) or "b" (blue).
fn on_top(body: &str) -> &'static str {
    let page = format!("<!doctype html><html><head><style>body{{margin:0;background:#fff}}</style></head><body>{body}</body></html>");
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds::new(0, 0, 400, 300)).expect("view");
    engine.load_html(view, &page).expect("load_html");
    let list = engine.views[&view].display_list.as_ref().expect("display list").commands.clone();
    let mut last = "none";
    for c in &list {
        if let DisplayCommand::SolidColor(c, _) = c {
            match (c.r, c.g, c.b) {
                (255, 0, 0) => last = "a",
                (0, 0, 255) => last = "b",
                _ => {}
            }
        }
    }
    last
}

#[test]
#[cfg(feature = "headless")]
fn overlapping_positioned_layers_stack_by_z_index_as_in_chrome() {
    let cases: BTreeMap<String, String> = serde_json::from_str(CASES).unwrap();
    let chrome: BTreeMap<String, serde_json::Value> = serde_json::from_str(CHROME).unwrap();
    let mut wrong = Vec::new();
    for (name, body) in &cases {
        let want = chrome[name]["top"].as_str().unwrap();
        let got = on_top(body);
        if got != want {
            wrong.push(format!("{name}: chrome {want}, here {got}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), cases.len(), wrong.join("\n"));
}
