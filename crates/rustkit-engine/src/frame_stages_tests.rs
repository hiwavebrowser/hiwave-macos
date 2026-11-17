//! FRAME-STAGES: the per-turn stage line a stress run reads (MotionMark's
//! stage split, docs/diagnostics/2026-11-17/motionmark_stage_split.md).
//! Shapes and counts only: no test here bounds a time.

use super::*;
use crate::script_net_tests::serve_routes;
use crate::wgate::HoldStage;

/// A canvas redrawn and a box moved on every animation frame.
const PAGE: &str = "<html><body style='margin:0'>\
    <canvas id=c width=100 height=100></canvas>\
    <div id=d style='width:10px;height:10px;background:#c33'></div>\
    <script>\
    var g = document.getElementById('c').getContext('2d'), n = 0;\
    function f() { n++; g.clearRect(0, 0, 100, 100); g.fillStyle = '#36c'; g.fillRect(n % 50, 0, 10, 10);\
      g.beginPath(); g.arc(50, 50, 20, 0, 6.28); g.stroke();\
      document.getElementById('d').style.marginLeft = (n % 20) + 'px'; requestAnimationFrame(f); }\
    requestAnimationFrame(f);\
    </script></body></html>";

fn load(page: &str) -> (Engine, EngineViewId, crate::script_net_tests::Server, tokio::runtime::Runtime) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 200 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server, rt)
}

/// The line names every field once, in the published order, each with a
/// number: `tools/stress/frame_stages.py` reads it by these names.
#[test]
fn the_frame_stage_line_names_every_field_once_in_order() {
    let line = frame_stage_line(3, 48.0, 12.5, 4.0, &FrameStageTotals::default());
    let body = line.strip_prefix("Frame stages ").expect("the line's prefix");
    let keys: Vec<&str> = body.split(' ').map(|kv| kv.split('=').next().unwrap()).collect();
    assert_eq!(keys, FRAME_STAGE_FIELDS);
    for kv in body.split(' ') {
        let value = kv.split_once('=').expect("key=value").1;
        assert!(value.parse::<f64>().is_ok(), "{kv} is not a number");
    }
    // What no stage claims is the turn less the stages (never below 0).
    let mut d = FrameStageTotals::default();
    d.script_ms = 5.0;
    d.render_ms = 3.0;
    assert!(frame_stage_line(1, 0.0, 10.0, 0.0, &d).ends_with(" other_ms=2.000"));
    assert!(frame_stage_line(1, 0.0, 1.0, 0.0, &d).ends_with(" other_ms=0.000"));
}

/// Live turns of a page that redraws a canvas and moves a box on every
/// animation frame: each draw into the canvas bitmap counts, and so does
/// each render of the view.
#[test]
fn live_turns_of_an_animating_page_count_its_canvas_draws_and_renders() {
    let (mut engine, view, _server, rt) = load(PAGE);
    set_frame_stage_timing(true);
    // The frames the page has run (the load's settle ran some too).
    let count = |engine: &mut Engine| -> u64 {
        let shown = engine.execute_script(view, "String(n)").unwrap();
        shown.trim_matches(|c: char| !c.is_ascii_digit()).parse().expect(&shown)
    };
    let ran_before = count(&mut engine);
    let before = frame_stage_totals();
    for _ in 0..5 {
        rt.block_on(engine.pump_live(view, 16));
    }
    let d = frame_stage_totals().since(&before);
    let frames = count(&mut engine) - ran_before;
    assert!(frames >= 3, "{frames} frames in 5 turns");
    assert_eq!(d.canvas_draws, 3 * frames, "clear, fill and stroke in each frame: {d:?}");
    assert!(d.renders >= frames, "one render per frame that moved: {d:?}");
    // Off, the canvas keeps no totals (the stage timers still run).
    set_frame_stage_timing(false);
    let before = frame_stage_totals();
    for _ in 0..3 {
        rt.block_on(engine.pump_live(view, 16));
    }
    let d = frame_stage_totals().since(&before);
    assert_eq!((d.canvas_draws, d.canvas_raster_ms), (0, 0.0), "timing off: {d:?}");
    assert!(d.renders > 0, "the page still animated: {d:?}");
}

