//! CSS-TRANS A2 follow-up: an `<svg>` whose paint map entry (or a descendant's)
//! holds a non-empty `animated` string is a moving cache key. Only those svgs
//! must be collected; an empty `animated` is a no-op. Zero unit callers before
//! this file — the sole prior path is the inline-SVG cache pass.

use super::*;
use std::collections::HashMap;

fn paint_animated(animated: &str) -> SvgPaintStyle {
    let mut s = SvgPaintStyle::default();
    s.animated = animated.to_string();
    s
}

#[test]
fn an_animated_child_marks_its_ancestor_svg() {
    let document = Document::parse_html(
        r#"<!doctype html><svg id=outer><rect id=r width=10 height=10/></svg>"#,
    )
    .expect("parse");
    let svg = document.get_element_by_id("outer").expect("svg");
    let rect = document.get_element_by_id("r").expect("rect");
    let mut paint = HashMap::new();
    paint.insert(rect.id.raw(), paint_animated("fill:rgb(255, 0, 0)"));

    let holding = layout_ctx::LayoutCtx::svgs_holding_animated(&document, &paint);
    assert_eq!(holding, std::collections::HashSet::from([svg.id.raw()]));
}

#[test]
fn an_empty_animated_string_marks_no_svg() {
    let document = Document::parse_html(
        r#"<!doctype html><svg id=outer><rect id=r width=10 height=10/></svg>"#,
    )
    .expect("parse");
    let rect = document.get_element_by_id("r").expect("rect");
    let mut paint = HashMap::new();
    paint.insert(rect.id.raw(), paint_animated(""));

    let holding = layout_ctx::LayoutCtx::svgs_holding_animated(&document, &paint);
    assert!(holding.is_empty());
}

#[test]
fn a_nested_svg_marks_every_svg_ancestor() {
    let document = Document::parse_html(
        r#"<!doctype html><svg id=outer><svg id=inner><circle id=c r=4/></svg></svg>"#,
    )
    .expect("parse");
    let outer = document.get_element_by_id("outer").expect("outer");
    let inner = document.get_element_by_id("inner").expect("inner");
    let circle = document.get_element_by_id("c").expect("circle");
    let mut paint = HashMap::new();
    paint.insert(circle.id.raw(), paint_animated("stroke:rgb(0, 0, 255)"));

    let holding = layout_ctx::LayoutCtx::svgs_holding_animated(&document, &paint);
    assert_eq!(
        holding,
        std::collections::HashSet::from([outer.id.raw(), inner.id.raw()])
    );
}
