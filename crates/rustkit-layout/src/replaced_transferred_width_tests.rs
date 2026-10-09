//! The width of a replaced box whose `width` is `auto` and whose `height` is
//! given, where no block container works it out: a flex base size and a
//! min/max-content contribution. The height crosses the ratio (css-sizing-4
//! "transferred size"); the natural width is only for a box with neither.
//!
//! reddit's wordmark (H26c) is `<a style="display: flex"><svg viewBox="0 0
//! 514 149" style="height: 22px">` in the header's flex row. Chrome 148 has
//! it 75.89 x 22 in every container below; RustKit had 514 as a row item, 16
//! inside the nested `<a>` and 0 inside an inline-flex one, and 75.89 only
//! as a block child.

use super::*;
use rustkit_css::Display;

const WORDMARK_WIDTH: f32 = 22.0 * 514.0 / 149.0;

fn wordmark() -> LayoutBox {
    let mut style = ComputedStyle::new();
    style.display = Display::InlineBlock;
    style.height = Length::Px(22.0);
    LayoutBox::new(
        BoxType::Image {
            url: "inline-svg:wordmark".to_string(),
            natural_width: 514.0,
            natural_height: 149.0,
        },
        style,
    )
}

fn flex_box(children: Vec<LayoutBox>) -> LayoutBox {
    let mut style = ComputedStyle::new();
    style.display = Display::Flex;
    let mut b = LayoutBox::new(BoxType::Block, style);
    b.children = children;
    b
}

fn laid_out(mut root: LayoutBox) -> LayoutBox {
    root.set_viewport(1280.0, 800.0);
    let cb = Dimensions {
        content: Rect::new(0.0, 0.0, 600.0, 0.0),
        ..Default::default()
    };
    let mut mc = MarginCollapseContext::new();
    let mut fc = FloatContext::new();
    root.layout_with_collapse(&cb, &mut mc, &mut fc);
    root
}

fn close(got: f32, want: f32) -> bool {
    (got - want).abs() < 0.1
}

#[test]
fn a_row_item_with_a_height_and_a_ratio_takes_its_width_from_them() {
    let row = laid_out(flex_box(vec![wordmark()]));
    let item = &row.children[0].dimensions.content;
    assert!(
        close(item.width, WORDMARK_WIDTH) && close(item.height, 22.0),
        "514x149 at height 22 in a flex row: {} x {}, Chromium has 75.89 x 22",
        item.width,
        item.height
    );
}

#[test]
fn a_row_item_with_an_em_height_and_a_ratio_takes_its_width_from_them() {
    // `svg { height: 1.5em }` at a 20px font: 30 x 30 for a square viewBox.
    let mut icon = wordmark();
    icon.box_type = BoxType::Image {
        url: "inline-svg:icon".to_string(),
        natural_width: 24.0,
        natural_height: 24.0,
    };
    icon.style.font_size = Length::Px(20.0);
    icon.style.height = Length::Em(1.5);
    let row = laid_out(flex_box(vec![icon]));
    let item = &row.children[0].dimensions.content;
    assert!(close(item.width, 30.0), "width {}, Chromium has 30", item.width);
}

#[test]
fn a_flex_link_around_it_is_as_wide_as_it_is() {
    // The page's nesting: the row's item is a flex container holding the svg.
    let row = laid_out(flex_box(vec![flex_box(vec![wordmark()])]));
    let link = &row.children[0];
    assert!(
        close(link.dimensions.content.width, WORDMARK_WIDTH),
        "the link is {} wide, Chromium has 75.89",
        link.dimensions.content.width
    );
    let svg = &link.children[0].dimensions.content;
    assert!(close(svg.width, WORDMARK_WIDTH), "the svg is {} wide, Chromium has 75.89", svg.width);
}

#[test]
fn its_content_contributions_are_that_width() {
    let svg = wordmark();
    let max = crate::grid::estimate_max_content_width(&svg);
    let min = crate::grid::estimate_min_content_width(&svg);
    assert!(close(max, WORDMARK_WIDTH), "max-content {max}");
    assert!(close(min, WORDMARK_WIDTH), "min-content {min}");
}

#[test]
fn a_natural_size_with_no_height_given_contributes_its_natural_width() {
    let mut img = wordmark();
    img.style.height = Length::Auto;
    assert!(close(crate::grid::estimate_max_content_width(&img), 514.0));
    assert!(close(crate::grid::estimate_min_content_width(&img), 514.0));
}

#[test]
fn padding_and_border_are_added_to_the_contribution() {
    let mut svg = wordmark();
    svg.style.padding_left = Length::Px(4.0);
    svg.style.padding_right = Length::Px(6.0);
    let max = crate::grid::estimate_max_content_width(&svg);
    assert!(close(max, WORDMARK_WIDTH + 10.0), "max-content {max}");
}

#[test]
fn a_border_box_height_crosses_the_ratio_as_its_content_height() {
    // 30px border-box height with 4px padding above and below: 22px of
    // content, so the same 75.89 of content width.
    let mut svg = wordmark();
    svg.style.box_sizing = rustkit_css::BoxSizing::BorderBox;
    svg.style.height = Length::Px(30.0);
    svg.style.padding_top = Length::Px(4.0);
    svg.style.padding_bottom = Length::Px(4.0);
    let max = crate::grid::estimate_max_content_width(&svg);
    assert!(close(max, WORDMARK_WIDTH), "max-content {max}");
}

/// `img { max-width: 100% }`: the minimum contribution is 0 (css-sizing-3
/// §5.2.2), so the flex item around it can shrink and the image with it.
/// Chrome 148: a 100x50 image in a `<div>` item of a 60px row is 60 x 30.
#[test]
fn a_percentage_max_width_makes_the_minimum_contribution_zero() {
    let mut img = wordmark();
    img.style.height = Length::Auto;
    img.style.max_width = Length::Percent(100.0);
    assert!(close(crate::grid::estimate_min_content_width(&img), 0.0));
    assert!(close(crate::grid::estimate_max_content_width(&img), 514.0));
}

#[test]
fn a_pixel_max_width_caps_both_contributions() {
    let mut img = wordmark();
    img.style.height = Length::Auto;
    img.style.max_width = Length::Px(200.0);
    assert!(close(crate::grid::estimate_min_content_width(&img), 200.0));
    assert!(close(crate::grid::estimate_max_content_width(&img), 200.0));
}

/// Pins: a width the page gave is still the answer, and a box with no
/// natural size contributes what it did.
#[test]
fn a_given_width_and_an_unloaded_image_are_as_before() {
    let mut sized = wordmark();
    sized.style.width = Length::Px(100.0);
    assert!(close(crate::grid::estimate_max_content_width(&sized), 100.0));
    let row = laid_out(flex_box(vec![sized]));
    assert!(close(row.children[0].dimensions.content.width, 100.0));

    let mut unloaded = wordmark();
    unloaded.box_type = BoxType::Image {
        url: "x.png".to_string(),
        natural_width: 0.0,
        natural_height: 0.0,
    };
    assert!(close(crate::grid::estimate_max_content_width(&unloaded), 0.0));
}
