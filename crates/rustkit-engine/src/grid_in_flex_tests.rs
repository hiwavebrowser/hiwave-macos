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
///
/// The three below are bing's search box (a textarea that is `height: 100%`
/// and shares `grid-area: content`, a name no area has, with an `::after`).
/// Their heights and rows match since the missing name resolves to the first
/// implicit line; `percent-height-item-sharing-an-area-in-a-block` and
/// `...-in-a-row-with-a-width` pin that. What is left is width:
///
/// `percent-height-item-sharing-an-area-in-a-row`: the grid is 38 wide, its
/// word, where the item's `min-width: 200px` makes it 200 in Chromium (the
/// content-width estimate does not read `min-width`).
///
/// `bing-textarea-and-its-replica-in-a-row`: the grid and the textarea are
/// 214 wide; Chromium has 200, the `min-width`.
///
/// `bing-textarea-and-its-replica-in-a-block`: the textarea's `width: 100%`
/// counts as the whole 600 toward its column, so the empty first column is 0
/// wide; in Chromium the columns are 200 and 400.
///
/// google's logo block (two `height: 100%` items in one cell of a grid whose
/// height is `calc(100% - 560px)`, in a column) is no gap any more:
/// `two-items-in-one-cell-of-a-calc-height-grid-in-a-column` matches since a
/// `height: 100%` column hands its items the height the block pass resolved.
///
/// The `percent-height-column-*` pages have no grid: they are the column
/// google's logo block sits in, a flex column whose own `height` is a
/// percentage (`html, body, #x { height: 100% }`). The block pass resolves
/// that height and hands it to the flex pass, so a `height: 25%` item, a
/// `flex: 1` item, `justify-content: center` and a border-box column with
/// padding are as in Chromium. Two neighbours are not:
///
/// `percent-height-column-items-shrink`: a `height: 200px` item in an 80px
/// column stays 200; Chromium shrinks it to 60 beside its 20px sibling.
///
/// `percent-height-row-is-not-touched`: a `height: 50%` item of a row that
/// is 400 tall is 110; Chromium has 200.
const GAPS: &[&str] = &[
    "span-by-row-end-in-a-row",
    "percent-height-item-sharing-an-area-in-a-row",
    "bing-textarea-and-its-replica-in-a-row",
    "bing-textarea-and-its-replica-in-a-block",
    "percent-height-column-items-shrink",
    "percent-height-row-is-not-touched",
];

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
