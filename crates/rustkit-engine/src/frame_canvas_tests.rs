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

/// The probe's record for `page` (with `fragment`, `{port}` in it the
/// server's), or `None` when the page
/// never reported.
fn record(nested_contexts: bool, page: &'static str, fragment: &str) -> Option<serde_json::Value> {
    let server = serve_byte_routes(vec![
        ("/frame_canvas.html", "text/html", PARENT.as_bytes().to_vec()),
        ("/frame_canvas_inner.html", "text/html", INNER.as_bytes().to_vec()),
        ("/frame_canvas_shape.svg", "image/svg+xml", SVG.to_vec()),
        ("/frame_canvas_shape.png", "image/png", PNG.to_vec()),
    ]);
    let config = EngineConfig { nested_contexts, post_load_requests_live: true, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    // Another origin's image, as a response already in the image store: a
    // test's loopback page cannot ask another origin for one.
    let far = Url::parse(&format!("{OTHER}/frame_canvas_shape.png")).unwrap();
    engine.lctx.image_manager.insert_fetched(&far, Some("image/png"), PNG).expect("the PNG decodes");
    engine.lctx.image_manager.record_response_urls(&far, [far.clone()]);
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let fragment = fragment.replace("{port}", &server.port.to_string());
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{}/{page}{fragment}", server.port)).unwrap())).expect("load_url");
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

/// The other origin the probe is told of (`#other=`).
const OTHER: &str = "http://other.test";

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

/// A canvas that drew another origin's image cannot be read
/// (`getImageData`: SecurityError), nor can a canvas that
/// drew that canvas; a canvas beside them can. In a frame the decision is
/// made with the frame's own origin, and comes out as at top level.
#[test]
fn a_frames_canvas_that_drew_another_origins_image_cannot_be_read_as_in_chrome() {
    let pin = pin();
    assert_eq!(pin["framed"]["other_origin"]["getImageData"], "SecurityError");
    assert_eq!(pin["framed"]["other_origin"]["toDataURL"], "SecurityError");
    let fragment = format!("#other={}", OTHER.replace(':', "%3A").replace('/', "%2F"));
    let mut top = record(true, "frame_canvas_inner.html", &fragment).expect("the document on its own reports");
    let mut framed = record(true, "frame_canvas.html", &fragment).expect("the frame reports");
    // Known difference, in a frame as at top level: `canvas.toDataURL` does
    // not exist yet (web_canvas.js lists it), so the call is a TypeError
    // where Chrome's is a SecurityError. Nothing is read either way. When
    // it lands this turns red and the two lines go.
    for (got, level) in [(&mut top, "top"), (&mut framed, "framed")] {
        assert_eq!(got[level]["other_origin"]["toDataURL"], "TypeError", "{level}");
        got[level]["other_origin"]["toDataURL"] = "SecurityError".into();
    }
    assert_eq!(top["top"], pin["top"]);
    assert_eq!(framed["framed"], pin["framed"]);
}
