//! LAT19: a geometry read in the same frame as a Web Animations write.
//!
//! A page that runs `Element.animate()` and reads `getBoundingClientRect()`
//! from a `requestAnimationFrame` loop (a carousel, a sticky header that
//! measures itself) used to lay the WHOLE page out for every frame's read
//! (`Forced layouts ... cause=animation:rect`, a full tree build), and then
//! paint nothing: the forced build re-captured the animated elements'
//! values (`Animations::captured`), so the paint-only frame that followed
//! found nothing that moved, built no display list and never painted. The
//! page froze on whatever frame it showed before the loop began (blank on
//! a page still loading).
//!
//! Now a forced layout leaves the animation baseline (what was painted) as
//! it found it, and a read after a paint-only layer write (transform,
//! opacity, colours) is answered from an animation frame over the
//! published boxes: no tree build.

use super::*;
use crate::script_net_tests::serve_routes;
use rustkit_layout::DisplayCommand;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

/// `body` after a block of `rows` x 10 small boxes (`rows * 10 + rows`
/// elements), so a full build costs what a real page's does.
fn page(style: &str, body: &str, rows: usize) -> String {
    let mut page = format!(
        "<!doctype html><style>body{{margin:0;font:12px sans-serif}}\
         .r{{display:flex}}.r div{{flex:1;padding:1px;border:1px solid #ccc}}{style}</style>{body}"
    );
    for row in 0..rows {
        page.push_str("<div class=r>");
        for cell in 0..10 {
            page.push_str(&format!("<div>{row}.{cell}</div>"));
        }
        page.push_str("</div>");
    }
    page
}

fn load(page: &str) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 1024, height: 768 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

fn run(engine: &mut Engine, view: EngineViewId, script: &str) {
    engine.execute_script(view, script).unwrap();
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine.execute_script(view, &format!("String({expr})")).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn frame(engine: &mut Engine, view: EngineViewId) {
    rt().block_on(engine.pump_live(view, 16));
}

fn transforms(engine: &Engine, view: EngineViewId) -> Vec<[f32; 6]> {
    engine.views[&view]
        .display_list
        .as_ref()
        .expect("a display list")
        .commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::PushTransform { matrix, .. } => Some(*matrix),
            _ => None,
        })
        .collect()
}

/// The painted width of `#a`'s green fill.
fn green_width(engine: &Engine, view: EngineViewId) -> Option<f32> {
    engine.views[&view]
        .display_list
        .as_ref()
        .expect("a display list")
        .commands
        .iter()
        .find_map(|c| match c {
            DisplayCommand::SolidColor(color, rect) if format!("{color:?}").contains("g: 128") => Some(rect.width),
            _ => None,
        })
}

fn stats(engine: &Engine, view: EngineViewId) -> RelayoutStats {
    engine.relayout_stats(view).expect("view").clone()
}

/// The reddit shape: a transform Web Animation and a rect read in every
/// frame, on a ~2000-element page. Every frame is painted (the transform
/// in the display list moves with the clock), no frame builds the tree
/// (no forced layout, no relayout), and the read sees the frame's
/// transform: it is the rect a clean read finds once the frame is painted.
#[test]
fn a_rect_read_in_a_transform_web_animation_frame_builds_no_tree_and_paints_the_frame() {
    let (mut engine, view, _server) = load(&page(
        "#a{width:50px;height:50px;background:green}",
        "<div id=a></div><script>var a=document.getElementById('a');</script>",
        180,
    ));
    run(
        &mut engine,
        view,
        "a.animate([{transform:'translateX(0px)'},{transform:'translateX(500px)'}],{duration:4000,iterations:Infinity});\
         window.__r=[];(function f(){requestAnimationFrame(function(){\
         window.__r.push(a.getBoundingClientRect().left);f();});})()",
    );
    // The first frames write the layer for the first time: a layout.
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let s0 = stats(&engine, view);
    let mut last = transforms(&engine, view);
    const N: usize = 6;
    let started = std::time::Instant::now();
    for i in 0..N {
        frame(&mut engine, view);
        let now = transforms(&engine, view);
        assert_eq!(now.len(), 1, "frame {i}: the transform is painted: {now:?}");
        assert_ne!(now, last, "frame {i}: the painted transform moved with the clock (the frame was presented)");
        last = now;
        // The rect the frame's read saw is the painted frame's.
        let seen = read(&mut engine, view, "window.__r[window.__r.length-1]");
        let clean = read(&mut engine, view, "a.getBoundingClientRect().left");
        assert_eq!(seen, clean, "frame {i}: the read saw this frame's transform");
        assert_eq!(last[0][4].round(), clean.parse::<f32>().unwrap().round(), "frame {i}: painted where read");
    }
    let per_frame = started.elapsed().as_secs_f64() * 1000.0 / N as f64;
    let s = stats(&engine, view);
    eprintln!(
        "LAT19 transform: {per_frame:.1} ms per frame; forced layouts {} -> {}, relayouts {} -> {}, painted frames {} -> {}",
        s0.forced_layouts, s.forced_layouts, s0.relayouts, s.relayouts,
        s0.animation_frames_painted, s.animation_frames_painted
    );
    assert_eq!(s.forced_layouts, s0.forced_layouts, "no read built the layout tree");
    assert_eq!(s.relayouts, s0.relayouts, "no frame relaid the page out");
    assert!(s.animation_frames_painted >= s0.animation_frames_painted + N as u32, "{s:?}");
    assert!(s.forced_frames >= s0.forced_frames + N as u64, "each read was answered from a frame: {s:?}");
}

/// A Web Animation of `width` (a layout property) with a rect read in
/// every frame: each frame is painted with the new width, and costs at most
/// one forced layout (the read) and one relayout (the frame).
#[test]
fn a_rect_read_in_a_width_web_animation_frame_paints_every_frame_at_a_bounded_cost() {
    let (mut engine, view, _server) = load(&page(
        "#a{width:50px;height:20px;background:green}",
        "<div id=a></div><script>var a=document.getElementById('a');</script>",
        180,
    ));
    run(
        &mut engine,
        view,
        "a.animate([{width:'50px'},{width:'550px'}],{duration:4000,iterations:Infinity});\
         window.__r=[];(function f(){requestAnimationFrame(function(){\
         window.__r.push(a.getBoundingClientRect().width);f();});})()",
    );
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let s0 = stats(&engine, view);
    let mut last = green_width(&engine, view);
    const N: usize = 4;
    for i in 0..N {
        frame(&mut engine, view);
        let now = green_width(&engine, view);
        assert!(now.is_some(), "frame {i}: #a is painted");
        assert_ne!(now, last, "frame {i}: the painted width moved with the clock (the frame was presented)");
        last = now;
        let seen = read(&mut engine, view, "window.__r[window.__r.length-1]");
        let clean = read(&mut engine, view, "a.getBoundingClientRect().width");
        assert_eq!(seen, clean, "frame {i}: the read saw this frame's width");
        assert_eq!(now.unwrap().round(), clean.parse::<f32>().unwrap().round(), "frame {i}: painted as read");
    }
    let s = stats(&engine, view);
    assert!(s.forced_layouts <= s0.forced_layouts + N as u64, "at most one forced layout a frame: {s:?}");
    assert!(s.relayouts <= s0.relayouts + N as u32, "at most one relayout a frame: {s:?}");
}
