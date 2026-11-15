//! The iframe `sandbox` attribute with nested browsing contexts on (HTML "sandboxing").
//!
//! A frame takes its attribute when its document is created, by a navigation; a change of the
//! attribute afterwards waits for the next navigation. In this stage a frame that has the
//! attribute, whatever its value, gets what a cross-origin frame gets here
//! (`frames_stage2_tests`): nothing is requested, no document is made, and the element hears
//! one `load`. The `allow-scripts` and `allow-same-origin` tokens are not acted on yet.
//!
//! The expected answers are the pinned Chrome's (CfT 148.0.7778.216, headless, recorded
//! 2026-11-15) on `docs/diagnostics/2026-11-15/frame_sandbox_{parent.html,child.html,page.js}`,
//! in `frame_sandbox_cft148.json`: per frame, the `load`s its element heard, whether its script
//! wrote to the page (`marked`), and `contentDocument` from the page. Rows this stage does not
//! match are asserted as known differences, each against Chrome's row, so that a change of
//! either side shows:
//!   - `allow-same-origin` (alone or with `allow-scripts`): Chrome gives the page the frame's
//!     document, and with both tokens the frame's script runs and reaches the page;
//!   - `contentWindow` of a frame without a document is an object in Chrome and `null` here
//!     (as for a cross-origin frame in this stage);
//!   - in Chrome a frame with `allow-scripts` alone runs its script in its own origin; here
//!     it does not run (neither writes to the page);
//!   - a markup frame given the attribute by the page's first script (`markup.mk_added`).
//!
//! With `EngineConfig::nested_contexts` off nothing here changes: the last test pins that.

use super::*;
use crate::script_net_tests::serve_routes;
use std::time::{Duration, Instant};

const PIN: &str = include_str!("../../../docs/diagnostics/2026-11-15/frame_sandbox_cft148.json");

/// A frame's document: its script counts itself on the page under the name in its path.
const CHILD: &str = "<!doctype html><title>child</title><p id=c>child text</p><script>\
var name = location.pathname.slice(1).replace('.html', ''); parent.marks[name] = (parent.marks[name] || 0) + 1;</script>";

const CHILDREN: [&str; 14] = [
    "/src_none.html",
    "/src_empty.html",
    "/src_scripts.html",
    "/src_origin.html",
    "/src_both.html",
    "/added.html",
    "/removed.html",
    "/mk_removed.html",
    "/mk_added.html",
    "/mk_added_src_first.html",
    "/mk_added_src.html",
    "/patched.html",
    "/off_with.html",
    "/off_without.html",
];

/// The page's part, as the probe's: frames by id, their `load`s, and what the page reads.
const PRELUDE: &str = "window.marks = {}; window.loads = {};
function count(f) { loads[f.id] = 0; f.addEventListener('load', function () { loads[f.id]++; }); return f; }
function make(id, sandbox, how) {
  var f = document.createElement('iframe'); f.id = id;
  if (sandbox !== null) f.setAttribute('sandbox', sandbox);
  if (how === 'src') f.src = '/' + id + '.html';
  if (how === 'srcdoc') f.srcdoc = '<p>inline</p><script>parent.marks.' + id + ' = 1<\\/script>';
  count(f); document.body.appendChild(f); return f;
}
function read(id) {
  var f = document.getElementById(id), d;
  try { d = f.contentDocument; d = d === null ? 'null' : typeof d; } catch (e) { d = 'throws ' + e.name; }
  return [loads[id], marks[id] || 0, d].join('|');
}
function all(ids, n) { return ids.every(function (id) { return loads[id] >= n; }); }
var VALUES = { none: null, empty: '', scripts: 'allow-scripts', origin: 'allow-same-origin', both: 'allow-scripts allow-same-origin' };";

struct Page {
    engine: Engine,
    view: EngineViewId,
    rt: tokio::runtime::Runtime,
    server: crate::script_net_tests::Server,
}

impl Page {
    fn load(on: bool, body: &str) -> Self {
        let mut routes = vec![("/", "text/html", format!("<!doctype html><body>{body}</body>"))];
        routes.extend(CHILDREN.iter().map(|path| (*path, "text/html", CHILD.to_string())));
        let server = serve_routes(routes);
        let config = EngineConfig { nested_contexts: on, post_load_requests_live: true, ..EngineConfig::default() };
        let mut engine = Engine::new(config).expect("engine");
        let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
        let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(engine.load_url(view, url)).expect("load_url");
        Page { engine, view, rt, server }
    }

