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
/// height, as it does for `height: auto` (RustKit had 20, the line). Despite
/// the name, this pins both sides: the width fills AND the height follows
/// the ratio.
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

/// A `calc()` with no percentage term is a length: it drives the width even
/// under an auto-height parent (Chrome: `height: calc(2em + 4px);
/// aspect-ratio: 2` at 16px is 72x36 in the 600px parent).
#[test]
fn a_calc_height_without_a_percentage_drives_the_width_like_a_length() {
    let got = sizes(r#"<div><div id="c" style="height:calc(2em + 4px);aspect-ratio:2">x</div></div>"#, &["c"]);
    assert_eq!(got, vec![("c".to_string(), 72.0, 36.0)]);
}

/// LIMIT (Prometheus's nit 2 on #1430, measured rather than pinned as "fills"):
/// `min()` / `clamp()` over a percentage in a 200px parent are 200x100 in
/// Chrome (the percentage resolves to 100 and drives the width). RustKit
/// resolves the height itself wrongly here (300, the clamp's maximum) and
/// fills the width: a math-function height follow-up, outside this PR.
#[test]
#[ignore = "min()/clamp() heights over a percentage resolve wrongly (height-side math-function follow-up)"]
fn a_math_function_height_over_a_percentage_of_a_definite_parent() {
    let got = sizes(
        r#"<div style="height:200px"><div id="cl" style="height:clamp(10px, 50%, 300px);aspect-ratio:2">x</div></div>
<div style="height:200px"><div id="mn" style="height:min(50%, 300px);aspect-ratio:2">x</div></div>"#,
        &["cl", "mn"],
    );
    let want = [("cl", 200.0, 100.0), ("mn", 200.0, 100.0)];
    let want: Vec<(String, f32, f32)> = want.iter().map(|(i, w, h)| (i.to_string(), *w, *h)).collect();
    assert_eq!(got, want);
}

/// FLOAT-ASPECT-RATIO: a box whose auto width would shrink to fit (float,
/// inline-block, absolutely positioned, table) still takes its width from a
/// definite height across the ratio. RustKit shrank every one to its content
/// (8px for "x"). Chrome for Testing 148 linux64, aspect-ratio 2 unless noted:
/// - float, height 100px: 200x100; 50% of a 200px parent: 200x100;
///   border-box with padding 10: 200x100; ratio 10: 1000x100 (not clamped);
///   with max-width 150: 150x100; ratio 1 with min-width 300: 300x100;
/// - inline-block, 100px and 50% of 200px: 200x100;
/// - absolutely positioned (left only, right only, and left+right), 100px and
///   50% of a 200px positioned parent: 200x100 (the ratio wins over the
///   insets' stretch);
/// - table, height 100px: 200x100.
#[test]
fn a_shrink_to_fit_box_takes_its_width_from_a_definite_height() {
    let got = sizes(
        r#"<div style="height:120px"><div id="fl_len" style="float:left;height:100px;aspect-ratio:2">x</div></div>
<div style="height:200px"><div id="fl_pct" style="float:left;height:50%;aspect-ratio:2">x</div></div>
<div style="height:120px"><div id="fl_bb" style="float:left;height:100px;aspect-ratio:2;padding:10px;box-sizing:border-box">x</div></div>
<div style="height:120px"><div id="fl_wide" style="float:left;height:100px;aspect-ratio:10">x</div></div>
<div style="height:120px"><div id="fl_maxw" style="float:left;height:100px;aspect-ratio:2;max-width:150px">x</div></div>
<div style="height:120px"><div id="fl_minw" style="float:left;height:100px;aspect-ratio:1;min-width:300px">x</div></div>
<div style="height:120px"><div id="ib_len" style="display:inline-block;height:100px;aspect-ratio:2">x</div></div>
<div style="height:200px"><div id="ib_pct" style="display:inline-block;height:50%;aspect-ratio:2">x</div></div>
<div style="position:relative;height:120px"><div id="ab_len" style="position:absolute;top:0;left:0;height:100px;aspect-ratio:2">x</div></div>
<div style="position:relative;height:200px"><div id="ab_pct" style="position:absolute;top:0;left:0;height:50%;aspect-ratio:2">x</div></div>
<div style="position:relative;height:120px"><div id="ab_both" style="position:absolute;top:0;left:0;right:0;height:100px;aspect-ratio:2">x</div></div>
<div style="position:relative;height:120px"><div id="ab_right" style="position:absolute;top:0;right:0;height:100px;aspect-ratio:2">x</div></div>
<div style="height:120px"><table id="tb" style="height:100px;aspect-ratio:2;border-spacing:0"><tr><td>x</td></tr></table></div>"#,
        &["fl_len", "fl_pct", "fl_bb", "fl_wide", "fl_maxw", "fl_minw", "ib_len", "ib_pct", "ab_len", "ab_pct", "ab_both", "ab_right", "tb"],
    );
    let want = [
        ("fl_len", 200.0, 100.0), ("fl_pct", 200.0, 100.0), ("fl_bb", 200.0, 100.0),
        ("fl_wide", 1000.0, 100.0), ("fl_maxw", 150.0, 100.0), ("fl_minw", 300.0, 100.0),
        ("ib_len", 200.0, 100.0), ("ib_pct", 200.0, 100.0),
        ("ab_len", 200.0, 100.0), ("ab_pct", 200.0, 100.0), ("ab_both", 200.0, 100.0), ("ab_right", 200.0, 100.0),
        ("tb", 200.0, 100.0),
    ];
    let want: Vec<(String, f32, f32)> = want.iter().map(|(i, w, h)| (i.to_string(), *w, *h)).collect();
    assert_eq!(got, want);
}

/// `sizes` with the x of each border box too.
fn rects(body: &str, ids: &[&str]) -> Vec<(String, f32, f32, f32)> {
    let e = Engine::new(EngineConfig::default()).expect("engine");
    let html = format!(
        r#"<!doctype html><html><body style="margin:0;font:16px/20px sans-serif"><div style="width:600px">{body}</div></body></html>"#
    );
    let d = Document::parse_html(&html).expect("parse");
    let mut root = e.build_layout_from_document(&d, &[]);
    root.layout_with_collapse(
        &Dimensions { content: Rect::new(0.0, 0.0, 600.0, 0.0), ..Default::default() },
        &mut rustkit_layout::MarginCollapseContext::new(),
        &mut rustkit_layout::FloatContext::new(),
    );
    ids.iter()
        .map(|id| {
            let b = by_id(&root, id).unwrap_or_else(|| panic!("#{id} has no box"));
            let r = b.dimensions.border_box();
            (id.to_string(), r.x, r.width, r.height)
        })
        .collect()
}

/// #1443 R1 nit 1: when re-anchoring gives an absolutely positioned box its
/// aspect-ratio width from a percentage height, its children are laid out at
/// that width. Chrome for Testing 148 (docs/diagnostics/2026-11-24/
/// far_nits_cft148.txt): a centred 8px inline-block in the 200x100 box is at
/// x=96; at ratio 0.5 (50x100) the text wraps exactly as in the same box with
/// `height:100px` (both 80 tall there; equal heights, the font aside).
#[test]
fn a_reanchored_ratio_width_lays_its_children_out_at_that_width() {
    let got = rects(
        r#"<div style="position:relative;height:200px"><div id="cen" style="position:absolute;top:0;left:0;height:50%;aspect-ratio:2;text-align:center"><span id="ci" style="display:inline-block;width:8px;height:8px"></span></div></div>
<div style="position:relative;height:200px"><div id="nar" style="position:absolute;top:0;left:0;height:50%;aspect-ratio:0.5"><div id="nt">hello world foo bar</div></div></div>
<div style="position:relative;height:200px"><div id="twin" style="position:absolute;top:0;left:0;height:100px;aspect-ratio:0.5"><div id="tt">hello world foo bar</div></div></div>"#,
        &["cen", "ci", "nar", "nt", "twin", "tt"],
    );
    let get = |id: &str| got.iter().find(|r| r.0 == id).cloned().unwrap();
    assert_eq!((get("cen").2, get("cen").3), (200.0, 100.0));
    assert_eq!(get("ci").1 - get("cen").1, 96.0, "the centred child");
    assert_eq!((get("nar").2, get("nar").3), (50.0, 100.0));
    assert_eq!(get("nt").2, 50.0, "the text block is the box's width");
    assert_eq!(get("nt").3, get("tt").3, "it wraps as the height:100px twin does");
    assert!(get("tt").3 > 20.0, "and that is more than one line");
}

/// #1443 R1 nit 2: right-only and left+right at a percentage height too.
/// Chrome for Testing 148: right:0 at x=400, 200x100; left:0;right:0 at x=0,
/// 200x100 (the ratio width wins over the stretch).
#[test]
fn right_and_both_insets_at_a_percentage_height_take_the_ratio_width() {
    let got = rects(
        r#"<div style="position:relative;height:200px"><div id="abr" style="position:absolute;top:0;right:0;height:50%;aspect-ratio:2">x</div></div>
<div style="position:relative;height:200px"><div id="abb" style="position:absolute;top:0;left:0;right:0;height:50%;aspect-ratio:2">x</div></div>"#,
        &["abr", "abb"],
    );
    assert_eq!(got[0], ("abr".to_string(), 400.0, 200.0, 100.0));
    assert_eq!(got[1], ("abb".to_string(), 0.0, 200.0, 100.0));
}
