//! Ordering and timing of the HTML script lists, against a local server.
//!
//! Each case is one of the parser-timeline violations from the reddit replay:
//! an `async` fetch must not hold an inline script or `DOMContentLoaded`;
//! static module imports start together; `fetch()` does not hold
//! `DOMContentLoaded`; a script inserted by the page runs on insertion.

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Server {
    port: u16,
    starts: Arc<Mutex<Vec<(String, Instant)>>>,
}

struct Route {
    path: &'static str,
    content_type: &'static str,
    body: String,
    delay: Duration,
}

fn serve(routes: Vec<Route>) -> Server {
    let routes = Arc::new(routes);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let starts = Arc::new(Mutex::new(Vec::new()));
    let recorded = starts.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let routes = routes.clone();
            let recorded = recorded.clone();
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                loop {
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&request);
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                recorded
                    .lock()
                    .unwrap()
                    .push((path.clone(), Instant::now()));
                let route = routes.iter().find(|route| route.path == path);
                if let Some(route) = route {
                    if !route.delay.is_zero() {
                        std::thread::sleep(route.delay);
                    }
                }
                let content_type = route
                    .map(|route| route.content_type)
                    .unwrap_or("text/plain");
                let body = route.map(|route| route.body.clone()).unwrap_or_default();
                let status = if route.is_some() {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    Server { port, starts }
}

fn route(path: &'static str, content_type: &'static str, body: &str, delay: Duration) -> Route {
    Route {
        path,
        content_type,
        body: body.to_string(),
        delay,
    }
}

fn load(html: &str, mut routes: Vec<Route>) -> (Engine, EngineViewId, Server) {
    routes.insert(0, route("/", "text/html", html, Duration::ZERO));
    let server = serve(routes);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load");
    (engine, view, server)
}

fn js(engine: &mut Engine, view: EngineViewId, source: &str) -> String {
    let value = engine.execute_script(view, source).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn part_ms(order: &str, name: &str) -> u128 {
    for part in order.split(',') {
        let Some((tag, ms)) = part.split_once(':') else {
            continue;
        };
        if tag == name {
            return ms.parse().unwrap_or_else(|_| panic!("bad ms in {order}"));
        }
    }
    panic!("no {name} in {order}");
}

const SLOW: Duration = Duration::from_millis(800);
const IMPORT_DELAY: Duration = Duration::from_millis(250);

/// An `async` script's slow fetch does not delay the following inline script
/// or DOMContentLoaded. The async script still runs, after that event.
#[test]
fn an_async_fetch_does_not_delay_an_inline_script_or_dom_content_loaded() {
    let trace_path = std::env::temp_dir().join(format!(
        "hiwave-schedule-async-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var t0 = Date.now(); var order = [];</script>
<script src="/async.js" async></script>
<script>
order.push('inline:' + (Date.now() - t0));
document.addEventListener('DOMContentLoaded', function () {
  order.push('dcl:' + (Date.now() - t0));
});
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![route(
            "/async.js",
            "text/javascript",
            "order.push('async:' + (Date.now() - t0));",
            SLOW,
        )],
    );
    let order = js(&mut engine, view, "order.join(',')");
    let tags: Vec<&str> = order
        .split(',')
        .map(|part| part.split(':').next().unwrap())
        .collect();
    assert_eq!(tags, ["inline", "dcl", "async"], "{order}");
    assert!(
        part_ms(&order, "inline") < 400,
        "inline waited on the async fetch: {order}"
    );
    assert!(
        part_ms(&order, "dcl") < 400,
        "DOMContentLoaded waited on the async fetch: {order}"
    );
    assert!(
        part_ms(&order, "async") >= 700,
        "async script did not wait for its body: {order}"
    );

    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let dcl_ms = trace["dcl_ms"].as_u64().unwrap();
    let async_at = trace["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|script| {
            script["url"].as_str().unwrap_or("").contains("async.js")
                && script["event"] == "execute"
        })
        .and_then(|script| script["t_ms"].as_u64())
        .expect("async execute");
    assert!(
        async_at >= dcl_ms + 700,
        "async script ran in the parser's blocking stretch: dcl {dcl_ms} execute {async_at}"
    );
    let parser = trace["parser"].as_array().unwrap();
    assert!(
        parser.iter().all(|pause| {
            !(pause["blocked_on"] == "script fetch"
                && pause["target"].as_str().unwrap_or("").contains("async.js"))
        }),
        "async fetch paused the parser: {parser:?}"
    );
}

/// Three static imports of one module are requested together, not after
/// each previous body has been parsed.
#[test]
fn static_module_imports_are_fetched_concurrently() {
    let page = r#"<html><head>
<script type="module">
import './a.js';
import './b.js';
import './c.js';
window.mods = 'ran';
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, server) = load(
        page,
        vec![
            route(
                "/a.js",
                "text/javascript",
                "export const a = 1;",
                IMPORT_DELAY,
            ),
            route(
                "/b.js",
                "text/javascript",
                "export const b = 2;",
                IMPORT_DELAY,
            ),
            route(
                "/c.js",
                "text/javascript",
                "export const c = 3;",
                IMPORT_DELAY,
            ),
        ],
    );
    assert_eq!(js(&mut engine, view, "window.mods"), "ran");
    let starts = server.starts.lock().unwrap();
    let mut times = Vec::new();
    for name in ["/a.js", "/b.js", "/c.js"] {
        let stamp = starts
            .iter()
            .find(|(path, _)| path == name)
            .map(|(_, at)| *at)
            .unwrap_or_else(|| panic!("{name} was not requested: {starts:?}"));
        times.push(stamp);
    }
    let spread = times
        .iter()
        .max()
        .unwrap()
        .saturating_duration_since(*times.iter().min().unwrap());
    assert!(
        spread < Duration::from_millis(150),
        "static imports were not concurrent ({spread:?}): {starts:?}"
    );
}

/// A dynamic import is not held behind an unrelated fetch from the same turn.
#[test]
fn a_dynamic_import_is_not_serialized_behind_an_unrelated_fetch() {
    let page = r#"<html><head>
<script>
fetch('/slow-net');
import('./mod.js');
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, server) = load(
        page,
        vec![
            route("/slow-net", "text/plain", "ok", SLOW),
            route(
                "/mod.js",
                "text/javascript",
                "window.mod = true;",
                Duration::from_millis(50),
            ),
        ],
    );
    assert_eq!(js(&mut engine, view, "String(window.mod)"), "true");
    let starts = server.starts.lock().unwrap();
    let at = |name: &str| {
        starts
            .iter()
            .find(|(path, _)| path == name)
            .map(|(_, stamp)| *stamp)
            .unwrap_or_else(|| panic!("{name} was not requested: {starts:?}"))
    };
    let spread = at("/mod.js").saturating_duration_since(at("/slow-net"));
    let spread = std::cmp::max(
        spread,
        at("/slow-net").saturating_duration_since(at("/mod.js")),
    );
    assert!(
        spread < Duration::from_millis(200),
        "dynamic import waited for the unrelated fetch ({spread:?}): {starts:?}"
    );
}