/// A relayout stage that runs inside a script turn (a forced layout) is
/// that stage's time, not also the script's. The add is synthetic and
/// larger than the turn, so the script's share goes below zero: a real
/// stage never claims more than the turn it ran in.
#[test]
fn a_relayout_stage_inside_a_script_turn_is_not_also_script_time() {
    let before = frame_stage_totals();
    {
        let _turn = wgate::script_turn();
        wgate::add(HoldStage::Build, 1_000_000.0);
    }
    let d = frame_stage_totals().since(&before);
    assert_eq!(d.build_ms, 1_000_000.0);
    assert!(d.script_ms < 0.0, "the build was taken out of the script turn: {d:?}");
    // Outside a script turn it is the stage's alone.
    let before = frame_stage_totals();
    wgate::add(HoldStage::Layout, 7.0);
    let d = frame_stage_totals().since(&before);
    assert_eq!((d.layout_ms, d.script_ms), (7.0, 0.0));
}

/// A frame tree's turn whose timers ran past a frame's worth of time still
/// runs the tree's due animation frame callbacks once (#776). That round is
/// script: the turn's script time holds at least what the page itself
/// measured its callbacks spending (the same spans, read by the page's own
/// clock inside them). Was: the round ran outside any script turn and its
/// time was the turn's `other_ms`.
#[test]
fn the_animation_round_a_stopped_frame_tree_turn_still_runs_is_script_time() {
    let parent = "<!doctype html><body><script>window.ready = false;</script>\
        <iframe id=f src='/child.html' onload='window.ready = true'></iframe></body>";
    let child = "<!doctype html><body><script>\
        window.spent = 0; window.rafs = 0; var k = 0;\
        function spin(ms) { var t = performance.now(); while (performance.now() - t < ms) {} spent += performance.now() - t; }\
        window.go = function () {\
          (function tick() { setTimeout(function () { spin(20); if (++k < 20) tick(); }, 0); })();\
          (function frame() { requestAnimationFrame(function () { spin(8); rafs++; if (rafs < 40) frame(); }); })();\
        };</script></body>";
    let server = serve_routes(vec![
        ("/", "text/html", parent.to_string()),
        ("/child.html", "text/html", child.to_string()),
    ]);
    let config = EngineConfig { nested_contexts: true, post_load_requests_live: true, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let read = |engine: &mut Engine, expr: &str| -> String {
        let v = engine.execute_script(view, &format!("String({expr})")).unwrap();
        v.trim_start_matches("String(\"").trim_end_matches("\")").trim_matches('"').to_string()
    };
    for _ in 0..50 {
        if read(&mut engine, "window.ready") == "true" {
            break;
        }
        rt.block_on(engine.pump_live_real(view, 20));
    }
    assert_eq!(read(&mut engine, "window.ready"), "true", "the frame did not load");
    engine.execute_script(view, "f.contentWindow.go()").unwrap();
    let before = frame_stage_totals();
    let mut stopped = 0;
    for _ in 0..30 {
        let turn = rt.block_on(engine.pump_live_real(view, 16));
        stopped += turn.yielded_for_frame as u32;
    }
    let d = frame_stage_totals().since(&before);
    let spent: f64 = read(&mut engine, "f.contentWindow.spent").parse().unwrap();
    let rafs: f64 = read(&mut engine, "f.contentWindow.rafs").parse().unwrap();
    assert!(stopped > 0 && rafs > 0.0, "no turn stopped for its frame ({stopped}), or no frame ran ({rafs})");
    assert!(
        d.script_ms >= spent,
        "the turns' script time {:.1} ms is less than the page's callbacks measured themselves spending, {spent:.1} ms \
         ({rafs} frames, {stopped} stopped turns): {d:?}",
        d.script_ms
    );
}
