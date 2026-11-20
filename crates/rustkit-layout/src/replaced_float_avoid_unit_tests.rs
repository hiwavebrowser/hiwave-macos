//! CSS 2.1 §9.5 / row REPLACED-FLOAT-AVOID (#1356): a block-level replaced box or a
//! BFC (`overflow:hidden`, `flow-root`) must not overlap a float. Its own margins may
//! lie over the float; only the float past those margins narrows the box (R1: a 20px
//! `margin-left` beside a 100px float stays 300 wide at x=100; a 150px margin is 250
//! wide at x=150). The sole prior pin is the headless engine oracle
//! `replaced_float_avoid_tests` (Chrome for Testing 148.0.7778.216 receipt
//! `docs/diagnostics/2026-11-19/replaced_float_avoid_cft148.json`).
//!
//! These cases use fixed-size boxes (no text metrics) so Linux default `--lib` catches
//! a silent revert of `narrow_beside_floats` / `place_beside_floats`. Each case runs
//! through both `layout` and `layout_with_collapse`.

use super::*;
use rustkit_css::{Display, DisplayModifier, Overflow};

const EPS: f32 = 0.5;

fn float_box(side: Float, width: f32, height: f32) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.width = Length::Px(width);
    s.height = Length::Px(height);
    s.float = side;
    LayoutBox::with_float(BoxType::Block, s, side)
}

fn bfc(edit: impl Fn(&mut ComputedStyle)) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.overflow_x = Overflow::Hidden;
    s.height = Length::Px(40.0);
    edit(&mut s);
    LayoutBox::new(BoxType::Block, s)
}

fn block_image(width: f32, height: f32, edit: impl Fn(&mut ComputedStyle)) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Block;
    s.width = Length::Px(width);
    s.height = Length::Px(height);
    edit(&mut s);
    LayoutBox::new(
        BoxType::Image {
            url: String::new(),
            natural_width: width,
            natural_height: height,
        },
        s,
    )
}

fn laid_out(children: Vec<LayoutBox>, collapse: bool) -> LayoutBox {
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.children = children;
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 400.0, 0.0),
        ..Default::default()
    };
    if collapse {
        let mut mc = MarginCollapseContext::new();
        let mut fc = FloatContext::new();
        body.layout_with_collapse(&cb, &mut mc, &mut fc);
    } else {
        body.layout(&cb);
    }
    body
}

fn for_both(name: &str, check: impl Fn(&str, bool)) {
    for collapse in [false, true] {
        let label = if collapse {
            format!("{name} (collapse)")
        } else {
            format!("{name} (layout)")
        };
        check(&label, collapse);
    }
}

fn assert_box(name: &str, b: &LayoutBox, x: f32, y: f32, w: f32, h: f32) {
    let r = b.dimensions.border_box();
    assert!(
        (r.x - x).abs() < EPS
            && (r.y - y).abs() < EPS
            && (r.width - w).abs() < EPS
            && (r.height - h).abs() < EPS,
        "{name}: got {},{},{},{} want {},{},{},{}",
        r.x,
        r.y,
        r.width,
        r.height,
        x,
        y,
        w,
        h
    );
}

/// R1 media-object: `margin-left: 20px` beside a 100px left float → border box at
/// 100 with width 300 (the margin lies over the float; the band is not shortened by
/// the whole margin).
#[test]
fn a_bfc_margin_beside_a_float_keeps_the_band_width() {
    for_both("overflow:hidden; margin-left:20 beside float:left 100", |name, collapse| {
        let body = laid_out(
            vec![
                float_box(Float::Left, 100.0, 80.0),
                bfc(|s| s.margin_left = Length::Px(20.0)),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 100.0, 0.0, 300.0, 40.0);
    });
}

/// R1: `margin-left: 150px` past a 100px float → the border box starts at 150 and is
/// only 250 wide (the margin past the float does narrow).
#[test]
fn a_bfc_margin_past_a_float_narrows_the_remaining_band() {
    for_both("overflow:hidden; margin-left:150 beside float:left 100", |name, collapse| {
        let body = laid_out(
            vec![
                float_box(Float::Left, 100.0, 80.0),
                bfc(|s| s.margin_left = Length::Px(150.0)),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 150.0, 0.0, 250.0, 40.0);
    });
}

/// Mirror of the left-margin pin for a right float + `margin-right: 20px`.
#[test]
fn a_bfc_end_margin_beside_a_right_float_keeps_the_band_width() {
    for_both("overflow:hidden; margin-right:20 beside float:right 100", |name, collapse| {
        let body = laid_out(
            vec![
                float_box(Float::Right, 100.0, 80.0),
                bfc(|s| s.margin_right = Length::Px(20.0)),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 0.0, 0.0, 300.0, 40.0);
    });
}

/// A block-level replaced box with `margin-left: 20px` sits at the float's edge
/// (margin over the float), not at 20.
#[test]
fn a_block_image_margin_beside_a_float_starts_at_the_float_edge() {
    for_both("img block; margin-left:20 beside float:left 100", |name, collapse| {
        let body = laid_out(
            vec![
                float_box(Float::Left, 100.0, 80.0),
                block_image(150.0, 40.0, |s| s.margin_left = Length::Px(20.0)),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 100.0, 0.0, 150.0, 40.0);
    });
}

/// Negative start margin beside a right float is clamped to the band (x=0), not
/// pushed before it.
#[test]
fn a_negative_start_margin_beside_a_right_float_is_clamped_to_the_band() {
    for_both("img block; margin-left:-20 beside float:right 100", |name, collapse| {
        let body = laid_out(
            vec![
                float_box(Float::Right, 100.0, 80.0),
                block_image(150.0, 40.0, |s| s.margin_left = Length::Px(-20.0)),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 0.0, 0.0, 150.0, 40.0);
    });
}

/// Moved below every float, a negative start margin is kept (Chrome: -20 at y=80).
#[test]
fn a_negative_start_margin_below_a_float_is_kept() {
    for_both("img 350 wide; margin-left:-20 below float:left 100", |name, collapse| {
        let body = laid_out(
            vec![
                float_box(Float::Left, 100.0, 80.0),
                block_image(350.0, 40.0, |s| s.margin_left = Length::Px(-20.0)),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], -20.0, 80.0, 350.0, 40.0);
    });
}

/// Control: a plain block still runs under the float at full width.
#[test]
fn a_plain_block_still_overlaps_the_float() {
    for_both("plain block under float:left 100", |name, collapse| {
        let mut s = ComputedStyle::new();
        s.height = Length::Px(40.0);
        let body = laid_out(
            vec![
                float_box(Float::Left, 100.0, 80.0),
                LayoutBox::new(BoxType::Block, s),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 0.0, 0.0, 400.0, 40.0);
    });
}

/// `display: flow-root` is a float-avoiding BFC even without overflow.
#[test]
fn a_flow_root_sits_beside_the_float() {
    for_both("display:flow-root beside float:left 100", |name, collapse| {
        let mut s = ComputedStyle::new();
        s.display = Display::Block;
        s.display_modifier = DisplayModifier::FlowRoot;
        s.height = Length::Px(40.0);
        let body = laid_out(
            vec![
                float_box(Float::Left, 100.0, 80.0),
                LayoutBox::new(BoxType::Block, s),
            ],
            collapse,
        );
        assert_box(name, &body.children[1], 100.0, 0.0, 300.0, 40.0);
    });
}
