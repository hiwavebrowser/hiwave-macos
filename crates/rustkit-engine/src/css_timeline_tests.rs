//! TIMELINE2: CSS animations and transitions on the document timeline, the one script animations
//! run on (TIMELINE1). Pinned on Chrome for Testing 148.0.7778.216, headless,
//! `docs/diagnostics/2026-11-16/css_timeline_probe_cft148.json` (5 runs):
//! - In every frame and every task a running CSS animation's `startTime + currentTime` is
//!   `document.timeline.currentTime` (0 in 25 of 25 frame rows and 15 of 15 task rows), and its
//!   `startTime` stays put.
//! - In a frame a long callback held up 100 ms, the same holds, and the painted value is the one
//!   for that time (`currentTime - opacity * 10000` = 0 in 25 of 25 rows).
//! - In a frame document inserted later than its parent (time origin 1850-1939 ms later), the
//!   start time is on the frame's own timeline (11-15 ms there, not the parent's), and a script
//!   animation given the CSS animation's start time runs at the same current time (0 in 15 of 15).

use super::*;
use crate::script_net_tests::serve_routes;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn load_with(config: EngineConfig, routes: Vec<(&'static str, &'static str, String)>) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(routes);
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine
        .execute_script(view, &format!("String({expr})"))
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn run(engine: &mut Engine, view: EngineViewId, script: &str) {
    engine.execute_script(view, script).unwrap();
}

/// Live turns of `step` ms until `total` ms have passed (the window loop).
fn pump(engine: &mut Engine, view: EngineViewId, total: u64, step: u64) {
    let rt = rt();
    let mut t = 0;
    while t < total {
        rt.block_on(engine.pump_live(view, step));
        t += step;
    }
}

/// Live turns of `step` ms until `done` reads true, at most `turns` of them.
fn pump_until(engine: &mut Engine, view: EngineViewId, done: &str, turns: usize, step: u64) -> bool {
    let rt = rt();
    for _ in 0..turns {
        if read(engine, view, done) == "true" {
            return true;
        }
        rt.block_on(engine.pump_live(view, step));
    }
    read(engine, view, done) == "true"
}

const FADE: &str = "@keyframes fade{from{opacity:0}to{opacity:1}}";

fn page() -> String {
    format!(
        "<!doctype html><style>body{{margin:0}}#h{{width:20px;height:20px;background:#123;animation:fade 10s linear}}{FADE}</style>\
         <div id=h></div><script>var h=document.getElementById('h');\
         function tl(){{return document.timeline.currentTime}}</script>"
    )
}

/// Numbers from a comma-joined reading.
fn numbers(got: &str) -> Vec<f64> {
    got.split(',')
        .map(|v| v.parse::<f64>().unwrap_or_else(|_| panic!("{got}")))
        .collect()
}

/// In a timer task the timeline is the time of the task's frame, and a running CSS animation's
/// `startTime + currentTime` is that time, its `startTime` the same in every task (Chrome
/// "inTask": 0 in 15 of 15 rows).
#[test]
fn in_a_task_a_css_animation_is_on_the_document_timeline() {
    let (mut engine, view, _server) = load_with(EngineConfig::default(), vec![("/", "text/html", page())]);
    pump(&mut engine, view, 100, 20);
    run(
        &mut engine,
        view,
        "window.got = []; var c = h.getAnimations()[0]; (function f() { setTimeout(function () { \
           got.push(c.startTime + c.currentTime - tl(), c.startTime); if (got.length < 6) f(); }, 37); })();",
    );
    assert!(pump_until(&mut engine, view, "got.length >= 6", 40, 20), "{}", read(&mut engine, view, "got"));
    let got = numbers(&read(&mut engine, view, "got"));
    for row in got.chunks(2) {
        assert!(row[0].abs() < 0.01, "startTime + currentTime - timeline: {got:?} (Chrome: 0)");
        assert_eq!(row[1], got[1], "the start time moved: {got:?}");
    }
}

/// A frame the turn ran late, on the deterministic clock: the timeline reads the frame's time,
/// and the CSS animation's `startTime + currentTime` is that time (was: the turn's), its
/// painted value the one for that time (Chrome "late": 0 and 0 in 25 of 25 rows).
#[test]
fn in_a_late_frame_a_css_animation_is_on_the_document_timeline_and_paints_its_time() {
    let config = EngineConfig { deterministic_page_clock: true, ..EngineConfig::default() };
    let (mut engine, view, _server) = load_with(config, vec![("/", "text/html", page())]);
    pump(&mut engine, view, 100, 10);
    run(
        &mut engine,
        view,
        "window.got = ''; var c = h.getAnimations()[0]; requestAnimationFrame(function (t) { \
           got = [t === tl(), c.startTime + c.currentTime - tl(), c.currentTime - parseFloat(getComputedStyle(h).opacity) * 10000].join(); });",
    );
    pump(&mut engine, view, 40, 40);
    let got = read(&mut engine, view, "got");
    let (same, rest) = got.split_once(',').unwrap_or_else(|| panic!("{got}"));
    assert_eq!(same, "true", "{got}");
    let rest = numbers(rest);
    assert!(rest[0].abs() < 0.01, "startTime + currentTime - timeline: {got} (Chrome: 0)");
    assert!(rest[1].abs() < 0.5, "currentTime - the painted opacity's time: {got} (Chrome: 0)");
}

