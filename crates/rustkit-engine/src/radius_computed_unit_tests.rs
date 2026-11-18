//! #1296: getComputedStyle keeps a percentage / calc() border-radius.
//!
//! The headless oracle (`border_radius_computed_tests`) needs a GPU view.
//! These pins keep `radius_computed` / `border_radius_shorthand` on every
//! `cargo test -p rustkit-engine --lib`.

use super::{border_radius_shorthand, radius_computed};
use crate::computed_read::set_root_font;
use rustkit_css::{CalcSum, ComputedStyle, Length};

#[test]
fn a_percentage_radius_stays_a_percentage() {
    assert_eq!(radius_computed(&Length::Percent(50.0), 16.0, 200.0), "50%");
    assert_eq!(radius_computed(&Length::Percent(80.0), 16.0, 100.0), "80%");
    assert_eq!(
        radius_computed(&Length::Percent(0.0), 16.0, 100.0),
        "0%",
        "zero percent is still a percentage, not 0px"
    );
}

#[test]
fn a_calc_mix_keeps_percent_and_folds_em_rem_to_px() {
    assert_eq!(
        radius_computed(
            &Length::Calc(Box::new(CalcSum {
                px: 10.0,
                percent: 5.0,
                ..Default::default()
            })),
            16.0,
            200.0
        ),
        "calc(5% + 10px)"
    );
    assert_eq!(
        radius_computed(
            &Length::Calc(Box::new(CalcSum {
                px: -10.0,
                percent: 50.0,
                ..Default::default()
            })),
            16.0,
            200.0
        ),
        "calc(50% - 10px)"
    );
    assert_eq!(
        radius_computed(
            &Length::Calc(Box::new(CalcSum {
                em: 1.0,
                percent: 10.0,
                ..Default::default()
            })),
            16.0,
            200.0
        ),
        "calc(10% + 16px)"
    );

    let mut root = ComputedStyle::new();
    root.font_size = Length::Px(10.0);
    set_root_font(&root);
    assert_eq!(
        radius_computed(
            &Length::Calc(Box::new(CalcSum {
                rem: 1.5,
                percent: 10.0,
                ..Default::default()
            })),
            16.0,
            200.0
        ),
        "calc(10% + 15px)"
    );
    // Restore the thread-local default so later --lib tests see 16px.
    root.font_size = Length::Px(16.0);
    set_root_font(&root);
}

#[test]
fn plain_lengths_resolve_to_px_and_the_shorthand_minimises() {
    assert_eq!(radius_computed(&Length::Px(10.0), 16.0, 200.0), "10px");
    assert_eq!(radius_computed(&Length::Em(2.0), 16.0, 200.0), "32px");

    let all_round = [
        ("50%".into(), "50%".into()),
        ("50%".into(), "50%".into()),
        ("50%".into(), "50%".into()),
        ("50%".into(), "50%".into()),
    ];
    assert_eq!(border_radius_shorthand(&all_round), "50%");

    let elliptical = [
        ("10%".into(), "20%".into()),
        ("10%".into(), "20%".into()),
        ("10%".into(), "20%".into()),
        ("10%".into(), "20%".into()),
    ];
    assert_eq!(border_radius_shorthand(&elliptical), "10% / 20%");

    let mixed = [
        ("10px".into(), "10px".into()),
        ("50%".into(), "50%".into()),
        ("32px".into(), "32px".into()),
        ("16px".into(), "16px".into()),
    ];
    assert_eq!(border_radius_shorthand(&mixed), "10px 50% 32px 16px");
}
