//! A float with `clear` starts a new line of floats, so
//! `estimate_max_content_width` ends the run on the cleared side instead of
//! summing stacked clears (independent R1 HIGH on #1079).
//!
//! Engine `cleared_float_width_tests` pins the full Chromium page under
//! macos+headless only. These layout units call `estimate_max_content_width`
//! directly and run on default Linux `--lib`.
//!
//! Chrome for Testing 148.0.7778.216 sizes from
//! `docs/diagnostics/2026-11-11/cleared_floats_max_content_cft148.txt`
//! (`ib_*` rows): two `clear:left` 100px floats → 100; left 100 + right 60
//! with `clear:right` → 160; no-clear pair → 200; cleared float + fixed
//! inline-block + cleared float → text beside the widest float line.

use super::*;
use rustkit_css::Display;

fn float_box(side: Float, width: f32, clear: Clear) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.width = Length::Px(width);
    s.height = Length::Px(10.0);
    s.float = side;
    s.clear = clear;
    let mut b = LayoutBox::with_float(BoxType::Block, s, side);
    b.clear = clear;
    b
}

fn inline_px(width: f32) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::InlineBlock;
    s.width = Length::Px(width);
    s.height = Length::Px(10.0);
    LayoutBox::new(BoxType::Block, s)
}

fn container(children: Vec<LayoutBox>) -> LayoutBox {
    LayoutBox::new(BoxType::Block, ComputedStyle::new()).with_children(children)
}

trait WithChildren {
    fn with_children(self, children: Vec<LayoutBox>) -> Self;
}

impl WithChildren for LayoutBox {
    fn with_children(mut self, children: Vec<LayoutBox>) -> Self {
        self.children = children;
        self
    }
}

fn close(got: f32, want: f32) {
    assert!(
        (got - want).abs() < 0.5,
        "estimate_max_content_width={got}, Chrome/want={want}"
    );
}

/// #1079 / ib_ll: two `float:left; clear:left; width:100px` → 100, not 200.
#[test]
fn two_clear_left_floats_contribute_one_width_not_their_sum() {
    let root = container(vec![
        float_box(Float::Left, 100.0, Clear::Left),
        float_box(Float::Left, 100.0, Clear::Left),
    ]);
    close(crate::grid::estimate_max_content_width(&root), 100.0);
}

/// #1079 / ib_none: without clear, the same floats still sum to 200.
#[test]
fn two_uncleared_left_floats_still_sum() {
    let root = container(vec![
        float_box(Float::Left, 100.0, Clear::None),
        float_box(Float::Left, 100.0, Clear::None),
    ]);
    close(crate::grid::estimate_max_content_width(&root), 200.0);
}

/// #1079 / ib_lr2: `clear:right` resets only the right sum (100+60 → 160).
#[test]
fn clear_right_keeps_the_left_float_sum() {
    let root = container(vec![
        float_box(Float::Left, 100.0, Clear::None),
        float_box(Float::Right, 60.0, Clear::Right),
    ]);
    close(crate::grid::estimate_max_content_width(&root), 160.0);
}

/// #1079 / ib_l_r_cl: clear:left after left+right keeps the prior side sum
/// as the widest closed line (100+60), then a 30 left sits on a new line.
#[test]
fn clear_left_keeps_the_right_float_sum_as_widest_line() {
    let root = container(vec![
        float_box(Float::Left, 100.0, Clear::None),
        float_box(Float::Right, 60.0, Clear::None),
        float_box(Float::Left, 30.0, Clear::Left),
    ]);
    close(crate::grid::estimate_max_content_width(&root), 160.0);
}

/// #1079 / ib_three: clear:left, then 40 beside it, then clear:left 70 → 140.
#[test]
fn a_cleared_line_then_a_sibling_float_takes_the_widest_line() {
    let root = container(vec![
        float_box(Float::Left, 100.0, Clear::Left),
        float_box(Float::Left, 40.0, Clear::None),
        float_box(Float::Left, 70.0, Clear::Left),
    ]);
    close(crate::grid::estimate_max_content_width(&root), 140.0);
}

/// Portable stand-in for ib_txt: a fixed inline-block (not font-metric
/// "text") sits beside the widest cleared-float line → 100 + 25 = 125.
#[test]
fn inline_content_sits_beside_the_widest_cleared_float_line() {
    let root = container(vec![
        float_box(Float::Left, 100.0, Clear::Left),
        inline_px(25.0),
        float_box(Float::Left, 100.0, Clear::Left),
    ]);
    close(crate::grid::estimate_max_content_width(&root), 125.0);
}
