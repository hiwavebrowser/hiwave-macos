//! `unhandledrejection` and `rejectionhandled`: a promise rejected with no handler by the end of
//! its task's microtasks is announced at the Window, and one that gets a handler later is
//! announced again.
//!
//! The expected values are the pinned Chrome's (CfT 148.0.7778.216, headless, 2026-11-15; three
//! runs gave the same file): the probe page `docs/diagnostics/2026-11-15/unhandled_rejection.html`
//! with `unhandled_rejection_page.js` was run there by `unhandled_rejection_chrome.mjs`, and its
//! answers are `unhandled_rejection_cft148.json`. The tests run the same script here, one timer a
//! turn, and compare scenario by scenario. What Chrome does:
//!   - the event comes after the microtasks of the task in which the promise was rejected, in the
//!     order the promises were rejected, with a microtask checkpoint after each event;
//!   - it is a trusted, cancelable `PromiseRejectionEvent` at the Window (it does not bubble; a
//!     listener on the document hears nothing) whose `promise` and `reason` are the page's own;
//!   - a promise that got a handler in the same task, or in one of its microtasks, has no event;
//!   - `preventDefault()`, or `false` returned from `onunhandledrejection`, keeps the rejection
//!     out of the console; everything else is reported after its listeners ran;
//!   - a handler added in a later task brings one `rejectionhandled` (not cancelable), after that
//!     task's microtasks; a handler added inside the `unhandledrejection` listener brings none.
//!
//! Where this engine still answers otherwise is listed in [`DIFFERENT`], each with its reason.

use super::*;

const SCRIPT: &str = include_str!("../../../docs/diagnostics/2026-11-15/unhandled_rejection_page.js");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-15/unhandled_rejection_cft148.json");

/// Scenarios that do not answer as Chrome does, and why. Every other scenario of the probe must.
const DIFFERENT: &[(&str, &str)] = &[
    (
        "order.script",
        "Chrome queues the event as a task, behind a 0 ms timer the same task had already set; here the event \
         comes at the end of the turn, before that timer",
    ),
    ("order.raf", "as order.script: the 0 ms timer set in the frame callback runs after the event here, before it in Chrome"),
    (
        "listeners.order",
        "an on<type> handler runs after the listeners here, whenever it was assigned (every event type); \
         Chrome runs it in the order it was first assigned among them",
    ),
    (
        "listeners.elsewhere",
        "an onunhandledrejection attribute on <body> is the Window's handler here (HTML's body forwarding); \
         Chrome 148 does not forward this one",
    ),
];

fn object(json: &str) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::from_str(json).expect("json") {
        serde_json::Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

fn page() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html("<!doctype html><html><head></head><body></body></html>").unwrap()))
        .unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => panic!("{other:?}"),
    }
}

/// Run the page's timers one a turn (each turn ends with its microtasks, as a task does) until
/// `done` is true. The bound is a count of turns, not a time.
fn turns_until(b: &DomBindings, done: &str) {
    for _ in 0..5000 {
        if ev(b, &format!("String(!!({done}))")) == "true" {
            return;
        }
        b.run_timers(3_600_000, 1).unwrap();
    }
    panic!("`{done}` was not true after 5000 turns");
}

/// What the engine reported as an unhandled rejection, as a browser's console names it: an
/// error's message, else the value's text.
fn reported(b: &DomBindings) -> Vec<String> {
    b.take_error_reports()
        .into_iter()
        .filter(|r| r.rejection)
        .map(|r| r.message.unwrap_or_else(|| r.value.to_string()))
        .collect()
}

/// The probe here: each scenario's answer, and the rejections reported, in order.
fn ours() -> (serde_json::Map<String, serde_json::Value>, Vec<String>) {
    let b = page();
    b.set_error_details(true, true).unwrap();
    b.evaluate(SCRIPT).unwrap();
    turns_until(&b, "window.__out");
    (object(&ev(&b, "window.__out")), reported(&b))
}

#[test]
fn rejection_events_come_as_in_chrome() {
    let pin = object(CHROME);
    assert_eq!(pin["browser"], "148.0.7778.216");
    let chrome = pin["out"].as_object().expect("out");
    let (ours, _) = ours();
    let mut wrong = Vec::new();
    let mut fixed = Vec::new();
    for (scenario, want) in chrome {
        let got = ours.get(scenario);
        let listed = DIFFERENT.iter().any(|(name, _)| name == scenario);
        match (got == Some(want), listed) {
            (true, false) | (false, true) => {}
            (true, true) => fixed.push(scenario.as_str()),
            (false, false) => {
                wrong.push(format!("{scenario}: chrome {want}, here {}", got.map_or("nothing".to_string(), |g| g.to_string())))
            }
        }
    }
    assert!(wrong.is_empty(), "{} of {} scenarios differ from Chrome:\n{}", wrong.len(), chrome.len(), wrong.join("\n"));
    assert!(fixed.is_empty(), "these now answer as Chrome does; take them off DIFFERENT: {fixed:?}");
    for (name, _) in DIFFERENT {
        assert!(chrome.contains_key(*name), "{name} is not a scenario of the probe");
    }
}