/// `fetch()` does not delay DOMContentLoaded. The callback runs after it.
#[test]
fn a_fetch_does_not_delay_dom_content_loaded() {
    let page = r#"<html><head>
<script>
var t0 = Date.now();
var order = [];
fetch('/policy').then(function () { order.push('policy:' + (Date.now() - t0)); });
document.addEventListener('DOMContentLoaded', function () {
  order.push('dcl:' + (Date.now() - t0));
});
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(page, vec![route("/policy", "text/plain", "ok", SLOW)]);
    let order = js(&mut engine, view, "order.join(',')");
    let tags: Vec<&str> = order
        .split(',')
        .map(|part| part.split(':').next().unwrap())
        .collect();
    assert_eq!(tags, ["dcl", "policy"], "{order}");
    assert!(
        part_ms(&order, "dcl") < 400,
        "DOMContentLoaded waited on fetch(): {order}"
    );
    assert!(part_ms(&order, "policy") >= 700, "{order}");
}

/// A dynamically inserted inline script runs before the next statement.
#[test]
fn an_inserted_inline_script_runs_synchronously() {
    let page = r#"<html><head>
<script>
var order = [];
var s = document.createElement('script');
s.textContent = "order.push('inserted')";
document.head.appendChild(s);
order.push('after');
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(page, Vec::new());
    assert_eq!(js(&mut engine, view, "order.join(',')"), "inserted,after");
}

