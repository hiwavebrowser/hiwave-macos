//! `window.matchMedia(query).matches` evaluates the query against the view.
//!
//! The binding is a stub that answers `matches: false` for every query
//! (rustkit-bindings `window_js`), so a script that picks its layout from a
//! media query always takes the narrowest branch. netflix.com's hero form
//! is served with `data-issplitform="true"` (email field and button
//! stacked) and its script flips it to `"false"` (one row) once
//! `(min-width: …)` matches: Chrome 148 at 1280x800 shows the row, RustKit
//! the stacked form (Pete's hand test 10-27). CSS `@media` in sheets is
//! evaluated by the engine and is not affected.
//!
//! Expected values are what the pinned Chrome 148 answers in a 1280x800
//! viewport.

use super::*;

const QUERIES: &str = "[\
    '(min-width: 600px)', 'screen and (min-width: 960px)', '(min-width: 1280px)', '(min-width: 1281px)',\
    '(max-width: 599.98px)', '(orientation: landscape)', 'print', 'not print', 'all', '(min-height: 801px)'\
    ].map(function (q) { return q + '=' + window.matchMedia(q).matches; }).join(' ')";

const CHROME: &str = "(min-width: 600px)=true screen and (min-width: 960px)=true (min-width: 1280px)=true \
    (min-width: 1281px)=false (max-width: 599.98px)=false (orientation: landscape)=true print=false \
    not print=true all=true (min-height: 801px)=false";

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn match_media_answers_for_the_views_viewport() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 800))
        .expect("headless view");
    engine
        .load_html(id, "<!doctype html><html><body><p>x</p></body></html>")
        .expect("load_html");
    let value = engine.execute_script(id, QUERIES).expect("script");
    let got = value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value);
    assert_eq!(got, CHROME.split_whitespace().collect::<Vec<_>>().join(" "));
}
