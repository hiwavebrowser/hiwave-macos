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

/// A resize that flips the answer fires `change` once, after `resize`, on
/// the listener and then on `onchange` (in the order they were added); a
/// resize that does not flip it fires nothing. Chrome 148, 1280x800 then
/// 900 then 950 wide (tools/parity_oracle/mm_resize_log.mjs).
#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn match_media_fires_change_after_resize_when_the_answer_flips() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 800))
        .expect("headless view");
    engine
        .load_html(id, "<!doctype html><html><body><p>x</p></body></html>")
        .expect("load_html");
    engine
        .execute_script(
            id,
            "window.__log = []; var q = matchMedia('(min-width: 1000px)'); window.__q = q; \
             q.addEventListener('change', function (e) { __log.push('listener:' + e.matches + ':' + e.media + ':' + \
                 (e instanceof MediaQueryListEvent)); }); \
             q.onchange = function (e) { __log.push('onchange:' + e.matches); }; \
             addEventListener('resize', function () { __log.push('resize:' + innerWidth); }); \
             __log.push('start:' + q.matches);",
        )
        .expect("script");
    engine.resize_view(id, Bounds::new(0, 0, 900, 800)).expect("resize");
    engine.resize_view(id, Bounds::new(0, 0, 950, 800)).expect("resize");
    let value = engine
        .execute_script(id, "__log.join(' ') + ' now:' + __q.matches")
        .expect("script");
    assert_eq!(
        value,
        "String(\"start:true resize:900 listener:false:(min-width: 1000px):true onchange:false resize:950 now:false\")"
    );
}
