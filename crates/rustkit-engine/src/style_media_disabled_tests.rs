//! CH2.E17: a `<style media>` applies only where its media query list
//! matches, and `<style>.sheet.disabled = true` takes the sheet out of the
//! cascade. Before this, both were no-ops: `<style media=print>` applied on
//! screen and a disabled sheet kept applying.
//!
//! Every expected value is Chromium 143.0.7499.4 on the same markup at an
//! 800x600 viewport (`scratch/e17_oracle.mjs` in the PR, hub receipt
//! `ch2-e17-oracle.mjs`). `#t` is 10px wide by the first sheet and 20px by
//! the second when that one applies.

use super::*;

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine.execute_script(view, &format!("String({expr})")).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

const BASE: &str = "<style>body{margin:0} #t{width:10px}</style>";

fn page(body: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let html = format!("<html><head>{BASE}</head><body>{body}<div id=t></div></body></html>");
    engine.load_html(view, &html).expect("load_html");
    (engine, view)
}

/// `#t`'s offsetWidth x offsetHeight after `script` (if any).
fn size(body: &str, script: &str) -> String {
    let (mut engine, view) = page(body);
    if !script.is_empty() {
        engine.execute_script(view, script).expect("script");
    }
    let w = read(&mut engine, view, "document.getElementById('t').offsetWidth");
    let h = read(&mut engine, view, "document.getElementById('t').offsetHeight");
    format!("{w}x{h}")
}

fn media(list: &str) -> String {
    size(&format!("<style media=\"{list}\">#t{{width:20px}}</style>"), "")
}

