//! `getComputedStyle(...).fontFamily` as Chrome serializes it, per family in the
//! list: a generic keyword written bare stays bare (`monospace`); a quoted one
//! stays quoted (`"monospace"`: a family name, not the generic); any other name is
//! bare when it is a CSS identifier (`Arial`, `My-Font2`) and quoted otherwise
//! (`"Times New Roman"`, `"2fast"`); an unquoted run of identifiers is one name with
//! its whitespace collapsed. Receipt: Chrome for Testing 148.0.7778.216 on Linux,
//! `docs/diagnostics/2026-11-21/font_family_serialization_cft148.json`.
//!
//! The pure helpers (`parse_font_family_list` / `serialize_font_family_list`) are
//! what the cascade and CSSOM use. Pin them on default `--lib` so a silent
//! revert fails without needing a headless Engine + script round-trip. The
//! Engine oracle below stays as the end-to-end companion.

use super::*;

const RECEIPT: &str = include_str!("../../../docs/diagnostics/2026-11-21/font_family_serialization_cft148.json");

/// Declared → stored → CSSOM, matching Chrome's receipt without a view.
#[test]
fn parse_then_serialize_matches_the_chrome_receipt() {
    let receipt: serde_json::Value = serde_json::from_str(RECEIPT).unwrap();
    assert_eq!(receipt["chrome"], "148.0.7778.216");
    let wrong: Vec<String> = receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| {
            let (declared, chrome) = (c["declared"].as_str().unwrap(), c["computed"].as_str().unwrap());
            let stored = parse_font_family_list(declared);
            let got = serialize_font_family_list(&stored);
            (got != chrome).then(|| format!("{declared}: stored={stored:?} got={got}, Chrome {chrome}"))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {} wrong:\n{}",
        wrong.len(),
        receipt["cases"].as_array().unwrap().len(),
        wrong.join("\n")
    );
}

#[test]
fn a_quoted_generic_stays_quoted_in_storage_and_a_bare_one_stays_bare() {
    assert_eq!(parse_font_family_list("\"monospace\""), "\"monospace\"");
    assert_eq!(serialize_font_family_list("\"monospace\""), "\"monospace\"");
    assert_eq!(parse_font_family_list("monospace"), "monospace");
    assert_eq!(serialize_font_family_list("monospace"), "monospace");
}

#[test]
fn unquoted_whitespace_collapses_but_quoted_whitespace_is_kept() {
    assert_eq!(parse_font_family_list("Times   New   Roman"), "Times New Roman");
    assert_eq!(serialize_font_family_list("Times New Roman"), "\"Times New Roman\"");
    assert_eq!(parse_font_family_list("\"Times   New   Roman\""), "Times   New   Roman");
    assert_eq!(
        serialize_font_family_list("Times   New   Roman"),
        "\"Times   New   Roman\""
    );
}

#[test]
fn identifier_edges_decide_whether_serialize_quotes() {
    assert!(is_css_identifier("Arial"));
    assert!(is_css_identifier("My-Font2"));
    assert!(is_css_identifier("-apple-system"));
    assert!(!is_css_identifier("2fast"));
    assert!(!is_css_identifier("Times New Roman"));
    assert_eq!(serialize_font_family_list("My-Font2"), "My-Font2");
    assert_eq!(serialize_font_family_list("2fast"), "\"2fast\"");
}

#[cfg(feature = "headless")]
fn computed(declared: &str) -> String {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    engine.load_html(view, "<!doctype html><p id=p>x</p>").expect("load_html");
    let script = format!(
        "(function () {{ var p = document.getElementById('p'); p.style.fontFamily = {}; return String(getComputedStyle(p).fontFamily); }})()",
        serde_json::to_string(declared).unwrap()
    );
    let out = engine.execute_script(view, &script).expect("script");
    let inner = out
        .strip_prefix("String(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{declared}: {out}"));
    serde_json::from_str(inner).unwrap_or_else(|_| panic!("{declared}: {out}"))
}

/// End-to-end: style setter + getComputedStyle through a headless view.
#[cfg(feature = "headless")]
#[test]
fn font_family_serializes_as_chrome_serializes_it() {
    let receipt: serde_json::Value = serde_json::from_str(RECEIPT).unwrap();
    assert_eq!(receipt["chrome"], "148.0.7778.216");
    let wrong: Vec<String> = receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| {
            let (declared, chrome) = (c["declared"].as_str().unwrap(), c["computed"].as_str().unwrap());
            let got = computed(declared);
            (got != chrome).then(|| format!("{declared}: {got}, Chrome {chrome}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{} of {} wrong:\n{}", wrong.len(), receipt["cases"].as_array().unwrap().len(), wrong.join("\n"));
}
