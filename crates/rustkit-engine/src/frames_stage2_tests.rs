//! Nested browsing contexts, Stage 2: a same-origin `<iframe>` loads
//! (docs/diagnostics/2026-11-11/nested-browsing-contexts-plan.md).
//!
//! A real loopback server and the engine's live loop, as
//! `inserted_script_live_tests`. The expected answers are Chrome for
//! Testing 148.0.7778.216's on the same pages (headless, recorded
//! 2026-11-12 with the parity oracle's pinned binary; quoted where asserted;
//! the cross-origin test's floor is a stated deviation).
//! With `EngineConfig::nested_contexts` off nothing here changes: the last
//! test pins that.

use super::*;
use crate::script_net_tests::serve_routes;
use std::time::{Duration, Instant};

struct Page {
    engine: Engine,
    view: EngineViewId,
    rt: tokio::runtime::Runtime,
    server: crate::script_net_tests::Server,
}

fn config(on: bool) -> EngineConfig {
    EngineConfig { nested_contexts: on, post_load_requests_live: true, ..EngineConfig::default() }
}

const CHILD: &str = "<!doctype html><html><head><title>child-title</title></head><body><div id=c>child text</div>\
<script>function childFn() { return 'child-fn-ok:' + document.title; } window.ranAt = (window.ranAt || 0) + 1; \
window.sawParent = (window.parent !== window) + ':' + (window.top === window.parent) + ':' + String(window.frameElement && window.frameElement.id);</script>\
<script src=/child.js></script></body></html>";

impl Page {
    fn load(on: bool, page: &str) -> Self {
        let routes = vec![
            ("/", "text/html", page.to_string()),
            ("/child.html", "text/html", CHILD.to_string()),
            ("/other.html", "text/html", "<!doctype html><title>other-title</title><p id=o>other</p>".to_string()),
            ("/child.js", "text/javascript", "window.externalRan = true;".to_string()),
        ];
        let server = serve_routes(routes);
        let mut engine = Engine::new(config(on)).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
            .expect("view");
        let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(engine.load_url(view, url)).expect("load_url");
        Page { engine, view, rt, server }
    }

    fn js(&mut self, script: &str) -> String {
        self.engine.execute_script(self.view, script).unwrap()
    }

    fn turns_until(&mut self, done: &str, within: Duration) {
        let started = Instant::now();
        while started.elapsed() < within {
            std::thread::sleep(Duration::from_millis(20));
            self.rt.block_on(self.engine.pump_live(self.view, 20));
            if self.js(done) == "Boolean(true)" {
                return;
            }
        }
    }

    fn served(&self, path: &str) -> usize {
        self.server.served(path).len()
    }
}

