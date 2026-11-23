//! JETSTREAM-IFRAME-SYNC (docs/design/JETSTREAM-IFRAME-SYNC.md): a frame's
//! about:blank document at insertion, and document.open / write / close.
//!
//! Expected answers: Chrome for Testing 148.0.7778.216 (linux64),
//! docs/diagnostics/2026-11-23/chrome148_iframe_write.json
//! (tools/parity_oracle/iframe_write_log.mjs). The harness is
//! frames_stage2_tests's: a loopback server and the engine's live loop.
//!
//! Spares are lazy ([R1]): a document's first frame drain builds them. The
//! pages here that need a synchronous frame therefore start with one markup
//! iframe, as JetStream's does (`#magic`).

use super::*;
use crate::script_net_tests::serve_routes;
use std::time::Duration;

struct Page {
    engine: Engine,
    view: EngineViewId,
    rt: tokio::runtime::Runtime,
    _server: crate::script_net_tests::Server,
}

/// /chain/0 .. /chain/39, each framing the next (`Page::load`'s routes).
const CHAIN: [&str; 40] = [
    "/chain/0", "/chain/1", "/chain/2", "/chain/3", "/chain/4", "/chain/5", "/chain/6", "/chain/7", "/chain/8", "/chain/9",
    "/chain/10", "/chain/11", "/chain/12", "/chain/13", "/chain/14", "/chain/15", "/chain/16", "/chain/17", "/chain/18", "/chain/19",
    "/chain/20", "/chain/21", "/chain/22", "/chain/23", "/chain/24", "/chain/25", "/chain/26", "/chain/27", "/chain/28", "/chain/29",
    "/chain/30", "/chain/31", "/chain/32", "/chain/33", "/chain/34", "/chain/35", "/chain/36", "/chain/37", "/chain/38", "/chain/39",
];

impl Page {
    fn load(nested_contexts: bool, page: &str) -> Self {
        Self::load_spares(nested_contexts, true, page)
    }

    fn load_spares(nested_contexts: bool, frame_spares: bool, page: &str) -> Self {
        let mut routes = vec![
            ("/", "text/html", page.to_string()),
            ("/next.html", "text/html", "<!doctype html><body><iframe id=n></iframe></body>".to_string()),
            // A page that frames itself (Chrome nests it once).
            ("/self", "text/html", "<!doctype html><body><iframe src=/self></iframe></body>".to_string()),
            // A frame whose document has a frame of its own.
            ("/nests.html", "text/html", "<!doctype html><body><iframe src=/leaf.html></iframe></body>".to_string()),
            ("/leaf.html", "text/html", "<!doctype html><body>leaf</body>".to_string()),
        ];
        // A chain of distinct URLs, each framing the next.
        for (i, path) in CHAIN.iter().enumerate() {
            routes.push((path, "text/html", format!("<!doctype html><body><iframe src=/chain/{}></iframe></body>", i + 1)));
        }
        let server = serve_routes(routes);
        let config = EngineConfig { nested_contexts, frame_spares, post_load_requests_live: true, ..EngineConfig::default() };
        let mut engine = Engine::new(config).expect("engine");
        let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
        let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(engine.load_url(view, url)).expect("load_url");
        let mut page = Page { engine, view, rt, _server: server };
        // Let the markup frames drain (and their spares be built).
        page.turns("true", 10);
        page
    }

    fn js(&mut self, script: &str) -> String {
        self.engine.execute_script(self.view, script).unwrap()
    }

    fn served(&self, path: &str) -> usize {
        self._server.served(path).len()
    }

    fn turns(&mut self, done: &str, max: usize) -> bool {
        for _ in 0..max {
            self.rt.block_on(self.engine.pump_live(self.view, 20));
            if self.js(done) == "Boolean(true)" {
                return true;
            }
        }
        false
    }
}

/// A page whose markup has one frame (JetStream's `#magic`).
const WITH_MAGIC: &str = "<!doctype html><body><iframe id=magic></iframe></body>";

/// Test 1. Chrome: null before insertion; right after appendChild, with no
/// src and with src=about:blank, an about:blank document, complete, with an
/// empty head and body, and contentWindow.document the same object.
///
/// The second frame (src=about:blank) is inserted by a later script: one
/// spare per document per script (the limit below).
#[test]
fn the_blank_document_is_there_right_after_insertion() {
    let mut page = Page::load(true, WITH_MAGIC);
    assert_eq!(
        page.js(
            "var a = document.createElement('iframe'); var before = a.contentDocument === null; \
             document.body.appendChild(a); var d = a.contentDocument; \
             [before, !!d && d.URL, !!d && d.readyState, !!d && d.documentElement.outerHTML, \
              !!d && a.contentWindow.document === d].join('|')"
        ),
        r#"String("true|about:blank|complete|<html><head></head><body></body></html>|true")"#
    );
    page.turns("true", 2);
    assert_eq!(
        page.js(
            "var b = document.createElement('iframe'); b.src = 'about:blank'; document.body.appendChild(b); \
             String(!!b.contentDocument && b.contentDocument.URL)"
        ),
        r#"String("about:blank")"#
    );
}

