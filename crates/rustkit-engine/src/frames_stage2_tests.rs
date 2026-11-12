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
            .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
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

// ── Stage 3: layout, paint, the frame's timers and network ─────────────────

/// The iframe is a replaced box: 300x150 by default, its width/height
/// attributes, author CSS, and a 2px inset border. Chrome 148 on this page
/// (body margin 0): a 0,0 304x154; b 304,70 204x84; c 0,158 324x204; each
/// frame's viewport is the content box (300x150, 200x80, 320x200), and the
/// border is 2px inset (painted solid here: not asserted).
#[test]
fn a_frame_is_a_replaced_box_with_a_2px_inset_border() {
    let page = "<!doctype html><body style='margin:0'><iframe id=a></iframe><iframe id=b width=200 height=80></iframe><br>\
<iframe id=c style='width:320px;height:200px'></iframe><script>
window.boxes = function () { return ['a','b','c'].map(function (id) { var e = document.getElementById(id), r = e.getBoundingClientRect(), s = getComputedStyle(e);
  return [r.x, r.y, r.width, r.height, s.borderTopWidth, e.contentWindow ? e.contentWindow.innerWidth + 'x' + e.contentWindow.innerHeight : 'none'].join(','); }).join(' | '); };
</script></body>";
    let mut page = Page::load(true, page);
    page.turns_until("['a','b','c'].every(function (id) { return document.getElementById(id).contentWindow; })", Duration::from_secs(5));
    assert_eq!(
        page.js("boxes()"),
        r#"String("0,0,304,154,2px,300x150 | 304,70,204,84,2px,200x80 | 0,158,324,204,2px,320x200")"#
    );
}

/// The frame's timers and its fetch() run, and what its script writes
/// reaches its layout and the parent's paint. Chrome 148: the child's
/// timer and fetch both answer, and its body's height read from the parent
/// is the new one in the same task.
#[test]
fn a_frames_timers_network_and_layout_run() {
    let page = "<!doctype html><body><iframe id=f src=/live.html style='width:200px;height:100px'></iframe><script>
window.loaded = false; document.getElementById('f').addEventListener('load', function () { loaded = true; });
</script></body>";
    let routes_page = page.to_string();
    let live = "<!doctype html><body style='margin:0'><div id=box style='height:10px;background:rgb(0,255,0)'></div><script>
window.log = [];
setTimeout(function () { log.push('timer'); }, 20);
requestAnimationFrame(function () { log.push('frame'); });
fetch('/data.txt').then(function (r) { return r.text(); }).then(function (t) { log.push('fetch:' + t); });
</script></body>";
    let server = serve_routes(vec![
        ("/", "text/html", routes_page),
        ("/live.html", "text/html", live.to_string()),
        ("/data.txt", "text/plain", "hello".to_string()),
    ]);
    let mut engine = Engine::new(config(true)).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap())).unwrap();
    let mut page = Page { engine, view, rt, server };
    page.turns_until("window.loaded && document.getElementById('f').contentWindow.log.length >= 3", Duration::from_secs(8));
    assert_eq!(page.js("document.getElementById('f').contentWindow.log.slice().sort().join()"), r#"String("fetch:hello,frame,timer")"#);
    // A write in the frame, read across the boundary at once.
    assert_eq!(
        page.js("var d = document.getElementById('f').contentDocument; d.getElementById('box').style.height = '40px'; d.body.getBoundingClientRect().height"),
        "Number(40.0)"
    );
    // The frame's green box is in the parent's paint, inside the iframe.
    page.turns_until("false", Duration::from_millis(100));
    let list = page.engine.views[&page.view].doc.display_list.clone().expect("a display list");
    let green = list.commands.iter().any(|c| matches!(c,
        rustkit_layout::DisplayCommand::SolidColor(color, _) if color.r == 0 && color.g == 255 && color.b == 0));
    assert!(green, "the frame's paint is not in the parent's display list");
}