/// The plan's table, row for row. Chrome 148 on this page:
/// contentWindow/contentDocument objects, the child ran (its scripts saw
/// a parent, `top === parent`, `frameElement` the iframe), `load` at the
/// element, then the parent reads the child's title, a node, a function,
/// its viewport (the iframe's CSS size) and its external script's global.
#[test]
fn a_same_origin_frame_loads_runs_and_is_reachable() {
    let page = "<!doctype html><body><script>
window.events = [];
var f = document.createElement('iframe');
f.id = 'f';
f.style.width = '320px'; f.style.height = '200px';
f.addEventListener('load', function () { events.push('listener'); });
f.onload = function () { events.push('onload'); };
f.src = '/child.html';
document.body.appendChild(f);
window.done = function () { return events.length >= 2; };
</script></body>";
    let mut page = Page::load(true, page);
    page.turns_until("window.done()", Duration::from_secs(5));
    assert_eq!(page.js("events.join()"), r#"String("listener,onload")"#);
    assert_eq!(
        page.js(
            "var w = f.contentWindow, d = f.contentDocument; \
             [typeof w, d === w.document, d.title, d.querySelector('#c').textContent, w.childFn(), \
              w.innerWidth + 'x' + w.innerHeight, w.ranAt, w.sawParent, w.externalRan, \
              w.location.pathname, d.getElementById('c').ownerDocument === d].join('|')"
        ),
        r#"String("object|true|child-title|child text|child-fn-ok:child-title|320x200|1|true:true:f|true|/child.html|true")"#
    );
    assert_eq!(page.served("/child.html"), 1);
}

/// Speedometer's runner sets `src` again on the same frame: a second load,
/// a new document, the old window's globals gone. Chrome 148: two `load`s,
/// `other-title`, `undefined`.
#[test]
fn setting_src_again_navigates_the_frame() {
    let page = "<!doctype html><body><iframe id=f></iframe><script>
window.loads = 0;
var f = document.getElementById('f');
f.onload = function () { loads++; if (loads === 1) { window.first = f.contentDocument.title; f.contentWindow.mark = 1; f.src = '/other.html'; } };
f.src = '/child.html';
</script></body>";
    let mut page = Page::load(true, page);
    page.turns_until("window.loads >= 2", Duration::from_secs(5));
    assert_eq!(
        page.js("[loads, first, f.contentDocument.title, typeof f.contentWindow.mark].join()"),
        r#"String("2,child-title,other-title,undefined")"#
    );
}

/// A cross-origin `src` is never requested in this stage (the plan's
/// floor): no document is reachable, and the element still hears `load`
/// so a page waiting on it goes on. (Chrome loads the document; the
/// floor is a stated deviation until cross-origin frames get their own
/// design.)
#[test]
fn a_cross_origin_frame_is_not_fetched_and_still_fires_load() {
    let page = "<!doctype html><body><script>
window.loads = 0;
var f = document.createElement('iframe');
f.onload = function () { loads++; };
f.src = 'http://localhost:9/elsewhere.html';
document.body.appendChild(f);
</script></body>";
    let mut page = Page::load(true, page);
    page.turns_until("window.loads >= 1", Duration::from_secs(5));
    assert_eq!(page.js("[loads, String(f.contentDocument), String(f.contentWindow)].join()"), r#"String("1,null,null")"#);
}

/// A frame in the parser's markup loads too (after the parent's own
/// `load` in this stage).
#[test]
fn a_frame_in_the_markup_loads() {
    let page = "<!doctype html><body><iframe id=f src=/child.html></iframe><script>
window.loaded = false;
document.getElementById('f').addEventListener('load', function () { loaded = true; });
</script></body>";
    let mut page = Page::load(true, page);
    page.turns_until("window.loaded", Duration::from_secs(5));
    assert_eq!(page.js("document.getElementById('f').contentDocument.title"), r#"String("child-title")"#);
}

/// A removed frame is dropped: its window is no longer the element's.
/// Chrome 148: `contentWindow` and `contentDocument` are null once the
/// element is out of the document.
#[test]
fn a_removed_frame_is_dropped() {
    let page = "<!doctype html><body><iframe id=f src=/child.html></iframe><script>
window.loaded = false;
var f = document.getElementById('f');
f.addEventListener('load', function () { loaded = true; });
</script></body>";
    let mut page = Page::load(true, page);
    page.turns_until("window.loaded", Duration::from_secs(5));
    page.js("window.held = f.contentWindow; f.parentNode.removeChild(f);");
    page.turns_until("false", Duration::from_millis(100));
    assert_eq!(page.js("[String(f.contentWindow), String(f.contentDocument), held.document.title].join()"), r#"String("null,null,child-title")"#);
}

/// Off (the default), nothing changes: no request for the frame's
/// document, `contentWindow` null, no `load`.
#[test]
fn with_nested_contexts_off_a_frame_stays_empty() {
    let page = "<!doctype html><body><iframe id=f src=/child.html></iframe><script>
window.loads = 0; document.getElementById('f').onload = function () { loads++; };
</script></body>";
    let mut page = Page::load(false, page);
    page.turns_until("false", Duration::from_millis(300));
    assert_eq!(page.js("[loads, String(document.getElementById('f').contentWindow)].join()"), r#"String("0,null")"#);
    assert_eq!(page.served("/child.html"), 0);
}