/// LIMIT, pinned so it is seen: one spare per document per script. A second
/// frame inserted by the same script attaches late (Chrome: at once). JetStream
/// inserts one frame per document per script; a page inserting several (ad
/// slots) gets the first at once and the others after the script.
#[test]
fn a_second_frame_in_the_same_script_attaches_late() {
    let mut page = Page::load(true, WITH_MAGIC);
    assert_eq!(
        page.js(
            "var a = document.createElement('iframe'); document.body.appendChild(a); \
             var b = document.createElement('iframe'); document.body.appendChild(b); \
             window.bFrame = b; [!!a.contentDocument, b.contentDocument === null].join('|')"
        ),
        r#"String("true|true")"#
    );
    page.turns("!!window.bFrame.contentDocument", 10);
    assert_eq!(page.js("String(!!bFrame.contentDocument && bFrame.contentDocument.URL)"), r#"String("about:blank")"#);
}

/// Test 2. JetStream's pattern: a frame made by innerHTML inside the
/// (already drained) markup frame has its document at once.
#[test]
fn a_frame_made_inside_a_frame_has_its_document_at_once() {
    let mut page = Page::load(true, WITH_MAGIC);
    assert_eq!(
        page.js(
            "var magic = document.getElementById('magic'); \
             magic.contentDocument.body.innerHTML = '<iframe id=\"magicframe\" frameborder=\"0\">'; \
             var m = magic.contentDocument.getElementById('magicframe'); \
             [!!m, !!m && !!m.contentDocument, !!m && !!m.contentDocument && m.contentDocument.URL, \
              !!m && !!m.contentDocument && !!m.contentDocument.body].join('|')"
        ),
        r#"String("true|true|about:blank|true")"#
    );
}

/// Test 3. Frames off: no child realm, but contentDocument is an inert
/// about:blank document right after insertion, never null ([R6]).
#[test]
fn the_blank_document_is_there_with_frames_off() {
    let mut page = Page::load(false, "<!doctype html><body></body>");
    assert_eq!(
        page.js(
            "var a = document.createElement('iframe'); document.body.appendChild(a); var d = a.contentDocument; \
             [!!d, !!d && d.URL, !!d && d.readyState, !!d && !!d.body].join('|')"
        ),
        r#"String("true|about:blank|complete|true")"#
    );
}

/// The receipt's sequence, run in a frame made inside `#magic`.
const WRITE_SEQUENCE: &str = "var magic = document.getElementById('magic'); \
    magic.contentDocument.body.innerHTML = '<iframe id=\"magicframe\">'; \
    var md = magic.contentDocument.getElementById('magicframe').contentDocument; \
    window.__log = []; md.open(); \
    var afterOpen = [md.documentElement, md.body].join(','); \
    md.write('<!DOCTYPE html><html><head><script>top.__log.push(\"s1:\" + document.readyState); var frameGlobal = 1;</script></head><body><p id=\"x\">hi</p><script>top.__log.push(\"s2:\" + !!document.getElementById(\"x\"));</script>'); \
    var afterWrite = __log.join() + '/' + md.readyState + '/' + !!md.getElementById('x'); \
    md.write('<script>top.__log.push(\"s3\");</script>'); md.close(); \
    var afterClose = __log.join() + '/' + md.readyState;";

/// Test 4. Chrome: open() empties (documentElement and body null); write()
/// runs each inline script as the parser reaches it (s1 sees `loading`, s2
/// sees the <p> written before it), a second write's script too; close()
/// makes it `complete`. The scripts ran in the frame's realm: `frameGlobal`
/// is the frame's global, not the parent's.
#[test]
fn write_runs_inline_scripts_as_the_parser_reaches_them() {
    let mut page = Page::load(true, WITH_MAGIC);
    assert_eq!(
        page.js(&format!(
            "{WRITE_SEQUENCE} [afterOpen, afterWrite, afterClose, typeof window.frameGlobal, \
             md.defaultView.frameGlobal].join('|')"
        )),
        r#"String(",|s1:loading,s2:true/loading/true|s1:loading,s2:true,s3/complete|undefined|1")"#
    );
}

/// Test 5. A written `<script src="blob:...">` blocks the parser: it runs
/// before the markup after it is parsed (JetStream writes its benchmark
/// files as blob: URLs).
#[test]
fn a_written_blob_script_runs_before_the_markup_after_it() {
    let mut page = Page::load(true, WITH_MAGIC);
    assert_eq!(
        page.js(
            "var magic = document.getElementById('magic'); \
             magic.contentDocument.body.innerHTML = '<iframe id=\"mf\">'; \
             var md = magic.contentDocument.getElementById('mf').contentDocument; \
             var url = URL.createObjectURL(new Blob(['top.__seen = !!document.getElementById(\"after\");'], {type: 'text/javascript'})); \
             md.open(); md.write('<body><script src=\"' + url + '\"></script><p id=\"after\">a</p></body>'); md.close(); \
             String(top.__seen)"
        ),
        r#"String("false")"#
    );
}

/// Test 6. Each frame's written scripts keep their globals to their frame
/// (JetStream relies on one frame per benchmark).
#[test]
fn two_frames_keep_their_globals_apart() {
    let mut page = Page::load(true, "<!doctype html><body><iframe id=a></iframe><iframe id=b></iframe></body>");
    assert_eq!(
        page.js(
            "function framed(host, v) { host.contentDocument.body.innerHTML = '<iframe id=\"w\">'; \
               var d = host.contentDocument.getElementById('w').contentDocument; \
               d.open(); d.write('<script>var shared = ' + v + ';</script>'); d.close(); return d.defaultView; } \
             var wa = framed(document.getElementById('a'), 1), wb = framed(document.getElementById('b'), 2); \
             [wa.shared, wb.shared, typeof window.shared].join('|')"
        ),
        r#"String("1|2|undefined")"#
    );
}

/// Test 7. Frames off: write parses into the inert document but runs no
/// script (there is no browsing context to run it in).
#[test]
fn frames_off_write_runs_no_script() {
    let mut page = Page::load(false, "<!doctype html><body></body>");
    assert_eq!(
        page.js(
            "var a = document.createElement('iframe'); document.body.appendChild(a); var d = a.contentDocument; \
             window.__ran = false; d.open(); d.write('<p id=\"x\">x</p><script>top.__ran = true;</script>'); d.close(); \
             [String(window.__ran), !!d.getElementById('x')].join('|')"
        ),
        r#"String("false|true")"#
    );
}

/// Test 8 ([R4]). Inside write the stack is not empty: no microtask
/// checkpoint between written scripts, as in Chrome. Script 1 queues a
/// reaction; script 2 sees it not yet run; after the outer script it has.
#[test]
fn no_microtask_checkpoint_between_written_scripts() {
    let mut page = Page::load(true, WITH_MAGIC);
    assert_eq!(
        page.js(
            "var magic = document.getElementById('magic'); \
             magic.contentDocument.body.innerHTML = '<iframe id=\"mf\">'; \
             var md = magic.contentDocument.getElementById('mf').contentDocument; \
             top.__flag = 'unset'; md.open(); \
             md.write('<script>Promise.resolve().then(function () { top.__flag = \"set\"; });</script><script>top.__seenInScript2 = top.__flag;</script>'); \
             md.close(); top.__seenInScript2"
        ),
        r#"String("unset")"#
    );
    page.turns("true", 2);
    assert_eq!(page.js("top.__flag"), r#"String("set")"#);
}

/// Test 9 ([R2]). A spare belongs to one document. After the top document
/// navigates, a frame inserted in the new document gets a window whose parent
/// is the new document's window.
///
/// What this can and cannot show: reuse of the OLD document's spare is
/// impossible by construction, not merely untested. A navigation builds a new
/// JsRuntime and new DomBindings (`load_url_for_nav`, `load_html_doc`), and the
/// spare lives in the old bindings, so it is dropped with them and their
/// runtime. No page behaviour could reveal a reuse (the new page's script
/// would attach even an old spare to the new window), so this test is a
/// behavioural pin of the new document's frame, not a falsifiable guard.
#[test]
fn a_spare_taken_after_the_parent_navigated_is_never_used() {
    let mut page = Page::load(true, WITH_MAGIC);
    let next = Url::parse(&page.js("location.origin").trim_start_matches("String(\"").trim_end_matches("\")").to_string())
        .unwrap()
        .join("/next.html")
        .unwrap();
    page.rt.block_on(page.engine.load_url(page.view, next)).expect("navigate");
    page.turns("true", 10);
    assert_eq!(
        page.js(
            "var f = document.createElement('iframe'); document.body.appendChild(f); \
             var w = f.contentWindow; String(w === null || w.parent === window)"
        ),
        r#"String("true")"#
    );
}

/// Guard: only a frame's document gets open/write/close. A page's own
/// document keeps what it had before this row, with frames on and off: no
/// `open`, nothing on Document.prototype (its `write` is the legacy stub,
/// untouched). Real sites feature-detect these. A first draft put them on
/// Document.prototype; this test fails against it.
#[test]
fn the_pages_own_document_gains_no_open_or_write() {
    for on in [false, true] {
        let mut page = Page::load(on, "<!doctype html><body></body>");
        assert_eq!(
            page.js("[typeof document.open, typeof document.close, typeof Document.prototype.open, \
                     typeof Document.prototype.write, typeof Document.prototype.close].join('|')"),
            r#"String("undefined|undefined|undefined|undefined|undefined")"#,
            "nested_contexts = {on}"
        );
    }
}

/// Phase 2a, the ground the spares stand on: a frame made inside a frame is
/// a browsing context at all (late is fine here). Before, only top-level
/// pages had frames enabled (`enable_frames`), so a frame inside a frame
/// never got a window. Chrome: a window whose document is the frame's
/// contentDocument, whose parent is the outer frame's window.
#[test]
fn a_frame_inside_a_frame_becomes_a_browsing_context() {
    let mut page = Page::load(true, WITH_MAGIC);
    page.js(
        "var magic = document.getElementById('magic'); \
         magic.contentDocument.body.innerHTML = '<iframe id=\"inner\">'; \
         window.inner = magic.contentDocument.getElementById('inner'); 0",
    );
    page.turns("!!window.inner.contentWindow", 20);
    assert_eq!(
        page.js(
            "var w = inner.contentWindow, m = document.getElementById('magic').contentWindow; \
             [!!w, !!w && w.document === inner.contentDocument, !!w && w.parent === m, !!w && w.top === window].join('|')"
        ),
        r#"String("true|true|true|true")"#
    );
}

/// How many frames down, through each level's first iframe, have a window
/// and a body (Chrome's probe in docs/diagnostics/2026-11-23, same walk).
const DEPTH: &str = "(function () { var d = 0, w = window; while (d < 100) { var f = w.document.querySelector('iframe'); \
    if (!f || !f.contentWindow || !f.contentDocument || !f.contentDocument.body) break; d++; w = f.contentWindow; } return d; })()";

/// Atlas's condition on phase 2a: a page that frames itself does not recurse.
/// Chrome 148 (a top page that is /self) nests it exactly once: its
/// self-frame loads, that frame's self-frame is left blank. Blink allows one
/// level of self-reference: a frame is not loaded when its URL is already the
/// URL of two of its ancestors. Here the top page is `/`, so /self loads in
/// the first frame and once inside it (the one self-reference); the third is
/// left blank: depth 3 by the walk (a blank frame has a body), /self fetched
/// twice.
#[test]
fn a_page_that_frames_itself_nests_once() {
    let mut page = Page::load(true, "<!doctype html><body><iframe src=/self></iframe></body>");
    page.turns("false", 30);
    assert_eq!(page.js(DEPTH), "Number(3.0)");
    assert_eq!(page.served("/self"), 2, "the page's own frame and one self-frame inside it");
}

/// A chain of distinct URLs stops at the nesting cap (16 levels). A stated
/// deviation: Chrome caps the frame count, not the depth, and was 149 deep
/// after 3 s on the same chain; JetStream needs 2.
#[test]
fn a_chain_of_frames_stops_at_the_depth_cap() {
    let mut page = Page::load(true, "<!doctype html><body><iframe src=/chain/0></iframe></body>");
    page.turns("false", 80);
    assert_eq!(page.js(DEPTH), "Number(16.0)");
    assert_eq!(page.served("/chain/16"), 0, "nothing below the cap is fetched");
}

/// A sandboxed frame (no allow-scripts) gets no browsing context, so no
/// frames of its own: its document's frame is never requested.
#[test]
fn a_sandboxed_frame_gets_no_frames_of_its_own() {
    let mut page = Page::load(true, "<!doctype html><body><iframe sandbox src=/nests.html></iframe></body>");
    page.turns("false", 20);
    assert_eq!(page.served("/leaf.html"), 0);
    assert_eq!(page.js("String(document.querySelector('iframe').contentWindow)"), r#"String("null")"#);
}

/// The switch (`HIWAVE_FRAME_SPARES=0`, the cost gate's spares-off arm and a
/// way back): with spares off, a frame inserted by script has no window and
/// no document inside the insertion (frames on: the getters answer the
/// attached window's, and there is none yet) and gets them at the next
/// drain, exactly as before phase 2b.
#[test]
fn with_spares_off_an_inserted_frame_attaches_at_the_drain() {
    let mut page = Page::load_spares(true, false, WITH_MAGIC);
    assert_eq!(
        page.js(
            "var a = document.createElement('iframe'); document.body.appendChild(a); window.aFrame = a; \
             [a.contentWindow === null, a.contentDocument === null].join('|')"
        ),
        r#"String("true|true")"#
    );
    page.turns("!!window.aFrame.contentWindow", 10);
    assert_eq!(page.js("String(!!aFrame.contentWindow && aFrame.contentWindow.document.URL)"), r#"String("about:blank")"#);
}
