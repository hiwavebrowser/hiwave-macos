//! One page clock for a frame tree. A page and its same-origin frames are one event loop with one
//! timer queue, so they read one clock: `performance.now()`, the animation frame timestamp and
//! `Event.timeStamp` of every document are the same instant, each counted from its own document's
//! time origin.
//!
//! The expected relations are the pinned Chrome's (CfT 148.0.7778.216, headless, a page with a
//! same-origin `<iframe>` that starts 150 ms after it, recorded 2026-11-15;
//! `docs/diagnostics/2026-11-15/frame_clock_{parent.html,child.html,page.js,chrome.mjs,cft148.json}`):
//!   - each document has its own `performance.timeOrigin`; the frame's is later;
//!   - at one instant `parent.performance.now() - frame.performance.now()` is the difference of
//!     the two origins (the frame's reading is the smaller);
//!   - animation frame callbacks asked for in one task by the page and by the frame run in one
//!     frame, the page's first, and each gets its own document's reading of the frame's time (the
//!     two numbers differ by the origin difference; they are not equal);
//!   - an event's `timeStamp` is fixed when it is made, on the clock of the document whose
//!     constructor made it; dispatching it in the other document does not change it; `click()`
//!     in the frame is stamped on the frame's clock.
//!
//! With one document nothing changes: the first document's origin is the clock's zero.

use super::*;

fn page() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html("<html><head></head><body><button id=b>x</button></body></html>").unwrap(),
    ))
    .unwrap();
    b
}

/// A frame of `parent`'s tree: `child` in the page is its window, `up` in the frame the page's.
fn frame_of(parent: &DomBindings) -> DomBindings {
    let child = parent.new_child().unwrap();
    child
        .set_document(Rc::new(
            Document::parse_html("<html><head></head><body><button id=cb>c</button></body></html>").unwrap(),
        ))
        .unwrap();
    parent.expose_window("child", &child).unwrap();
    child.expose_window("up", &parent).unwrap();
    child
}

/// A page, and a frame made 150 ms of real time and 300 ms of timer time later.
fn tree() -> (DomBindings, DomBindings) {
    let parent = page();
    parent.evaluate("setTimeout(function () {}, 300);").unwrap();
    parent.run_timers(1000, 100).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    let child = frame_of(&parent);
    (parent, child)
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => panic!("{other:?}"),
    }
}

/// The clock reads in a page: a number that does not go back, for `performance.now()`, a new
/// event, an event the engine fires and an animation frame.
#[test]
fn the_page_clock_reads_in_a_page() {
    let b = page();
    let got = ev(
        &b,
        "var a = performance.now(), e = new Event('x'), seen = [], frame; \
         document.getElementById('b').addEventListener('click', function (e) { seen.push(typeof e.timeStamp); }); \
         document.getElementById('b').click(); \
         requestAnimationFrame(function (t) { frame = typeof t; }); \
         [typeof a, typeof e.timeStamp, performance.now() >= a, seen.join()].join()",
    );
    assert_eq!(got, "number,number,true,number");
    b.run_timers(100, 100).unwrap();
    assert_eq!(ev(&b, "String(frame)"), "number");
}

const CAPTURE_LOG: &str = "var log = []; var btn = document.getElementById('b'); \
    btn.addEventListener('click', function (e) { log.push(e.timeStamp); }); btn.click(); log.push(performance.now()); \
    var s = performance.now(); while (performance.now() - s < 5) {} log.push(performance.now()); \
    var n = 0; requestAnimationFrame(function f(t) { log.push(t); log.push(new Event('x').timeStamp); \
    if (++n < 4) requestAnimationFrame(f); }); \
    requestAnimationFrame(function (t) { log.push(t); log.push(performance.now()); }); \
    setTimeout(function () { log.push(performance.now()); log.push(performance.now()); }, 40);";

