//! STRESS-SPEED (WINDOW-PATH-WALK): an event that reaches the Window must not ask the Window for
//! `parentNode` or `nodeType`. RustKit's dispatch did, on every event: shadow retargeting
//! (`web_shadow.js` `rootNode` / `hostOf`) and the body/frameset handler check (`dom.rs`
//! `isBodyOrFrameset`). The Window has neither property, so each read fell through to named access
//! on the Window object (`web_named_access.js`), a walk of the whole document
//! (`__rustkit_dom_collect`). On Speedometer's TodoMVC-Lit-Complex-DOM that was 10,017 walks and
//! about 1.2 s of its 4.3 s (docs/diagnostics/2026-11-24/STRESS-SPEED-lit-complex.md).
//!
//! What must not change is pinned too: every listener runs as often as in Chrome, and with an
//! element whose id is `parentNode` (or a form named `nodeType`) the Window's named access still
//! answers it. Chrome for Testing 148.0.7778.216 (win64),
//! docs/diagnostics/2026-11-24/window_event_path_cft148.json
//! (tools/parity_oracle/window_event_path_log.mjs, the same page and script).

use super::*;
use crate::script_net_tests::serve_routes;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// The oracle's script, in two parts: the dispatches, then the reads.
const DISPATCH: &str = "var n = { win: 0, doc: 0, body: 0, btn: 0, inline: 0 };\
    window.addEventListener('click', function () { n.win++; });\
    window.addEventListener('todo-add', function () { n.win++; });\
    document.addEventListener('click', function () { n.doc++; });\
    document.body.onclick = function () { n.body++; };\
    var b = document.getElementById('b');\
    b.addEventListener('todo-add', function () { n.btn++; });\
    for (var i = 0; i < 50; i++) {\
      b.dispatchEvent(new MouseEvent('click', { bubbles: true }));\
      b.dispatchEvent(new CustomEvent('todo-add', { bubbles: true, composed: true }));\
    }\
    'dispatched'";
const READ: &str = "var pn = window.parentNode, nt = window.nodeType;\
    String([n.win, n.doc, n.body, n.btn, n.inline, pn === undefined ? 'undefined' : pn.id,\
    nt === undefined ? 'undefined' : nt.id].join(' '))";

/// (the reads' result, the document walks named access made during the 100 dispatches)
fn run(body: &str) -> (String, u64) {
    let page = format!("<!doctype html><body>{body}</body>");
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    rustkit_js::set_host_call_timing(true);
    let before = rustkit_js::host_call_rows();
    engine.execute_script(view, DISPATCH).unwrap();
    let walks: u64 = rustkit_js::host_call_rows_since(&before)
        .iter()
        .filter(|r| r.name == "__rustkit_dom_collect")
        .map(|r| r.calls)
        .sum();
    rustkit_js::set_host_call_timing(false);
    let value = engine.execute_script(view, READ).unwrap();
    let value = value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string();
    (value, walks)
}

#[test]
fn an_event_that_reaches_the_window_does_not_walk_the_document() {
    let (got, walks) = run("<div id=a><button id=b onclick=\"n.inline++\">x</button></div>");
    assert_eq!(got, "100 50 50 50 50 undefined undefined", "listener counts and window reads, as Chrome");
    assert_eq!(walks, 0, "100 dispatches that reach the Window walked the document {walks} times");
}

#[test]
fn the_windows_named_access_still_answers_an_element_called_parent_node() {
    let (got, walks) = run(
        "<div id=parentNode></div><form name=nodeType id=nt></form>\
         <div id=a><button id=b onclick=\"n.inline++\">x</button></div>",
    );
    assert_eq!(got, "100 50 50 50 50 parentNode nt", "listener counts and window reads, as Chrome");
    assert_eq!(walks, 0, "100 dispatches that reach the Window walked the document {walks} times");
}
