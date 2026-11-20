//! TREE-REUSE step (2) / #1259: classify an inline `style` attribute write as
//! paint-only so a geometry read can answer without a forced layout.
//!
//! The engine oracle (`paint_style_read_tests`) is headless-only. These pins
//! keep `paint_style_reads` / `plain_declarations` on every
//! `cargo test -p rustkit-bindings --lib`.

use crate::dom::{paint_style_reads, plain_declarations, DomHost};
use crate::{LAYER_PAINT, LAYER_STACKING};
use std::rc::Rc;

fn host_without_style_attr_rules() -> DomHost {
    let mut host = DomHost::default();
    host.style_attr_rules = Some(Rc::new(|| false));
    host
}

#[test]
fn colour_and_opacity_writes_are_paint_reads() {
    let host = host_without_style_attr_rules();
    assert_eq!(
        paint_style_reads(&host, Some("color:red"), Some("color:blue")),
        Some(LAYER_PAINT)
    );
    assert_eq!(
        paint_style_reads(&host, Some("opacity:1"), Some("opacity:0.5")),
        Some(LAYER_PAINT | LAYER_STACKING)
    );
    assert_eq!(
        paint_style_reads(
            &host,
            Some("color:red; opacity:1"),
            Some("color:blue; opacity:0.5")
        ),
        Some(LAYER_PAINT | LAYER_STACKING)
    );
    // A paint longhand whose value mentions `var(` is still paint at this
    // gate: only the property name is classified.
    assert_eq!(
        paint_style_reads(&host, None, Some("color: var(--c)")),
        Some(LAYER_PAINT)
    );
}

#[test]
fn a_layout_property_or_shorthand_is_not_paint_only() {
    let host = host_without_style_attr_rules();
    assert_eq!(
        paint_style_reads(&host, Some("width:10px"), Some("width:20px")),
        None
    );
    assert_eq!(
        paint_style_reads(&host, Some("color:red"), Some("color:blue; width:1px")),
        None
    );
    // `background` also sets what lays out; only `background-color` is paint.
    assert_eq!(paint_style_reads(&host, None, Some("background:red")), None);
}

#[test]
fn quoted_comment_or_style_attr_rules_refuse_the_fast_path() {
    let host = host_without_style_attr_rules();
    assert_eq!(paint_style_reads(&host, None, Some("color: \"red\"")), None);
    assert!(plain_declarations("color: /*x*/ red").is_none());
    assert!(plain_declarations("color: red; background: url(a);").is_some());
    assert!(
        plain_declarations("color: rgb(0, 0, 0;").is_none(),
        "a declaration with unbalanced brackets refuses the whole attribute"
    );

    let mut gated = DomHost::default();
    // `None` probe means "assume yes" — refuse.
    assert_eq!(
        paint_style_reads(&gated, Some("color:red"), Some("color:blue")),
        None
    );
    gated.style_attr_rules = Some(Rc::new(|| true));
    assert_eq!(
        paint_style_reads(&gated, Some("color:red"), Some("color:blue")),
        None
    );
}

#[test]
fn an_unchanged_paint_property_is_not_a_read() {
    let host = host_without_style_attr_rules();
    assert_eq!(
        paint_style_reads(&host, Some("color:red"), Some("color:red")),
        None
    );
    // Only the changed paint name contributes bits.
    assert_eq!(
        paint_style_reads(
            &host,
            Some("color:red; opacity:1"),
            Some("color:red; opacity:0.2")
        ),
        Some(LAYER_PAINT | LAYER_STACKING)
    );
}