/// One document, capture mode: every reading is what it was before the clock's state moved onto
/// the frame tree's record (the numbers are the same script's on the tree this was merged with).
#[test]
fn one_documents_capture_clock_reads_what_it_did() {
    let b = page();
    b.set_deterministic_clock(true);
    b.evaluate(CAPTURE_LOG).unwrap();
    b.run_timers(1000, 1000).unwrap();
    b.advance_timers(16, 100).unwrap();
    b.evaluate("log.push(performance.now()); requestAnimationFrame(function (t) { log.push(t); });").unwrap();
    b.advance_timers(16, 100).unwrap();
    let got = ev(&b, "JSON.stringify(log)");
    assert_eq!(got, ONE_DOCUMENT_CAPTURE);
}

/// Recorded with this test's script on the merged-in tree (batch 83, 470f0ae4), one document.
const ONE_DOCUMENT_CAPTURE: &str = "[0.1,0.2,5.4,16,16.1,16,16.2,32,32.1,40,40.1,48,48.1,64,64.1,80,96]";

/// Chrome's `now` rows: the frame's origin is later, it reads less than the page at one instant,
/// and the difference is the difference of the origins, before and after the clock moves.
#[test]
fn a_frames_clock_is_the_pages_instant_from_its_own_origin() {
    let (parent, child) = tree();
    let read = "(function () { var a = performance.now(), c = child.performance.now(), b = performance.now(); \
        return [a, c, b, child.performance.timeOrigin - performance.timeOrigin].join(); })()";
    let numbers = |s: String| s.split(',').map(|x| x.parse::<f64>().unwrap()).collect::<Vec<_>>();
    let first = numbers(ev(&parent, read));
    let (a, c, b, origins) = (first[0], first[1], first[2], first[3]);
    assert!(origins >= 100.0, "the frame's time origin is {origins} ms after the page's");
    assert!(c >= 0.0 && c < a - 100.0, "the frame reads {c}, the page {a}");
    let apart = (a + b) / 2.0 - c;
    // Chrome: within 2 ms. `timeOrigin` is in whole milliseconds here.
    assert!((apart - origins).abs() < 3.0, "the readings are {apart} ms apart, the origins {origins}");
    // The frame's own script reads the same.
    let own = numbers(ev(&child, "[up.performance.now(), performance.now()].join()"));
    assert!((own[0] - own[1] - apart).abs() < 0.5, "read in the frame: {} apart, in the page {apart}", own[0] - own[1]);
    // After real time and a jump of the timers' clock, in whichever document set the timer.
    std::thread::sleep(std::time::Duration::from_millis(30));
    child.evaluate("setTimeout(function () {}, 2000);").unwrap();
    parent.run_timers(5000, 100).unwrap();
    let later = numbers(ev(&parent, read));
    // The timers' clock is at 2300 ms now, and the page clock is never behind it.
    assert!(later[0] >= 2300.0 && later[1] >= c + 1000.0, "both moved with the timers: {later:?} after {first:?}");
    let apart_later = (later[0] + later[2]) / 2.0 - later[1];
    assert!((apart_later - apart).abs() < 0.5, "the readings were {apart} ms apart and are {apart_later}");
}

/// Reading the two documents' clocks in turn does not run the clock fast, and neither goes back.
#[test]
fn reading_two_documents_in_turn_keeps_wall_rate() {
    let (parent, _child) = tree();
    let wall = std::time::Instant::now();
    let got = ev(
        &parent,
        "(function () { var s = performance.now(), p = s, c = child.performance.now(), back = 0, n = 0; \
         while (p - s < 100) { var c2 = child.performance.now(), p2 = performance.now(); \
           if (c2 < c || p2 < p) back++; c = c2; p = p2; n++; } \
         return [p - s, back, n].join(); })()",
    );
    let wall_ms = wall.elapsed().as_secs_f64() * 1000.0;
    let parts: Vec<f64> = got.split(',').map(|x| x.parse().unwrap()).collect();
    assert_eq!(parts[1], 0.0, "a clock went back ({got})");
    let ratio = parts[0] / wall_ms;
    assert!((0.85..=1.2).contains(&ratio), "{} ms on the clock in {wall_ms:.1} ms of wall time ({got})", parts[0]);
}

