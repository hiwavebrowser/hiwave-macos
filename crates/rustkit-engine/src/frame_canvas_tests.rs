//! A canvas in a frame's document draws images, and decides whether it may
//! be read, as a canvas in a top-level document does (IMG-F1).
//!
//! Chrome for Testing 148.0.7778.216 on the probe pages
//! (`docs/diagnostics/2026-11-16/frame_canvas{.html,_inner.html,_shape.*}`,
//! record `frame_canvas_cft148.json`).

use super::*;
use crate::script_net_tests::serve_byte_routes;
use std::time::Duration;

const PARENT: &str = include_str!("../../../docs/diagnostics/2026-11-16/frame_canvas.html");
const INNER: &str = include_str!("../../../docs/diagnostics/2026-11-16/frame_canvas_inner.html");
const SVG: &[u8] = include_bytes!("../../../docs/diagnostics/2026-11-16/frame_canvas_shape.svg");
const PNG: &[u8] = include_bytes!("../../../docs/diagnostics/2026-11-16/frame_canvas_shape.png");
const PIN: &str = include_str!("../../../docs/diagnostics/2026-11-16/frame_canvas_cft148.json");

/// The probe's record for `page` (with `query`), or `None` when the page
/// never reported.
fn record(nested_contexts: bool, page: &'static str, query: &str) -> Option<serde_json::Value> {
    let server = serve_byte_routes(vec![
        ("/frame_canvas.html", "text/html", PARENT.as_bytes().to_vec()),
        ("/frame_canvas_inner.html", "text/html", INNER.as_bytes().to_vec()),
        ("/frame_canvas_shape.svg", "image/svg+xml", SVG.to_vec()),
        ("/frame_canvas_shape.png", "image/png", PNG.to_vec()),
    ]);
    let config = EngineConfig { nested_contexts, post_load_requests_live: true, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let query = query.replace("{port}", &server.port.to_string());
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{}/{page}{query}", server.port)).unwrap())).expect("load_url");
    for _ in 0..150 {
        std::thread::sleep(Duration::from_millis(10));
        rt.block_on(engine.pump_live(view, 20));
        let out = engine.execute_script(view, "window.__out || ''").unwrap();
        if out.len() > "String(\"\")".len() {
            let text: String = serde_json::from_str(out.trim_start_matches("String(").trim_end_matches(')')).unwrap();
            return Some(serde_json::from_str(&text).unwrap());
        }
    }
    None
}

fn pin() -> serde_json::Value {
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    assert_eq!(pin["browser"], "148.0.7778.216");
    pin
}

/// A same-origin frame's script draws a loaded PNG and a loaded SVG image
/// into a canvas and reads it back: the pixels are there, as at top level.
#[test]
fn a_frames_canvas_draws_a_loaded_image_as_in_chrome() {
    let pin = pin();
    let top = record(true, "frame_canvas_inner.html", "").expect("the document on its own reports");
    let framed = record(true, "frame_canvas.html", "").expect("the frame reports");
    for kind in ["svg", "png"] {
        assert_eq!(top["top"][kind], pin["top"][kind], "top {kind}");
        assert_eq!(framed["framed"][kind], pin["framed"][kind], "framed {kind}");
    }
}

/// With nested contexts off a frame has no document: nothing reports.
#[test]
fn with_nested_contexts_off_a_frames_canvas_probe_does_not_run() {
    assert_eq!(record(false, "frame_canvas.html", ""), None);
    let pin = pin();
    let top = record(false, "frame_canvas_inner.html", "").expect("the document on its own reports");
    assert_eq!(top["top"]["png"], pin["top"]["png"]);
}
