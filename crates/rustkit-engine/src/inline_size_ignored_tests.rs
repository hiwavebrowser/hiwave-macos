//! `width` / `height` do not apply to a non-replaced inline (CSS 2 §10.3.1,
//! §10.6.1): Chrome sizes the box by its content, whatever they say
//! (INLINE-SIZE-IGNORED). RustKit gave the box the declared size, so a
//! `span{width:300px;height:300px;background:red}` painted a 330x300 block
//! where Chrome paints a word: five pairs that differ only by the
//! declaration paint byte-identically in Chrome 148 and differ by 458 to
//! 91,445 pixels in RustKit (background, text after the span, border,
//! outline, a wrapped span).
//!
//! They still size an inline-block, a replaced element, and a flex or grid
//! item (blockified). Numbers: Chrome for Testing 148.0.7778.216 (linux64),
//! `getBoundingClientRect` on docs/diagnostics/2026-11-24/inline_size_ignored.html
//! (receipt: inline_size_ignored_cft148.txt): plain 8x17 and pct 8x17 (text),
//! inline-block 300x30, img 40x30, flex item 120x30, grid item 120x30.

use super::*;
use rustkit_layout::{Dimensions, Rect};

fn by_id<'a>(b: &'a LayoutBox, id: &str) -> Option<&'a LayoutBox> {
    if b.identity.as_ref().is_some_and(|i| i.selector == format!("#{id}")) {
        return Some(b);
    }
    b.children.iter().find_map(|c| by_id(c, id))
}

fn laid_out(body: &str) -> LayoutBox {
    let e = Engine::new(EngineConfig::default()).expect("engine");
    let html = format!(r#"<!doctype html><html><body style="margin:0;font:16px/20px sans-serif">{body}</body></html>"#);
    let d = Document::parse_html(&html).expect("parse");
    let mut root = e.build_layout_from_document(&d, &[]);
    root.layout_with_collapse(
        &Dimensions { content: Rect::new(0.0, 0.0, 600.0, 0.0), ..Default::default() },
        &mut rustkit_layout::MarginCollapseContext::new(),
        &mut rustkit_layout::FloatContext::new(),
    );
    root
}

fn rect(root: &LayoutBox, id: &str) -> (f32, f32, f32, f32) {
    let b = by_id(root, id).unwrap_or_else(|| panic!("#{id} has no box"));
    let r = b.dimensions.border_box();
    (r.x, r.y, r.width, r.height)
}

/// The span, and what comes after it on its line and below it, are where
/// they are without the declaration (Chrome: byte-identical frames).
#[test]
fn width_and_height_do_not_size_a_non_replaced_inline() {
    let with = laid_out(
        r#"<div>aaa <span id="s" style="width:300px;height:300px;background:#f00">x</span> bbb<span id="t">T</span></div><div id="n">next</div>"#,
    );
    let without = laid_out(r#"<div>aaa <span id="s" style="background:#f00">x</span> bbb<span id="t">T</span></div><div id="n">next</div>"#);
    for id in ["s", "t", "n"] {
        assert_eq!(rect(&with, id), rect(&without, id), "#{id}");
    }
}

/// Percentages likewise, and with a border and padding the box is still its
/// content plus them.
#[test]
fn percentages_and_decorations_do_not_bring_them_back() {
    let with = laid_out(
        r#"<div style="height:200px"><span id="s" style="width:50%;height:50%;border:2px solid;padding:3px">x</span></div>"#,
    );
    let without = laid_out(r#"<div style="height:200px"><span id="s" style="border:2px solid;padding:3px">x</span></div>"#);
    assert_eq!(rect(&with, "s"), rect(&without, "s"));
}

/// Pins: an inline-block, a replaced element and a flex or grid item keep
/// their declared size (Chrome 148: 300x30, 40x30, 120x30, 120x30).
#[test]
fn atomic_inlines_replaced_elements_and_flex_and_grid_items_keep_it() {
    let root = laid_out(
        r#"<div><span id="ib" style="display:inline-block;width:300px;height:30px">x</span></div>
<div><img id="img" style="width:40px;height:30px" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></div>
<div style="display:flex"><span id="fi" style="width:120px;height:30px">x</span></div>
<div style="display:grid"><span id="gi" style="width:120px;height:30px">x</span></div>"#,
    );
    let wh = |id: &str| { let r = rect(&root, id); (r.2, r.3) };
    assert_eq!(wh("ib"), (300.0, 30.0));
    assert_eq!(wh("img"), (40.0, 30.0));
    assert_eq!(wh("fi"), (120.0, 30.0));
    assert_eq!(wh("gi"), (120.0, 30.0));
}

/// #1444 R1 HOLD: replaced elements that RustKit builds as inline boxes (an
/// `<iframe>` with nested contexts off, an `<object>`) keep their CSS size.
/// Chrome for Testing 148 (inline_size_ignored_replaced_cft148.txt): iframe
/// 120x30, object 120x30, the iframe centred in a 600px line at x=240. (An
/// `<embed>` with no src is not rendered in Chrome, 0x0: not pinned.)
#[test]
fn replaced_elements_built_as_inline_boxes_keep_their_size() {
    let root = laid_out(
        r#"<div><iframe id="ifr" style="width:120px;height:30px;border:0"></iframe></div>
<div><object id="obj" style="width:120px;height:30px"></object></div>
<div style="width:600px;text-align:center"><iframe id="ifc" style="width:120px;height:30px;border:0"></iframe></div>"#,
    );
    let r = |id: &str| rect(&root, id);
    assert_eq!((r("ifr").2, r("ifr").3), (120.0, 30.0), "iframe");
    assert_eq!((r("obj").2, r("obj").3), (120.0, 30.0), "object");
    assert_eq!((r("ifc").0, r("ifc").2), (240.0, 120.0), "the centred iframe");
}
