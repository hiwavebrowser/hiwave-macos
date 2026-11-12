//! `document.title` is the document's `<title>` element (HTML §3.1.3).
//!
//! It was a plain property of the page's `document`: reading it never saw
//! a `<title>` script changed, and setting it changed no element, so a
//! page that sets its title with none in the markup had no `<title>` at
//! all (a Next.js page does on every navigation: Speedometer's
//! NewsSite-Next was one element short of Chrome's document).
//!
//! Expected lines: docs/diagnostics/2026-11-12/document_title_cft148.txt,
//! `window.report()` of the page beside it in Chrome for Testing
//! 148.0.7778.216. One line there is Chrome's own, not the standard's:
//! set with no `<title>` and no `<head>`, the value is still read back.

use super::*;

const PAGE: &str = include_str!("../../../docs/diagnostics/2026-11-12/document_title.html");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-12/document_title_cft148.txt");

#[test]
fn document_title_reads_and_writes_the_title_element() {
    let mut engine = Engine::new_page(EngineConfig::default(), None, Arc::new(AtomicU64::new(0))).expect("engine");
    let id = engine.create_page_view(Bounds::new(0, 0, 800, 600)).expect("page view");
    engine.load_html(id, PAGE).expect("load_html");
    let out = engine.execute_script(id, "String(window.report())").expect("script");
    let got = out
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
        .map(|s| s.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\"))
        .unwrap_or(out);
    let got: Vec<&str> = got.lines().collect();
    let want: Vec<&str> = CHROME.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).collect();
    assert_eq!(got.len(), want.len(), "lines: {got:#?}");
    let wrong: Vec<String> = got.iter().zip(&want).filter(|(g, w)| g != w).map(|(g, w)| format!("got  {g}\n     want {w}")).collect();
    assert!(wrong.is_empty(), "the lines that differ from Chrome 148:\n{}", wrong.join("\n"));
}

/// A page whose markup has a `<title>` reads it, as before.
#[test]
fn a_title_in_the_markup_is_read() {
    let mut engine = Engine::new_page(EngineConfig::default(), None, Arc::new(AtomicU64::new(0))).expect("engine");
    let id = engine.create_page_view(Bounds::new(0, 0, 800, 600)).expect("page view");
    engine.load_html(id, "<html><head><title> From  markup </title></head><body></body></html>").expect("load_html");
    assert_eq!(engine.execute_script(id, "document.title").expect("script"), "String(\"From markup\")");
    assert_eq!(
        engine.execute_script(id, "document.title = 'changed'; document.querySelector('title').textContent + '|' + document.querySelectorAll('title').length").expect("script"),
        "String(\"changed|1\")"
    );
}