/// Chrome's `raf` rows, in a load and in a live turn: one frame, the page's callback first, each
/// document's own reading of the frame's time.
#[test]
fn one_frames_callbacks_get_each_documents_reading_of_one_time() {
    for live in [false, true] {
        let (parent, _child) = tree();
        parent
            .evaluate(
                "var got = { order: [] }; var apart = (function () { var a = performance.now(), c = child.performance.now(), b = performance.now(); return (a + b) / 2 - c; })(); \
                 requestAnimationFrame(function (t) { got.p = t; got.pNow = performance.now(); got.order.push('parent'); }); \
                 child.requestAnimationFrame(function (t) { got.c = t; got.cNow = child.performance.now(); got.order.push('frame'); }); \
                 child.requestAnimationFrame(function (t) { got.c2 = t; }); requestAnimationFrame(function (t) { got.p2 = t; });",
            )
            .unwrap();
        if live {
            std::thread::sleep(std::time::Duration::from_millis(16));
            parent.advance_timers(16, 100).unwrap();
        } else {
            parent.run_timers(1000, 100).unwrap();
        }
        let got = ev(
            &parent,
            "[got.p === got.c, Math.abs(got.p - got.c - apart) < 0.5, got.pNow >= got.p && got.cNow >= got.c, \
              got.p2 === got.p && got.c2 === got.c, got.c >= 0, got.order.join('+')].join()",
        );
        assert_eq!(
            got, "false,true,true,true,true,parent+frame",
            "live {live}: equal numbers, apart by the origins, not after each document's now, one stamp a document, the frame's from its origin, order"
        );
    }
}

/// Chrome's `ev` rows.
#[test]
fn an_events_time_stamp_is_on_its_makers_clock_and_stays() {
    let (parent, _child) = tree();
    let got = ev(
        &parent,
        "(function () { var near = function (a, b) { return Math.abs(a - b) < 20; }, out = []; \
         var e1 = new Event('x'), pn = performance.now(), cn = child.performance.now(), e2 = new child.Event('x'); \
         out.push('made by the page: ' + near(e1.timeStamp, pn) + ' ' + near(e1.timeStamp, cn)); \
         out.push('made with the frame\\'s constructor: ' + near(e2.timeStamp, pn) + ' ' + near(e2.timeStamp, cn)); \
         var cb = child.document.getElementById('cb'), seen; \
         cb.addEventListener('x', function (e) { seen = e.timeStamp; }); cb.dispatchEvent(e1); \
         out.push('the page\\'s, dispatched in the frame: ' + (seen === e1.timeStamp) + ' ' + near(seen, pn) + ' ' + near(seen, cn)); \
         var pb = document.getElementById('b'), pseen; \
         pb.addEventListener('x', function (e) { pseen = e.timeStamp; }); pb.dispatchEvent(e2); \
         out.push('the frame\\'s, dispatched in the page: ' + (pseen === e2.timeStamp) + ' ' + near(pseen, pn) + ' ' + near(pseen, cn)); \
         var click; cb.addEventListener('click', function (e) { click = [e.timeStamp, performance.now(), child.performance.now()]; }); cb.click(); \
         out.push('click() in the frame: ' + near(click[0], click[1]) + ' ' + near(click[0], click[2])); \
         return out.join('; '); })()",
    );
    // near the page's now, near the frame's now
    assert_eq!(
        got,
        "made by the page: true false; made with the frame's constructor: false true; \
         the page's, dispatched in the frame: true true false; the frame's, dispatched in the page: true false true; \
         click() in the frame: false true"
    );
}

