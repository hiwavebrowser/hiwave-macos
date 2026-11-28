//! Portable STRESS-SPEED WINDOW-PATH pins (#1444 / batch 126).
//!
//! The engine oracle in `window_event_path_tests` needs `feature = "headless"`, so default
//! Linux `--lib` never saw that dispatch stops at the Window without named-access walks.
//! These run the same Chrome 148 counts through DomBindings and count `named_walks()`.

use super::*;

/// The oracle's dispatch script (Chrome for Testing 148.0.7778.216 win64,
/// docs/diagnostics/2026-11-24/window_event_path_cft148.json).
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

fn bound(body: &str) -> DomBindings {
    let page = format!("<!doctype html><body>{body}</body>");
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html(&page).unwrap()))
        .unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(x) => x.to_string(),
        JsValue::Number(n) => format!("{n}"),
        JsValue::Undefined => "undefined".into(),
        JsValue::Null => "null".into(),
        other => format!("{other:?}"),
    }
}

fn run(body: &str) -> (String, u64) {
    let b = bound(body);
    let before = crate::dom::named_walks();
    assert_eq!(ev(&b, DISPATCH), "dispatched");
    let walks = crate::dom::named_walks() - before;
    (ev(&b, READ), walks)
}

#[test]
fn an_event_that_reaches_the_window_does_not_walk_the_document() {
    let (got, walks) = run("<div id=a><button id=b onclick=\"n.inline++\">x</button></div>");
    assert_eq!(
        got, "100 50 50 50 50 undefined undefined",
        "listener counts and window reads, as Chrome"
    );
    assert_eq!(
        walks, 0,
        "100 dispatches that reach the Window walked the document {walks} times"
    );
}

#[test]
fn the_windows_named_access_still_answers_an_element_called_parent_node() {
    let (got, walks) = run(
        "<div id=parentNode></div><form name=nodeType id=nt></form>\
         <div id=a><button id=b onclick=\"n.inline++\">x</button></div>",
    );
    assert_eq!(
        got, "100 50 50 50 50 parentNode nt",
        "listener counts and window reads, as Chrome"
    );
    assert_eq!(
        walks, 0,
        "100 dispatches that reach the Window walked the document {walks} times"
    );
}
