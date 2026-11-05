//! S9: a grid item with a height of its own is the percent base of what it
//! holds, both when it is sized (max-content width of a non-stretched item)
//! and when its children flow.
//!
//! google's 2026-11-03 election doodle, as shape: a 240px grid, one
//! `minmax(0, 1fr)` row, `justify-items` / `align-items: center`; the item
//! is `height: 100%; max-height: 230px` and holds a percentage-height block
//! and `<a><img width=1000 height=400 style="max-height:100%;
//! max-width:100%">`. Chromium 148: item 575 wide, image 575x230, a
//! `height: 50%` block 115 and `calc(100% - 400px)` 0. The engine measured
//! the item at the image's 1000px width attribute and, because the item's
//! children are inline-level, flowed them with no percent base (the
//! viewport's 800 instead): the block was 400 and the image sat below the
//! search box, drawn over the footer.

use super::*;
use rustkit_css::{AlignItems, Display, GridTemplate, JustifyItems, TrackSize};

fn doodle() -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Inline;
    // The box builder writes the width= and height= attributes into
    // `auto` sizes.
    s.width = Length::Px(1000.0);
    s.height = Length::Px(400.0);
    s.max_width = Length::Percent(100.0);
    s.max_height = Length::Percent(100.0);
    LayoutBox::new(
        BoxType::Image {
            url: String::new(),
            natural_width: 1000.0,
            natural_height: 400.0,
        },
        s,
    )
}

fn anchor(children: Vec<LayoutBox>) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Inline;
    let mut b = LayoutBox::new(BoxType::Inline, s);
    b.children = children;
    b
}

fn half_block() -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.height = Length::Percent(50.0);
    LayoutBox::new(BoxType::Block, s)
}

/// The doodle shape laid out through the real entry point at 1280x800.
/// Returns the grid item.
fn laid_out_item(item_children: Vec<LayoutBox>) -> LayoutBox {
    let mut grid = ComputedStyle::new();
    grid.display = Display::Grid;
    grid.height = Length::Px(240.0);
    grid.grid_template_rows = GridTemplate::from_sizes(vec![TrackSize::MinMax(
        Box::new(TrackSize::Px(0.0)),
        Box::new(TrackSize::Fr(1.0)),
    )]);
    grid.justify_items = JustifyItems::Center;
    grid.align_items = AlignItems::Center;
    let mut container = LayoutBox::new(BoxType::Block, grid);

    let mut item_style = ComputedStyle::new();
    item_style.height = Length::Percent(100.0);
    item_style.max_height = Length::Px(230.0);
    let mut item = LayoutBox::new(BoxType::Block, item_style);
    item.children = item_children;
    container.children.push(item);

    let mut root = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    root.children.push(container);
    let viewport = Dimensions {
        content: Rect::new(0.0, 0.0, 1280.0, 800.0),
        ..Default::default()
    };
    root.layout(&viewport);
    root.children.remove(0).children.remove(0)
}

/// The item is as wide as the image its height caps: 230 x 2.5 = 575.
#[test]
fn a_centred_item_is_as_wide_as_the_image_its_height_caps() {
    let item = laid_out_item(vec![anchor(vec![doodle()])]);
    let w = item.dimensions.border_box().width;
    assert!((w - 575.0).abs() < 0.01, "item width {w}, Chromium 575");
    let img = item.children[0].children[0].dimensions.content;
    assert_eq!((img.width, img.height), (575.0, 230.0));
}

/// A `height: 50%` block beside the anchor is half the item's 230.
#[test]
fn a_percent_block_beside_an_anchor_takes_the_items_height() {
    let item = laid_out_item(vec![half_block(), anchor(vec![doodle()])]);
    let half = item.children[0].dimensions.content.height;
    assert!((half - 115.0).abs() < 0.01, "half {half}, Chromium 115");
}
