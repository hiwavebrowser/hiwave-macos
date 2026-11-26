//! LOAD-VIRTUAL-TIME: the load's timer horizon runs the page's timers on the virtual clock, and
//! between them, once a frame, lays the page out and tells its observers (`refresh_layout_for_script`).
//! The frames were cut by real time: a slice ended once 16 ms of it had passed. So whether a
//! ResizeObserver callback ran between two timers of the same virtual frame depended on how long the
//! first one took on this machine, and a capture's page could differ with the engine's speed alone
//! (walmart, replayed: with the layout memo on, the faster load put more of the horizon's timers in a
//! slice and the page opened its fulfilment dialog; with the memo off, or the memo on with every core
//! busy, it did not). With `virtual_load_frames` the frames are the timers' clock's 16 ms frames.
//!
//! No Chrome number: in Chrome this is a matter of when the rendering update comes, which is real
//! time. What is pinned is that a capture does not depend on its own speed.

use super::*;
use crate::script_net_tests::serve_routes;

/// A box watched by a ResizeObserver; a 0 ms timer that spends `busy_ms` of real time and then
/// widens the box; a 5 ms timer (the same virtual frame) and a 40 ms timer (a later frame) that write
/// the width the observer last reported.
fn page(busy_ms: u32) -> String {
    format!(
        "<!doctype html><style>body{{margin:0}} #box{{width:100px;height:10px}}</style>\
         <div id=box></div><div id=out></div><script>\
         var box = document.getElementById('box'), out = document.getElementById('out'), w = 'none';\
         new ResizeObserver(function (es) {{ w = String(Math.round(es[es.length - 1].contentRect.width)); }}).observe(box);\
         setTimeout(function () {{ var end = Date.now() + {busy_ms}; while (Date.now() < end) {{}} box.style.width = '200px'; }}, 0);\
         setTimeout(function () {{ out.textContent += 'b' + w; }}, 5);\
         setTimeout(function () {{ out.textContent += ' c' + w; }}, 40);\
         </script>"
    )
}

fn load(busy_ms: u32) -> String {
    let config = EngineConfig { virtual_load_frames: true, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    // A URL load: its timer horizon is the one a capture runs (`load_html` runs no timers).
    let server = serve_routes(vec![("/", "text/html", page(busy_ms))]);
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let got = engine.execute_script(view, "document.getElementById('out').textContent").expect("script");
    got.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&got).to_string()
}

/// A timer of the same virtual frame sees the same observations whether the timer before it took
/// no real time or longer than a frame of it; a timer of a later frame sees the widened box.
#[test]
fn the_loads_timers_see_observers_at_virtual_frames_whatever_a_callback_costs() {
    let fast = load(0);
    let slow = load(40);
    eprintln!("fast: {fast:?}, slow: {slow:?}");
    assert!(fast.ends_with(" c200"), "a later frame's timer sees the widened box: {fast:?}");
    assert_eq!(fast, slow, "the same virtual frames, with a 0 ms and a 40 ms callback before them");
}