/// A dynamically inserted external script runs before the load returns.
#[test]
fn an_inserted_external_script_runs() {
    let page = r#"<html><head>
<script>
var s = document.createElement('script');
s.src = '/ext.js';
document.head.appendChild(s);
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![route(
            "/ext.js",
            "text/javascript",
            "window.ext = true;",
            Duration::ZERO,
        )],
    );
    assert_eq!(js(&mut engine, view, "String(window.ext)"), "true");
}

/// `.async = false` keeps insertion order, and neither script delays
/// DOMContentLoaded. They still run before `load` returns.
#[test]
fn inserted_scripts_with_async_false_run_in_order_after_dom_content_loaded() {
    let page = r#"<html><head>
<script>
var order = [];
document.addEventListener('DOMContentLoaded', function () { order.push('dcl'); });
var slow = document.createElement('script');
slow.src = '/slow.js';
slow.async = false;
var fast = document.createElement('script');
fast.src = '/fast.js';
fast.async = false;
document.head.appendChild(slow);
document.head.appendChild(fast);
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![
            route(
                "/slow.js",
                "text/javascript",
                "order.push('slow');",
                Duration::from_millis(400),
            ),
            route(
                "/fast.js",
                "text/javascript",
                "order.push('fast');",
                Duration::ZERO,
            ),
        ],
    );
    assert_eq!(js(&mut engine, view, "order.join(',')"), "dcl,slow,fast");
}

