//! S9: an image inside an inline box (`<a><img></a>`, `<a><picture><img>`)
//! resolves a percentage `max-height` against its block container. An
//! inline box is not a containing block (CSS 2.1 §10.1): the image's
//! containing block is the nearest block container, and when that has a
//! definite height a `max-height: 100%` caps the image.
//!
//! google's 2026-11-03 election doodle: `<img width=1000 height=400>` with
//! `max-height:100%; max-width:100%; width:auto` inside `<a><picture>` in a
//! 230px-tall block. Chromium 148: 575x230. The engine laid the inline's
//! children out with no percent base, so `max-height` was `none` and the
//! image kept 992x396.8 (`max-width` only), drawn over the footer.

use super::*;
use rustkit_css::Display;

fn doodle() -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Inline;
    // The box builder writes the `height` attribute into an `auto` height.
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

fn inline(children: Vec<LayoutBox>) -> LayoutBox {
    let mut s = ComputedStyle::new();
    s.display = Display::Inline;
    let mut b = LayoutBox::new(BoxType::Inline, s);
    b.children = children;
    b
}

/// `<div style="width:992px; height:{height}">{child}</div>` laid out through
/// the real entry point; returns the image's content size.
fn image_size_in_block(height: Length, child: LayoutBox, path: &[usize]) -> (f32, f32) {
    let mut s = ComputedStyle::new();
    s.width = Length::Px(992.0);
    s.height = height;
    let mut block = LayoutBox::new(BoxType::Block, s);
    block.children.push(child);
    let viewport = Dimensions {
        content: Rect::new(0.0, 0.0, 1280.0, 800.0),
        ..Default::default()
    };
    block.layout(&viewport);
    let mut b = &block;
    for &i in path {
        b = &b.children[i];
    }
    (b.dimensions.content.width, b.dimensions.content.height)
}

/// Control: the image directly in the block already takes the cap.
#[test]
fn an_image_in_a_definite_block_takes_its_percent_max_height() {
    let size = image_size_in_block(Length::Px(230.0), doodle(), &[0]);
    assert_eq!(size, (575.0, 230.0));
}

/// `<a><img></a>`: the anchor is not the image's containing block.
#[test]
fn an_image_in_an_anchor_takes_the_blocks_percent_max_height() {
    let size = image_size_in_block(Length::Px(230.0), inline(vec![doodle()]), &[0, 0]);
    assert_eq!(size, (575.0, 230.0));
}

/// `<a><picture><img></picture></a>`, google's markup.
#[test]
fn an_image_in_a_picture_in_an_anchor_takes_the_blocks_percent_max_height() {
    let size = image_size_in_block(
        Length::Px(230.0),
        inline(vec![inline(vec![doodle()])]),
        &[0, 0, 0],
    );
    assert_eq!(size, (575.0, 230.0));
}

/// An auto-height block has no definite height: `max-height: 100%` is
/// `none` and only `max-width` applies (992 x 396.8).
#[test]
fn an_image_in_an_anchor_in_an_auto_height_block_keeps_max_width_only() {
    let (w, h) = image_size_in_block(Length::Auto, inline(vec![doodle()]), &[0, 0]);
    assert_eq!(w, 992.0);
    assert!((h - 396.8).abs() < 0.01, "height {h}");
}
