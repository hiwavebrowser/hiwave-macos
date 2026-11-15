//! The iframe `sandbox` attribute on a frame request (HTML "sandboxing": a frame's flags are the
//! attribute's when its document is created, by a navigation; a later change of the attribute
//! waits for the next navigation).
//!
//! A frame request carries the attribute's value as it was when the page asked for the
//! navigation. It is read from the document by the request's host function, not by page script:
//! what the page has put on the element or the prototypes does not change it, and neither does a
//! change of the attribute after the request.
//!
//! A frame in the parser's markup is asked for when the engine scans the document, which is
//! after the page's first scripts. The pinned Chrome (CfT 148.0.7778.216, headless,
//! `docs/diagnostics/2026-11-15/frame_sandbox_{parent.html,child.html,page.js,chrome.mjs,cft148.json}`,
//! rows `markup.*`) starts that navigation at the parse: an attribute a script removes at once
//! still counts (`mk_removed`). So the attribute as parsed counts for a markup frame's first
//! request, with what the element has then.

use super::*;

const PIN: &str = include_str!("../../../docs/diagnostics/2026-11-15/frame_sandbox_cft148.json");

fn page(markup: &str) -> (DomBindings, Rc<Document>) {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.enable_frames().unwrap();
    let doc = Rc::new(Document::parse_html(&format!("<html><head></head><body>{markup}</body></html>")).unwrap());
    b.set_document(doc.clone()).unwrap();
    (b, doc)
}

fn run(b: &DomBindings, script: &str) {
    b.evaluate(script).unwrap_or_else(|e| panic!("{script}: {e}"));
}

/// The `sandbox` of each request with a URL, in the order made.
fn asked(b: &DomBindings) -> Vec<Option<String>> {
    b.take_frame_requests().into_iter().filter(|r| r.url.is_some()).map(|r| r.sandbox).collect()
}

fn some(value: &str) -> Option<String> {
    Some(value.to_string())
}

/// Chrome's row for a frame, by group and name: was a document reachable from the page.
fn chrome_has_document(group: &str, name: &str) -> bool {
    let pin: serde_json::Value = serde_json::from_str(PIN).unwrap();
    match pin[group][name]["contentDocument"].as_str() {
        Some("object") => true,
        Some("null") => false,
        other => panic!("{group}.{name}: no contentDocument row ({other:?})"),
    }
}

#[test]
fn a_request_carries_the_attribute_value() {
    let (b, _) = page("");
    run(
        &b,
        "['', 'allow-scripts', 'allow-same-origin', 'allow-scripts allow-same-origin', null].forEach(function (v) { \
           var f = document.createElement('iframe'); if (v !== null) f.setAttribute('sandbox', v); document.body.appendChild(f); });",
    );
    assert_eq!(
        asked(&b),
        vec![some(""), some("allow-scripts"), some("allow-same-origin"), some("allow-scripts allow-same-origin"), None]
    );
}

#[test]
fn a_frame_in_the_markup_carries_it_too() {
    let (b, _) = page("<iframe sandbox></iframe><iframe></iframe><iframe sandbox='allow-forms'></iframe>");
    b.scan_frames().unwrap();
    assert_eq!(asked(&b), vec![some(""), None, some("allow-forms")]);
}

/// The value is the one at the request. Chrome (`afterLoad.removed_changed`, `added_changed`):
/// the document made before the change stays as it was.
#[test]
fn a_change_after_the_request_does_not_change_it() {
    let (b, _) = page("");
    run(
        &b,
        "var a = document.createElement('iframe'); a.setAttribute('sandbox', ''); document.body.appendChild(a); \
         var n = document.createElement('iframe'); document.body.appendChild(n); \
         a.removeAttribute('sandbox'); n.setAttribute('sandbox', '');",
    );
    assert!(!chrome_has_document("afterLoad", "removed_changed") && chrome_has_document("afterLoad", "added_changed"));
    assert_eq!(asked(&b), vec![some(""), None]);
}

/// Chrome (`afterLoad.removed_navigated`, `added_navigated`): the next navigation takes the
/// attribute as it is then.
#[test]
fn the_next_navigation_takes_the_attribute_as_it_is_then() {
    let (b, _) = page("");
    run(
        &b,
        "var a = document.createElement('iframe'); a.setAttribute('sandbox', ''); document.body.appendChild(a); \
         var n = document.createElement('iframe'); document.body.appendChild(n);",
    );
    let _ = b.take_frame_requests();
    run(&b, "a.removeAttribute('sandbox'); n.setAttribute('sandbox', 'allow-scripts'); a.src = '/x.html'; n.src = '/y.html';");
    assert!(chrome_has_document("afterLoad", "removed_navigated") && !chrome_has_document("afterLoad", "added_navigated"));
    assert_eq!(asked(&b), vec![None, some("allow-scripts")]);
}

