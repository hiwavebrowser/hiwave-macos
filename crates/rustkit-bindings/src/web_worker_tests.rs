//! STR-4 W1: a classic dedicated worker. Expected values are the pinned
//! Chrome's (CfT 148.0.7778.216) on the same page and worker script
//! (`docs/diagnostics/2026-11-12/str4_worker_w1_chrome.mjs`, its output in
//! `str4_worker_w1_cft148.json`).

use super::*;
use crate::net_bridge::NetDelivery;
use std::time::{Duration, Instant};

const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-12/str4_worker_w1_cft148.json");

/// The worker the Chrome measurement ran, minus the `setTimeout` probe (W2).
const WORKER: &str = "\
self.onmessage = function (e) {
  if (e.data === 'close') { postMessage('closing'); self.close(); return; }
  if (e.data === 'info') {
    postMessage({ t: typeof self, doc: typeof document, win: typeof window, name: self.name,
      post: typeof postMessage, ael: typeof addEventListener, imp: typeof importScripts,
      loc: typeof location, nav: typeof navigator });
    return;
  }
  postMessage({ echo: e.data, isEvent: e instanceof MessageEvent, tgt: e.target === self });
};
addEventListener('message', function (e) { if (e.data === 'both') postMessage('listener'); });
postMessage('ready');
";

fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn bound() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html("<html><head></head><body></body></html>").unwrap(),
    ))
    .unwrap();
    b.enable_net_bridge().unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

/// Answer every queued worker-script request with `source`.
fn serve(b: &DomBindings, source: &str) {
    for r in b.take_net_requests() {
        assert_eq!(r.destination, "worker");
        b.deliver_net_response(
            r.id,
            NetDelivery::Response {
                url: r.url,
                status: 200,
                status_text: "OK".into(),
                headers: vec![("content-type".into(), "text/javascript".into())],
                body_b64: b64(source.as_bytes()),
                kind: "basic".into(),
                redirected: false,
            },
        )
        .unwrap();
    }
}

/// Turn the page's timer pump until `done` holds (the page drains its
/// workers' messages there), for at most three seconds.
fn pump_until(b: &DomBindings, done: &str) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        b.run_timers(0, 100).unwrap();
        if ev(b, &format!("String(!!({done}))")) == "true" {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn a_dedicated_worker_answers_as_in_chrome() {
    let b = bound();
    b.evaluate(
        "var res = [], w = new Worker('http://a.test/worker.js'); \
         w.onmessage = function (e) { res.push(e.data); }; \
         w.addEventListener('message', function (e) { if (e.data === 'ready') res.push('ael:ready'); }); \
         w.postMessage({ a: 1, b: [1, 2, 'x'], n: null }); \
         w.postMessage('info'); w.postMessage('both'); w.postMessage('close'); \
         setTimeout(function () { w.postMessage('after-close'); }, 100);",
    )
    .unwrap();
    serve(&b, WORKER);
    assert!(pump_until(&b, "res.length >= 7"), "got {}", ev(&b, "JSON.stringify(res)"));
    // a message after self.close() must not be answered
    b.run_timers(1000, 100).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    b.run_timers(1000, 100).unwrap();
    let mut want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    want["log"][3].as_object_mut().unwrap().remove("st");
    let got: serde_json::Value = serde_json::from_str(&ev(&b, "JSON.stringify(res)")).unwrap();
    assert_eq!(got, want["log"]);
}

#[test]
fn a_terminated_worker_never_answers() {
    let b = bound();
    b.evaluate(
        "var got = [], w = new Worker('http://a.test/worker.js'); \
         w.onmessage = function (e) { got.push(e.data); }; \
         w.terminate(); w.postMessage('x');",
    )
    .unwrap();
    serve(&b, WORKER);
    std::thread::sleep(Duration::from_millis(200));
    b.run_timers(1000, 100).unwrap();
    assert_eq!(ev(&b, "JSON.stringify(got)"), "[]");
    assert_eq!(CHROME.contains("\"afterTerminate\": []"), true);
}

#[test]
fn terminate_stops_a_worker_that_never_returns() {
    let b = bound();
    b.evaluate(
        "var got = [], w = new Worker('http://a.test/spin.js'); \
         w.onmessage = function (e) { got.push(e.data); }; w.postMessage('go');",
    )
    .unwrap();
    serve(&b, "self.onmessage = function () { postMessage('started'); while (true) {} };");
    assert!(pump_until(&b, "got.length >= 1"));
    assert_eq!(b.worker_threads_running(), 1);
    b.evaluate("w.terminate()").unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while b.worker_threads_running() != 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(b.worker_threads_running(), 0);
}

#[test]
fn a_worker_cannot_reach_the_page() {
    let b = bound();
    b.evaluate(
        "window.secret = 1; var got = [], w = new Worker('http://a.test/peek.js'); \
         w.onmessage = function (e) { got.push(e.data); };",
    )
    .unwrap();
    serve(&b, "postMessage([typeof secret, typeof document, typeof window, typeof parent, typeof localStorage].join());");
    assert!(pump_until(&b, "got.length >= 1"));
    assert_eq!(ev(&b, "got[0]"), "undefined,undefined,undefined,undefined,undefined");
}
