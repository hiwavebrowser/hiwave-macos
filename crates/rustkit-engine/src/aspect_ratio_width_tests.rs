//! A block with `aspect-ratio`, an auto width and a definite height takes its
//! width from the height (css-sizing-4 §5.1), as in Chrome.
//!
//! RustKit filled the containing block instead: `height:100px; aspect-ratio:1`
//! was 600 wide in a 600px parent, where Chrome makes it 100. Found through
//! the empty-block change (a container query under such a box was measured
//! at 600 and fired where Chrome's does not).
//!
//! Numbers: Chrome for Testing 148.0.7778.216 (linux64), `getBoundingClientRect`,
//! all inside a 600px-wide parent.

use super::*;
use rustkit_layout::{Dimensions, Rect};

fn by_id<'a>(b: &'a LayoutBox, id: &str) -> Option<&'a LayoutBox> {
    if b.identity
        .as_ref()
        .is_some_and(|i| i.selector == format!("#{id}"))
    {
        return Some(b);
    }
    b.children.iter().find_map(|c| by_id(c, id))
}

/// (width, height) of each `#id`'s border box.
fn sizes(body: &str, ids: &[&str]) -> Vec<(String, f32, f32)> {
    let e = Engine::new(EngineConfig::default()).expect("engine");
    let html = format!(
        r#"<!doctype html><html><body style="margin:0;font:16px/20px sans-serif"><div style="width:600px">{body}</div></body></html>"#
    );
    let d = Document::parse_html(&html).expect("parse");
    let mut root = e.build_layout_from_document(&d, &[]);
    root.layout_with_collapse(
        &Dimensions {
            content: Rect::new(0.0, 0.0, 600.0, 0.0),
            ..Default::default()
        },
        &mut rustkit_layout::MarginCollapseContext::new(),
        &mut rustkit_layout::FloatContext::new(),
    );
    ids.iter()
        .map(|id| {
            let b = by_id(&root, id).unwrap_or_else(|| panic!("#{id} has no box"));
            let r = b.dimensions.border_box();
            (id.to_string(), r.width, r.height)
        })
        .collect()
}

#[test]
fn an_auto_width_follows_a_definite_height_across_the_ratio() {
    let got = sizes(
        r#"<div id="plain" style="height:100px;aspect-ratio:1">x</div>
<div id="cb" style="height:100px;aspect-ratio:2;padding:10px">x</div>
<div id="bb" style="height:100px;aspect-ratio:2;padding:10px;box-sizing:border-box">x</div>
<div id="maxw" style="height:100px;aspect-ratio:2;max-width:150px">x</div>
<div id="minw" style="height:100px;aspect-ratio:1;min-width:300px">x</div>
<div id="wide" style="height:100px;aspect-ratio:10">x</div>
<div id="ratioauto" style="height:100px;aspect-ratio:auto 1">x</div>"#,
        &["plain", "cb", "bb", "maxw", "minw", "wide", "ratioauto"],
    );
    let want = [
        ("plain", 100.0, 100.0),
        // content-box: the ratio is the content box's (200x100), plus 20 padding.
        ("cb", 220.0, 120.0),
        // border-box: the ratio is the border box's.
        ("bb", 200.0, 100.0),
        ("maxw", 150.0, 100.0),
        ("minw", 300.0, 100.0),
        // Not clamped to the containing block.
        ("wide", 1000.0, 100.0),
        ("ratioauto", 100.0, 100.0),
    ];
    let want: Vec<(String, f32, f32)> = want.iter().map(|(i, w, h)| (i.to_string(), *w, *h)).collect();
    assert_eq!(got, want);
}

/// A percentage height against an indefinite containing block is not
/// definite: the width fills the parent and the height follows it (Chrome:
/// 600x600). The percentage computes to `auto`, so the ratio gives the
/// height, as it does for `height: auto` (RustKit had 20, the line).
#[test]
fn a_percentage_height_of_an_auto_height_parent_does_not_drive_the_width() {
    let got = sizes(
        r#"<div id="pct" style="height:50%;aspect-ratio:1">x</div>
<div id="pct2" style="height:50%;aspect-ratio:2">x</div>
<div id="calc" style="height:calc(50% + 10px);aspect-ratio:2">x</div>
<div id="bb" style="height:50%;aspect-ratio:2;padding:10px;box-sizing:border-box">x</div>"#,
        &["pct", "pct2", "calc", "bb"],
    );
    // A percentage inside calc() is indefinite too; under border-box the ratio is the border box's.
    let want = [("pct", 600.0, 600.0), ("pct2", 600.0, 300.0), ("calc", 600.0, 300.0), ("bb", 600.0, 300.0)];
    let want: Vec<(String, f32, f32)> = want.iter().map(|(i, w, h)| (i.to_string(), *w, *h)).collect();
    assert_eq!(got, want);
}

/// Against a definite containing block the percentage is definite, so it
/// drives the width across the ratio like a length (Chrome, each in a
/// `height:200px` parent inside the 600px one): 50%/2 200x100;
/// calc(50% + 10px)/2 220x110; 100%/10 2000x200 (not clamped to the
/// parent); border-box with padding 200x100.
#[test]
fn a_percentage_height_of_a_definite_parent_drives_the_width() {
    let got = sizes(
        r#"<div style="height:200px"><div id="d1" style="height:50%;aspect-ratio:2">x</div></div>
<div style="height:200px"><div id="d2" style="height:calc(50% + 10px);aspect-ratio:2">x</div></div>
<div style="height:200px"><div id="d3" style="height:100%;aspect-ratio:10">x</div></div>
<div style="height:200px"><div id="d4" style="height:50%;aspect-ratio:2;padding:10px;box-sizing:border-box">x</div></div>"#,
        &["d1", "d2", "d3", "d4"],
    );
    let want = [("d1", 200.0, 100.0), ("d2", 220.0, 110.0), ("d3", 2000.0, 200.0), ("d4", 200.0, 100.0)];
    let want: Vec<(String, f32, f32)> = want.iter().map(|(i, w, h)| (i.to_string(), *w, *h)).collect();
    assert_eq!(got, want);
}

/// A grid item stretched to a 200px row has a definite height (css-grid-1
/// §11.6), so its child's percentage drives the width the same way (Chrome:
/// item 600x200, child 50%/2 200x100). The grid's pass that re-sizes an
/// item's block children dropped that base and filled the item.
#[test]
fn a_percentage_height_in_a_stretched_grid_item_drives_the_width() {
    let got = sizes(
        r#"<div style="display:grid;grid-template-rows:200px"><div id="gi"><div id="g1" style="height:50%;aspect-ratio:2">x</div></div></div>"#,
        &["gi", "g1"],
    );
    let want = [("gi", 600.0, 200.0), ("g1", 200.0, 100.0)];
    let want: Vec<(String, f32, f32)> = want.iter().map(|(i, w, h)| (i.to_string(), *w, *h)).collect();
    assert_eq!(got, want);
}
