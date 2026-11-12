//! Stage 1 of the nested-browsing-contexts plan
//! (docs/diagnostics/2026-11-11/nested-browsing-contexts-plan.md): two
//! documents in one JavaScript runtime, each in its own realm, the way a
//! page and its same-origin `<iframe>` must be for script to reach across.
//! No engine, no layout, no loading: this is the proof that the runtime and
//! the bindings can be shared at all, or the place the plan stops.
//!
//! The expected answers are a browser's for a page and a same-origin frame
//! (`parent.child` standing for `iframe.contentWindow`). Where a comment
//! says "Chrome 148", the value was recorded from Chrome for Testing
//! 148.0.7778.216 (headless, 2026-11-11) on a page with a same-origin
//! `<iframe>` running the same expressions against `iframe.contentWindow`:
//!   node:     true,false,true,true,false,child text
//!   dispatch: true, and the child heard `ping:true:true:false,bubbled`
//!   error:    child broke:true:false

use super::*;

const PARENT: &str = "<html><head><title>parent-title</title></head><body><p id=p>parent text</p></body></html>";
const CHILD: &str = "<html><head><title>child-title</title></head><body><div id=c>child text</div><input id=i></body></html>";

struct Tree {
    parent: DomBindings,
    child: DomBindings,
}

fn tree() -> Tree {
    let parent = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    parent.set_document(Rc::new(Document::parse_html(PARENT).unwrap())).unwrap();
    let child = parent.new_child().unwrap();
    child.set_document(Rc::new(Document::parse_html(CHILD).unwrap())).unwrap();
    parent.expose_window("child", &child).unwrap();
    child.expose_window("up", &parent).unwrap();
    Tree { parent, child }
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script) {
        Ok(JsValue::String(s)) => s,
        Ok(JsValue::Boolean(x)) => x.to_string(),
        Ok(JsValue::Number(n)) => format!("{n}"),
        Ok(JsValue::Undefined) => "undefined".into(),
        Ok(JsValue::Null) => "null".into(),
        Ok(other) => format!("{other:?}"),
        Err(e) => format!("ERROR {e}"),
    }
}

#[test]
fn each_page_has_its_own_globals_and_its_own_document() {
    let t = tree();
    ev(&t.parent, "var mark = 'parent mark';");
    ev(&t.child, "var mark = 'child mark';");
    assert_eq!(ev(&t.parent, "mark + '|' + document.title + '|' + document.getElementById('p').textContent"), "parent mark|parent-title|parent text");
    assert_eq!(ev(&t.child, "mark + '|' + document.title + '|' + document.getElementById('c').textContent"), "child mark|child-title|child text");
    assert_eq!(ev(&t.parent, "String(document.getElementById('c'))"), "null", "the parent's document is not the child's");
    assert_eq!(ev(&t.child, "typeof p_only + '|' + (window === up)"), "undefined|false");
}

#[test]
fn the_parent_reaches_the_childs_window_document_and_functions() {
    let t = tree();
    ev(&t.child, "function childFn(a) { return 'child-fn:' + a + ':' + document.title; } var counter = 0;");
    // Speedometer's runner: frame.contentWindow[name](), contentDocument.querySelector(...)
    assert_eq!(ev(&t.parent, "child.childFn('x')"), "child-fn:x:child-title");
    assert_eq!(ev(&t.parent, "child.document.title + '|' + child.document.querySelector('#c').textContent"), "child-title|child text");
    assert_eq!(ev(&t.parent, "child.counter = 5; child.counter"), "5");
    assert_eq!(ev(&t.child, "counter"), "5", "a write from the parent is the child's global");
    assert_eq!(ev(&t.child, "up.document.title + '|' + (up.child === window)"), "parent-title|true");
}

#[test]
fn a_child_node_held_by_the_parent_is_the_childs_node() {
    let t = tree();
    // Chrome 148: true,false,true,true,false,child text
    assert_eq!(
        ev(
            &t.parent,
            "var n = child.document.getElementById('c'); \
             [n instanceof child.HTMLElement, n instanceof HTMLElement, n.ownerDocument === child.document, \
              n.ownerDocument.defaultView === child, child.Array === Array, n.textContent].join()"
        ),
        "true,false,true,true,false,child text"
    );
    // Written from the parent, read in the child.
    ev(&t.parent, "n.textContent = 'written by parent'; n.setAttribute('data-from', 'parent'); child.document.getElementById('i').value = 'typed';");
    assert_eq!(
        ev(&t.child, "var c = document.getElementById('c'); c.textContent + '|' + c.dataset.from + '|' + document.getElementById('i').value"),
        "written by parent|parent|typed"
    );
    assert_ne!(t.child.take_dirty(), DomDirty::Clean, "the write marked the child's document for layout");
    assert_eq!(t.parent.take_dirty(), DomDirty::Clean, "and not the parent's");
}

