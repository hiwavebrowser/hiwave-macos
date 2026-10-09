//! A row flex item's cross-size floor. An item gets a minimum of one line of
//! its own font so that inline content has a line box before it is laid out;
//! an item whose in-flow children are all blocks has no line box, and the
//! floor made it one line tall whatever it held. The box itself was repaired
//! later in the pass, but `align-items` had already placed it by the floor.
//! Reduced from ebay's search form (hand test H15). Every expected number was
//! measured on the oracle Chromium 143 at a 400px viewport
//! (`getBoundingClientRect`, shapes F1 to F14 of the PR); each case runs
//! through both entry points, `layout` and `layout_with_collapse`.

use super::*;
use rustkit_css::{AlignItems, Display, FlexBasis};

fn row(height: Length, align: AlignItems) -> ComputedStyle {
    let mut s = ComputedStyle::new();
    s.display = Display::Flex;
    s.height = height;
    s.align_items = align;
    s
}

fn grow() -> ComputedStyle {
    let mut s = ComputedStyle::new();
    s.flex_grow = 1.0;
    s.flex_shrink = 1.0;
    s.flex_basis = FlexBasis::Percent(0.0);
    s
}

fn boxed(style: ComputedStyle, children: Vec<LayoutBox>) -> LayoutBox {
    let mut b = LayoutBox::new(BoxType::Block, style);
    b.children = children;
    b
}

/// `<div style="width: 100px; height: <h>px">`.
fn bar(h: f32) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.width = Length::Px(100.0);
    s.height = Length::Px(h);
    boxed(s, Vec::new())
}

/// Lays `root` out in a block `<body>` 400px wide through the named entry
/// point and returns it.
fn laid_out(root: LayoutBox, collapse: bool) -> LayoutBox {
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.children.push(root);
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

/// Asserts the first item's content (y, height) through both entry points.
fn assert_item(name: &str, build: impl Fn() -> LayoutBox, want: (f32, f32)) {
    for collapse in [false, true] {
        let row = laid_out(build(), collapse);
        let c = row.children[0].dimensions.content;
        assert!(
            (c.y - want.0).abs() < 0.5 && (c.height - want.1).abs() < 0.5,
            "{name} (collapse = {collapse}): y {} height {}, Chromium has y {} height {}",
            c.y,
            c.height,
            want.0,
            want.1
        );
    }
}

#[test]
fn an_item_that_holds_only_blocks_is_as_tall_as_they_are() {
    // F1: one 10px block.
    assert_item(
        "one 10px block",
        || {
            boxed(
                row(Length::Auto, AlignItems::Stretch),
                vec![boxed(grow(), vec![bar(10.0)])],
            )
        },
        (0.0, 10.0),
    );
    // F2: two 5px blocks.
    assert_item(
        "two 5px blocks",
        || {
            boxed(
                row(Length::Auto, AlignItems::Stretch),
                vec![boxed(grow(), vec![bar(5.0), bar(5.0)])],
            )
        },
        (0.0, 10.0),
    );
    // F8: the block one level further down.
    assert_item(
        "a block that holds a 10px block",
        || {
            boxed(
                row(Length::Auto, AlignItems::Stretch),
                vec![boxed(
                    grow(),
                    vec![boxed(ComputedStyle::new(), vec![bar(10.0)])],
                )],
            )
        },
        (0.0, 10.0),
    );
    // F13: the floor was one line of the ITEM's font, 34px at 30px Arial.
    assert_item(
        "an 8px block in a 30px font",
        || {
            let mut item = grow();
            item.font_size = Length::Px(30.0);
            boxed(
                row(Length::Auto, AlignItems::Stretch),
                vec![boxed(item, vec![bar(8.0)])],
            )
        },
        (0.0, 8.0),
    );
}

#[test]
fn an_item_that_holds_only_blocks_is_aligned_by_its_own_height() {
    // F6: centred in a 44px row, a 10px item sits at 17. It sat at 14, the
    // centre of a 16px box.
    assert_item(
        "align-items: center",
        || {
            boxed(
                row(Length::Px(44.0), AlignItems::Center),
                vec![boxed(grow(), vec![bar(10.0)])],
            )
        },
        (17.0, 10.0),
    );
    // F14: at the end of the row, 34; it sat at 28.
    assert_item(
        "align-items: flex-end",
        || {
            boxed(
                row(Length::Px(44.0), AlignItems::FlexEnd),
                vec![boxed(grow(), vec![bar(10.0)])],
            )
        },
        (34.0, 10.0),
    );
}

#[test]
fn an_item_with_inline_content_keeps_one_line() {
    // F11: an inline-block is on a line, and the line is at least the strut.
    for collapse in [false, true] {
        let mut chip = ComputedStyle::new();
        chip.display = Display::InlineBlock;
        chip.width = Length::Px(100.0);
        chip.height = Length::Px(10.0);
        let row = laid_out(
            boxed(
                row(Length::Auto, AlignItems::Stretch),
                vec![boxed(grow(), vec![boxed(chip, Vec::new())])],
            ),
            collapse,
        );
        let h = row.children[0].dimensions.content.height;
        assert!(
            h > 12.0,
            "collapse = {collapse}: an item holding a 10px inline-block is one line tall \
             (16 in Chromium at 14px Arial), got {h}"
        );
    }
}
