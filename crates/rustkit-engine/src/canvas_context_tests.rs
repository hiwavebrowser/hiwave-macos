//! Z plan CH2.B4: HiWave's report page (report.html, Chart.js 4.4.0) shows
//! "Failed to load analytics data: cannot convert 'null' or 'undefined' to
//! object" in RustKit and two charts in Chromium 143.
//!
//! The cause is not `getComputedStyle`: `HTMLCanvasElement.getContext` returns
//! null for every kind (`rustkit-bindings/src/dom.rs`), so the first
//! `new Chart(ctx)` cannot acquire a context (console error, a chart with no
//! canvas), and the second reads `o.canvas.id` on that chart's null canvas
//! while it checks "Canvas is already in use". Boa reports any property read
//! on null with that one message, which is why it looked like a
//! getPropertyValue bug.
//!
//! Chromium 143: `getContext('2d')` returns a CanvasRenderingContext2D whose
//! `canvas` is the element, the same object on every call; report.html's two
//! charts are created and its error container stays empty.

#![cfg(all(target_os = "macos", feature = "headless"))]

use super::*;

const CHART_JS: &str = include_str!("../../hiwave-app/src/ui/chart.umd.min.js");
const REPORT_HTML: &str = include_str!("../../hiwave-app/src/ui/report.html");

fn eval(engine: &mut Engine, id: EngineViewId, script: &str) -> String {
    let value = engine.execute_script(id, script).expect("script");
    let inner = value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value);
    inner.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn view(html: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 800))
        .expect("view");
    engine.load_html(id, html).expect("load");
    (engine, id)
}

#[test]
fn canvas_2d_context_is_the_canvas_own_and_stable() {
    let (mut engine, id) = view("<!doctype html><canvas id=c width=300 height=150></canvas>");
    let read = eval(
        &mut engine,
        id,
        "var c = document.getElementById('c'); var x = c.getContext('2d'); \
         JSON.stringify([x !== null, x && x.canvas === c, c.getContext('2d') === x, \
                         String(x && Object.prototype.toString.call(x))])",
    );
    assert_eq!(
        read, r#"[true,true,true,"[object CanvasRenderingContext2D]"]"#,
        "Chromium 143: a context, its canvas is the element, the same object again"
    );
}

/// The page as the app serves it: Chart.js where the app injects it, a host
/// that answers `get_today_stats` the way the real one does (a
/// `hiwave-response` event after the post).
#[test]
fn report_page_creates_both_charts_without_an_error() {
    let host = "<script>window.ipc = { postMessage: function (raw) { \
        var cmd = JSON.parse(raw).cmd; if (cmd !== 'get_today_stats') return; \
        Promise.resolve().then(function () { window.dispatchEvent(new CustomEvent('hiwave-response', \
        { detail: { type: 'success', data: { pages_visited: 42, trackers_blocked: 311, ads_blocked: 97, \
        popups_blocked: 3, bandwidth_saved: 0, time_saved: 0, focus_time: 0 } } })); }); } };\
        window.__errs = []; console.error = function (m) { window.__errs.push(String(m)); };</script>";
    let marker = "<!-- Chart.js is injected by Rust before page load -->";
    assert!(REPORT_HTML.contains(marker), "report.html's injection marker");
    let html = REPORT_HTML.replacen(marker, &format!("{host}<script>{CHART_JS}</script>"), 1);
    let (mut engine, id) = view(&html);
    let read = eval(
        &mut engine,
        id,
        "JSON.stringify([document.getElementById('error-container').textContent.trim(), \
                         window.__errs, document.getElementById('stat-trackers').textContent, \
                         !!(activityChart && activityChart.canvas), !!(protectionChart && protectionChart.canvas)])",
    );
    assert_eq!(
        read, r#"["",[],"311",true,true]"#,
        "Chromium 143: no error banner, no console error, both charts on their canvases"
    );
}
