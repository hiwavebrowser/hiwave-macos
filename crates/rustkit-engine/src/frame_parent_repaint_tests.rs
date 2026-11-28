//! NO-MUTATION-RELAYOUT: a parent whose frame relaid out, and which itself did not change, is painted
//! again with the frame's new paint, not built and laid out again.
//!
//! `pump_frames` relaid the parent out (`RelayoutCause::Timer`) whenever any of its frames had, because
//! the parent's paint includes the frame's. MotionMark's runner page (one test frame changing two styles
//! a frame) paid a full style and box build every frame for no mutation of its own: 673 of 678 of its
//! relayouts had `mutations={}` (legion5, Multiply held at complexity 1). Speedometer's runner holds one
//! test frame too.

use super::*;
use crate::script_net_tests::serve_routes;
use std::time::{Duration, Instant};

const PARENT: &str = "<!doctype html><body style='margin:0'><iframe id=f src=/tick.html \
    style='width:200px;height:100px;border:0'></iframe><script>window.loaded = false; \
    document.getElementById('f').addEventListener('load', function () { loaded = true; });</script></body>";

/// The frame's box flips colour on a timer: n counts the flips, odd is red, even is blue.
const TICK: &str = "<!doctype html><body style='margin:0'><div id=box style='height:50px;background:rgb(0,0,255)'></div>\
    <script>window.n = 0; setInterval(function () { n++; document.getElementById('box').style.background = \
    n % 2 ? 'rgb(255,0,0)' : 'rgb(0,0,255)'; }, 16);</script></body>";

#[test]
fn a_parent_is_repainted_not_relaid_out_when_only_its_frame_changed() {
    let server = serve_routes(vec![("/", "text/html", PARENT.to_string()), ("/tick.html", "text/html", TICK.to_string())]);
    let config = EngineConfig { nested_contexts: true, post_load_requests_live: true, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap())).unwrap();
    let mut turn = |engine: &mut Engine| {
        std::thread::sleep(Duration::from_millis(20));
        rt.block_on(engine.pump_live(view, 20));
    };
    let ready = "window.loaded && document.getElementById('f').contentWindow.n >= 2";
    let started = Instant::now();
    while engine.execute_script(view, ready).unwrap() != "Boolean(true)" && started.elapsed() < Duration::from_secs(8) {
        turn(&mut engine);
    }
    assert_eq!(engine.execute_script(view, ready).unwrap(), "Boolean(true)", "the frame loaded and ticked");
    let frame = *engine.views[&view].frames.values().map(|f| &f.view).next().expect("the frame's view");

    let (parent_before, frame_before) =
        (engine.views[&view].relayout_stats.relayouts, engine.views[&frame].relayout_stats.relayouts);
    for _ in 0..10 {
        turn(&mut engine);
    }
    let (parent_after, frame_after) =
        (engine.views[&view].relayout_stats.relayouts, engine.views[&frame].relayout_stats.relayouts);
    assert!(frame_after > frame_before, "the frame relaid out ({frame_before} -> {frame_after})");
    assert_eq!(
        parent_after, parent_before,
        "the parent changed nothing, yet was relaid out {} times while its frame relaid out {} times",
        parent_after - parent_before,
        frame_after - frame_before
    );

    // The parent's paint shows the frame's current colour.
    let n = engine.execute_script(view, "String(document.getElementById('f').contentWindow.n)").unwrap();
    let odd = n.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().expect("n") % 2 == 1;
    let want = if odd { (255, 0, 0) } else { (0, 0, 255) };
    let list = engine.views[&view].doc.display_list.clone().expect("a display list");
    let shown = list.commands.iter().any(|c| {
        matches!(c, rustkit_layout::DisplayCommand::SolidColor(color, _) if (color.r, color.g, color.b) == want)
    });
    assert!(shown, "the parent's paint does not show the frame's current colour {want:?} (n = {n})");
}
