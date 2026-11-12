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
    let wrong: Vec<String> = expected
        .iter()
        .zip(&got)
        .filter(|((_, w, h), got)| (got.0 - w).abs() > 0.5 || (got.1 - h).abs() > 0.5)
        .map(|((id, w, h), got)| format!("{id}: {}x{} (Chromium {w}x{h})", got.0, got.1))
        .collect();
    assert!(wrong.is_empty(), "{} of {} boxes differ:\n{}", wrong.len(), expected.len(), wrong.join("\n"));
}
