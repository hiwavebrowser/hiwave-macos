//! A block flex item in an auto-height row is as tall as its children laid
//! out, not as the one-line guess (`line-height` of its own font) the
//! hypothetical cross size starts from. Step 11b only ever GREW an item to
//! its laid-out children, so a 16px-font item holding a 12px line kept the
//! 16px font's 18px line height: HiWave's shelf rows were 2px taller than
//! Chromium's (CB6b, `.shelf-item-header`, `align-items: flex-start`).
//!
//! Expected numbers are Chrome for Testing 148.0.7778.216
//! (`getBoundingClientRect`) for `<div style="display: flex; align-items:
//! flex-start; font: 16px Helvetica"><div><div style="height: H">` (a 16px
//! font line is 18px): the row and the item are both H tall, for H = 5
//! and 14.

use super::*;
use rustkit_css::{AlignItems, Display};

/// (row height, item height) for an item whose only child is `child_h` tall.
fn heights(child_h: f32) -> (f32, f32) {
    let mut row_style = ComputedStyle::new();
    row_style.display = Display::Flex;
    row_style.align_items = AlignItems::FlexStart;
    row_style.font_size = Length::Px(16.0);
    let mut item_style = ComputedStyle::new();
    item_style.font_size = Length::Px(16.0);
    let mut kid_style = ComputedStyle::new();
    kid_style.height = Length::Px(child_h);
    kid_style.font_size = Length::Px(16.0);

    let mut item = LayoutBox::new(BoxType::Block, item_style);
    item.children.push(LayoutBox::new(BoxType::Block, kid_style));
    let mut row = LayoutBox::new(BoxType::Block, row_style);
    row.children.push(item);
    let mut body = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    body.children.push(row);
    body.set_viewport(1280.0, 800.0);
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 400.0, 0.0),
        ..Default::default()
    };
    let mut mc = MarginCollapseContext::new();
    let mut fc = FloatContext::new();
    body.layout_with_collapse(&cb, &mut mc, &mut fc);
    let row = &body.children[0];
    (row.dimensions.border_box().height, row.children[0].dimensions.border_box().height)
}

#[test]
fn a_flex_start_item_shorter_than_its_font_line_sets_the_row_height() {
    for h in [5.0, 14.0] {
        let (row, item) = heights(h);
        assert!(
            (row - h).abs() < 0.5 && (item - h).abs() < 0.5,
            "child {h}px: row {row}, item {item}; Chromium has {h} and {h}"
        );
    }
}