#[test]
fn an_event_made_in_the_parent_reaches_a_listener_in_the_child() {
    let t = tree();
    ev(&t.child, "var heard = []; document.getElementById('c').addEventListener('ping', function (e) { heard.push(e.type + ':' + e.bubbles + ':' + (e.target === this) + ':' + (e instanceof Event)); }); document.addEventListener('ping', function () { heard.push('bubbled'); });");
    // PageElement.dispatchEvent: `new Event(...)` is the parent's Event.
    // Chrome 148: the listener runs; `e instanceof Event` in the child is false.
    assert_eq!(ev(&t.parent, "child.document.getElementById('c').dispatchEvent(new Event('ping', { bubbles: true }))"), "true");
    assert_eq!(ev(&t.child, "heard.join()"), "ping:true:true:false,bubbled");
    assert_eq!(ev(&t.parent, "child.document.getElementById('c').click(); 'clicked'"), "clicked");
}

#[test]
fn each_page_runs_its_own_timers() {
    let t = tree();
    ev(&t.parent, "var ticks = []; setTimeout(function () { ticks.push('parent'); }, 10);");
    ev(&t.child, "var ticks = []; setTimeout(function () { ticks.push('child'); }, 10); requestAnimationFrame(function () { ticks.push('child frame'); });");
    t.parent.run_timers(100, 100).unwrap();
    assert_eq!(ev(&t.parent, "ticks.join()"), "parent");
    assert_eq!(ev(&t.child, "ticks.join()"), "", "the parent's clock does not run the child's timers");
    t.child.run_timers(100, 100).unwrap();
    assert_eq!(ev(&t.child, "ticks.join()"), "child,child frame");
    // A timer the parent sets through the child's window is the child's.
    ev(&t.parent, "child.setTimeout(function () { child.ticks.push('set by parent'); }, 10);");
    t.parent.run_timers(100, 100).unwrap();
    assert_eq!(ev(&t.child, "ticks.length"), "2");
    t.child.run_timers(100, 100).unwrap();
    assert_eq!(ev(&t.child, "ticks[2]"), "set by parent");
}

#[test]
fn promises_cross_the_boundary_on_the_one_job_queue() {
    let t = tree();
    ev(&t.child, "function later(v) { return Promise.resolve(v).then(function (x) { return x + ' via child'; }); }");
    ev(&t.parent, "var got = 'pending'; child.later('value').then(function (v) { got = v; });");
    assert_eq!(ev(&t.parent, "got"), "value via child");
}

#[test]
fn a_runaway_loop_in_the_child_is_stopped_whoever_called_it() {
    let t = tree();
    ev(&t.child, "function spin() { var n = 0; while (true) { n++; } }");
    t.parent.set_loop_iteration_limit(10_000);
    let stopped = ev(&t.parent, "var caught = 'not caught'; try { child.spin(); } catch (e) { caught = 'caught'; } caught");
    assert!(stopped.starts_with("ERROR"), "the stop is not catchable: {stopped}");
    assert_eq!(ev(&t.parent, "1 + 1"), "2", "the parent runs again");
    assert_eq!(ev(&t.child, "document.title"), "child-title", "and so does the child");
}

#[test]
fn an_uncaught_error_in_the_child_is_the_childs_and_leaves_both_pages_running() {
    let t = tree();
    ev(&t.child, "function boom() { throw new RangeError('child broke'); }");
    // Chrome 148: child broke:true:false
    assert_eq!(ev(&t.parent, "var seen; try { child.boom(); } catch (e) { seen = e.message + ':' + (e instanceof child.RangeError) + ':' + (e instanceof RangeError); } seen"), "child broke:true:false");
    assert_eq!(ev(&t.child, "typeof boom"), "function");
}

#[test]
fn dropping_the_child_leaves_the_parent_working() {
    let Tree { parent, child } = tree();
    ev(&child, "var kept = { from: 'child' };");
    ev(&parent, "var held = child.kept; var heldNode = child.document.getElementById('c');");
    assert!(parent.drop_child(child));
    assert_eq!(ev(&parent, "held.from + '|' + document.title"), "child|parent-title", "an object it held outlives the child's page");
    // A second child takes the first one's place.
    let again = parent.new_child().unwrap();
    again.set_document(Rc::new(Document::parse_html(CHILD).unwrap())).unwrap();
    parent.expose_window("child", &again).unwrap();
    assert_eq!(ev(&parent, "child.document.title + '|' + typeof child.kept"), "child-title|undefined");
}

#[test]
fn ten_children_in_one_runtime() {
    let parent = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    parent.set_document(Rc::new(Document::parse_html(PARENT).unwrap())).unwrap();
    let started = std::time::Instant::now();
    let children: Vec<DomBindings> = (0..10)
        .map(|i| {
            let child = parent.new_child().unwrap();
            child.set_document(Rc::new(Document::parse_html(CHILD).unwrap())).unwrap();
            ev(&child, &format!("var me = {i};"));
            parent.expose_window(&format!("frame{i}"), &child).unwrap();
            child
        })
        .collect();
    eprintln!("ten child pages: {} ms", started.elapsed().as_millis());
    assert_eq!(ev(&parent, "[frame0.me, frame4.me, frame9.me, frame9.document.title].join()"), "0,4,9,child-title");
    assert_eq!(ev(&children[3], "me"), "3");
}