const CHILD: &str = "<!doctype html><style>@keyframes fade{from{opacity:0}to{opacity:1}}\
    #a{width:10px;height:10px;animation:fade 10s linear}#s{width:10px;height:10px}</style>\
    <div id=a></div><div id=s></div><script>function tl(){return document.timeline.currentTime}\
    window.go = function () { var a = document.getElementById('a'), s = document.getElementById('s');\
      var c = a.getAnimations()[0]; window.rows = []; window.start0 = c.startTime;\
      (function f() { requestAnimationFrame(function () {\
        rows.push(c.startTime + c.currentTime - tl(), c.startTime - tl());\
        if (rows.length < 6) f(); else sync(); }); })();\
      function sync() { var sa = s.animate({opacity:[0,1]}, {duration:10000}); sa.startTime = c.startTime;\
        (function f() { requestAnimationFrame(function () {\
          rows.push(sa.currentTime - c.currentTime, c.startTime - tl());\
          if (rows.length < 12) f(); }); })(); } };</script>";

/// A frame document inserted after its parent's start counts from its own time origin:
/// its CSS animation's start time is on the frame's timeline, never ahead of it, and a script
/// animation given that start time runs at the CSS animation's current time (Chrome "child": 0
/// in 15 of 15 rows; start times 11-15 ms on the frame's timeline, origin 1850-1939 ms later).
/// Was: the frame's CSS start time was 0 on the tree's clock, before the frame was made, so it
/// read 0 on the frame's timeline and painted the frame's origin ahead of it.
#[test]
fn in_a_frame_document_a_css_animation_is_on_the_frames_own_timeline() {
    let parent = "<!doctype html><body><script>window.ready = false;\
        setTimeout(function () { var f = document.createElement('iframe'); f.id = 'f';\
          f.onload = function () { window.ready = true; }; f.src = '/child.html'; document.body.appendChild(f); }, 300);</script></body>";
    let config = EngineConfig { nested_contexts: true, post_load_requests_live: true, ..EngineConfig::default() };
    let (mut engine, view, _server) = load_with(
        config,
        vec![("/", "text/html", parent.to_string()), ("/child.html", "text/html", CHILD.to_string())],
    );
    assert!(pump_until(&mut engine, view, "window.ready", 100, 20), "the frame did not load");
    let origin = read(&mut engine, view, "f.contentWindow.performance.timeOrigin - performance.timeOrigin");
    // Its origin is later than the parent's (here ~140 ms: the frame loads on the live turns).
    assert!(origin.parse::<f64>().unwrap() >= 50.0, "the frame's origin is later: {origin}");
    run(&mut engine, view, "f.contentWindow.go()");
    assert!(
        pump_until(&mut engine, view, "f.contentWindow.rows.length >= 12", 60, 20),
        "{}",
        read(&mut engine, view, "f.contentWindow.rows")
    );
    // The start time is on the frame's timeline: the frame's first frames, not the parent's time
    // (Chrome 11-15 ms; the start time cancels out of the rows below, so it is read on its own).
    let start = read(&mut engine, view, "f.contentWindow.start0");
    let start: f64 = start.parse().unwrap_or_else(|_| panic!("{start}"));
    assert!((0.0..50.0).contains(&start), "the CSS start time on the frame's timeline: {start} (Chrome 11-15)");
    // ... and after each turn the frame shows the value for the time its animation reports (was:
    // its start was the tree's clock zero, before the frame was made, so it painted ~200 ms
    // ahead). Read after the turn, as a capture paints it: inside a frame callback a computed read
    // answers from the last turn's layout here (Chrome updates animations before the callbacks).
    let paint_expr = "f.contentWindow.document.getElementById('a').getAnimations()[0].currentTime - \
        parseFloat(f.contentWindow.getComputedStyle(f.contentDocument.getElementById('a')).opacity) * 10000";
    let mut paint = Vec::new();
    for _ in 0..3 {
        pump(&mut engine, view, 20, 20);
        paint.push(read(&mut engine, view, paint_expr).parse::<f64>().unwrap());
    }
    for p in &paint {
        assert!(p.abs() < 0.5, "currentTime - the painted opacity's time: {paint:?} (Chrome: 0)");
    }
    let rows = numbers(&read(&mut engine, view, "f.contentWindow.rows"));
    for (i, pair) in rows.chunks(2).enumerate() {
        assert!(pair[0].abs() < 0.01, "row {i}: {rows:?} (Chrome: 0)");
        assert!(pair[1] <= 0.0, "row {i}: the start time is ahead of the frame's timeline: {rows:?}");
    }
}
