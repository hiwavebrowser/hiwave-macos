//! CB3: `button{font-size:16px;line-height:1;padding:0 4px;border:none}` is 22.8 px tall in RustKit
//! and 16 in Chrome (the settings page's blocklist remove button; 93 of the 128 geometry diffs at
//! 1280 are the shift below it). A button's content height is its `line-height`, so with no
//! vertical padding and no border the box is exactly the line box. Pinned from the CSS box model
//! (and the row's Chrome measurement of 16), so the pins do not depend on a font's `normal` height.
//! Reduced page: docs/diagnostics/2026-11-09/settings_button_line_height_1.html.

use super::*;

const PAGE: &str = "<html><head><style>body{margin:0;font:12.8px Arial}div{margin:8px}\
    .i{display:flex;align-items:center;gap:4px;padding:4px 8px;background:#ccc;width:max-content}\
    .r{background:none;border:none;padding:0 4px;font-size:16px;line-height:1}</style></head><body>\
    <div class=i id=item><span>example.com</span><button class=r id=rm>&times;</button></div>\
    <div><button class=r id=rmx>x</button></div>\
    <div><button id=lh1 style='font-size:16px;line-height:1'>x</button></div>\
    <div><button id=all style='font-size:16px;line-height:1;padding:0;border:none'>x</button></div>\
    <div><span id=sp style='display:inline-block;font-size:16px;line-height:1'>&times;</span></div>\
    </body></html>";

fn heights() -> std::collections::HashMap<String, f64> {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 400 })
        .expect("view");
    engine.load_html(view, PAGE).expect("load_html");
    let ids = ["item", "rm", "rmx", "lh1", "all", "sp"];
    let expr = ids
        .iter()
        .map(|id| format!("document.getElementById('{id}').getBoundingClientRect().height"))
        .collect::<Vec<_>>()
        .join(" + ',' + ");
    let out = engine.execute_script(view, &format!("String({expr})")).unwrap();
    let text = out.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&out).to_string();
    ids.iter().map(|s| s.to_string()).zip(text.split(',').map(|n| n.parse::<f64>().unwrap_or(f64::NAN))).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.5
}

#[test]
fn a_button_with_line_height_1_no_vertical_padding_and_no_border_is_one_line_box_tall() {
    let h = heights();
    assert!(close(h["all"], 16.0), "padding:0, border:none, line-height:1 at 16px is 16 px, got {}", h["all"]);
    assert!(close(h["rm"], 16.0), "the settings remove button is 16 px, got {}", h["rm"]);
    assert!(close(h["rmx"], 16.0), "the same button alone is 16 px, got {}", h["rmx"]);
}

#[test]
fn a_partly_declared_button_is_unchanged_until_an_oracle_says_otherwise() {
    // `line-height:1` alone leaves the UA padding and border in place. The right height needs
    // Chrome's UA padding and border for a button at this font (CB3b); it is 22.8 on develop and
    // stays so: this PR only changes a button whose author declared the vertical box too.
    let h = heights();
    assert!(close(h["lh1"], 22.8), "a button with only line-height:1 is unchanged, got {}", h["lh1"]);
}

#[test]
fn the_flex_row_around_the_remove_button_is_the_button_plus_its_padding() {
    // .i: 4px padding above and below a 16 px button (the span is shorter): 24 px, not 30.8.
    let h = heights();
    assert!(close(h["item"], 24.0), "the item row is 24 px, got {}", h["item"]);
}

#[test]
fn an_inline_block_with_line_height_1_is_the_control() {
    let h = heights();
    assert!(close(h["sp"], 16.0), "a line-height:1 inline-block at 16px is 16 px, got {}", h["sp"]);
}