/// The console's side: what Chrome reported as uncaught (`pageErrors`), less the one exception a
/// listener threw, is the rejections reported here, in the same order. A rejection whose event
/// was canceled is in neither.
#[test]
fn a_rejection_is_reported_unless_its_event_was_canceled() {
    let pin = object(CHROME);
    let chrome: Vec<String> = pin["pageErrors"]
        .as_array()
        .expect("pageErrors")
        .iter()
        .map(|m| m.as_str().expect("text").to_string())
        .filter(|m| m != "listener-threw")
        .collect();
    assert!(chrome.contains(&"reported-N".to_string()) && chrome.contains(&"L4".to_string()), "{chrome:?}");
    for canceled in ["prevented-P", "prevented-F", "L2"] {
        assert!(!chrome.contains(&canceled.to_string()), "{canceled} was canceled in Chrome: {chrome:?}");
    }
    let (_, ours) = ours();
    assert_eq!(ours, chrome);
}

/// The events do not wait for an embedder that records errors: with error details off (the
/// default) a listener still hears the rejection, and nothing is kept for a report.
#[test]
fn the_events_fire_with_error_reports_off() {
    let b = page();
    b.evaluate(
        "var log = []; \
         window.addEventListener('unhandledrejection', function (e) { log.push('unhandled:' + e.reason); }); \
         window.addEventListener('rejectionhandled', function (e) { log.push('handled:' + e.reason); }); \
         var p = Promise.reject('r1'); Promise.resolve().then(function () { log.push('micro'); }); log.push('sync');",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "sync,micro,unhandled:r1");
    b.evaluate("p.catch(function () { log.push('caught'); });").unwrap();
    assert_eq!(ev(&b, "log.join()"), "sync,micro,unhandled:r1,caught,handled:r1");
    assert!(b.take_error_reports().is_empty());
}

/// A rejection that is handled before its task ends is not reported and has no event; one whose
/// event nobody cancels is reported once.
#[test]
fn reports_are_as_before_for_a_page_with_no_listener() {
    let b = page();
    b.set_error_details(true, true).unwrap();
    b.evaluate("Promise.reject(new Error('handled')).catch(function () {}); Promise.reject(new RangeError('left'));").unwrap();
    assert_eq!(reported(&b), ["left"]);
    assert!(reported(&b).is_empty(), "reported once");
    // A handler added later does not report it again, and takes nothing back.
    b.evaluate("Promise.reject(new Error('late')).catch(function () {});").unwrap();
    assert!(reported(&b).is_empty());
}

/// The dispatch is the engine's own: the event reaches the Window's listeners, trusted, whatever
/// a page has put in place of the constructors and methods it could have gone through. (What is
/// replaced here is what announcing a rejection could newly reach; the event builder and the
/// listener walk it shares with every other engine event have their own tests.)
#[test]
fn the_dispatch_does_not_go_through_what_a_page_replaced() {
    let b = page();
    b.evaluate(
        "var calls = 0, seen = []; function spy() { calls++; throw new Error('replaced'); } \
         window.addEventListener('unhandledrejection', function (e) { \
             seen.push([e.type, e.isTrusted, e.reason, e.promise === p, String(e)].join(':')); }); \
         window.PromiseRejectionEvent = spy; window.Event = spy; window.dispatchEvent = spy; \
         EventTarget.prototype.dispatchEvent = spy; \
         Reflect.apply = spy; Reflect.construct = spy; \
         WeakMap.prototype.set = spy; WeakSet.prototype.add = spy; WeakSet.prototype.delete = spy; \
         Promise.prototype.then = spy; Promise.prototype.catch = spy; Promise.resolve = spy; Array.prototype.push = [].push; \
         var p = Promise.reject('r');",
    )
    .unwrap();
    assert_eq!(ev(&b, "seen.join() + ' calls=' + calls"), "unhandledrejection:true:r:true:[object PromiseRejectionEvent] calls=0");
}

/// A frame's rejection is announced at the frame's Window, and the page's at the page's.
#[test]
fn a_rejection_is_announced_in_the_document_whose_script_rejected() {
    let parent = page();
    let child = parent.new_child().unwrap();
    child
        .set_document(Rc::new(Document::parse_html("<html><head></head><body></body></html>").unwrap()))
        .unwrap();
    parent.expose_window("child", &child).unwrap();
    parent.evaluate("var log = []; window.addEventListener('unhandledrejection', function (e) { log.push('page:' + e.reason); });").unwrap();
    child
        .evaluate("var log = []; window.addEventListener('unhandledrejection', function (e) { log.push('frame:' + e.reason + ':' + (e.target === window)); });")
        .unwrap();
    child.evaluate("Promise.reject('in-frame');").unwrap();
    parent.evaluate("Promise.reject('in-page');").unwrap();
    assert_eq!(ev(&child, "log.join()"), "frame:in-frame:true");
    assert_eq!(ev(&parent, "log.join()"), "page:in-page");
}

/// A page that rejects in every listener call is stopped by a count, and the turn ends.
#[test]
fn a_listener_that_rejects_again_each_time_does_not_hold_the_turn() {
    let b = page();
    b.evaluate(
        "var n = 0; window.addEventListener('unhandledrejection', function (e) { e.preventDefault(); n++; Promise.reject(n); }); \
         Promise.reject(0);",
    )
    .unwrap();
    let first: f64 = ev(&b, "String(n)").parse().unwrap();
    assert!((1.0..=64.0).contains(&first), "{first} events in the first turn");
    // The rest come with later turns: none is lost.
    b.evaluate("void 0").unwrap();
    let second: f64 = ev(&b, "String(n)").parse().unwrap();
    assert!(second > first, "{second} after another turn, {first} before");
}
