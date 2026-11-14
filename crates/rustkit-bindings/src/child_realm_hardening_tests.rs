//! A frame's realm holds to the same rules as the first: the engine's own
//! steps there use the built-ins of realm creation, and the engine's names
//! are not reachable from a frame's page either.

use super::*;

const PARENT: &str = "<html><head><title>parent</title></head><body><p id=p>parent text</p>\
    <iframe id=f width=120 height=40></iframe></body></html>";
const CHILD: &str = "<html><head><title>child</title></head><body><div id=c>child text</div>\
    <canvas id=v></canvas></body></html>";

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script) {
        Ok(JsValue::String(s)) => s,
        Ok(JsValue::Boolean(x)) => x.to_string(),
        Ok(JsValue::Number(n)) => format!("{n}"),
        Ok(other) => format!("{other:?}"),
        Err(e) => format!("ERROR {e}"),
    }
}

fn host(b: &DomBindings, body: &str) -> String {
    match b.host_turn(body, &[]) {
        Ok(JsValue::String(s)) => s,
        Ok(JsValue::Boolean(x)) => x.to_string(),
        Ok(other) => format!("{other:?}"),
        Err(e) => format!("ERROR {e}"),
    }
}

fn parent() -> (DomBindings, Rc<Document>) {
    let parent = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html(PARENT).unwrap());
    parent.set_document(doc.clone()).unwrap();
    parent.enable_frames().unwrap();
    (parent, doc)
}

fn child_of(parent: &DomBindings) -> DomBindings {
    let child = parent.new_child().unwrap();
    child.set_document(Rc::new(Document::parse_html(CHILD).unwrap())).unwrap();
    child
}

/// A test-only predicate on the window: is this the realm's engine object?
fn give_probe(b: &DomBindings) {
    host(b, "Object.defineProperty(globalThis, 'isEngines', { value: function (o) { return o === __rk; } }); return '';");
}

/// The first use of a lazily installed interface, after the frame's page
/// has replaced `Function.prototype.call` and `apply`: the replacements
/// see only the page's own objects, and the interface answers.
#[test]
fn a_lazy_interface_installs_with_the_built_ins_of_realm_creation() {
    let (parent, _doc) = parent();
    let child = child_of(&parent);
    give_probe(&child);
    ev(
        &child,
        "var engines = 0, apply = Reflect.apply; \
         function watch(name) { var real = Function.prototype[name]; \
             Function.prototype[name] = function () { \
                 for (var i = 0; i < arguments.length; i++) { \
                     var a = arguments[i]; \
                     if (isEngines(a)) engines++; \
                     if (name === 'apply' && a && typeof a === 'object' && typeof a.length === 'number') \
                         for (var j = 0; j < a.length; j++) if (isEngines(a[j])) engines++; \
                 } \
                 return apply(real, this, arguments); }; } \
         watch('call'); watch('apply'); 0",
    );
    let answers = ev(
        &child,
        "[new Intl.NumberFormat('en').format(1234.5), typeof ReadableStream, typeof document.getElementById('v').getContext('2d'), \
          typeof document.getElementById('c').animate].join('|')",
    );
    assert_eq!(answers, "1,234.5|function|object|function");
    assert_eq!(ev(&child, "String(engines)"), "0", "engine objects among the arguments");
}

/// The scan for the parser's iframes and a frame's size are read with the
/// built-ins of install time: none of the page's replacements runs.
#[test]
fn the_frame_scan_and_size_run_no_page_function() {
    let (parent, doc) = parent();
    ev(
        &parent,
        "var ran = 0; function count() { ran++; return []; } \
         Array.prototype.forEach = count; Document.prototype.querySelectorAll = count; \
         Element.prototype.querySelectorAll = count; Element.prototype.getAttribute = count; \
         Object.defineProperty(Node.prototype, 'isConnected', { get: function () { ran++; return false; }, configurable: true }); \
         Object.defineProperty(HTMLElement.prototype, 'style', { get: function () { ran++; return null; }, configurable: true }); 0",
    );
    parent.scan_frames().unwrap();
    let node = doc.get_element_by_id("f").unwrap().id.raw();
    let requests: Vec<usize> = parent.take_frame_requests().into_iter().map(|r| r.node).collect();
    let size = parent.frame_size(node);
    assert_eq!(ev(&parent, "String(ran)"), "0", "page functions run by the scan and the size");
    assert_eq!((requests, size), (vec![node], (120.0, 40.0)));
}

