//! LPF1: a relayout asked for by script style writes that change only what is painted builds only
//! the written elements and patches their paint onto the laid-out boxes, as the pointer restyle
//! does, instead of building the whole document (LAYOUT-PER-FRAME: on MotionMark Multiply's shape
//! the build was ~78% of a 112-122 ms frame and scaled with the document,
//! `docs/diagnostics/2026-11-16/layout_per_frame.md`). Chrome keeps 60 frames a second on that page
//! (`layout_per_frame_probe_cft148.json`).
//!
//! The short path must paint exactly what the full rebuild paints: `the_short_path_paints_what_the_full_rebuild_paints`
//! runs the same writes with the path on and off and compares the display lists after every frame.

use super::*;
use crate::script_net_tests::serve_routes;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

/// `tiles` absolutely positioned tiles in a positioned, clipped stage, and an animation-frame loop
/// that runs `write` (with `el` a tile and `n` the frame) on 3 tiles a frame.
fn page(tiles: usize, write: &str) -> String {
    let mut p = String::from(
        "<!doctype html><style>body{margin:0}#stage{position:relative;width:800px;height:600px;overflow:hidden}\
         .t{position:absolute;width:12px;height:12px;background:#08f}</style><div id=stage>",
    );
    for i in 0..tiles {
        let (x, y) = ((i % 60) * 13, (i / 60) * 13);
        p.push_str(&format!("<div class=t style=\"left:{x}px;top:{y}px\"></div>"));
    }
    p.push_str(&format!(
        "</div><script>var tiles = document.querySelectorAll('.t'), n = 0; \
         requestAnimationFrame(function f() {{ n++; for (var k = 0; k < 3; k++) {{ var el = tiles[(n * 7 + k * 331) % tiles.length]; {write} }} requestAnimationFrame(f); }});</script>"
    ));
    p
}

fn load(page: &str) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

fn frame(engine: &mut Engine, view: EngineViewId) {
    rt().block_on(engine.pump_live(view, 17));
}

const BACKGROUND: &str = "el.style.backgroundColor = 'hsl(' + (n * 37 % 360) + ',60%,50%)';";

/// After the first write (which builds the page in full and keeps the tree it built), every frame
/// whose writes change only a written tile's paint takes the short path: no full build, and the
/// build goes into a handful of elements (the written tiles and the path down to them), not the
/// page's 2,000.
#[test]
fn a_script_paint_write_builds_only_the_written_elements() {
    let (mut engine, view, _server) = load(&page(2_000, BACKGROUND));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let before = engine.relayout_stats(view).expect("view").clone();
    let mut walked = Vec::new();
    for _ in 0..5 {
        frame(&mut engine, view);
        walked.push(engine.relayout_stats(view).expect("view").last_restyle_walked);
    }
    let after = engine.relayout_stats(view).expect("view");
    assert_eq!(
        after.script_restyles - before.script_restyles,
        5,
        "frames that built only the written tiles (relayouts {}, layouts skipped {}, last miss {:?})",
        after.relayouts - before.relayouts,
        after.layouts_skipped - before.layouts_skipped,
        last_restyle_miss_for_test()
    );
    assert!(walked.iter().all(|&w| w > 0 && w <= 40), "elements the build went into per frame: {walked:?}");
}

/// The short path paints exactly what the full rebuild paints: the same page and writes, the path
/// on in one engine and off in the other, and the two display lists equal after every frame.
#[test]
fn the_short_path_paints_what_the_full_rebuild_paints() {
    let lists = |on: bool| {
        set_script_restyle_for_test(on);
        let (mut engine, view, _server) = load(&page(300, BACKGROUND));
        let mut out = Vec::new();
        for _ in 0..8 {
            frame(&mut engine, view);
            out.push(format!("{:?}", engine.views[&view].display_list.as_ref().expect("display list").commands));
        }
        set_script_restyle_for_test(true);
        (out, engine.relayout_stats(view).expect("view").script_restyles)
    };
    let ((short, took), (full, none)) = (lists(true), lists(false));
    assert!(took >= 6, "the short path ran {took} times of 8 frames (the first builds in full)");
    assert_eq!(none, 0, "the path was off in the other arm");
    for (i, (a, b)) in short.iter().zip(&full).enumerate() {
        assert!(a == b, "frame {i}: the short path's display list differs from the full rebuild's");
    }
}