#[test]
fn a_style_media_list_applies_only_where_it_matches() {
    let cases = [
        ("print", "10x0"),
        ("screen", "20x0"),
        ("all", "20x0"),
        ("", "20x0"),
        ("(min-width: 700px)", "20x0"),
        ("(min-width: 900px)", "10x0"),
        ("print, screen", "20x0"),
        ("foo bar", "10x0"),
        ("tv", "10x0"),
        ("SCREEN", "20x0"),
        ("not print", "20x0"),
    ];
    let mut wrong = Vec::new();
    for (list, want) in cases {
        let got = media(list);
        if got != want {
            wrong.push(format!("media={list:?}: got {got}, Chromium {want}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn an_inner_media_rule_cannot_widen_the_elements_media() {
    // media=print around `@media screen { ... }`: both must hold.
    assert_eq!(size("<style media=print>@media screen { #t{width:20px} }</style>", ""), "10x0");
}

#[test]
fn a_media_attribute_set_or_removed_by_script_applies() {
    let set = size("<style id=s>#t{width:20px}</style>", "document.getElementById('s').setAttribute('media','print')");
    assert_eq!(set, "10x0", "media=print set by script");
    let removed = size("<style id=s media=print>#t{width:20px}</style>", "document.getElementById('s').removeAttribute('media')");
    assert_eq!(removed, "20x0", "media removed by script");
}

#[test]
fn a_style_media_query_follows_a_viewport_resize() {
    let (mut engine, view) = page("<style media=\"(max-width: 500px)\">#t{width:20px}</style>");
    assert_eq!(read(&mut engine, view, "document.getElementById('t').offsetWidth"), "10");
    engine
        .resize_view(view, Bounds { x: 0, y: 0, width: 400, height: 600 })
        .expect("resize");
    assert_eq!(read(&mut engine, view, "document.getElementById('t').offsetWidth"), "20");
}

#[test]
fn a_disabled_style_sheet_leaves_the_cascade() {
    let s = "<style id=s>#t{width:20px}</style>";
    let cases = [
        ("sheet.disabled", "document.getElementById('s').sheet.disabled = true", "10x0"),
        ("styleSheets[1]", "document.styleSheets[1].disabled = true", "10x0"),
        (
            "disabled then enabled",
            "var s = document.getElementById('s').sheet; s.disabled = true; document.body.offsetWidth; s.disabled = false",
            "20x0",
        ),
        (
            "insertRule keeps it disabled",
            "var e = document.getElementById('s'); e.sheet.disabled = true; e.sheet.insertRule('#t{height:5px}', 1)",
            "10x0",
        ),
    ];
    let mut wrong = Vec::new();
    for (name, script, want) in cases {
        let got = size(s, script);
        if got != want {
            wrong.push(format!("{name}: got {got}, Chromium {want}"));
        }
    }
    let first = size(
        "",
        "var e = document.createElement('style'); e.id = 'x'; e.textContent = '#t{width:20px}'; document.head.appendChild(e);\
         document.querySelector('style').sheet.disabled = true",
    );
    if first != "20x0" {
        wrong.push(format!("first sheet disabled: got {first}, Chromium 20x0"));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn new_text_makes_a_new_enabled_sheet() {
    // Chromium makes a new CSSStyleSheet when a <style>'s text changes,
    // and a new sheet is not disabled.
    let s = "<style id=s>#t{width:20px}</style>";
    let changed = size(s, "var e = document.getElementById('s'); e.sheet.disabled = true; e.textContent = '#t{width:30px}'");
    assert_eq!(changed, "30x0", "textContent replaced");
    let appended = size(
        s,
        "var e = document.getElementById('s'); e.sheet.disabled = true; e.appendChild(document.createTextNode('#t{height:5px}'))",
    );
    assert_eq!(appended, "20x5", "text appended");
    let (mut engine, view) = page(s);
    engine
        .execute_script(view, "var e = document.getElementById('s'); e.sheet.disabled = true; e.textContent = '#t{width:30px}'")
        .unwrap();
    assert_eq!(read(&mut engine, view, "document.getElementById('s').sheet.disabled"), "false");
}

#[test]
fn the_disabled_attribute_on_a_style_is_not_the_sheets_flag() {
    // `<style disabled>` is not a thing in HTML: Chromium applies it.
    assert_eq!(size("<style disabled>#t{width:20px}</style>", ""), "20x0");
}

#[test]
fn sheet_text_cannot_close_the_media_attributes_condition_early() {
    // A stray `}` at the top level of a sheet is part of the next rule's
    // prelude, which then fails: Chromium drops that one rule, `media` or
    // not. Wrapping the text as `@media <list> { ... }` let the `}` close
    // the wrapper instead, so the rules after it applied everywhere.
    // Chromium 148.0.7778.216 headless, 800x600 (#964 R1 nit 1).
    let cases = [
        ("screen, stray }", "<style media=\"screen\">a{} } #t{width:20px}</style>", "10x0"),
        ("print, stray }", "<style media=\"print\">a{} } #t{width:20px}</style>", "10x0"),
        ("no media, stray }", "<style>a{} } #t{width:20px}</style>", "10x0"),
        ("screen, stray } then ;", "<style media=\"screen\">a{} }; #t{width:20px}</style>", "10x0"),
        ("screen, only the next rule", "<style media=\"screen\">a{} } b{} #t{width:20px}</style>", "20x0"),
        ("print, only the next rule", "<style media=\"print\">a{} } b{} #t{width:20px}</style>", "10x0"),
        ("screen, at-rule with } ends at ;", "<style media=\"screen\">@layer x }; #t{width:20px}</style>", "20x0"),
        ("screen, } in a function", "<style media=\"screen\">a{b:c(})} #t{width:20px}</style>", "20x0"),
        ("screen, stray } at the end", "<style media=\"screen\">#t{width:20px} }</style>", "20x0"),
        ("print, stray } at the end", "<style media=\"print\">#t{width:20px} }</style>", "10x0"),
    ];
    let mut wrong = Vec::new();
    for (name, body, want) in cases {
        let got = size(body, "");
        if got != want {
            wrong.push(format!("{name}: got {got}, Chromium {want}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
