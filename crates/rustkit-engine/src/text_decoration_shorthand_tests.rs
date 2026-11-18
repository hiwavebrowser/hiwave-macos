//! The `text-decoration` shorthand (css-text-decor-3 §2.4:
//! `<line> || <style> || <color> || <thickness>`). Only the line was read:
//! `text-decoration: underline red` left the colour at currentcolor (the
//! line drew in the text colour), a colour alone kept an earlier line, an
//! omitted part did not reset its longhand, and an invalid value still set
//! the line.
//!
//! Oracle: Chrome for Testing 148.0.7778.216 through
//! `tools/parity_oracle/text_decoration_shorthand_log.mjs`, receipt
//! `docs/diagnostics/2026-11-17/text_decoration_shorthand_cft148.json`:
//! `#t` has `color: rgb(0,0,255)`; each case reads the line, style and
//! colour longhands and the shorthand.

use super::*;
use crate::script_net_tests::serve_routes;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn load(page: &str) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

/// `line | style | color | shorthand` as Chrome's oracle prints them.
fn read(css: &str) -> String {
    let (mut engine, view, _server) = load(&format!(
        "<!doctype html><style>body{{margin:0}}#t{{color:rgb(0,0,255);font:16px sans-serif}}{css}</style>\
         <div id=t>text</div>"
    ));
    let value = engine
        .execute_script(
            view,
            "var c=getComputedStyle(document.getElementById('t'));\
             String(['text-decoration-line','text-decoration-style','text-decoration-color','text-decoration']\
             .map(function(p){return c.getPropertyValue(p)}).join(' | '))",
        )
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

#[track_caller]
fn check(css: &str, chrome: &str) {
    assert_eq!(read(css), chrome, "{css}");
}

#[test]
fn a_shorthand_sets_line_style_and_colour() {
    check(
        "#t{text-decoration:underline red}",
        "underline | solid | rgb(255, 0, 0) | underline rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:red underline}",
        "underline | solid | rgb(255, 0, 0) | underline rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:underline wavy rgb(0,128,0)}",
        "underline | wavy | rgb(0, 128, 0) | underline wavy rgb(0, 128, 0)",
    );
    check(
        "#t{text-decoration:underline overline red}",
        "underline overline | solid | rgb(255, 0, 0) | underline overline rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:underline currentcolor}",
        "underline | solid | rgb(0, 0, 255) | underline",
    );
}

/// A part left out is reset: a colour alone is no line, a style alone
/// keeps no line, and an earlier colour or style goes back to its initial.
#[test]
fn omitted_parts_reset_their_longhands() {
    check(
        "#t{text-decoration:red}",
        "none | solid | rgb(255, 0, 0) | rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:underline}#t{text-decoration:red}",
        "none | solid | rgb(255, 0, 0) | rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:dotted}",
        "none | dotted | rgb(0, 0, 255) | dotted",
    );
    check(
        "#t{text-decoration-color:red;text-decoration:underline}",
        "underline | solid | rgb(0, 0, 255) | underline",
    );
    check(
        "#t{text-decoration-style:wavy;text-decoration:underline red}",
        "underline | solid | rgb(255, 0, 0) | underline rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:none red}",
        "none | solid | rgb(255, 0, 0) | rgb(255, 0, 0)",
    );
    check(
        "#t{text-decoration:underline red;text-decoration-color:rgb(0,128,0)}",
        "underline | solid | rgb(0, 128, 0) | underline rgb(0, 128, 0)",
    );
}

/// A thickness is read and dropped (not modelled: Chrome 148's shorthand
/// reads `underline 2px rgb(255, 0, 0)`); the other parts still apply.
#[test]
fn a_thickness_does_not_drop_the_declaration() {
    let got = read("#t{text-decoration:underline red 2px}");
    assert!(
        got.starts_with("underline | solid | rgb(255, 0, 0) | "),
        "{got}"
    );
}

/// An unreadable part drops the whole declaration (the earlier one stands).
#[test]
fn an_invalid_value_is_dropped() {
    check(
        "#t{text-decoration:underline}#t{text-decoration:overline bogus red}",
        "underline | solid | rgb(0, 0, 255) | underline",
    );
    check(
        "#t{text-decoration:underline}#t{text-decoration:overline red blue}",
        "underline | solid | rgb(0, 0, 255) | underline",
    );
}

/// The line is painted in the shorthand's colour: red pixels under blue
/// text (none before, when the line took the text colour).
#[test]
fn the_line_paints_in_the_shorthands_colour() {
    let red_pixels = |css: &str| {
        let (mut engine, view, _server) = load(&format!(
            "<!doctype html><style>body{{margin:0}}\
             #t{{color:rgb(0,0,255);font:40px sans-serif;{css}}}</style><div id=t>text text</div>"
        ));
        let (w, h, rgba) = engine.frame_rgba(view).expect("frame");
        (0..(w * h.min(80)) as usize)
            .filter(|i| {
                let p = &rgba[i * 4..i * 4 + 3];
                p[0] >= 200 && p[1] <= 60 && p[2] <= 60
            })
            .count()
    };
    assert_eq!(red_pixels("text-decoration:none"), 0);
    assert!(
        red_pixels("text-decoration:underline red") > 20,
        "a red underline"
    );
}
