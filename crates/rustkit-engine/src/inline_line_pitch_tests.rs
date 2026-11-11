//! SET2: the line pitch of a wrapped inline element with its own font size
//! or line-height inside a block with a different strut. Every line box is
//! the union of the block's strut and the inline's box on the baseline
//! (CSS2 §10.8), so a 12.8px `line-height: 1.5` span (19.2) in a 16px
//! system-ui block steps 20.19 per line in Chrome, not 19.2. RustKit gets
//! the first line right and spaces the inline's later lines at its own
//! line-height (the wrapped text box keeps one pitch, `line_height`, for
//! every line), so settings' Privacy Notice paragraph was 2px short and
//! everything below it 2.6px high.
//!
//! Expected heights: docs/diagnostics/2026-11-11/set2_inline_line_pitch_cft148.txt
//! for set2_inline_line_pitch.html beside it (60 two-line blocks; 20 differ).

use super::*;

const PAGE: &str = include_str!("../../../docs/diagnostics/2026-11-11/set2_inline_line_pitch.html");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-11/set2_inline_line_pitch_cft148.txt");

#[test]
fn a_wrapped_inline_steps_by_the_strut_union() {
    let expected: Vec<(&str, f32, String)> = CHROME
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut it = l.split_whitespace();
            let id = it.next().unwrap();
            let h = it.next().unwrap().parse().unwrap();
            (id, h, it.collect::<Vec<_>>().join(" "))
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
                "[{}].map(function (i) {{ return document.getElementById(i).getBoundingClientRect().height; }}).join(' ')",
                ids.join(",")
            ),
        )
        .expect("script");
    let got: Vec<f32> = got
        .trim_start_matches("String(\"")
        .trim_end_matches("\")")
        .split(' ')
        .map(|v| v.parse().unwrap())
        .collect();
    let wrong: Vec<String> = expected
        .iter()
        .zip(&got)
        .filter(|((_, want, _), got)| (*got - *want).abs() > 0.5)
        .map(|((id, want, what), got)| format!("{id} ({what}): {got} (Chromium {want})"))
        .collect();
    assert!(wrong.is_empty(), "{} of {} blocks differ:\n{}", wrong.len(), expected.len(), wrong.join("\n"));
}
