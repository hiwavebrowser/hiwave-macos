//! An emoji that is text by default (no Emoji_Presentation) is drawn from
//! the system's cascade face, as in Chromium; only an Emoji_Presentation
//! character, or one followed by U+FE0F, goes to Apple Color Emoji first.
//! The chrome sidebar's action rows (flex, 8px padding, 14px icon in the
//! system font) are 33 tall for `🛡` and `⚙` and 39 for `🚫` and `📈`;
//! RustKit made all four 39 and `⚙` 18px wide, which put every box below
//! the actions section 12px low on the chrome board.
//!
//! Expected values are getBoundingClientRect in Chrome for Testing
//! 148.0.7778.216 on the same markup (impl-board3 probe, 2026-11-08):
//! `🛡` is `.AppleColorEmojiUI`, `⚙` Menlo, `🚫` / `📈` / `🛡️` Apple Color
//! Emoji (CSS.getPlatformFontsForNode).

use super::*;

fn eval(page: &str, script: &str) -> String {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, page).expect("load");
    let got = engine.execute_script(view, script).expect("script");
    got.trim_start_matches("String(\"").trim_end_matches("\")").to_string()
}

const ROWS: &str = "<!doctype html><body style=\"font-family:-apple-system,BlinkMacSystemFont,sans-serif;margin:0\">\
    <div class=row style=\"display:flex;align-items:center;gap:8px;padding:8px\"><span class=i style=\"font-size:14px\">&#128737;</span><span style=\"font-size:13px\">Shield</span></div>\
    <div class=row style=\"display:flex;align-items:center;gap:8px;padding:8px\"><span class=i style=\"font-size:14px\">&#128683;</span><span style=\"font-size:13px\">Block</span></div>\
    <div class=row style=\"display:flex;align-items:center;gap:8px;padding:8px\"><span class=i style=\"font-size:14px\">&#128200;</span><span style=\"font-size:13px\">Report</span></div>\
    <div class=row style=\"display:flex;align-items:center;gap:8px;padding:8px\"><span class=i style=\"font-size:14px\">&#9881;</span><span style=\"font-size:13px\">Settings</span></div>\
    <div class=row style=\"display:flex;align-items:center;gap:8px;padding:8px\"><span class=i style=\"font-size:14px\">&#128737;&#65039;</span><span style=\"font-size:13px\">ShieldVS16</span></div>";

#[test]
fn text_default_emoji_rows_match_chromium() {
    let rows = eval(
        ROWS,
        "Array.prototype.map.call(document.querySelectorAll('.row'), function (e) { \
         var r = e.getBoundingClientRect(); return r.y + '/' + r.height; }).join(' ')",
    );
    assert_eq!(rows, "0/33 33/39 72/39 111/33 144/39", "Chromium 148 row y/height");
    let icons = eval(
        ROWS,
        "Array.prototype.map.call(document.querySelectorAll('.i'), function (e) { \
         var r = e.getBoundingClientRect(); return Math.round(r.width * 100) / 100 + 'x' + r.height; }).join(' ')",
    );
    assert_eq!(icons, "18x17 18x23 18x23 8.44x17 18x23", "Chromium 148 icon width x height");
}
