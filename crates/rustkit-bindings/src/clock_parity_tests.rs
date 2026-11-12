//! One clock: requestAnimationFrame's timestamp, Event.timeStamp and document.timeline.currentTime
//! are DOMHighResTimeStamps on the same time origin as performance.now(). A load runs the timers on
//! a virtual clock ahead of real time, so `requestAnimationFrame(t => performance.now() - t)` was
//! negative and Event.timeStamp was epoch milliseconds (MotionMark and Speedometer subtract them).
//!
//! The page script and the expected relations: the pinned Chrome (CfT 148.0.7778.216),
//! `docs/diagnostics/2026-11-12/clock_parity_{page.js,chrome.mjs,cft148.json}`.

use super::*;

const PAGE_JS: &str = include_str!("../../../docs/diagnostics/2026-11-12/clock_parity_page.js");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-12/clock_parity_cft148.json");

fn page() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html("<html><head></head><body><button id=b>x</button></body></html>").unwrap(),
    ))
    .unwrap();
    b
}

fn observed(b: &DomBindings) -> serde_json::Value {
    match b.evaluate("window.__out").unwrap() {
        JsValue::String(s) => serde_json::from_str(&s).unwrap(),
        other => panic!("page did not finish: {other:?}"),
    }
}

/// The load: every timer up to the horizon runs at once, so the timers' clock is far ahead of the
/// real one (the 300 ms timeout and two frames run in a few milliseconds of real time).
#[test]
fn the_frame_timestamp_and_event_time_stamps_share_performance_nows_clock_in_a_load() {
    let b = page();
    b.evaluate(&format!("(0, eval)({})", serde_json::to_string(PAGE_JS).unwrap())).unwrap();
    b.run_timers(5000, 1000).unwrap();
    let want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    assert_eq!(observed(&b), want);
}

/// The live loop: real time advances in turns; the same relations hold.
#[test]
fn the_same_relations_hold_when_the_page_runs_live() {
    let b = page();
    b.evaluate(&format!("(0, eval)({})", serde_json::to_string(PAGE_JS).unwrap())).unwrap();
    for _ in 0..60 {
        b.advance_timers(16, 100).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    let want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    assert_eq!(observed(&b), want);
}
