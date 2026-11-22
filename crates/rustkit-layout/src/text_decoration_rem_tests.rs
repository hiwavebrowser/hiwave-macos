//! ROOT-REM (#1408): rem text-decoration-thickness uses the layout rem TLS.
//!
//! `test_text_decoration` only pins Auto. After ROOT-REM, Rem thickness multiplies
//! `root_font_px()`, not a hard-coded 16 — never asserted on default `--lib`.

use rustkit_css::{set_root_font_px, Color, Length, TextDecorationLine, TextDecorationStyle};

use super::TextDecoration;

#[test]
fn rem_thickness_follows_the_root_font_not_sixteen() {
    let prev = set_root_font_px(10.0);
    let decoration = TextDecoration::from_style(
        TextDecorationLine::UNDERLINE,
        Some(Color::from_rgb(0, 0, 0)),
        TextDecorationStyle::Solid,
        Length::Rem(0.5),
        16.0, // element font must not win
    );
    assert_eq!(decoration.thickness, Some(5.0));
    let _ = set_root_font_px(prev);
}