    /// A script's string answer.
    fn js(&mut self, script: &str) -> String {
        let answer = self.engine.execute_script(self.view, script).unwrap();
        answer.strip_prefix("String(\"").and_then(|s| s.strip_suffix("\")")).map(str::to_string).unwrap_or(answer)
    }

    fn turn(&mut self) {
        std::thread::sleep(Duration::from_millis(20));
        self.rt.block_on(self.engine.pump_live(self.view, 20));
    }

    /// Live turns until `done` (the give-up is not what is asserted), then
    /// a few more, for anything that would still come.
    fn settle(&mut self, done: &str) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) && self.js(&format!("String(!!({done}))")) != "true" {
            self.turn();
        }
        self.turns(5);
    }

    fn turns(&mut self, count: usize) {
        for _ in 0..count {
            self.turn();
        }
    }

    fn row(&mut self, id: &str) -> String {
        self.js(&format!("read('{id}')"))
    }

    fn served(&self, path: &str) -> usize {
        self.server.served(path).len()
    }
}

/// Chrome's row: `loads|marked|contentDocument`.
fn chrome(group: &str, name: &str) -> String {
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    let row = &pin[group][name];
    assert!(row.is_object(), "no pinned row {group}.{name}");
    format!("{}|{}|{}", row["loads"], row["marked"], row["contentDocument"].as_str().unwrap())
}

/// This stage's answer for a frame that has the attribute: one `load`, no
/// write from the frame, no document. It is Chrome's row for `sandbox=""`.
const FLOOR: &str = "1|0|null";

#[test]
fn the_pin_is_the_pinned_chromes() {
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    assert_eq!(pin["browser"], "148.0.7778.216");
    assert_eq!(chrome("src", "empty"), FLOOR);
}

/// The frame's script would count itself on the page: with the attribute it does not, the
/// element hears one `load`, `contentDocument` is `null`, and nothing was requested.
#[test]
fn a_frame_with_the_attribute_gets_no_document_and_one_load() {
    let body = format!("<script>{PRELUDE} for (var k in VALUES) make('src_' + k, VALUES[k], 'src');</script>");
    let mut page = Page::load(true, &body);
    page.settle("all(['src_none', 'src_empty', 'src_scripts', 'src_origin', 'src_both'], 1)");
    // As Chrome: no attribute, the empty attribute, `allow-scripts` alone.
    for name in ["none", "empty", "scripts"] {
        assert_eq!(page.row(&format!("src_{name}")), chrome("src", name), "src_{name}");
    }
    assert_eq!(page.row("src_none"), "1|1|object");
    assert_eq!(page.row("src_empty"), FLOOR);
    assert_eq!(page.row("src_scripts"), FLOOR);
    // Known differences: `allow-same-origin` is not acted on yet.
    assert_eq!(chrome("src", "origin"), "1|0|object");
    assert_eq!(chrome("src", "both"), "1|1|object");
    assert_eq!(page.row("src_origin"), FLOOR);
    assert_eq!(page.row("src_both"), FLOOR);
    // Nothing of such a frame is requested, and (a known difference, as
    // for a cross-origin frame here) it has no `contentWindow`.
    for name in ["empty", "scripts", "origin", "both"] {
        assert_eq!(page.served(&format!("/src_{name}.html")), 0, "src_{name} was requested");
        assert_eq!(page.js(&format!("String(document.getElementById('src_{name}').contentWindow)")), "null");
    }
    assert_eq!(page.served("/src_none.html"), 1);
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    assert_eq!(pin["src"]["empty"]["contentWindow"], "object");
}

