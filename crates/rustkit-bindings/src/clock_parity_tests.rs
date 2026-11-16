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
    live(8);
}

/// FLK6: the same, with the turns as far apart as a loaded runner made them (batch 83's CI failed
/// `.rafLoop.medianNearAFrame` with 8 ms sleeps). Each turn is still one virtual frame of 16 ms.
#[test]
fn the_same_relations_hold_when_the_live_turns_are_slow() {
    live(45);
}

/// Every animation frame callback's timestamp, with the live turn that ran it (`window.__turn`, set
/// by the test before each turn; -1 before the first).
const RECORD_FRAMES: &str = "window.__turn = -1; window.__frames = []; (function () { \
    var raf = window.requestAnimationFrame; \
    window.requestAnimationFrame = function (cb) { \
        return raf(function (t) { window.__frames.push([t, window.__turn]); return cb(t); }); \
    }; })();";

/// 60 live turns of 16 ms of virtual time, `sleep_ms` of real time apart.
///
/// Live, a frame's timestamp is `performance.now()` at its turn, held at least one virtual frame
/// (16 ms) past the frame before it, so how far apart the page's frames are is how far apart this
/// test's turns are in real time: the page's `.rafLoop.medianNearAFrame` (Chrome: the middle gap is
/// 5 to 40 ms) is a wall-clock bound here, and a loaded runner failed it (FLK6). That one relation is
/// checked on the values instead, against the turns this test made, which holds at any load:
/// - each frame runs one turn after the one before (a frame asked for in a turn is due a virtual
///   frame later, the next turn), and every callback of a turn gets the same timestamp;
/// - consecutive frames in turns `a < b` are at least `16 * (b - a)` ms apart (the virtual floor)
///   and at most `max(16 * (b - a), end[b] - start[a])`, where `start[k]` and `end[k]` are
///   `performance.now()` just before and just after turn `k`: the clock is monotonic, so frame `a`'s
///   stamp is not before `start[a]` and frame `b`'s is not past `end[b]` unless the floor put it
///   there. The span covers the two turns and the one sleep between them.
/// Every other relation is compared with Chrome as it stands.
fn live(sleep_ms: u64) {
    let b = page();
    b.evaluate(RECORD_FRAMES).unwrap();
    b.evaluate(&format!("(0, eval)({})", serde_json::to_string(PAGE_JS).unwrap())).unwrap();
    let now = |b: &DomBindings| ev(b, "String(performance.now())").parse::<f64>().unwrap();
    let (mut start, mut end) = (Vec::new(), Vec::new());
    for turn in 0..60 {
        b.evaluate(&format!("window.__turn = {turn}")).unwrap();
        start.push(now(&b));
        b.advance_timers(16, 100).unwrap();
        end.push(now(&b));
        std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
    }
    let frames: Vec<(f64, i64)> = serde_json::from_str(&ev(&b, "JSON.stringify(window.__frames)")).unwrap();
    // One timestamp per turn, in turn order.
    let mut stamps: Vec<(i64, f64)> = Vec::new();
    for &(t, turn) in &frames {
        assert!(turn >= 0, "a frame ran outside a live turn: {frames:?}");
        match stamps.last() {
            Some(&(last, at)) if last == turn => assert_eq!(at, t, "two timestamps in turn {turn}: {frames:?}"),
            Some(&(last, _)) => assert!(turn > last, "frames out of turn order: {frames:?}"),
            None => {}
        }
        if stamps.last().map_or(true, |&(last, _)| last != turn) {
            stamps.push((turn, t));
        }
    }
    // The page asks for 2 frames, then a loop of 11: turns 0 to 10.
    assert_eq!(stamps.len(), 11, "{frames:?}");
    for pair in stamps.windows(2) {
        let ((a, ta), (b2, tb)) = (pair[0], pair[1]);
        assert_eq!(b2, a + 1, "a frame skipped a turn: {stamps:?}");
        let floor = 16.0 * (b2 - a) as f64;
        let span = end[b2 as usize] - start[a as usize];
        assert!(tb - ta >= floor - 1e-6, "frames {a} and {b2} less than a frame apart: {stamps:?}");
        assert!(
            tb - ta <= floor.max(span) + 1e-6,
            "frames {a} and {b2} are {} ms apart, more than the {span} ms their turns took: {stamps:?} {end:?}",
            tb - ta
        );
    }
    let want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    let mut got = observed(&b);
    // Checked above, against the turns.
    got["rafLoop"]["medianNearAFrame"] = want["rafLoop"]["medianNearAFrame"].clone();
    same(&got, &want);
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
            "var log = []; var btn = document.getElementById('b'); btn.addEventListener('click', function (e) { log.push(e.timeStamp); }); btn.click(); log.push(performance.now()); var s = performance.now();              while (performance.now() - s < 5) {} log.push(performance.now());              var n = 0; requestAnimationFrame(function f(t) { log.push(t); log.push(new Event('x').timeStamp);              if (++n < 4) requestAnimationFrame(f); }); setTimeout(function () { log.push(performance.now()); }, 40);",
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

/// The real-flowing page clock never loses real time. It used to count real time only since the
/// virtual clock last moved, and held its last value until virtual time caught up: after a long
/// real wait with no virtual step, the next timer left `performance.now()` far behind real time.
#[test]
fn the_page_clock_keeps_real_time_across_a_virtual_step() {
    let b = page();
    b.evaluate(
        "var p0 = performance.now(), p1; setTimeout(function () { p1 = performance.now(); }, 16);",
    )
    .unwrap();
    // 150 ms of real time pass with the virtual clock standing still, then it steps 16 ms
    std::thread::sleep(std::time::Duration::from_millis(150));
    b.run_timers(100, 100).unwrap();
    let gap: f64 = ev(&b, "String(p1 - p0)").parse().unwrap();
    assert!(gap >= 140.0, "performance.now() moved {gap} ms across 150 ms of real time and a virtual step");
}

/// A long task, then the timers' clock catching up. The task's real time is given to the timers'
/// clock on the turn after it, so the first frame after the task runs with that clock still behind
/// and is stamped real time; the next frame used to be stamped that stamp plus the whole catch-up:
/// the task counted twice, and `performance.now()` ran ahead of real time by the task's length from
/// then on. In the app's window a frame's 3000 ms task left the frame's clock 3202 ms ahead
/// (`docs/diagnostics/2026-11-15/long_task_clock_app.json`); the pinned Chrome's stays within a
/// millisecond of `Date.now()` (`long_task_clock_cft148.json`). MotionMark reads this clock to
/// decide a test is over.
///
/// The bound is on the values: two frames are no further apart than the real time between them
/// plus one frame's floor, however long this test's own calls took.
#[test]
fn a_frame_after_a_long_tasks_catch_up_is_not_stamped_the_task_twice() {
    let b = page();
    b.evaluate(
        "var log = []; requestAnimationFrame(function f(t) { log.push(t, performance.now()); \
         if (log.length < 4) requestAnimationFrame(f); });",
    )
    .unwrap();
    // The task: real time the timers' clock has not been given.
    let task_ms = 300;
    std::thread::sleep(std::time::Duration::from_millis(task_ms));
    let began = std::time::Instant::now();
    b.advance_timers(16, 10).unwrap(); // the first frame, the timers' clock a task behind
    b.advance_timers(task_ms, 10).unwrap(); // the catch-up, and the second frame
    let real_ms = began.elapsed().as_secs_f64() * 1000.0;
    let log: Vec<f64> = ev(&b, "log.join()").split(',').map(|x| x.parse().unwrap()).collect();
    assert_eq!(log.len(), 4, "{log:?}");
    let (stamps, clock) = (log[2] - log[0], log[3] - log[1]);
    assert!(stamps >= 16.0 - 1e-6, "the two frames are {stamps} ms apart, less than a frame: {log:?}");
    assert!(
        stamps <= real_ms + 16.0 + 1e-6 && clock <= real_ms + 16.0 + 1e-6,
        "{real_ms} ms of real time passed between two frames; their stamps are {stamps} ms apart and \
         performance.now() moved {clock} ms: {log:?}"
    );
}
