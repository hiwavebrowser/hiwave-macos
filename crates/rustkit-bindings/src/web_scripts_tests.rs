//! Portable pins for `web_scripts.js` (#712): HTML `createElementNS` and
//! `cloneNode` of a not-yet-started script prepare on insert; SVG scripts
//! and copies of already-started / `innerHTML` scripts do not.
//!
//! Headless engine coverage lives in `script_loading_tests` (network +
//! shield). These bindings-only cases prove the prepare markers without
//! macOS or `--features headless`.

use super::*;
use rustkit_dom::Document;
use std::rc::Rc;

fn bound(html: &str) -> DomBindings {
    let bindings = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    bindings
        .set_document(Rc::new(Document::parse_html(html).unwrap()))
        .unwrap();
    bindings
        .set_location(&Url::parse("https://site.test/page").unwrap())
        .unwrap();
    bindings
}

fn ev(bindings: &DomBindings, script: &str) -> String {
    match bindings.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(b) => b.to_string(),
        JsValue::Number(n) => n.to_string(),
        JsValue::Null => "null".to_string(),
        JsValue::Undefined => "undefined".to_string(),
        other => panic!("{script} evaluated to {other:?}"),
    }
}

#[test]
fn create_element_ns_html_script_runs_when_inserted() {
    let b = bound("<!DOCTYPE html><html><body></body></html>");
    assert_eq!(
        ev(
            &b,
            "window.mark = []; \
             var s = document.createElementNS('http://www.w3.org/1999/xhtml', 'script'); \
             s.textContent = \"window.mark.push('ns')\"; \
             document.body.appendChild(s); \
             mark.join()"
        ),
        "ns"
    );
}

#[test]
fn create_element_ns_svg_script_is_not_prepared() {
    let b = bound("<!DOCTYPE html><html><body></body></html>");
    assert_eq!(
        ev(
            &b,
            "window.mark = []; \
             var s = document.createElementNS('http://www.w3.org/2000/svg', 'script'); \
             s.textContent = \"window.mark.push('svg')\"; \
             document.body.appendChild(s); \
             mark.length + ',' + s.namespaceURI"
        ),
        "0,http://www.w3.org/2000/svg"
    );
}

#[test]
fn clone_of_a_fresh_script_runs_when_inserted() {
    let b = bound("<!DOCTYPE html><html><body></body></html>");
    assert_eq!(
        ev(
            &b,
            "window.mark = []; \
             var t = document.createElement('script'); \
             t.textContent = \"window.mark.push('clone')\"; \
             document.body.appendChild(t.cloneNode(true)); \
             mark.join()"
        ),
        "clone"
    );
}

#[test]
fn clone_of_a_started_or_innerhtml_script_never_runs() {
    let b = bound("<!DOCTYPE html><html><body></body></html>");
    assert_eq!(
        ev(
            &b,
            "window.mark = []; \
             var ran = document.createElement('script'); \
             ran.textContent = \"window.mark.push('once')\"; \
             document.body.appendChild(ran); \
             document.body.appendChild(ran.cloneNode(true)); \
             var host = document.createElement('div'); \
             host.innerHTML = '<script>window.mark.push(\"html\")</script>'; \
             document.body.appendChild(host.firstChild.cloneNode(true)); \
             mark.join()"
        ),
        "once"
    );
}
