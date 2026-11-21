//! Portable pins for the short-path paint contract: `copy_paint_only`,
//! `same_paint`, and `differs_only_in_paint`. The list is deliberately
//! short (opacity/transform/visibility are not on it); a property left
//! off costs a layout, never a wrong frame. No unit covered these on
//! default `--lib`.

use super::*;

#[test]
fn a_color_only_change_differs_only_in_paint() {
    let a = ComputedStyle::default();
    let mut b = a.clone();
    b.color = Color::from_rgb(255, 0, 0);
    assert!(!a.same_paint(&b));
    assert!(a.differs_only_in_paint(&b));
}

#[test]
fn a_width_change_is_not_paint_only() {
    let a = ComputedStyle::default();
    let mut b = a.clone();
    b.width = Length::Px(40.0);
    assert!(a.same_paint(&b), "width is not on the paint list");
    assert!(!a.differs_only_in_paint(&b));
}

#[test]
fn a_transform_change_is_not_paint_only_on_this_list() {
    // LPF2 treats transform as paint when it stays established; this list
    // still leaves it off so a wrong copy never skips layout.
    let a = ComputedStyle::default();
    let mut b = a.clone();
    b.transform.ops = vec![TransformOp::TranslateX(Length::Px(8.0))];
    assert!(a.same_paint(&b));
    assert!(!a.differs_only_in_paint(&b));
}

#[test]
fn copy_paint_only_moves_color_and_leaves_layout_fields() {
    let mut dest = ComputedStyle::default();
    dest.width = Length::Px(12.0);
    dest.display = Display::Flex;

    let mut src = ComputedStyle::default();
    src.color = Color::from_rgb(0, 128, 0);
    src.background_color = Color::from_rgb(1, 2, 3);
    src.width = Length::Px(99.0);
    src.display = Display::None;

    dest.copy_paint_only(&src);
    assert_eq!(dest.color, Color::from_rgb(0, 128, 0));
    assert_eq!(dest.background_color, Color::from_rgb(1, 2, 3));
    assert_eq!(dest.width, Length::Px(12.0));
    assert_eq!(dest.display, Display::Flex);
}