/// A loop that turns again only when the last turn asked for it (a timer
/// due, a request out), as the app's and parity-capture's do, still
/// finishes a frame navigation started from a `load` handler. Found with
/// Speedometer 3.1 on the integration branch: its runner sets `src` for
/// the next workload from the frame's `load` path, the page then had no
/// timer and nothing out, and the loop slept (149 s, until its window
/// ended) before the frame request was taken.
#[test]
fn a_frame_request_made_during_a_turn_asks_for_the_next_turn() {
    let page = "<!doctype html><body><iframe id=f></iframe><script>
window.loads = 0; var f = document.getElementById('f');
f.onload = function () { loads++; if (loads === 1) f.src = '/other.html'; };
f.src = '/child.html';
</script></body>";
    let mut page = Page::load(true, page);
    let mut turns = 0;
    loop {
        let turn = page.rt.block_on(page.engine.pump_live(page.view, 20));
        turns += 1;
        if turns >= 40 || (turn.next_timer_ms.is_none() && turn.in_flight == 0) {
            break;
        }
        std::thread::sleep(Duration::from_millis(turn.next_timer_ms.unwrap_or(10).min(20)));
    }
    assert_eq!(page.js("[loads, f.contentDocument.title].join()"), r#"String("2,other-title")"#, "after {turns} turns");
}

/// A frame's module scripts run, after its classic ones, with their
/// imports (Speedometer 3.1: the WebComponents and Angular workloads are
/// `<script type="module">`; without this the first found no elements and
/// the second waited for ever). Chrome 148: `classic,dep,dep-value,main` and the
/// custom element is upgraded.
#[test]
fn a_frames_module_scripts_run_with_their_imports() {
    let page = "<!doctype html><body><iframe id=f src=/mod.html></iframe><script>
window.loaded = false; document.getElementById('f').addEventListener('load', function () { loaded = true; });
</script></body>";
    let child = "<!doctype html><body><x-hello id=x></x-hello><script type=module src=/main.mjs></script>\
<script>window.order = ['classic'];</script></body>";
    let server = serve_routes(vec![
        ("/", "text/html", page.to_string()),
        ("/mod.html", "text/html", child.to_string()),
        ("/main.mjs", "text/javascript", "import { dep } from './dep.mjs'; order.push(dep, 'main'); \
            customElements.define('x-hello', class extends HTMLElement { connectedCallback() { this.textContent = 'upgraded'; } });".to_string()),
        ("/dep.mjs", "text/javascript", "order.push('dep'); export const dep = 'dep-value';".to_string()),
    ]);
    let mut engine = Engine::new(config(true)).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap())).unwrap();
    let mut page = Page { engine, view, rt, server };
    page.turns_until("window.loaded", Duration::from_secs(8));
    assert_eq!(
        page.js("var w = document.getElementById('f').contentWindow; w.order.join() + '|' + w.document.getElementById('x').textContent"),
        r#"String("classic,dep,dep-value,main|upgraded")"#
    );
}

/// `querySelector` inside a shadow tree of a frame's document (Speedometer
/// 3.1 TodoMVC-WebComponents and Lit: the runner walks `todo-app` ->
/// shadow root -> `todo-topbar`, and got null inside the frame while the
/// same page top-level answered). The frame's bindings had no selector
/// matcher from the engine. Chrome 148: the elements are found.
#[test]
fn a_frames_script_finds_elements_inside_a_shadow_tree() {
    let page = "<!doctype html><body><iframe id=f src=/shadow.html></iframe><script>
window.loaded = false; document.getElementById('f').addEventListener('load', function () { loaded = true; });
</script></body>";
    let child = "<!doctype html><body><div id=h></div><script>
var r = document.getElementById('h').attachShadow({ mode: 'open' });
r.innerHTML = '<section class=app><x-top></x-top><span class=s>in</span></section>';
window.inside = [String(r.querySelector('x-top')), String(r.querySelector('.s')), r.querySelectorAll('*').length].join();
</script></body>";
    let server = serve_routes(vec![("/", "text/html", page.to_string()), ("/shadow.html", "text/html", child.to_string())]);
    let mut engine = Engine::new(config(true)).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap())).unwrap();
    let mut page = Page { engine, view, rt, server };
    page.turns_until("window.loaded", Duration::from_secs(8));
    assert_eq!(
        page.js("var d = document.getElementById('f').contentDocument; document.getElementById('f').contentWindow.inside + '|' + String(d.getElementById('h').shadowRoot.querySelector('x-top'))"),
        r#"String("[object HTMLElement],[object HTMLSpanElement],3|[object HTMLElement]")"#
    );
}