/// A script inside `<template>` is recorded and does not run until its
/// content is inserted. The insert runs it synchronously.
#[test]
fn a_template_script_is_inert_until_its_content_is_inserted() {
    let trace_path = std::env::temp_dir().join(format!(
        "hiwave-schedule-template-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var order = [];</script>
<template id="t"><script>order.push('templated')</script></template>
<script>
document.body.appendChild(document.getElementById('t').content);
order.push('after');
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(page, Vec::new());
    assert_eq!(js(&mut engine, view, "order.join(',')"), "templated,after");
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let fetched = trace["fetched_scripts"].as_array().unwrap();
    let template = fetched
        .iter()
        .find(|item| item["detail"] == "template")
        .expect("template script missing from the trace");
    assert_eq!(template["executed"], true, "{template}");
}

/// A declarative shadow script runs in tree order, before a following
/// light-DOM script. A head script is in the trace and runs.
#[test]
fn a_shadow_script_and_a_head_script_run_in_tree_order() {
    let trace_path = std::env::temp_dir().join(format!(
        "hiwave-schedule-shadow-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var order = ['head'];</script>
</head><body>
<div><template shadowrootmode="open"><script>order.push('shadow')</script></template></div>
<script>order.push('after')</script>
</body></html>"#;
    let (mut engine, view, _server) = load(page, Vec::new());
    assert_eq!(
        js(&mut engine, view, "order.join(',')"),
        "head,shadow,after"
    );
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let fetched = trace["fetched_scripts"].as_array().unwrap();
    assert!(
        fetched
            .iter()
            .any(|item| item["detail"] == "shadow tree" && item["executed"] == true),
        "{fetched:?}"
    );
    assert!(
        fetched
            .iter()
            .any(|item| item["url"] == "inline#1" && item["executed"] == true),
        "head script missing: {fetched:?}"
    );
}

fn trace_file(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "hiwave-schedule-{name}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// A slow image does not delay an inline script or DOMContentLoaded.
/// It does delay `window` `load`. The remaining stall is an `image` pause.
#[test]
fn an_image_does_not_delay_an_inline_script_or_dom_content_loaded() {
    let trace_path = trace_file("image");
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var t0 = Date.now(); var order = [];</script>
<script>
order.push('inline:' + (Date.now() - t0));
document.addEventListener('DOMContentLoaded', function () {
  order.push('dcl:' + (Date.now() - t0));
});
window.addEventListener('load', function () {
  order.push('load:' + (Date.now() - t0));
});
</script>
</head><body><img src="/slow.png"></body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![route("/slow.png", "image/png", "not-a-png", SLOW)],
    );
    let order = js(&mut engine, view, "order.join(',')");
    let tags: Vec<&str> = order
        .split(',')
        .map(|part| part.split(':').next().unwrap())
        .collect();
    assert_eq!(tags, ["inline", "dcl", "load"], "{order}");
    assert!(
        part_ms(&order, "inline") < 400,
        "inline waited on the image: {order}"
    );
    assert!(
        part_ms(&order, "dcl") < 400,
        "DOMContentLoaded waited on the image: {order}"
    );
    assert!(
        part_ms(&order, "load") >= 700,
        "load did not wait for the image: {order}"
    );
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let parser = trace["parser"].as_array().unwrap();
    assert!(
        parser.iter().any(|pause| pause["blocked_on"] == "image"),
        "image stall was not traced: {parser:?}"
    );
}

/// `import()` starts the fetch when the call runs, before later work in
/// the same script. A computed specifier cannot start any earlier.
#[test]
fn a_dynamic_import_starts_during_the_calling_script() {
    let trace_path = trace_file("import");
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>
import('/mod.js');
var end = Date.now() + 500;
while (Date.now() < end) {}
fetch('/mark');
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, server) = load(
        page,
        vec![
            route(
                "/mod.js",
                "text/javascript",
                "window.mod = true;",
                Duration::from_millis(20),
            ),
            route("/mark", "text/plain", "ok", Duration::ZERO),
        ],
    );
    assert_eq!(js(&mut engine, view, "String(window.mod)"), "true");
    let starts = server.starts.lock().unwrap();
    let at = |name: &str| {
        starts
            .iter()
            .find(|(path, _)| path == name)
            .map(|(_, stamp)| *stamp)
            .unwrap_or_else(|| panic!("{name} was not requested: {starts:?}"))
    };
    let ahead = at("/mark").saturating_duration_since(at("/mod.js"));
    assert!(
        ahead >= Duration::from_millis(300),
        "import() waited until the script finished ({ahead:?}): {starts:?}"
    );
    drop(starts);
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let timeline = trace["module_timeline"].as_array().expect("module_timeline");
    let module = timeline
        .iter()
        .find(|item| item["url"].as_str().unwrap_or("").ends_with("/mod.js"))
        .expect("mod.js missing from module_timeline");
    assert!(module["requested_ms"].is_number(), "{module}");
    assert!(module["start_ms"].is_number(), "{module}");
    assert!(module["body_ms"].is_number(), "{module}");
}

/// An external script consumes an inline slot. The label stays the URL.
/// The slot is `fetched_scripts.inline`, so inline#2 is not a missing node.
#[test]
fn an_external_script_records_the_inline_slot_it_consumed() {
    let trace_path = trace_file("slot");
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var a = 1;</script>
<script src="/ext.js"></script>
<script>var b = 2;</script>
</head><body>hi</body></html>"#;
    let (_engine, _view, _server) = load(
        page,
        vec![route(
            "/ext.js",
            "text/javascript",
            "var c = 3;",
            Duration::ZERO,
        )],
    );
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let fetched = trace["fetched_scripts"].as_array().unwrap();
    assert!(
        fetched
            .iter()
            .any(|item| item["url"] == "inline#1" && item["executed"] == true),
        "{fetched:?}"
    );
    assert!(
        fetched
            .iter()
            .any(|item| item["url"] == "inline#3" && item["executed"] == true),
        "{fetched:?}"
    );
    let external = fetched
        .iter()
        .find(|item| item["url"].as_str().unwrap_or("").ends_with("/ext.js"))
        .expect("external script");
    assert_eq!(external["inline"], 2, "{external}");
}

/// Cloning a template script into a connected shadow root runs it.
/// The original template entry stays unrun.
#[test]
fn a_template_script_cloned_into_a_shadow_root_runs() {
    let trace_path = trace_file("clone");
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var order = [];</script>
<template id="t"><script>order.push('templated')</script></template>
<script>
var host = document.createElement('div');
document.body.appendChild(host);
host.attachShadow({ mode: 'open' }).appendChild(document.getElementById('t').content.cloneNode(true));
order.push('after');
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(page, Vec::new());
    assert_eq!(js(&mut engine, view, "order.join(',')"), "templated,after");
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let fetched = trace["fetched_scripts"].as_array().unwrap();
    let template = fetched
        .iter()
        .find(|item| item["detail"] == "template")
        .expect("template script");
    assert_eq!(template["executed"], false, "{template}");
    // The clone lives in the shadow tree. The late catalog records it there
    // once it has run; a clone that is marked before that catalog is
    // `script-inserted inline`.
    assert!(
        fetched.iter().any(|item| {
            item["executed"] == true
                && (item["detail"] == "shadow tree" || item["detail"] == "script-inserted inline")
        }),
        "{fetched:?}"
    );
}

/// A script inserted into a disconnected shadow runs when the host connects.
#[test]
fn a_script_in_a_disconnected_shadow_runs_when_the_host_connects() {
    let page = r#"<html><head>
<script>
var order = [];
order.push('before');
var host = document.createElement('div');
var root = host.attachShadow({ mode: 'open' });
var s = document.createElement('script');
s.textContent = "order.push('shadow')";
root.appendChild(s);
order.push('mid');
document.body.appendChild(host);
order.push('after');
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(page, Vec::new());
    assert_eq!(
        js(&mut engine, view, "order.join(',')"),
        "before,mid,shadow,after"
    );
}

/// With scripting on, a script inside `<noscript>` does not run.
/// The trace records it. Chrome would not run it either.
#[test]
fn a_noscript_script_does_not_run() {
    let trace_path = trace_file("noscript");
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>var order = [];</script>
<noscript><script>order.push('noscript')</script></noscript>
<script>order.push('after')</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(page, Vec::new());
    assert_eq!(js(&mut engine, view, "order.join(',')"), "after");
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let fetched = trace["fetched_scripts"].as_array().unwrap();
    let noscript = fetched
        .iter()
        .find(|item| item["detail"] == "noscript")
        .expect("noscript script missing from the trace");
    assert_eq!(noscript["executed"], false, "{noscript}");
    assert_eq!(noscript["reason"], "never reached", "{noscript}");
}

/// A parser-inserted module is deferred. Its fetch and its run are not
/// parser pauses. DOMContentLoaded still waits for it, after the classic
/// inline script.
#[test]
fn a_deferred_module_does_not_pause_the_parser() {
    let trace_path = trace_file("defer-mod");
    let _trace = page_trace::force_file_for_test(&trace_path);
    let page = r#"<html><head>
<script>window.t0 = Date.now(); window.order = [];</script>
<script type="module" src="/mod.js"></script>
<script>
window.order.push('inline:' + (Date.now() - window.t0));
document.addEventListener('DOMContentLoaded', function () {
  window.order.push('dcl:' + (Date.now() - window.t0));
});
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![route(
            "/mod.js",
            "text/javascript",
            "window.order.push('mod:' + (Date.now() - window.t0));",
            SLOW,
        )],
    );
    let order = js(&mut engine, view, "window.order.join(',')");
    let tags: Vec<&str> = order
        .split(',')
        .map(|part| part.split(':').next().unwrap())
        .collect();
    assert_eq!(tags, ["inline", "mod", "dcl"], "{order}");
    assert!(
        part_ms(&order, "inline") < 400,
        "inline waited on the module: {order}"
    );
    assert!(
        part_ms(&order, "dcl") >= 700,
        "DOMContentLoaded did not wait for the deferred module: {order}"
    );
    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).expect("trace")).unwrap();
    let _ = std::fs::remove_file(&trace_path);
    let parser = trace["parser"].as_array().unwrap();
    assert!(
        parser.iter().all(|pause| {
            let target = pause["target"].as_str().unwrap_or("");
            !target.contains("mod.js")
        }),
        "deferred module paused the parser: {parser:?}"
    );
}

