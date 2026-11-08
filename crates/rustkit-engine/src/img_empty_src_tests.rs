//! CH2.E42 (hand test 11-08): an `<img>` whose source is empty fetched the
//! document's own URL and decoded the HTML as an image (`Failed to load
//! image … DecodeError("Unknown image format") url=https://www.pornhub.com/`).
//! `discover_images` joined the raw `src` onto the base URL, and `""` joined
//! onto a URL is that URL.
//!
//! Chromium 148 (`tools/parity_oracle/img_empty_src_log.mjs`, one page per
//! case, every request logged): `src=""`, `src="   "`, `srcset="" src=""`,
//! `srcset=" "` with no `src` and a script's `img.src = ""` make no request.
//! `src="#"`, `src="#top"` and `src="?x=1"` DO request the document URL
//! (fragment dropped) and fail as an image, so they are pinned as requests.
//! The `error` event Chromium fires for both is not here: the engine fires
//! no load/error events on `<img>` at all (row CH2.E42b).

use super::*;
use crate::script_net_tests::{serve_byte_routes, Server};

// 4x4 RGBA, every texel opaque red.
const RED_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00,
    0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x06, 0x00, 0x00, 0x00, 0xa9, 0xf1, 0x9e, 0x7e, 0x00, 0x00, 0x00,
    0x15, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0xf0, 0x9f, 0x01, 0x09, 0x30, 0x31, 0xa0,
    0x01, 0xc2, 0x02, 0x00, 0x83, 0xd1, 0x02, 0x06, 0xb3, 0x4b, 0xd2, 0x9b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// Every form of an empty image source, and one real image as the control.
const EMPTY: &str = "<html><body>\
    <img src=\"\">\
    <img src=\"   \">\
    <img src=\" \t\n\">\
    <img srcset=\"\" src=\"\">\
    <img srcset=\" \">\
    <img src=\"/ok.png\">\
    <script>var i = new Image(); i.src = ''; document.body.appendChild(i);</script>\
    </body></html>";

const HASH: &str = "<html><body><img src=\"#\"></body></html>";

fn load(path: &str) -> Server {
    let server = serve_byte_routes(vec![
        ("/empty", "text/html", EMPTY.as_bytes().to_vec()),
        ("/hash", "text/html", HASH.as_bytes().to_vec()),
        ("/ok.png", "image/png", RED_PNG.to_vec()),
    ]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 200 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}{path}", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    // `load_url` runs the image pass itself; a second pass would ask again
    // for every URL that failed, and the counts would be the rig's.
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    server
}

fn paths(server: &Server) -> Vec<String> {
    server.requests.lock().unwrap().iter().map(|r| r.path.clone()).collect()
}

/// develop: `["/empty", "/empty", "/ok.png"]` — the empty sources asked for
/// the page again (one request: the image pass asks once per URL).
#[test]
fn an_empty_or_blank_img_source_requests_nothing() {
    let server = load("/empty");
    assert_eq!(paths(&server), vec!["/empty", "/ok.png"], "the page, then only the real image");
}

/// Chromium requests the document again for `src="#"` (and fails it as an
/// image). Kept: only an EMPTY source is skipped.
#[test]
fn a_fragment_only_img_source_still_requests_the_document() {
    let server = load("/hash");
    assert_eq!(paths(&server), vec!["/hash", "/hash"]);
}
