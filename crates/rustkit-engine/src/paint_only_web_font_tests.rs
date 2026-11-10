//! E52: a frame painted without a layout keeps the page's web fonts when
//! another view painted since.
//!
//! The registered web faces are one process-wide slot the engine points at
//! a view's font partition before it measures or paints that view
//! (`LayoutCtx::install_web_fonts_for`). The display list shapes each line
//! of text when it is built, and the frames that build a list over the
//! laid-out boxes (a paint-only animation frame, LAT2; an element scroll)
//! did not point the slot at their view first. With a second view painting
//! between two of them (the app's chrome runs its own animations on the
//! same thread), the page's lines were shaped in the fallback font: on
//! netflix every line of text changed face from frame to frame (Pete's
//! hand test of 2026-11-09, "all the text has an odd wiggle"). A relayout
//! installs the view's faces first, so the page was right until LAT14 put
//! it on the paint-only path.

use super::*;
use crate::script_net_tests::{serve_byte_routes, Server};
use rustkit_layout::DisplayCommand;

const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");

/// Two lines in a web font (Ahem: every glyph a solid em square, 20px a
/// character here) and a spinner that never stops. The second paragraph
/// scrolls.
const PAGE: &str = r#"<!doctype html><html><head><style>
@font-face { font-family: E52Face; src: url(/Ahem.ttf) format("truetype"); }
body { margin: 20px; font: 20px/20px E52Face, Helvetica; background: white; color: black; }
p { margin: 0 0 10px 0; }
#box { width: 300px; height: 40px; overflow: auto; }
#spin { position: absolute; left: 340px; top: 150px; width: 20px; height: 20px; background: #08f;
  animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style></head><body><p>XXXXX xx</p><div id=box><p>Static words</p><p>more</p><p>lines</p><p>below</p></div>
<div id=spin></div></body></html>"#;

/// A page of another font partition with no web font and an animation of
/// its own, as the app's chrome is.
const OTHER: &str = r#"<!doctype html><html><head><style>
body { margin: 0; font: 14px Helvetica; }
#bar { width: 50px; height: 4px; background: red; animation: fade .7s linear infinite alternate; }
@keyframes fade { from { opacity: .2 } to { opacity: 1 } }
</style></head><body><div id=bar></div><p>tab title</p></body></html>"#;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// The page in one view and the other page in a second, both loaded and
/// one frame in.
fn two_views() -> (Engine, EngineViewId, EngineViewId, Server) {
    let server = serve_byte_routes(vec![
        ("/", "text/html", PAGE.as_bytes().to_vec()),
        ("/other", "text/html", OTHER.as_bytes().to_vec()),
        ("/Ahem.ttf", "font/ttf", AHEM.to_vec()),
    ]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let mut view = |height| {
        engine
            .create_headless_view(Bounds {
                x: 0,
                y: 0,
                width: 400,
                height,
            })
            .expect("view")
    };
    let (page, other) = (view(200), view(60));
    let url = |host: &str, path: &str| Url::parse(&format!("http://{host}:{}{path}", server.port)).unwrap();
    // Two hosts: two font partitions.
    rt().block_on(engine.load_url(page, url("127.0.0.1", "/"))).expect("load page");
    rt().block_on(engine.load_url(other, url("localhost", "/other"))).expect("load other");
    rt().block_on(engine.pump_live(page, 16));
    (engine, page, other, server)
}

/// Each line of text in the view's display list: its characters, the face
/// it was shaped in and its width.
fn lines(engine: &Engine, view: EngineViewId) -> Vec<(String, String, f32)> {
    engine.views[&view]
        .display_list
        .as_ref()
        .expect("a display list")
        .commands
        .iter()
        .filter_map(|c| match c {
            DisplayCommand::Text { text, run, advances, .. } => Some((
                text.clone(),
                run.as_ref().map_or(String::new(), |r| r.face.postscript_name.clone()),
                advances.as_ref().map_or(0.0, |a| a.iter().sum()),
            )),
            _ => None,
        })
        .collect()
}

/// The pixels of the view's frame over the first line of text.
fn first_line_pixels(engine: &Engine, view: EngineViewId) -> Vec<u8> {
    let viewhost_id = engine.views[&view].viewhost_id;
    let (width, _, rgba) = engine
        .presenter
        .as_ref()
        .expect("presenter")
        .read_headless_rgba(viewhost_id)
        .expect("frame");
    let width = width as usize;
    (20..40)
        .flat_map(|y| rgba[(y * width + 20) * 4..(y * width + 200) * 4].to_vec())
        .collect()
}

fn dark(pixels: &[u8]) -> usize {
    pixels.chunks(4).filter(|p| p[0] < 96).count()
}

/// Static text beside a running animation is the same pixels every
/// frame, in the page's web font, whether or not the other view painted
/// a frame in between; and the frames are still painted without a layout.
#[test]
fn static_text_keeps_its_web_font_across_paint_only_frames_when_another_view_paints() {
    let (mut engine, page, other, _server) = two_views();
    let first = first_line_pixels(&engine, page);
    // Five 20px squares, a space and two more: 140 x 20 px of ink.
    assert!(dark(&first) > 2700, "the page paints its web font: {} dark px", dark(&first));
    let laid_out = engine.relayout_stats(page).expect("view").relayouts;
    let painted = engine.relayout_stats(page).expect("view").animation_frames_painted;
    const FRAMES: u32 = 12;
    for frame in 0..FRAMES {
        // The other view's frame comes between two of every three.
        if frame % 3 != 0 {
            rt().block_on(engine.pump_live(other, 16));
        }
        rt().block_on(engine.pump_live(page, 16));
        for (text, face, width) in lines(&engine, page) {
            assert_eq!(face, "Ahem", "frame {frame}: {text:?} is shaped in the page's face ({width} px wide)");
        }
        let now = first_line_pixels(&engine, page);
        let moved = first.chunks(4).zip(now.chunks(4)).filter(|(a, b)| a != b).count();
        assert_eq!(moved, 0, "frame {frame}: {moved} px of the static line changed");
    }
    let stats = engine.relayout_stats(page).expect("view");
    assert_eq!(stats.relayouts, laid_out, "no frame laid the page out");
    assert_eq!(stats.animation_frames_painted - painted, FRAMES, "every frame was painted without a layout");
    assert!(
        engine.relayout_stats(other).expect("view").animation_frames_painted >= 8,
        "the other view painted its own frames"
    );
}

/// A wheel over a scroller builds the list again over the same boxes: its
/// text stays in the page's web font after the other view painted.
#[test]
fn an_element_scroll_keeps_the_web_font_when_another_view_painted() {
    let (mut engine, page, other, _server) = two_views();
    rt().block_on(engine.pump_live(other, 16));
    let laid_out = engine.relayout_stats(page).expect("view").relayouts;
    assert!(engine.scroll_at(page, 60.0, 70.0, 0.0, -30.0).expect("wheel"), "the box scrolled");
    assert_eq!(engine.relayout_stats(page).expect("view").relayouts, laid_out, "a scroll lays nothing out");
    for (text, face, width) in lines(&engine, page) {
        assert_eq!(face, "Ahem", "{text:?} is shaped in the page's face ({width} px wide)");
    }
}
