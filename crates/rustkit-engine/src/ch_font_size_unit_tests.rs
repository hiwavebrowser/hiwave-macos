//! Portable CH-REM-FONT-SIZE pins (#1469 / batch 129).
//!
//! `ch_font_size_tests` needs `feature = "headless"`, so default Linux `--lib` never saw the
//! rem/em/% resolution that made `font-size: 3.5rem; inline-size: 16ch` measure against a 56px
//! "0" (GitHub's hero). These assert the pure `ch_font_size_px` / `ch_parent_font_px` helpers —
//! not glyph advances, which diverge across platform fonts.

use super::{ch_font_size_px, ch_parent_font_px};
use rustkit_css::{set_root_font_px, ComputedStyle, Length};

fn style_with(font_size: Length) -> ComputedStyle {
    let mut style = ComputedStyle::new();
    style.font_size = font_size;
    style
}

#[test]
fn rem_em_and_percent_font_sizes_resolve_like_a_px_one() {
    let prev = set_root_font_px(16.0);
    let rem = ch_font_size_px(&style_with(Length::Rem(3.5)), 16.0);
    let em = ch_font_size_px(&style_with(Length::Em(3.5)), 16.0);
    let percent = ch_font_size_px(&style_with(Length::Percent(350.0)), 16.0);
    let px = ch_font_size_px(&style_with(Length::Px(56.0)), 16.0);
    set_root_font_px(prev);
    assert_eq!(rem, 56.0, "3.5rem at a 16px root");
    assert_eq!(em, 56.0, "3.5em against a 16px parent");
    assert_eq!(percent, 56.0, "350% against a 16px parent");
    assert_eq!(px, 56.0);
    assert_eq!(rem, px);
    assert_eq!(em, px);
    assert_eq!(percent, px);
}

#[test]
fn rem_uses_the_thread_root_font_not_the_parent() {
    let prev = set_root_font_px(20.0);
    let rem = ch_font_size_px(&style_with(Length::Rem(3.5)), 16.0);
    set_root_font_px(prev);
    assert_eq!(rem, 70.0, "3.5rem at a 20px root ignores the 16px parent");
}

#[test]
fn zero_and_unresolved_font_sizes_have_defined_fallbacks() {
    let prev = set_root_font_px(16.0);
    assert_eq!(ch_font_size_px(&style_with(Length::Zero), 16.0), 0.0);
    assert_eq!(
        ch_font_size_px(&style_with(Length::Vw(10.0)), 16.0),
        16.0,
        "viewport units fall back to the initial 16px before absolutize"
    );
    assert_eq!(
        ch_font_size_px(&style_with(Length::Auto), 16.0),
        16.0,
        "auto falls back to the initial 16px"
    );
    set_root_font_px(prev);
}

#[test]
fn the_parent_font_for_ch_keeps_a_rem_parent() {
    let prev = set_root_font_px(16.0);
    assert_eq!(ch_parent_font_px(None), 16.0);
    assert_eq!(
        ch_parent_font_px(Some(&style_with(Length::Px(20.0)))),
        20.0
    );
    assert_eq!(
        ch_parent_font_px(Some(&style_with(Length::Rem(1.25)))),
        20.0,
        "a rem parent must not fall through to 16"
    );
    assert_eq!(
        ch_parent_font_px(Some(&style_with(Length::Em(2.0)))),
        16.0,
        "an em parent is not yet absolutized here"
    );
    set_root_font_px(prev);
}
