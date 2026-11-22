//! CONTROL-LINE-HANG (#1409): portable pin for the auto-height button hang.
//!
//! `button_line_height_tests` is headless-only (font metrics + full IFC). Default
//! Linux `--lib` never saw that a button hangs its label descent *plus* bottom
//! padding and border under Blink seating — the arithmetic that made a line of
//! only a button equal the button's height instead of the strut poking under it.

use super::*;

#[test]
fn an_auto_height_button_hangs_pad_and_border_below_its_seated_baseline() {
    // Integer metrics so blink_baseline_offset's round is a no-op:
    // half_leading(21, 15, 4) = 1 → floor + ascent = 16.
    let hang = button_baseline_hang(21.0, 15.0, 4.0, 1.0, 2.0, 0.0);
    assert!((hang - 8.0).abs() < 1e-5, "UA pad+border hang: got {hang}");

    // Author padding/border:none → only the seated descent side of the line.
    let bare = button_baseline_hang(21.0, 15.0, 4.0, 0.0, 0.0, 0.0);
    assert!((bare - 5.0).abs() < 1e-5, "no pad/border hang: got {bare}");
}

#[test]
fn button_box_sides_feed_the_hang_bottom_edges() {
    let style = ComputedStyle::new();
    let (pad, border) = button_box_sides(&style, 16.0);
    assert_eq!(pad[2], UA_BUTTON_PADDING[2]);
    assert_eq!(border[2], UA_BUTTON_BORDER);

    let hang = button_baseline_hang(21.0, 15.0, 4.0, pad[2], border[2], 0.0);
    assert!((hang - 8.0).abs() < 1e-5, "UA sides through hang: got {hang}");

    let mut none = ComputedStyle::new();
    none.padding_declared = [true; 4];
    none.border_declared = [true; 4];
    none.padding_bottom = Length::Px(0.0);
    none.border_bottom_width = Length::Px(0.0);
    none.border_bottom_style = BorderStyle::None;
    let (pad, border) = button_box_sides(&none, 16.0);
    assert_eq!((pad[2], border[2]), (0.0, 0.0));
}

#[test]
fn a_bottom_margin_adds_to_the_hang() {
    let hang = button_baseline_hang(21.0, 15.0, 4.0, 1.0, 2.0, 3.0);
    assert!((hang - 11.0).abs() < 1e-5, "margin hang: got {hang}");
}
