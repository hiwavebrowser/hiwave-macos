//! The engine's own key, wheel and input events are built with the
//! constructors of install time, whatever a page has put on the window
//! since, and only an event the engine made is marked trusted.

use super::*;

const PAGE: &str = r#"<!DOCTYPE html><html><body><input id="t" value="v"></body></html>"#;

const PAGE_SCRIPT: &str = r#"
var pageMade = [], seen = [];
function Fake(type) { var e = new Event(type); pageMade.push(e); return e; }
window.KeyboardEvent = Fake;
window.WheelEvent = Fake;
window.InputEvent = Fake;
var t = document.getElementById('t');
['keydown', 'wheel', 'input'].forEach(function (type) {
    t.addEventListener(type, function (e) {
        seen.push(type + ':' + Object.prototype.toString.call(e) + ':' + e.isTrusted + ':' + (pageMade.indexOf(e) >= 0));
    });
});
"#;

fn text(v: JsValue) -> String {
    match v {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

#[test]
fn engine_events_come_from_install_time_constructors() {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html(PAGE).unwrap());
    let id = doc.get_element_by_id("t").unwrap().id.raw();
    b.set_document(doc).unwrap();
    b.evaluate(PAGE_SCRIPT).unwrap();
    b.host_turn(&format!("return __rk.fireKey({id}, 'keydown', {{ key: 'a', code: 'KeyA' }});"), &[]).unwrap();
    b.host_turn(&format!("return __rk.fireWheel({id}, {{ clientX: 1, clientY: 1, deltaX: 0, deltaY: 10 }}, true);"), &[]).unwrap();
    b.host_turn(&format!("return __rk.fireInput({id}, 'x');"), &[]).unwrap();
    let seen = text(b.evaluate("seen.join('\\n')").unwrap());
    assert_eq!(
        seen,
        "keydown:[object KeyboardEvent]:true:false\nwheel:[object WheelEvent]:true:false\ninput:[object InputEvent]:true:false",
        "what the page's listeners saw"
    );
    let page_trusted = text(b.evaluate("String(pageMade.filter(function (e) { return e.isTrusted; }).length)").unwrap());
    assert_eq!(page_trusted, "0", "page-made events read trusted");
}
