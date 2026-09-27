//! css-flexbox-1 §9.7, resolving flexible lengths: free space is measured
//! from each item's flex BASE size, inflexible items freeze first, and min/max
//! violations freeze and hand their share back to the rest. Every expected
//! number was measured on pinned Chrome for Testing 148 at a 400px viewport
//! (`getBoundingClientRect`); each case runs through both entry points,
//! `layout` and `layout_with_collapse`.

use super::*;
use rustkit_css::{AlignItems, Display, FlexBasis, JustifyContent};

fn row() -> ComputedStyle {
    let mut s = ComputedStyle::new();
    s.display = Display::Flex;
    s.height = Length::Px(10.0);
    s
}

fn item(grow: f32, shrink: f32, basis: FlexBasis) -> ComputedStyle {
    let mut s = ComputedStyle::new();
    s.flex_grow = grow;
    s.flex_shrink = shrink;
    s.flex_basis = basis;
    s
}

fn zero_pct() -> ComputedStyle {
    item(1.0, 1.0, FlexBasis::Percent(0.0))
}

fn sized_block(w: f32, h: f32) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.width = Length::Px(w);
    s.height = Length::Px(h);
    LayoutBox::new(BoxType::Block, s)
}

/// Lays the flex `container` out in a block `<body>` 400px wide through the
/// named entry point and returns it.
fn laid_out(container: LayoutBox, collapse: bool) -> LayoutBox {
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.children.push(container);
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
    body.children.remove(0)
}

/// Lays out a row of `items` and returns each item's (x, width).
fn row_of(items: Vec<LayoutBox>, collapse: bool) -> Vec<(f32, f32)> {
    let mut c = LayoutBox::new(BoxType::Block, row());
    c.children = items;
    laid_out(c, collapse)
        .children
        .iter()
        .map(|b| (b.dimensions.content.x, b.dimensions.content.width))
        .collect()
}

fn assert_row(got: &[(f32, f32)], want: &[(f32, f32)], what: &str) {
    let close = got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(g, w)| (g.0 - w.0).abs() < 0.5 && (g.1 - w.1).abs() < 0.5);
    assert!(
        close,
        "{what}: Chrome 148 has (x, width) {want:?}, got {got:?}"
    );
}

/// `flex: 1 1 0%` twice, one item holding a 50px box: 200/200, not 175/225.
/// The automatic minimum (50) lifts that item's hypothetical size, and free
/// space measured from hypothetical sizes gave it 50 extra.
#[test]
fn basis_zero_items_split_the_row_evenly_whatever_their_content() {
    for collapse in [false, true] {
        let mut a1 = zero_pct();
        a1.height = Length::Px(100.0);
        let mut a2 = LayoutBox::new(BoxType::Block, zero_pct());
        a2.children.push(sized_block(50.0, 50.0));
        let got = row_of(vec![LayoutBox::new(BoxType::Block, a1), a2], collapse);
        assert_row(&got, &[(0.0, 200.0), (200.0, 200.0)], "basis-0 split");
    }
}

/// The x.com shape end to end: the centring item gets its 200 and centres
/// the box in it (x 275; 262.5 before).
#[test]
fn a_centring_basis_zero_item_centres_in_its_even_share() {
    for collapse in [false, true] {
        let mut left = zero_pct();
        left.height = Length::Px(100.0);
        let mut right_s = zero_pct();
        right_s.display = Display::Flex;
        right_s.align_items = AlignItems::Center;
        right_s.justify_content = JustifyContent::Center;
        let mut right = LayoutBox::new(BoxType::Block, right_s);
        right.children.push(sized_block(50.0, 50.0));
        let mut c = LayoutBox::new(BoxType::Block, row());
        c.style.height = Length::Px(400.0);
        c.children = vec![LayoutBox::new(BoxType::Block, left), right];
        let root = laid_out(c, collapse);
        let x = root.children[1].children[0].dimensions.content.x;
        assert!(
            (x - 275.0).abs() < 0.5,
            "Chrome 148 puts the box at x=275, got {x}"
        );
    }
}

/// A min-width violation freezes that item at its minimum and the other two
/// share what is left: 250 / 75 / 75.
#[test]
fn a_min_violation_freezes_and_the_rest_share_the_remainder() {
    for collapse in [false, true] {
        let mut b1 = zero_pct();
        b1.min_width = Length::Px(250.0);
        let got = row_of(
            vec![
                LayoutBox::new(BoxType::Block, b1),
                LayoutBox::new(BoxType::Block, zero_pct()),
                LayoutBox::new(BoxType::Block, zero_pct()),
            ],
            collapse,
        );
        assert_row(
            &got,
            &[(0.0, 250.0), (250.0, 75.0), (325.0, 75.0)],
            "min violation",
        );
    }
}

/// A max-width violation hands its surplus to the others: 50 / 175 / 175.
#[test]
fn a_max_violation_freezes_and_the_rest_take_its_surplus() {
    for collapse in [false, true] {
        let mut c1 = zero_pct();
        c1.max_width = Length::Px(50.0);
        let got = row_of(
            vec![
                LayoutBox::new(BoxType::Block, c1),
                LayoutBox::new(BoxType::Block, zero_pct()),
                LayoutBox::new(BoxType::Block, zero_pct()),
            ],
            collapse,
        );
        assert_row(
            &got,
            &[(0.0, 50.0), (50.0, 175.0), (225.0, 175.0)],
            "max violation",
        );
    }
}

/// Shrinking: two 300px bases in 400, one with `min-width: 250`. Both shrink
/// by 100 to 200, the first violates, freezes at 250, and the second absorbs
/// the rest: 250 / 150.
#[test]
fn a_min_violation_while_shrinking_moves_the_overflow_to_the_other_item() {
    for collapse in [false, true] {
        let mut e1 = item(0.0, 1.0, FlexBasis::Length(300.0));
        e1.min_width = Length::Px(250.0);
        let got = row_of(
            vec![
                LayoutBox::new(BoxType::Block, e1),
                LayoutBox::new(BoxType::Block, item(0.0, 1.0, FlexBasis::Length(300.0))),
            ],
            collapse,
        );
        assert_row(
            &got,
            &[(0.0, 250.0), (250.0, 150.0)],
            "shrink min violation",
        );
    }
}

/// Grow factors summing below 1 take only that fraction of the free space:
/// two `flex-grow: 0.25` items fill half of 400.
#[test]
fn fractional_grow_factors_take_only_their_fraction_of_free_space() {
    for collapse in [false, true] {
        let got = row_of(
            vec![
                LayoutBox::new(BoxType::Block, item(0.25, 1.0, FlexBasis::Percent(0.0))),
                LayoutBox::new(BoxType::Block, item(0.25, 1.0, FlexBasis::Percent(0.0))),
            ],
            collapse,
        );
        assert_row(&got, &[(0.0, 100.0), (100.0, 100.0)], "fractional grow");
    }
}
