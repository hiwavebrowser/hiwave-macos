//! A grid container that is a flex item, against the pinned Chromium.
//!
//! `tools/parity_oracle/grid_in_flex_cases.json` holds small pages and, for
//! each, what the pinned Chromium reports for every element with an id:
//! `id:x:y:width:height` of its border box (`grid_flexible_row_log.mjs --file
//! grid_in_flex_cases.json --write`). The engine must give the same string.
//!
//! The first page is reddit's feed (H26b): `display: grid` with two
//! `minmax(0, <px>)` columns inside a flex row. The grid was as wide as its
//! longest word (46px) and its two items were stacked in one column, so the
//! sidebar sat below the feed.

use super::*;

/// The expression `grid_flexible_row_log.mjs` evaluates in Chromium
/// (`BOXES_XYWH`).
const BOXES: &str = "Array.prototype.map.call(document.querySelectorAll('[id]'), function (e) {\
    var r = e.getBoundingClientRect();\
    return e.id + ':' + Math.round(r.left) + ':' + Math.round(r.top) + ':' + Math.round(r.width) + ':' + Math.round(r.height);\
    }).join(' ')";

/// Shapes that do not match Chrome yet, each with its reason. A shape listed
/// here that starts to match fails the test, so the list cannot go stale.
///
/// `span-by-row-end-in-a-row`: the span is right (the item covers both
/// rows), but the first row is 20 tall, its text, where `grid-auto-rows:
/// 40px` makes it 40 in Chromium. The second row is 40.
const GAPS: &[&str] = &["span-by-row-end-in-a-row"];

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn a_grid_that_is_a_flex_item_is_as_in_chrome() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/parity_oracle/grid_in_flex_cases.json"))
            .expect("case file");
    let (w, h) = (data["viewport"][0].as_u64().unwrap(), data["viewport"][1].as_u64().unwrap());
    let mut wrong = Vec::new();
    let mut gaps_that_pass = Vec::new();
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

        match (got == chrome, GAPS.contains(&name)) {
            (false, false) => wrong.push(format!("{name}\n   engine {got}\n   chrome {chrome}")),
            (true, true) => gaps_that_pass.push(name),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{} wrong:\n{}", wrong.len(), wrong.join("\n"));
    assert!(
        gaps_that_pass.is_empty(),
        "listed as a gap but matches Chrome now, take it off the list: {gaps_that_pass:?}"
    );
}
