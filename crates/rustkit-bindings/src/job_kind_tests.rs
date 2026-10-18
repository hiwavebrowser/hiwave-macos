//! Task versus microtask. A scheduler that yields with `MessageChannel` or
//! `setTimeout(0)` must not be handed a microtask: that yield would run
//! again inside the same checkpoint. These tests check the kind, not the
//! full event-loop step (one task, then a checkpoint, then a rendering
//! opportunity), which is described in `docs/event-loop-gap.md`.

use super::*;

fn page() -> DomBindings {
    let bindings = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    bindings
        .set_document(Rc::new(
            Document::parse_html("<!DOCTYPE html><html><body></body></html>").unwrap(),
        ))
        .unwrap();
    bindings
        .set_location(&Url::parse("https://app.test/").unwrap())
        .unwrap();
    bindings
}

fn ev(bindings: &DomBindings, script: &str) -> String {
    match bindings.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(b) => b.to_string(),
        JsValue::Number(n) => {
            if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                n.to_string()
            }
        }
        JsValue::Null => "null".to_string(),
        JsValue::Undefined => "undefined".to_string(),
        other => panic!("{script} evaluated to {other:?}"),
    }
}

#[test]
fn timers_messages_frames_and_idle_callbacks_are_not_microtasks() {
    let b = page();
    b.evaluate(
        "var log = [];
         setTimeout(function timerCb() { log.push('timeout'); }, 0);
         setInterval(function intervalCb() { log.push('interval'); }, 0);
         var ch = new MessageChannel();
         ch.port1.onmessage = function messageCb() { log.push('message'); };
         ch.port2.postMessage(null);
         postMessage('w', '*');
         addEventListener('message', function windowMsg() { log.push('window'); });
         requestAnimationFrame(function frameCb() { log.push('frame'); });
         requestIdleCallback(function idleCb() { log.push('idle'); });
         queueMicrotask(function microCb() { log.push('micro'); });
         Promise.resolve().then(function reactionCb() { log.push('reaction'); });
         log.push('sync');",
    )
    .unwrap();
    let census = b.last_job_census();
    assert_eq!(ev(&b, "log.join()"), "sync,micro,reaction");
    assert!(census.promise >= 1, "{census:?}");
    assert!(census.count_labeled("queueMicrotask") >= 1, "{census:?}");
    assert!(census.count_labeled("reactionCb") >= 1, "{census:?}");
    assert_eq!(census.count_labeled("timerCb"), 0, "{census:?}");
    assert_eq!(census.count_labeled("messageCb"), 0, "{census:?}");
    assert_eq!(census.count_labeled("frameCb"), 0, "{census:?}");
    assert_eq!(census.count_labeled("idleCb"), 0, "{census:?}");

    // The load clock runs due timers, including a 0 ms animation frame and
    // the idle task, in this host call. None of them were microtasks of the
    // script above.
    b.run_timers(16, 100).unwrap();
    let after = ev(&b, "log.join()");
    for name in ["timeout", "interval", "message", "window", "frame", "idle"] {
        assert!(after.contains(name), "{name} missing from {after}");
    }
}

#[test]
fn mutation_delivery_is_a_microtask_of_the_write() {
    let b = page();
    b.evaluate(
        "var n = 0;
         var el = document.createElement('div');
         document.body.appendChild(el);
         var mo = new MutationObserver(function moCb() { n++; });
         mo.observe(el, { childList: true });
         el.appendChild(document.createElement('span'));",
    )
    .unwrap();
    let census = b.last_job_census();
    assert_eq!(ev(&b, "String(n)"), "1");
    assert!(census.promise >= 1, "{census:?}");
    assert_eq!(
        census.count_labeled("moCb"),
        0,
        "the observer runs from notify, not as its own job: {census:?}"
    );
    assert!(
        census.count_labeled("notify") >= 1,
        "delivery is the notify microtask: {census:?}"
    );
}

#[test]
fn fetch_and_xhr_completions_are_not_microtasks_of_the_starter() {
    let b = page();
    b.enable_net_bridge().unwrap();
    b.evaluate(
        "var log = [];
         fetch('https://app.test/a').then(function fetchThen() { log.push('fetch'); });
         var x = new XMLHttpRequest();
         x.open('GET', 'https://app.test/b');
         x.onload = function xhrLoad() { log.push('xhr'); };
         x.send();",
    )
    .unwrap();
    let during = b.last_job_census();
    assert_eq!(ev(&b, "log.join()"), "");
    assert_eq!(during.count_labeled("fetchThen"), 0, "{during:?}");
    assert_eq!(during.count_labeled("xhrLoad"), 0, "{during:?}");
    let queued = b.take_net_requests();
    assert_eq!(queued.len(), 2, "{queued:?}");
    let mut fetch_census = None;
    for request in queued {
        let is_fetch = request.url.ends_with("/a");
        b.deliver_net_response(
            request.id,
            NetDelivery::Response {
                url: request.url,
                status: 200,
                status_text: "OK".into(),
                headers: vec![("content-type".into(), "text/plain".into())],
                body_b64: "eA==".into(),
                kind: "basic".into(),
                redirected: false,
            },
        )
        .unwrap();
        if is_fetch {
            fetch_census = Some(b.last_job_census());
        }
    }
    assert_eq!(ev(&b, "log.slice().sort().join()"), "fetch,xhr");
    let census = fetch_census.expect("fetch was delivered");
    assert!(
        census.count_labeled("fetchThen") >= 1,
        "the fetch reaction is a microtask of the delivery task: {census:?}"
    );
}

