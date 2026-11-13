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

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => panic!("{other:?}"),
    }
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
    same(&observed(&b), &want);
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
    same(&observed(&b), &want);
}

/// Equal, naming the first path that differs (the relations are booleans, so say which one).
fn same(got: &serde_json::Value, want: &serde_json::Value) {
    fn walk(path: String, got: &serde_json::Value, want: &serde_json::Value, out: &mut Vec<String>) {
        match (got, want) {
            (serde_json::Value::Object(g), serde_json::Value::Object(w)) => {
                for (k, wv) in w {
                    walk(format!("{path}.{k}"), g.get(k).unwrap_or(&serde_json::Value::Null), wv, out);
                }
            }
            (serde_json::Value::Array(g), serde_json::Value::Array(w)) => {
                for (i, wv) in w.iter().enumerate() {
                    walk(format!("{path}[{i}]"), g.get(i).unwrap_or(&serde_json::Value::Null), wv, out);
                }
            }
            _ if got != want => out.push(format!("{path}: chrome {want}, here {got}")),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(String::new(), got, want, &mut out);
    assert!(out.is_empty(), "{}", out.join("
"));
}

/// Inside one script turn the page's clocks run at the rate of the wall clock. CodeMirror's parser
/// reads `Date.now()` for its slice deadline: a clock that ran 2.6x fast expired it early and it
/// yielded after 21 of 86 tokens (Speedometer 3). A guard, not a fix: on the Windows build the
/// clocks already track wall time, so this passes on the code before the page-clock change too.
#[test]
fn date_now_and_performance_now_advance_at_wall_rate_inside_a_busy_turn() {
    for (clock, what) in [("Date.now()", "Date.now"), ("performance.now()", "performance.now")] {
        let b = page();
        let wall = std::time::Instant::now();
        let seen = match b
            .evaluate(&format!(
                "var s = {clock}, n = 0; while ({clock} - s < 200) {{ n++; }} String({clock} - s)"
            ))
            .unwrap()
        {
            JsValue::String(s) => s.parse::<f64>().unwrap(),
            other => panic!("{other:?}"),
        };
        let wall_ms = wall.elapsed().as_secs_f64() * 1000.0;
        let ratio = seen / wall_ms;
        assert!(
            (0.85..=1.2).contains(&ratio),
            "{what} advanced {seen} ms in a turn that took {wall_ms:.1} ms of wall time (ratio {ratio:.2})"
        );
    }
}

/// A rAF loop in a load: every frame is its own time. All ten frames used to get the first frame's
/// stamp (a delta of 0 where Chrome's is about 16.7) because the stamp was fixed per `runTimers` call.
#[test]
fn consecutive_frames_in_a_load_have_distinct_timestamps_one_frame_apart() {
    let b = page();
    b.evaluate(
        "var last, d = []; requestAnimationFrame(function f(t) { if (last !== undefined) d.push(t - last);          last = t; if (d.length < 10) requestAnimationFrame(f); });",
    )
    .unwrap();
    b.run_timers(1000, 1000).unwrap();
    let got = ev(&b, "d.map(function (x) { return Math.round(x); }).join()");
    let deltas: Vec<i64> = got.split(',').map(|x| x.parse().unwrap()).collect();
    assert_eq!(deltas.len(), 10, "{got}");
    assert!(deltas.iter().all(|d| (15..=25).contains(d)), "frame deltas {got} (Chrome: about 16.7)");
}

/// Capture mode: the page clock has no real time in it, so two runs of one page see the same
/// timestamps however long each took, and a busy loop on `performance.now()` still ends.
#[test]
fn in_capture_mode_the_page_clock_reproduces_across_runs() {
    let run = || {
        let b = page();
        b.set_deterministic_clock(true);
        b.evaluate(
            "var log = []; log.push(performance.now()); var s = performance.now();              while (performance.now() - s < 5) {} log.push(performance.now());              var n = 0; requestAnimationFrame(function f(t) { log.push(t); log.push(new Event('x').timeStamp);              if (++n < 4) requestAnimationFrame(f); }); setTimeout(function () { log.push(performance.now()); }, 40);",
        )
        .unwrap();
        // however long this run takes in real time
        std::thread::sleep(std::time::Duration::from_millis(30));
        b.run_timers(1000, 1000).unwrap();
        ev(&b, "JSON.stringify(log)")
    };
    let first = run();
    std::thread::sleep(std::time::Duration::from_millis(60));
    let second = run();
    assert_eq!(first, second);
    assert!(first.starts_with('['), "{first}");
}
