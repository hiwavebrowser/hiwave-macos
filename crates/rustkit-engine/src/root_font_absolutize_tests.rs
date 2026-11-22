//! ROOT-REM (#1408): portable pin for absolutizing `<html>`'s own font-size.
//!
//! The headless `root_rem_*` suites need a view. Default Linux `--lib` never
//! saw that em/%/rem on the root resolve against the initial 16px, or that a
//! missing viewport leaves viewport units unpublished (fallback 16).

use super::absolutize_root_font_size;
use rustkit_css::{ComputedStyle, Length};

#[test]
fn root_em_percent_and_rem_are_taken_against_sixteen() {
    let mut style = ComputedStyle::new();
    style.font_size = Length::Percent(62.5);
    assert_eq!(absolutize_root_font_size(&mut style, None), 10.0);
    assert_eq!(style.font_size, Length::Px(10.0));

    let mut style = ComputedStyle::new();
    style.font_size = Length::Em(1.25);
    assert_eq!(absolutize_root_font_size(&mut style, None), 20.0);
    assert_eq!(style.font_size, Length::Px(20.0));

    let mut style = ComputedStyle::new();
    style.font_size = Length::Rem(2.0);
    assert_eq!(absolutize_root_font_size(&mut style, None), 32.0);
    assert_eq!(style.font_size, Length::Px(32.0));
}

#[test]
fn a_px_root_is_kept_and_zero_is_allowed() {
    let mut style = ComputedStyle::new();
    style.font_size = Length::Px(18.0);
    assert_eq!(absolutize_root_font_size(&mut style, Some((800.0, 600.0))), 18.0);
    assert_eq!(style.font_size, Length::Px(18.0));

    let mut style = ComputedStyle::new();
    style.font_size = Length::Zero;
    assert_eq!(absolutize_root_font_size(&mut style, None), 0.0);
    assert_eq!(style.font_size, Length::Px(0.0));
}

#[test]
fn a_viewport_unit_without_a_viewport_falls_back_to_sixteen() {
    let mut style = ComputedStyle::new();
    style.font_size = Length::Vw(2.0);
    assert_eq!(absolutize_root_font_size(&mut style, None), 16.0);
    // Unresolved: leave the length as written.
    assert_eq!(style.font_size, Length::Vw(2.0));

    let mut style = ComputedStyle::new();
    style.font_size = Length::Vw(2.0);
    assert_eq!(absolutize_root_font_size(&mut style, Some((500.0, 400.0))), 10.0);
    assert_eq!(style.font_size, Length::Px(10.0));
}
