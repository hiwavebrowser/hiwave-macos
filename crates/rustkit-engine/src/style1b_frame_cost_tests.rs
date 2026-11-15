//! STYLE1b (a): what the engine spends after a Multiply-shaped frame of inline style writes.
//! Measurement only (`--ignored`), in an optimised build:
//!
//! `cargo test --profile parity -p rustkit-engine --lib --features headless style1b_frame -- --ignored --nocapture`
//!
//! 5000 absolutely positioned tiles with three inline properties each; one script turn writes all
//! three on every tile (15000 writes, as Multiply's animate() does per frame), then one live turn
//! restyles and lays out. Printed: the script turn, the live turn, and whether the engine's
//! mutation log overflowed into a whole-page restyle. Three frames, so the first-frame costs show.

use crate::script_net_tests::serve_routes;
use crate::{Bounds, Engine, EngineConfig};
use std::time::Instant;

#[test]
#[ignore]
fn measure_a_multiply_frame_of_inline_style_writes() {
    let mut page = String::from("<html><body style='margin:0'>");
    for i in 0..5000 {
        page.push_str(&format!(
            "<div style='position: absolute; left: {}px; top: {}px; width: 8px; height: 8px; background: red; opacity: 0.5'></div>",
            (i % 100) * 10,
            (i / 100) * 10
        ));
    }
    page.push_str("<script>var tiles = Array.prototype.slice.call(document.querySelectorAll('div'));</script></body></html>");
    let server = serve_routes(vec![("/", "text/html", page)]);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 1000, height: 600 }).expect("view");
    let url = url::Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    rt.block_on(engine.pump_live(view, 0));
    for frame in 0..3 {
        let script = format!(
            "var t0 = performance.now(); for (var i = 0; i < tiles.length; i++) {{ var s = tiles[i].style; \
               s.left = ((i % 100) * 10 + {frame} + 1) + 'px'; s.top = ((i / 100 | 0) * 10 + {frame}) + 'px'; \
               s.opacity = String(0.4 + {frame} / 10); }} String(Math.round(performance.now() - t0))"
        );
        let t0 = Instant::now();
        let out = engine.execute_script(view, &script).unwrap();
        let script_ms = t0.elapsed().as_secs_f64() * 1000.0;
        // The page's own measure of its loop; the rest of the call is the flush of its DOM writes
        // (restyle and layout, `flush_script_dom_writes`).
        let js_ms: f64 = out.trim_start_matches("String(\"").trim_end_matches("\")").parse().unwrap_or(-1.0);
        let t1 = Instant::now();
        let turn = rt.block_on(engine.pump_live(view, 16));
        let turn_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let t2 = Instant::now();
        let _ = engine.render_view(view);
        let render_ms = t2.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "STYLE1B-FRAME frame {frame}: execute_script {script_ms:.1} ms = writes {js_ms:.0} ms + flush {:.1} ms; live turn {turn_ms:.1} ms (relaid out {}), render {render_ms:.1} ms",
            script_ms - js_ms,
            turn.relaid_out
        );
    }
}
