//! #1151 R3: a transform, filter, or perspective holds absolute boxes the
//! same way it holds fixed ones. Headless lifted-clip / fixed-zindex
//! oracles cover end-to-end; this pins the predicate on default `--lib`.

use super::*;
use rustkit_css::{Length, TransformOp, STACKS_PERSPECTIVE};

fn block() -> LayoutBox {
    LayoutBox::new(BoxType::Block, ComputedStyle::new())
}

#[test]
fn a_static_box_without_a_holding_effect_does_not_hold_absolutes() {
    let b = block();
    assert!(!b.holds_absolute_boxes());
    assert!(!b.holds_fixed_boxes());
}

#[test]
fn position_relative_holds_absolutes_even_when_laid_out_as_static() {
    // Layout stores relative as Static and keeps the computed style's Relative.
    let mut b = block();
    b.style.position = rustkit_css::Position::Relative;
    assert!(b.holds_absolute_boxes());
    assert!(!b.holds_fixed_boxes(), "relative alone does not hold fixed");

    let mut laid = block();
    laid.position = Position::Relative;
    assert!(laid.holds_absolute_boxes());
}

#[test]
fn transform_filter_and_perspective_hold_absolutes_like_fixed() {
    let mut transformed = block();
    transformed
        .style
        .transform
        .ops
        .push(TransformOp::TranslateX(Length::Px(1.0)));
    assert!(transformed.holds_fixed_boxes());
    assert!(transformed.holds_absolute_boxes());

    let mut filtered = block();
    filtered.style.filter = "blur(2px)".into();
    assert!(filtered.holds_fixed_boxes());
    assert!(filtered.holds_absolute_boxes());

    let mut perspective = block();
    perspective.style.stacks |= STACKS_PERSPECTIVE;
    assert!(perspective.holds_fixed_boxes());
    assert!(perspective.holds_absolute_boxes());
}
