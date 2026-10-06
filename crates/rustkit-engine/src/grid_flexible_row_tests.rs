//! A flexible (`fr`) row in a grid whose height is auto, against the pinned
//! Chromium (Z lane I0, 2026-10-05). The pages and Chrome's answers are in
//! tools/parity_oracle/grid_flexible_row_cases.json, written by
//! grid_flexible_row_log.mjs: `chrome_boxes` is "id:top:height" for every
//! element with an id.
//!
//! Found on a Wikipedia article: `main.mw-body` is a grid whose last row is
//! `1fr` and holds the article. The row kept track sizing's estimate (one
//! line per text node), so the page was 73554px tall around 12338px of
//! content.
//!
//! The same file holds the shapes for a grid item whose children are inline
//! (`Label: <b>value</b> tail`): they share lines, where Phase 9 of the grid
//! pass used to stack them one per line.

use super::*;

/// The expression grid_flexible_row_log.mjs evaluates in Chrome.
const BOXES: &str = "Array.prototype.map.call(document.querySelectorAll('[id]'), function (e) {\
    var r = e.getBoundingClientRect();\
    return e.id + ':' + Math.round(r.top) + ':' + Math.round(r.height);\
    }).join(' ')";

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn a_flexible_row_of_an_auto_height_grid_is_as_tall_as_in_chrome() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/parity_oracle/grid_flexible_row_cases.json"))
            .expect("case file");
    let (w, h) = (data["viewport"][0].as_u64().unwrap(), data["viewport"][1].as_u64().unwrap());
    let mut wrong = Vec::new();
    for case in data["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().unwrap();
        let chrome = case["chrome_boxes"].as_str().expect("run grid_flexible_row_log.mjs --write");

        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let id = engine
            .create_headless_view(Bounds::new(0, 0, w as u32, h as u32))
            .expect("headless view");
        engine.load_html(id, case["html"].as_str().unwrap()).expect("load_html");
        let value = engine.execute_script(id, BOXES).expect("script");
        let got = value
            .strip_prefix("String(\"")
            .and_then(|v| v.strip_suffix("\")"))
            .unwrap_or(&value);

        if got != chrome {
            wrong.push(format!("{name}\n   engine {got}\n   chrome {chrome}"));
        }
    }
    assert!(wrong.is_empty(), "{} wrong:\n{}", wrong.len(), wrong.join("\n"));
}