/// A classic `import()` is not a static dependency of the deferred module
/// that runs next. `DOMContentLoaded` does not wait for that body. Chrome
/// reaches DCL before a chunk the parser did not import statically; the
/// pre-DCL pump must not parse it just because an earlier script asked.
#[test]
fn an_earlier_dynamic_import_does_not_delay_the_deferred_module_or_dom_content_loaded() {
    let page = r#"<html><head>
<script>
window.t0 = Date.now();
window.order = [];
import('./slow.js');
window.order.push('classic:' + (Date.now() - window.t0));
</script>
<script type="module" src="/mod.js"></script>
<script>
document.addEventListener('DOMContentLoaded', function () {
  window.order.push('dcl:' + (Date.now() - window.t0));
});
window.addEventListener('load', function () {
  window.order.push('load:' + (Date.now() - window.t0));
});
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![
            route(
                "/mod.js",
                "text/javascript",
                "window.order.push('mod:' + (Date.now() - window.t0));",
                Duration::ZERO,
            ),
            route(
                "/slow.js",
                "text/javascript",
                "window.order.push('slow:' + (Date.now() - window.t0));",
                SLOW,
            ),
        ],
    );
    let order = js(&mut engine, view, "window.order.join(',')");
    let tags: Vec<&str> = order
        .split(',')
        .map(|part| part.split(':').next().unwrap())
        .collect();
    assert_eq!(tags, ["classic", "mod", "dcl", "slow", "load"], "{order}");
    assert!(
        part_ms(&order, "dcl") < 400,
        "DOMContentLoaded waited on the dynamic import: {order}"
    );
    assert!(
        part_ms(&order, "slow") >= 700,
        "slow.js did not wait out its fetch: {order}"
    );
}

