//! SET3: a single-line text `<input>`'s height. Chrome for Testing 148
//! builds it as a content line, the larger of the font's `normal` line and
//! the author's `line-height`, plus the vertical padding (the author's, else
//! the UA's 1px each side) and border (the author's, else the UA's 2px each
//! side). RustKit dropped the UA border once the author set a padding (a
//! `padding: 8px 12px` input at 13.6px was 31 for Chrome's 35), ignored
//! `line-height`, and used a scaled blob for a bare input at another font
//! size. text, email, password, number, search, url and tel are the same.
//!
//! Expected heights: docs/diagnostics/2026-11-11/set3_input_heights_cft148.txt
//! for set3_input_heights.html beside it (getBoundingClientRect).

use super::*;

const PAGE: &str = include_str!("../../../docs/diagnostics/2026-11-11/set3_input_heights.html");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-11/set3_input_heights_cft148.txt");

#[test]
fn a_text_input_is_as_tall_as_chrome_builds_it() {
    let expected: Vec<(&str, f32)> = CHROME
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut it = l.split_whitespace();
            (it.next().unwrap(), it.next().unwrap().parse().unwrap())
        })
        .collect();
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, PAGE).expect("load");
    let ids: Vec<String> = expected.iter().map(|(id, _)| format!("'{id}'")).collect();
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
        .filter(|((_, want), got)| (*got - *want).abs() > 0.5)
        .map(|((id, want), got)| format!("{id}: {got} (Chromium {want})"))
        .collect();
    assert!(wrong.is_empty(), "{} of {} inputs differ:\n{}", wrong.len(), expected.len(), wrong.join("\n"));
}
