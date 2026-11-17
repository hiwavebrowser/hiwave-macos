//! Portable pins for LPF2 (`keeps_what_it_establishes`): a transform or
//! opacity that stays established is paint; one that comes or goes is not.
//!
//! The page oracles for LPF1/2 are headless-only; default Linux `--lib` never
//! asserted this gate on hand-built styles.

use super::*;
use rustkit_css::{Length, TransformList, TransformOp};

fn style_with(transform: TransformList, opacity: f32) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.transform = transform;
    style.opacity = opacity;
    style
}

fn translated() -> TransformList {
    TransformList {
        ops: vec![TransformOp::TranslateX(Length::Px(10.0))],
    }
}

#[test]
fn empty_transforms_and_full_opacity_keep_what_they_establish() {
    let a = style_with(TransformList::none(), 1.0);
    let b = style_with(TransformList::none(), 1.0);
    assert!(keeps_what_it_establishes(&a, &b));
}

#[test]
fn a_transform_that_stays_a_transform_is_paint() {
    let a = style_with(translated(), 1.0);
    let mut b = style_with(translated(), 1.0);
    b.transform.ops = vec![TransformOp::TranslateY(Length::Px(20.0))];
    assert!(keeps_what_it_establishes(&a, &b));
}

#[test]
fn a_transform_that_comes_or_goes_is_not_paint() {
    let none = style_with(TransformList::none(), 1.0);
    let some = style_with(translated(), 1.0);
    assert!(!keeps_what_it_establishes(&none, &some));
    assert!(!keeps_what_it_establishes(&some, &none));
}

#[test]
fn opacity_that_stays_below_one_or_at_one_is_paint() {
    let a = style_with(TransformList::none(), 0.4);
    let b = style_with(TransformList::none(), 0.9);
    assert!(keeps_what_it_establishes(&a, &b));

    let full_a = style_with(TransformList::none(), 1.0);
    let full_b = style_with(TransformList::none(), 1.0);
    assert!(keeps_what_it_establishes(&full_a, &full_b));
}

#[test]
fn opacity_crossing_one_is_not_paint() {
    let translucent = style_with(TransformList::none(), 0.5);
    let opaque = style_with(TransformList::none(), 1.0);
    assert!(!keeps_what_it_establishes(&translucent, &opaque));
    assert!(!keeps_what_it_establishes(&opaque, &translucent));
}