/// `<script type=module async>` is the as-soon-as-possible set. It does not
/// delay DOMContentLoaded, including its static imports.
#[test]
fn an_async_module_does_not_delay_dom_content_loaded() {
    let page = r#"<html><head>
<script>window.t0 = Date.now(); window.order = [];</script>
<script type="module" async src="/slow.js"></script>
<script>
window.order.push('inline:' + (Date.now() - window.t0));
document.addEventListener('DOMContentLoaded', function () {
  window.order.push('dcl:' + (Date.now() - window.t0));
});
</script>
</head><body>hi</body></html>"#;
    let (mut engine, view, _server) = load(
        page,
        vec![route(
            "/slow.js",
            "text/javascript",
            "window.order.push('async:' + (Date.now() - window.t0));",
            SLOW,
        )],
    );
    let order = js(&mut engine, view, "window.order.join(',')");
    let tags: Vec<&str> = order
        .split(',')
        .map(|part| part.split(':').next().unwrap())
        .collect();
    assert_eq!(tags, ["inline", "dcl", "async"], "{order}");
    assert!(
        part_ms(&order, "dcl") < 400,
        "DOMContentLoaded waited on the async module: {order}"
    );
}

/// String-literal `import()` is prefetched, but it does not take a
/// connection ahead of a static import in the same module, and it does
/// not delay DOMContentLoaded.
#[test]
fn dynamic_imports_do_not_delay_a_static_import_or_dom_content_loaded() {
    let page = r#"<html><head>
<script>window.t0 = Date.now();</script>
<script type="module">
import('./d0.js');
import('./d1.js');
import('./d2.js');
import('./d3.js');
import('./d4.js');
import('./d5.js');
import './need.js';
</script>
<script>
document.addEventListener('DOMContentLoaded', function () {
  window.dcl = Date.now() - window.t0;
});
</script>
</head><body>hi</body></html>"#;
    let mut routes = vec![route(
        "/need.js",
        "text/javascript",
        "window.needed = true;",
        Duration::ZERO,
    )];
    for name in ["/d0.js", "/d1.js", "/d2.js", "/d3.js", "/d4.js", "/d5.js"] {
        routes.push(route(name, "text/javascript", "export const n = 1;", SLOW));
    }
    let (mut engine, view, server) = load(page, routes);
    assert_eq!(js(&mut engine, view, "String(window.needed)"), "true");
    let dcl = js(&mut engine, view, "String(window.dcl)")
        .parse::<u128>()
        .expect("dcl");
    assert!(dcl < 400, "DOMContentLoaded waited on dynamic import(): {dcl}");
    let starts = server.starts.lock().unwrap();
    let at = |name: &str| {
        starts
            .iter()
            .find(|(path, _)| path == name)
            .map(|(_, stamp)| *stamp)
    };
    let need = at("/need.js").expect("need.js was not requested");
    for name in ["/d0.js", "/d1.js", "/d2.js", "/d3.js", "/d4.js", "/d5.js"] {
        let stamp = at(name).unwrap_or_else(|| panic!("{name} was not requested: {starts:?}"));
        let behind = stamp.saturating_duration_since(need);
        assert!(
            behind < Duration::from_millis(400),
            "{name} started {behind:?} after need.js; static import was held"
        );
    }
}
