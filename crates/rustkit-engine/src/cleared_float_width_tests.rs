//! A float with `clear` starts a new line of floats, so in a shrink-to-fit
//! box's max-content width it ENDS the run of floats on its side instead of
//! adding to it (independent R1 HIGH on #1079): two `float: left; clear:
//! left; width: 100px` boxes in an inline-block are 100 wide, not 200.
//! Chrome keeps the left and right float sums apart: `clear: left` resets
//! only the left one (a left 100 and a right 60 with `clear: right` are
//! 160), and text on the line stays beside whichever floats are widest.
//!
//! Expected sizes: docs/diagnostics/2026-11-11/cleared_floats_max_content_cft148.txt
//! for cleared_floats_max_content.html beside it (Chrome for Testing
//! 148.0.7778.216, getBoundingClientRect): 11 float arrangements in an
//! inline-block, a float, a flex item, a grid item and an absolute box.

use super::*;

const PAGE: &str = include_str!("../../../docs/diagnostics/2026-11-11/cleared_floats_max_content.html");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-11/cleared_floats_max_content_cft148.txt");

/// Boxes whose WIDTH is Chrome's and whose height is not yet, for two causes
/// that are not the max-content rule (the floats are placed side by side or
/// not by the item's own layout, not by its measured width):
/// * a flex item keeps the height of an earlier, narrower pass in which
///   its floats were stacked (the floats end side by side, the item stays
///   one row too tall); the stretched-row change of CB6c (#1080) re-derives
///   that height;
/// * a grid item's floats are placed at an earlier, narrower width and stay
///   stacked (`gr_none`: the second float at 0,10 in a 200-wide item).
/// Their widths are asserted, and their heights are asserted to DIFFER: the
/// test fails when a listed height starts matching, so the entry is removed
/// with the fix that closes it.
const KNOWN_HEIGHT: &[&str] = &[
    "fx_none", "fx_lr2", "fx_l_r_cl", "fx_three", "gr_none", "gr_lr2", "gr_l_r_cl", "gr_three",
];

#[test]
fn a_cleared_float_ends_the_run_of_floats_on_its_side() {
    let expected: Vec<(&str, f32, f32)> = CHROME
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut it = l.split_whitespace();
            (it.next().unwrap(), it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap())
        })
        .collect();
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, PAGE).expect("load");
    let ids: Vec<String> = expected.iter().map(|(id, ..)| format!("'{id}'")).collect();
    let got = engine
        .execute_script(
            view,
            &format!(
                "[{}].map(function (i) {{ var r = document.getElementById(i).getBoundingClientRect(); return r.width + 'x' + r.height; }}).join(' ')",
                ids.join(",")
            ),
        )
        .expect("script");
    let got: Vec<(f32, f32)> = got
        .trim_start_matches("String(\"")
        .trim_end_matches("\")")
        .split(' ')
        .map(|v| {
            let (w, h) = v.split_once('x').unwrap();
            (w.parse().unwrap(), h.parse().unwrap())
        })
        .collect();
    assert_eq!(expected.len(), 55, "the Chrome table has 55 boxes");
    assert_eq!(got.len(), expected.len(), "the page answered for every box");
    // Widths to 0.1 px (the float + text rows are 125.8). A listed height
    // must still DIFFER: one that starts matching comes off `KNOWN_HEIGHT`.
    let wrong: Vec<String> = expected
        .iter()
        .zip(&got)
        .filter(|((id, w, h), got)| {
            let height_matches = (got.1 - h).abs() <= 0.5;
            (got.0 - w).abs() > 0.1 || height_matches == KNOWN_HEIGHT.contains(id)
        })
        .map(|((id, w, h), got)| {
            let listed = if KNOWN_HEIGHT.contains(id) { ", listed in KNOWN_HEIGHT" } else { "" };
            format!("{id}: {}x{} (Chromium {w}x{h}{listed})", got.0, got.1)
        })
        .collect();
    assert!(wrong.is_empty(), "{} of {} boxes differ:\n{}", wrong.len(), expected.len(), wrong.join("\n"));
}