/// An event the engine fires in a document is stamped on that document's clock (Chrome: `load`
/// at the frame's window reads the frame's clock, at the `<iframe>` element the page's).
#[test]
fn an_event_the_engine_fires_is_on_its_documents_clock() {
    let (parent, child) = tree();
    child.evaluate("var seen; addEventListener('load', function (e) { seen = [e.timeStamp, performance.now(), up.performance.now()]; });").unwrap();
    parent.evaluate("var seen; addEventListener('load', function (e) { seen = [e.timeStamp, performance.now(), child.performance.now()]; });").unwrap();
    child.fire_lifecycle_event(LifecycleTarget::Window, "load").unwrap();
    parent.fire_lifecycle_event(LifecycleTarget::Window, "load").unwrap();
    let near = "[Math.abs(seen[0] - seen[1]) < 20, Math.abs(seen[0] - seen[2]) < 20].join()";
    assert_eq!((ev(&child, near), ev(&parent, near)), ("true,false".to_string(), "true,false".to_string()), "near its own document's now, near the other's: in the frame, in the page");
}

/// What a page and its frame log in capture mode: both documents' readings, events, frames and
/// timers, and a wait on the frame's clock.
const TREE_LOG: &str = "var log = []; \
    log.push(performance.now()); log.push(child.performance.now()); log.push(new child.Event('x').timeStamp); log.push(new Event('x').timeStamp); \
    var s = child.performance.now(); while (child.performance.now() - s < 5) {} log.push(child.performance.now()); log.push(performance.now()); \
    var n = 0; child.requestAnimationFrame(function f(t) { log.push(t); log.push(child.performance.now()); if (++n < 3) child.requestAnimationFrame(f); }); \
    requestAnimationFrame(function (t) { log.push(t); log.push(performance.now()); }); \
    child.setTimeout(function () { log.push(child.performance.now()); log.push(performance.now()); }, 40);";

/// Capture mode with a frame: no real time in either document's clock, so two runs log the same
/// numbers however long each took, and a wait on the frame's clock ends. The frame is made after
/// the switch, as the engine makes it: the switch is the tree's.
#[test]
fn in_capture_mode_a_frame_trees_clock_reproduces_across_runs() {
    let run = |wait: u64| {
        let parent = page();
        parent.set_deterministic_clock(true);
        parent.evaluate("setTimeout(function () {}, 300);").unwrap();
        parent.run_timers(1000, 100).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(wait));
        let child = frame_of(&parent);
        parent.evaluate(TREE_LOG).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(wait));
        parent.run_timers(1000, 1000).unwrap();
        child.advance_timers(16, 100).unwrap();
        parent.advance_timers(16, 100).unwrap();
        ev(&parent, "log.push(child.performance.now()); JSON.stringify(log)")
    };
    let first = run(10);
    let second = run(70);
    assert_eq!(first, second);
    let log: Vec<f64> = serde_json::from_str(&first).unwrap();
    assert_eq!(log.len(), 17, "{first}");
    // The frame's readings count from its own origin: the page's first is past the 300 ms the
    // timers ran before the frame was made, the frame's is not.
    assert!(log[0] >= 300.0 && log[1] < 10.0, "{first}");
}

/// The capture switch thrown after a frame exists reaches the frame, from either document.
#[test]
fn the_capture_switch_reaches_every_document_of_the_tree() {
    let apart = |b: &DomBindings| -> f64 {
        let a: f64 = ev(b, "String(performance.now())").parse().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(40));
        let z: f64 = ev(b, "String(performance.now())").parse().unwrap();
        z - a
    };
    let (parent, child) = tree();
    assert!(apart(&child) >= 35.0, "the frame's clock flows before the switch");
    parent.set_deterministic_clock(true);
    let (in_frame, in_page) = (apart(&child), apart(&parent));
    assert!(in_frame < 5.0 && in_page < 5.0, "after the switch the frame moved {in_frame} ms and the page {in_page} ms across 40 ms");
    child.set_deterministic_clock(false);
    let (in_frame, in_page) = (apart(&child), apart(&parent));
    assert!(in_frame >= 35.0 && in_page >= 35.0, "switched back from the frame: the frame moved {in_frame} ms and the page {in_page} ms");
}