/// `srcdoc` with the attribute: the same. (`srcdoc` itself is not in this stage: without the
/// attribute the frame is about:blank, which is not asserted here.)
#[test]
fn a_srcdoc_frame_with_the_attribute_gets_no_document_and_one_load() {
    let body = format!("<script>{PRELUDE} for (var k in VALUES) if (k !== 'none') make('srcdoc_' + k, VALUES[k], 'srcdoc');</script>");
    let mut page = Page::load(true, &body);
    page.settle("all(['srcdoc_empty', 'srcdoc_scripts', 'srcdoc_origin', 'srcdoc_both'], 1)");
    for name in ["empty", "scripts"] {
        assert_eq!(page.row(&format!("srcdoc_{name}")), chrome("srcdoc", name), "srcdoc_{name}");
        assert_eq!(page.row(&format!("srcdoc_{name}")), FLOOR);
    }
    // Known differences: `allow-same-origin` is not acted on yet.
    assert_eq!(chrome("srcdoc", "origin"), "1|0|object");
    assert_eq!(chrome("srcdoc", "both"), "1|1|object");
    assert_eq!(page.row("srcdoc_origin"), FLOOR);
    assert_eq!(page.row("srcdoc_both"), FLOOR);
}

/// An about:blank frame with the attribute: the page has no document to write a script into.
#[test]
fn the_page_cannot_write_into_a_blank_frame_with_the_attribute() {
    let body = format!(
        "<script>{PRELUDE} window.wrote = {{}};
['empty', 'both'].forEach(function (k) {{
  var f = make('blank_' + k, VALUES[k], 'blank');
  try {{ var d = f.contentDocument; d.open(); d.write('<script>parent.marks.' + f.id + ' = 1<\\/script>'); d.close(); wrote[k] = 'ok'; }}
  catch (e) {{ wrote[k] = 'throws ' + e.name; }}
}});
window.again = function () {{ ['empty', 'both'].forEach(function (k) {{
  try {{ document.getElementById('blank_' + k).contentDocument.write('<script>parent.marks.blank_' + k + ' = 1<\\/script>'); wrote[k] += ',ok'; }}
  catch (e) {{ wrote[k] += ',throws ' + e.name; }} }}); return wrote.empty + ' ' + wrote.both; }};</script>"
    );
    let mut page = Page::load(true, &body);
    page.settle("all(['blank_empty', 'blank_both'], 1)");
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    assert_eq!(pin["blank"]["empty"]["write"], "throws TypeError");
    // At once, and again once the frame is settled.
    assert_eq!(page.js("again()"), "throws TypeError,throws TypeError throws TypeError,throws TypeError");
    assert_eq!(page.row("blank_empty"), chrome("blank", "empty"));
    assert_eq!(page.row("blank_empty"), FLOOR);
    // Known difference: with both tokens Chrome's page writes and the script runs.
    assert_eq!((chrome("blank", "both"), pin["blank"]["both"]["write"].as_str()), ("2|1|object".to_string(), Some("ok")));
    assert_eq!(page.row("blank_both"), FLOOR);
}

/// The attribute added to, or removed from, a frame whose document exists changes nothing of
/// that document; the frame's next navigation takes the attribute as it is then.
#[test]
fn an_attribute_change_after_load_waits_for_the_next_navigation() {
    let body = format!("<script>{PRELUDE} var added = make('added', null, 'src'), removed = make('removed', '', 'src');</script>");
    let mut page = Page::load(true, &body);
    page.settle("all(['added', 'removed'], 1)");
    assert_eq!(page.row("added"), chrome("afterLoad", "added_loaded"));
    assert_eq!(page.row("removed"), chrome("afterLoad", "removed_loaded"));

    page.js("added.setAttribute('sandbox', ''); removed.removeAttribute('sandbox'); 0");
    page.settle("true");
    assert_eq!(page.row("added"), chrome("afterLoad", "added_changed"));
    assert_eq!(page.row("added"), "1|1|object");
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    assert_eq!(pin["afterLoad"]["added_changed"]["sameOriginCallStillWorks"], "child");
    assert_eq!(page.js("added.contentWindow.document.title"), "child");
    assert_eq!(page.row("removed"), chrome("afterLoad", "removed_changed"));
    assert_eq!(page.row("removed"), FLOOR);
    assert_eq!((page.served("/added.html"), page.served("/removed.html")), (1, 0));

    page.js("marks = {}; added.src = '/added.html'; removed.src = '/removed.html'; 0");
    page.settle("all(['added', 'removed'], 2)");
    assert_eq!(page.row("added"), chrome("afterLoad", "added_navigated"));
    assert_eq!(page.row("added"), "2|0|null");
    assert_eq!(page.row("removed"), chrome("afterLoad", "removed_navigated"));
    assert_eq!(page.row("removed"), "2|1|object");
    assert_eq!((page.served("/added.html"), page.served("/removed.html")), (1, 1));
}

