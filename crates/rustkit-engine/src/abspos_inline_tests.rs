//! An absolutely positioned inline element is a block, against the pinned
//! Chromium (CSS 2.1 section 9.7).
//!
//! `tools/parity_oracle/abspos_inline_cases.json` holds small pages and, for
//! each, what the pinned Chromium reports for every element with an id:
//! `id:x:y:width:height` of its border box (`grid_flexible_row_log.mjs --file
//! abspos_inline_cases.json --write`). The engine must give the same string.
//!
//! The first page is a card with a whole-card link: text and an empty
//! `<a>` with `position: absolute; inset: 0`. The link stayed an inline, an
//! inline with no content gets no box, and so the link was nowhere: script
//! read 0:0:0:0 and no click could land on it.

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
/// `a-span-with-no-offsets`: with no offsets the box sits at its static
/// position, after the text before it on the line (x 48). Layout puts it at
/// the start of the line (x 0).
///
/// `an-empty-link-with-inset-0-in-a-static-parent-of-text`,
/// `a-span-inside-a-span`, `an-empty-span-inside-a-link`: the containing
/// block is not the parent (a static `<p>`, an inline `<span>` or `<a>`) but
/// the positioned box above it. Layout anchors an abspos box to its parent
/// only. The inline parent is no longer widened by the box.
const GAPS: &[&str] = &[
    "a-span-with-no-offsets",
    "an-empty-link-with-inset-0-in-a-static-parent-of-text",
    "a-span-inside-a-span",
    "an-empty-span-inside-a-link",
];

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn an_abspos_inline_is_a_block_as_in_chrome() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/parity_oracle/abspos_inline_cases.json"))
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
