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

/// LPF2's starting point: what the engine reports today (a full rebuild) for the transform and
/// opacity probe Chrome is pinned on (`transform_paint_probe_cft148.json`). Printed, not asserted.
#[test]
#[ignore]
fn transform_paint_probe_on_the_engine() {
    let html = include_str!("../../../docs/diagnostics/2026-11-16/transform_paint_probe.html");
    let (mut engine, view, _server) = load(html);
    let read = |engine: &mut Engine, expr: &str| engine.execute_script(view, &format!("JSON.stringify({expr})")).unwrap();
    for part in ["overflow", "fixed", "rect", "clip", "opacity"] {
        eprintln!("{part}: {}", read(&mut engine, &format!("window.rec && window.rec.{part}")));
    }
}

/// `tiles` tiles that already have a transform (as Multiply's do once animated), in the same stage.
fn transformed_page(tiles: usize, write: &str) -> String {
    page(tiles, write).replace(".t{position:absolute;", ".t{transform:rotate(0deg);opacity:0.9;position:absolute;")
}

const TRANSFORM: &str = "el.style.transform = 'rotate(' + (n * 13 % 360) + 'deg)'; \
                         el.style.backgroundColor = 'hsl(' + (n * 37 % 360) + ',60%,50%)'; \
                         el.style.opacity = String(0.5 + (n % 4) / 10);";

/// LPF2: a transform that stays a transform and an opacity that stays below 1 change only what is
/// painted: the box keeps its stacking context and its containing block for fixed and absolute
/// descendants (Chrome, `transform_paint_probe_cft148.json`: between two transforms a fixed child
/// moves with its parent; opacity moves no box). MotionMark Multiply's writes, on tiles that already
/// have a transform, take the short path every frame. They took the full build.
#[test]
fn a_transform_and_opacity_write_that_keeps_what_the_box_establishes_is_paint() {
    let (mut engine, view, _server) = load(&transformed_page(2_000, TRANSFORM));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let before = engine.relayout_stats(view).expect("view").script_restyles;
    for _ in 0..5 {
        frame(&mut engine, view);
    }
    let after = engine.relayout_stats(view).expect("view").script_restyles;
    assert_eq!(after - before, 5, "frames on the short path (last miss {:?})", last_restyle_miss_for_test());
}

/// The cross-check for LPF2: transform and opacity writes give the full rebuild's display list and
/// the full rebuild's element rects (`getBoundingClientRect` follows the rotation, as Chrome's
/// does) after every frame.
#[test]
fn transform_writes_on_the_short_path_paint_and_measure_what_the_full_rebuild_does() {
    let run = |on: bool| {
        set_script_restyle_for_test(on);
        let (mut engine, view, _server) = load(&transformed_page(300, TRANSFORM));
        let mut out = Vec::new();
        for _ in 0..8 {
            frame(&mut engine, view);
            let rects = engine
                .execute_script(view, "JSON.stringify([0, 7, 331].map(function (i) { var b = tiles[i].getBoundingClientRect(); return [b.left, b.top, b.width]; }))")
                .unwrap();
            out.push((format!("{:?}", engine.views[&view].display_list.as_ref().expect("display list").commands), rects));
        }
        set_script_restyle_for_test(true);
        (out, engine.relayout_stats(view).expect("view").script_restyles)
    };
    let ((short, took), (full, _)) = (run(true), run(false));
    assert!(took >= 6, "the short path ran {took} times of 8 frames");
    for (i, (a, b)) in short.iter().zip(&full).enumerate() {
        assert!(a.0 == b.0, "frame {i}: the display list differs from the full rebuild's");
        assert_eq!(a.1, b.1, "frame {i}: the rects differ from the full rebuild's");
    }
}

/// A transform that comes or goes changes what the box establishes: its fixed child is placed
/// against it (Chrome: `none` -> `translateX(0px)` moves a fixed child from (0, 0) to its parent's
/// corner). That write is laid out; the short path is not taken.
#[test]
fn a_transform_that_comes_is_laid_out() {
    let html = "<!doctype html><style>body{margin:0}#out{position:absolute;left:100px;top:300px;width:60px;height:60px}\
                #fx{position:fixed;left:0;top:0;width:10px;height:10px}</style><div id=out><div id=fx></div></div>\
                <script>var out = document.getElementById('out'), fx = document.getElementById('fx'), n = 0; \
                requestAnimationFrame(function f() { n++; out.style.transform = n < 3 ? 'none' : 'translateX(0px)'; requestAnimationFrame(f); });</script>";
    let (mut engine, view, _server) = load(html);
    for _ in 0..5 {
        frame(&mut engine, view);
    }
    let rect = engine
        .execute_script(view, "JSON.stringify((function (b) { return [b.left, b.top]; })(fx.getBoundingClientRect()))")
        .unwrap();
    assert!(rect.contains("[100,300]"), "the fixed child against its transformed parent: {rect}");
}

/// KNOWN DIVERGENCE, row TRANSFORM-OVERFLOW: Chrome counts a transformed box in its scroller's
/// scrollable overflow (`transform_paint_probe_cft148.json`, "overflow": `translateX(400px)` on a
/// 50 px box in a 200 px scroller makes `scrollWidth` 450, also under `overflow: hidden`); the
/// engine's layout does not (200), with or without the short path. When that is fixed, a transform
/// write in a scroll container must update the scroller's extent or leave the short path.
#[test]
fn known_divergence_a_transform_is_not_in_scrollable_overflow() {
    let html = "<!doctype html><style>body{margin:0}#sc{width:200px;height:100px;overflow:auto}#sc i{display:block;width:50px;height:50px}</style>\
                <div id=sc><i id=sci></i></div>";
    let (mut engine, view, _server) = load(html);
    let got = engine
        .execute_script(view, "String((sci.style.transform = 'translateX(400px)', sc.scrollWidth))")
        .unwrap();
    assert!(got.contains("200"), "Chrome: 450; here {got}");
}