/// Frames in the markup whose attribute the page's first script changes: the attribute as
/// parsed counts for the frame's first document, and one set before a navigation counts for it.
#[test]
fn a_markup_frames_attribute_as_parsed_counts() {
    let body = format!(
        "<iframe id=mk_removed sandbox src=/mk_removed.html></iframe><iframe id=mk_added src=/mk_added.html></iframe>\
         <iframe id=mk_added_src src=/mk_added_src_first.html></iframe><script>{PRELUDE}
['mk_removed', 'mk_added', 'mk_added_src'].forEach(function (id) {{ count(document.getElementById(id)); }});
document.getElementById('mk_removed').removeAttribute('sandbox');
document.getElementById('mk_added').setAttribute('sandbox', '');
document.getElementById('mk_added_src').setAttribute('sandbox', '');
document.getElementById('mk_added_src').src = '/mk_added_src.html';</script>"
    );
    let mut page = Page::load(true, &body);
    page.settle("all(['mk_removed', 'mk_added', 'mk_added_src'], 1)");
    assert_eq!(page.row("mk_removed"), chrome("markup", "mk_removed"));
    assert_eq!(page.row("mk_removed"), FLOOR);
    assert_eq!(page.row("mk_added_src"), chrome("markup", "mk_added_src"));
    assert_eq!(page.row("mk_added_src"), FLOOR);
    assert_eq!(page.js("String(marks.mk_added_src_first)"), "undefined");
    // Known difference: Chrome had started `mk_added`'s navigation at the
    // parse, before the attribute was set; here it is asked for after.
    assert_eq!(chrome("markup", "mk_added"), "1|1|object");
    assert_eq!(page.row("mk_added"), FLOOR);
    for path in ["/mk_removed.html", "/mk_added.html", "/mk_added_src_first.html", "/mk_added_src.html"] {
        assert_eq!(page.served(path), 0, "{path} was requested");
    }
}

/// What the page does around the request does not change the answer: built-ins it replaced,
/// accessors it defined on the element, and the attribute taken off right after the frame went
/// into the document.
#[test]
fn what_the_page_patches_does_not_change_the_answer() {
    let body = format!(
        "<script>{PRELUDE}
var f = document.createElement('iframe'); f.id = 'patched'; f.setAttribute('sandbox', ''); f.src = '/patched.html';
loads.patched = 0; f.addEventListener('load', function () {{ loads.patched++; }});
var remove = Element.prototype.removeAttribute;
Element.prototype.getAttribute = function (n) {{ return n === 'src' ? '/patched.html' : null; }};
Element.prototype.hasAttribute = function () {{ return false; }};
Object.defineProperty(f, 'sandbox', {{ get: function () {{ return null; }}, set: function () {{}} }});
Object.defineProperty(HTMLIFrameElement.prototype, 'sandbox', {{ get: function () {{ return null; }}, configurable: true }});
document.body.appendChild(f);
remove.call(f, 'sandbox');</script>"
    );
    let mut page = Page::load(true, &body);
    page.settle("all(['patched'], 1)");
    assert_eq!(page.row("patched"), FLOOR);
    assert_eq!(page.served("/patched.html"), 0);
}

/// Off (the default), nothing changes, with the attribute or without: no request for a
/// frame's document, `contentWindow` null, no `load`.
#[test]
fn with_nested_contexts_off_the_attribute_changes_nothing() {
    let body = format!(
        "<iframe id=off_with sandbox src=/off_with.html></iframe><iframe id=off_without src=/off_without.html></iframe>\
         <script>{PRELUDE} count(document.getElementById('off_with')); count(document.getElementById('off_without'));
make('src_empty', '', 'src'); make('src_none', null, 'src');</script>"
    );
    let mut page = Page::load(false, &body);
    page.turns(15);
    for id in ["off_with", "off_without", "src_empty", "src_none"] {
        assert_eq!(page.row(id), "0|0|null", "{id}");
        assert_eq!(page.js(&format!("String(document.getElementById('{id}').contentWindow)")), "null");
        assert_eq!(page.served(&format!("/{id}.html")), 0);
    }
}