/// The public Bv concat, evaluated as a module against a small SML stub.
/// The Mac gap is the checkpoint that resumes whoever was waiting on this
/// body; this measures the checkpoint the chunk's own factories produce.
/// Skipped when the chunk files are not on disk (they are not in the repo).
#[test]
fn reddit_bv_chunks_report_a_job_census() {
    let dir = std::path::Path::new("/tmp/reddit-chunks");
    let names = ["BvOWmaNPEJ.js", "DZDBx0nCO6.js", "Dyg3q1gB6W.js"];
    if names.iter().any(|name| !dir.join(name).is_file()) {
        eprintln!("BV_CENSUS skipped (chunk files not in /tmp/reddit-chunks)");
        return;
    }
    let mut source = String::new();
    for name in names {
        source.push_str(&std::fs::read_to_string(dir.join(name)).unwrap());
        source.push_str("\n/* @sml-chunk */\n");
    }
    let b = page();
    b.evaluate(
        r#"
        var defined = Object.create(null);
        var pending = Object.create(null);
        var started = Object.create(null);
        var errors = [];
        function smlThen(id, ns) { defined[id] = ns || {}; satisfy(); }
        function smlFail(id, e) { errors.push(id + ': ' + e); defined[id] = { error: String(e) }; satisfy(); }
        function satisfy() {
            var again = true;
            while (again) {
                again = false;
                for (var id in pending) {
                    if (started[id]) continue;
                    var m = pending[id];
                    var ready = true;
                    for (var i = 0; i < m.deps.length; i++) {
                        if (!defined[m.deps[i]]) ready = false;
                    }
                    if (!ready) continue;
                    started[id] = true;
                    again = true;
                    var args = [];
                    for (var j = 0; j < m.deps.length; j++) args.push(defined[m.deps[j]]);
                    m.factory.apply(null, args).then(smlThen.bind(null, id), smlFail.bind(null, id));
                }
            }
        }
        globalThis.SML = {
            dm: function (id, deps, factory) {
                pending[id] = { deps: deps || [], factory: factory };
                satisfy();
            },
            di: function () {}
        };
        globalThis.CLIENT_CONFIG = {
            STATIC_BASE_URL: 'https://www.redditstatic.com/',
            REDDITSTATIC_CF_MIGRATION: 'control_1'
        };
        "#,
    )
    .unwrap();
    rustkit_js::reset_host_calls();
    let url = "https://www.redditstatic.com/js/concat?prefix=en-US/&chunks=BvOWmaNPEJ,DZDBx0nCO6,Dyg3q1gB6W";
    let started = std::time::Instant::now();
    let handle = b.begin_module(url, &source).expect("module parsed");
    let begin_census = b.last_job_census();
    let state = b.poll_module(&handle);
    let poll_census = b.last_job_census();
    let defined = ev(&b, "Object.keys(defined).join(',')");
    let errors = ev(&b, "errors.join(' | ')");
    let host = rustkit_js::top_host_calls(20);
    let host_line = host
        .iter()
        .map(|source| format!("{}×{}/{}ms", source.label, source.count, source.elapsed_ms))
        .collect::<Vec<_>>()
        .join(",");
    eprintln!(
        "BV_CENSUS bytes={} eval_ms={} state={state:?} defined={defined} errors={errors} begin=[{}] poll=[{}] host=[{}]",
        source.len(),
        started.elapsed().as_millis(),
        begin_census.summary(),
        poll_census.summary(),
        if host_line.is_empty() { "-".to_string() } else { host_line }
    );
    assert!(
        poll_census.jobs > 0 || begin_census.jobs > 0 || !defined.is_empty() || !errors.is_empty(),
        "the chunk produced neither jobs nor a factory result"
    );
}

#[test]
fn a_class_write_is_named_by_the_host_binding() {
    rustkit_js::reset_host_calls();
    let b = page();
    b.evaluate(
        r#"
        var el = document.createElement('div');
        document.body.appendChild(el);
        el.setAttribute('class', 'a');
        el.setAttribute('style', 'color:red');
        "#,
    )
    .unwrap();
    let top = rustkit_js::top_host_calls(20);
    assert!(
        top.iter().any(|source| source.label == "__rustkit_dom_write:setAttr:class"),
        "setAttribute('class') is its own host row: {top:?}"
    );
    assert!(
        top.iter().any(|source| source.label == "__rustkit_dom_write:setAttr:style"),
        "setAttribute('style') is its own host row: {top:?}"
    );
}
