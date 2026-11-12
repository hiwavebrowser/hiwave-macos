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

// ---- W2: timers, importScripts, uncaught errors, load failure.
// Expected values: str4_worker_w2_cft148.json (docs/diagnostics/2026-11-12/).

const CHROME_W2: &str = include_str!("../../../docs/diagnostics/2026-11-12/str4_worker_w2_cft148.json");

fn w2_file(path: &str) -> Option<&'static str> {
    Some(match path {
        "/timers.js" => {
            "var order = [];
             setTimeout(function () { order.push('t20'); }, 20);
             setTimeout(function () { order.push('t0'); }, 0);
             var n = 0, iv = setInterval(function () { order.push('i' + (++n)); if (n === 2) clearInterval(iv); }, 5);
             var c = setTimeout(function () { order.push('cancelled'); }, 1); clearTimeout(c);
             Promise.resolve().then(function () { order.push('micro'); });
             setTimeout(function (a, b) { order.push('args:' + a + b); postMessage(order); }, 60, 'x', 'y');"
        }
        "/lib.js" => "var fromLib = 'lib-ran'; function libFn() { return 42; }",
        "/lib2.js" => "var fromLib2 = 'lib2-ran';",
        "/imp.js" => {
            "var r = [];
             importScripts('/lib.js');
             r.push(fromLib, libFn());
             importScripts('/lib.js', '/lib2.js');
             r.push(typeof fromLib2, importScripts());
             try { importScripts('/missing.js'); } catch (e) { r.push('missing:' + e.name); }
             postMessage(r);"
        }
        "/throws.js" => "onmessage = function () { throw new Error('boom'); }; postMessage('up');",
        "/throwsnow.js" => "postMessage('before'); throw new TypeError('top-level');",
        "/syntax.js" => "var = ;",
        "/err.js" => "self.onerror = function () { postMessage('self.onerror'); }; postMessage('ok');",
        _ => return None,
    })
}

/// Run the Chrome measurement's page for `key` and return what it logged.
fn observe(key: &str, file: &str, post: Option<&str>) -> serde_json::Value {
    let b = bound();
    let post = post.map(|p| format!("w.postMessage('{p}');")).unwrap_or_default();
    b.evaluate(&format!(
        "var log = [], w = new Worker('http://a.test{file}'); \
         w.onmessage = function (e) {{ log.push(['m', e.data]); }}; \
         w.onerror = function (e) {{ log.push(['e', e.message === undefined ? null : e.message, typeof e.filename, \
           typeof e.lineno, e instanceof ErrorEvent, e.defaultPrevented]); e.preventDefault(); }}; {post}"
    ))
    .unwrap();
    let end = Instant::now() + Duration::from_millis(500);
    while Instant::now() < end {
        for r in b.take_net_requests() {
            let path = r.url.trim_start_matches("http://a.test").to_string();
            let (status, body) = match w2_file(&path) {
                Some(s) => (200, s),
                None => (404, "no"),
            };
            b.deliver_net_response(
                r.id,
                NetDelivery::Response {
                    url: r.url,
                    status,
                    status_text: "".into(),
                    headers: vec![],
                    body_b64: b64(body.as_bytes()),
                    kind: "basic".into(),
                    redirected: false,
                },
            )
            .unwrap();
        }
        b.run_timers(0, 100).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    let got: serde_json::Value = serde_json::from_str(&ev(&b, "JSON.stringify(log)")).unwrap();
    let _ = key;
    got
}

fn chrome(key: &str) -> serde_json::Value {
    let all: serde_json::Value = serde_json::from_str(CHROME_W2).unwrap();
    all[key].clone()
}

#[test]
fn worker_timers_fire_in_chromes_order() {
    assert_eq!(observe("timers", "/timers.js", None), chrome("timers"));
}

#[test]
fn import_scripts_runs_scripts_in_the_worker_scope_and_fails_with_network_error() {
    assert_eq!(observe("imp", "/imp.js", None), chrome("imp"));
}

#[test]
fn an_uncaught_error_in_a_handler_reaches_the_worker_object_and_the_worker_lives_on() {
    assert_eq!(observe("throws", "/throws.js", Some("go")), chrome("throws"));
}

#[test]
fn an_uncaught_error_at_the_top_level_reaches_the_worker_object() {
    assert_eq!(observe("throwsnow", "/throwsnow.js", None), chrome("throwsnow"));
}

#[test]
fn a_syntax_error_in_the_worker_script_is_an_error_event() {
    let got = observe("syntax", "/syntax.js", None);
    let want = chrome("syntax");
    // Chrome's message names its own parser's token; the shape is what is pinned.
    assert_eq!(got.as_array().unwrap().len(), 1);
    let (g, w) = (&got[0], &want[0]);
    assert!(g[1].as_str().unwrap().starts_with("Uncaught SyntaxError"), "{g}");
    assert_eq!(g.as_array().unwrap()[2..], w.as_array().unwrap()[2..]);
}

#[test]
fn a_script_that_fails_to_load_is_a_plain_error_event() {
    assert_eq!(observe("nofile", "/nofile.js", None), chrome("nofile"));
}

#[test]
fn the_worker_scope_error_handler_sees_nothing_when_nothing_throws() {
    assert_eq!(observe("selferr", "/err.js", None), chrome("selferr"));
}
