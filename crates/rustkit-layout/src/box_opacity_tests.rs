//! CSS Color 4 §4 `opacity` at the display-list boundary.
//!
//! `opacity` was parsed into the style and read by nothing but the image
//! command (whose painter dropped it): a `div` at `opacity: 0` painted
//! solid. google.com's search box (2026-10-08) carries two such layers, a
//! conic-gradient glow and a black hover overlay, which Chrome does not
//! show and RustKit painted over the box
//! (docs/diagnostics/2026-10-08/google_searchbox_conic_h22.md).

use super::*;

fn red() -> Color {
    Color { r: 255, g: 0, b: 0, a: 1.0 }
}

fn blue() -> Color {
    Color { r: 0, g: 0, b: 255, a: 1.0 }
}

fn filled(color: Color, opacity: f32, rect: Rect) -> LayoutBox {
    let mut style = ComputedStyle::new();
    style.background_color = color;
    style.opacity = opacity;
    let mut b = LayoutBox::new(BoxType::Block, style);
    b.dimensions.content = rect;
    b
}

/// A white page holding one red 100x100 box at `opacity`, which holds a
/// blue 50x50 box.
fn page(opacity: f32) -> LayoutBox {
    let mut root = filled(Color::WHITE, 1.0, Rect::new(0.0, 0.0, 800.0, 600.0));
    let mut outer = filled(red(), opacity, Rect::new(10.0, 10.0, 100.0, 100.0));
    outer.children.push(filled(blue(), 1.0, Rect::new(10.0, 10.0, 50.0, 50.0)));
    root.children.push(outer);
    root
}

fn fills(list: &DisplayList, of: Color) -> usize {
    list.commands
        .iter()
        .filter(|c| match c {
            DisplayCommand::SolidColor(color, _) | DisplayCommand::RoundedRect { color, .. } => *color == of,
            _ => false,
        })
        .count()
}

#[test]
fn an_opaque_box_paints_as_before() {
    let list = DisplayList::build(&page(1.0));
    assert_eq!((fills(&list, red()), fills(&list, blue())), (1, 1), "{:?}", list.commands);
}

#[test]
fn a_box_at_opacity_zero_paints_nothing() {
    let list = DisplayList::build(&page(0.0));
    assert_eq!(fills(&list, red()), 0, "the box itself: {:?}", list.commands);
}

#[test]
fn the_subtree_of_a_box_at_opacity_zero_paints_nothing() {
    // The child's own opacity is 1: opacity is not inherited, the group is
    // faded as one (CSS Color 4 §4).
    let list = DisplayList::build(&page(0.0));
    assert_eq!(fills(&list, blue()), 0, "its child: {:?}", list.commands);
    assert_eq!(fills(&list, Color::WHITE), 1, "what is under it still paints");
}