/// The request reads the document, not the page's functions: built-ins the page replaced and
/// accessors it defined are not asked.
#[test]
fn the_attribute_is_read_from_the_document_not_through_page_script() {
    let (b, _) = page("<iframe id=m sandbox></iframe>");
    run(
        &b,
        "window.ran = 0; function no() { ran++; return null; } function never() { ran++; return false; } \
         Element.prototype.getAttribute = no; Element.prototype.hasAttribute = never; \
         Element.prototype.getAttributeNode = no; Element.prototype.getAttributeNames = function () { ran++; return []; }; \
         Object.defineProperty(HTMLIFrameElement.prototype, 'sandbox', { get: no, set: no, configurable: true }); \
         Object.defineProperty(Element.prototype, 'attributes', { get: no, configurable: true }); \
         String.prototype.toLowerCase = function () { ran++; return 'src'; }; \
         var f = document.createElement('iframe'); \
         Object.defineProperty(f, 'sandbox', { get: no, set: no }); \
         Element.prototype.setAttribute.call(f, 'sandbox', 'allow-scripts'); \
         ran = 0; document.body.appendChild(f);",
    );
    b.scan_frames().unwrap();
    assert_eq!(asked(&b), vec![some("allow-scripts"), some("")]);
    assert!(matches!(b.evaluate("ran"), Ok(JsValue::Number(n)) if n == 0.0), "page functions ran for the request");
}

/// A markup frame's first request, made at the engine's scan: the attribute as parsed counts
/// (Chrome `markup.mk_removed`: no document), and one the page set since counts (Chrome
/// `markup.mk_added_src`: set, then navigated: no document).
#[test]
fn a_markup_frames_first_request_counts_the_attribute_as_parsed() {
    let (b, _) = page("<iframe id=r sandbox='allow-forms'></iframe><iframe id=s></iframe>");
    run(
        &b,
        "document.getElementById('r').removeAttribute('sandbox'); \
         var s = document.getElementById('s'); s.setAttribute('sandbox', ''); s.src = '/x.html';",
    );
    b.scan_frames().unwrap();
    assert!(!chrome_has_document("markup", "mk_removed") && !chrome_has_document("markup", "mk_added_src"));
    // `s` asked first (its `src` was set by the script), `r` at the scan.
    assert_eq!(asked(&b), vec![some(""), some("allow-forms")]);
}

/// Only the first: once a markup frame has been asked for, the attribute as parsed is spent
/// (Chrome `afterLoad.removed_navigated`).
#[test]
fn the_attribute_as_parsed_counts_once() {
    let (b, _) = page("<iframe id=r sandbox></iframe>");
    b.scan_frames().unwrap();
    assert_eq!(asked(&b), vec![some("")]);
    run(&b, "var r = document.getElementById('r'); r.removeAttribute('sandbox'); r.src = '/x.html';");
    assert_eq!(asked(&b), vec![None]);
    // A new document starts again from its own markup.
    b.set_document(Rc::new(Document::parse_html("<html><head></head><body><iframe></iframe></body></html>").unwrap())).unwrap();
    b.scan_frames().unwrap();
    assert_eq!(asked(&b), vec![None]);
}

/// Known difference, recorded: a markup frame the page's first script gives the attribute to,
/// without navigating it. Chrome (`markup.mk_added`) had started the navigation at the parse,
/// so the frame has its document; here the request is made at the scan and carries the
/// attribute.
#[test]
fn known_difference_an_attribute_set_on_a_markup_frame_before_the_scan_counts() {
    let (b, _) = page("<iframe id=s></iframe>");
    run(&b, "document.getElementById('s').setAttribute('sandbox', '');");
    b.scan_frames().unwrap();
    assert!(chrome_has_document("markup", "mk_added"), "Chrome's row changed: revisit this difference");
    assert_eq!(asked(&b), vec![some("")]);
}

/// A frame that left the document: the request names no navigation and carries nothing.
#[test]
fn a_removed_frames_request_carries_nothing() {
    let (b, _) = page("");
    run(&b, "var f = document.createElement('iframe'); f.setAttribute('sandbox', ''); document.body.appendChild(f); f.remove();");
    let requests = b.take_frame_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!((requests[1].url.clone(), requests[1].sandbox.clone()), (None, None));
}