/// A frame's `top` is the tree's top window, whatever the parent's page
/// has defined as its own `top`, and attaching runs no page function.
#[test]
fn a_frames_top_is_the_trees_top_window() {
    let (parent, doc) = parent();
    parent.scan_frames().unwrap();
    let node = doc.get_element_by_id("f").unwrap().id.raw();
    let _ = parent.take_frame_requests();
    ev(
        &parent,
        "var ran = 0, fake = { fake: true }; \
         Object.defineProperty(window, 'top', { get: function () { ran++; return fake; }, configurable: true }); 0",
    );
    let child = child_of(&parent);
    assert!(parent.attach_frame(node, &child).unwrap());
    assert_eq!(ev(&parent, "String(ran)"), "0", "page functions run while attaching");
    assert_eq!(ev(&parent, "String(document.getElementById('f').contentWindow.top === window)"), "true");
    assert_eq!(ev(&child, "String(top === parent && top !== window && frameElement !== null)"), "true");
}

/// Making a frame's realm calls no array method of the first realm.
#[test]
fn making_a_frames_realm_calls_no_page_function() {
    let (parent, _doc) = parent();
    ev(
        &parent,
        "var ran = 0; Array.prototype.push = function () { ran++; return 0; }; \
         for (var i = 0; i < 8; i++) Object.defineProperty(Array.prototype, i, { get: function () { ran++; }, set: function () { ran++; }, configurable: true }); \
         ran = 0; 0",
    );
    let child = child_of(&parent);
    assert_eq!(ev(&parent, "String(ran)"), "0", "page functions run while the frame's realm was made");
    // The frame's own events are events of the tree.
    assert_eq!(
        ev(&child, "var e = new Event('x', { cancelable: true }); e.preventDefault(); String(e.defaultPrevented)"),
        "true"
    );
}

/// `stopPropagation()`, `stopImmediatePropagation()` and `composedPath()`
/// work on an event of one window dispatched in the other, and a stop does
/// not outlive the dispatch.
#[test]
fn stopping_and_the_path_work_across_windows() {
    let (parent, _doc) = parent();
    let child = child_of(&parent);
    parent.expose_window("child", &child).unwrap();
    ev(
        &child,
        "var heard = [], c = document.getElementById('c'); \
         c.addEventListener('ping', function (e) { heard.push('c:' + e.composedPath().length); e.stopPropagation(); }); \
         document.addEventListener('ping', function () { heard.push('doc'); }); \
         c.addEventListener('now', function (e) { heard.push('first'); e.stopImmediatePropagation(); }); \
         c.addEventListener('now', function () { heard.push('second'); }); 0",
    );
    // The parent's event (its prototype is the parent's) in the child's tree.
    assert_eq!(
        ev(
            &parent,
            "var c = child.document.getElementById('c'); var e = new Event('ping', { bubbles: true }); \
             c.dispatchEvent(e); c.dispatchEvent(new Event('now', { bubbles: true })); \
             var p = document.getElementById('p'), again = []; \
             p.addEventListener('ping', function () { again.push('p'); }); \
             document.addEventListener('ping', function () { again.push('doc'); }); \
             p.dispatchEvent(e); child.heard.join() + '|' + again.join() + '|' + e.composedPath().length",
        ),
        "c:5,first|p,doc|0"
    );
}

/// The engine's names are not reachable from a frame's page: the walk of
/// `internal_names_tests`, run inside a frame's realm, with the frame
/// attached.
#[test]
fn a_frames_page_reaches_none_of_the_engines_names() {
    let parent = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html(PARENT).unwrap());
    parent.set_document(doc.clone()).unwrap();
    parent.set_location(&Url::parse("https://example.test/page").unwrap()).unwrap();
    parent.enable_frames().unwrap();
    parent.enable_net_bridge().unwrap();
    parent.scan_frames().unwrap();
    let _ = parent.take_frame_requests();
    let child = parent.new_child().unwrap();
    child.set_document(Rc::new(Document::parse_html(internal_names_tests::PAGE).unwrap())).unwrap();
    child.set_location(&Url::parse("https://example.test/frame").unwrap()).unwrap();
    child.enable_frames().unwrap();
    child.enable_net_bridge().unwrap();
    assert!(parent.attach_frame(doc.get_element_by_id("f").unwrap().id.raw(), &child).unwrap());
    assert_eq!(ev(&child, internal_names_tests::WALK), "");
    // And from the parent, through the frame's window.
    assert_eq!(ev(&parent, internal_names_tests::WALK), "");
}
