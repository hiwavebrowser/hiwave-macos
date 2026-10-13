//! The Inspector's Console half through real headless loads: console calls,
//! script errors, unhandled rejections, unsupported APIs, the navigation and
//! lifecycle markers, the ring buffer and sinks, the privacy rules, and
//! that nothing is recorded when nobody listens.

use super::*;
use crate::inspector::{scrub_url, CollectingSink, MAX_MESSAGE_CHARS};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

/// One route of the test server.
#[derive(Clone)]
pub(crate) struct Route {
    pub path: &'static str,
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    /// Answer after this long.
    pub delay_ms: u64,
    /// Extra response headers.
    pub headers: Vec<(&'static str, String)>,
}

// Delays, redirects and the request record serve the Network half's tests.
#[allow(dead_code)]
impl Route {
    pub fn ok(path: &'static str, content_type: &'static str, body: impl Into<String>) -> Self {
        Route { path, status: 200, content_type, body: body.into(), delay_ms: 0, headers: Vec::new() }
    }
    pub fn html(path: &'static str, body: impl Into<String>) -> Self {
        Self::ok(path, "text/html", body)
    }
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
    pub fn delay(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }
    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
    /// A 302 to `to`.
    pub fn redirect(path: &'static str, to: impl Into<String>) -> Self {
        Self::ok(path, "text/plain", "").status(302).header("Location", to)
    }
}

#[allow(dead_code)]
pub(crate) struct TestServer {
    pub port: u16,
    /// Request paths, in arrival order (the request line's target, query
    /// included).
    pub requests: Arc<std::sync::Mutex<Vec<String>>>,
    pub hits: Arc<AtomicUsize>,
}

impl TestServer {
    pub fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{path}", self.port)).unwrap()
    }
}

/// Serve `routes` (matched on the path without its query) on 127.0.0.1;
/// anything else is a 404 with a short body.
pub(crate) fn serve(routes: Vec<Route>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let routes = Arc::new(routes);
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let hits = Arc::new(AtomicUsize::new(0));
    let (seen, counted) = (requests.clone(), hits.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (routes, seen, counted) = (routes.clone(), seen.clone(), counted.clone());
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let head = String::from_utf8_lossy(&request).to_string();
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                seen.lock().unwrap().push(target.clone());
                let path = target.split('?').next().unwrap_or("/");
                let route = routes.iter().find(|r| r.path == path).cloned().unwrap_or_else(|| {
                    Route::ok("", "text/plain", "not found").status(404)
                });
                if route.delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(route.delay_ms));
                }
                let reason = match route.status {
                    200 => "OK",
                    302 => "Found",
                    404 => "Not Found",
                    _ => "Status",
                };
                let mut head = format!(
                    "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    route.status,
                    route.content_type,
                    route.body.len()
                );
                for (name, value) in &route.headers {
                    head.push_str(&format!("{name}: {value}\r\n"));
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(route.body.as_bytes());
            });
        }
    });
    TestServer { port, requests, hits }
}

pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

/// An engine whose views keep an Inspector buffer.
pub(crate) fn inspected_engine(config: EngineConfig) -> (Engine, EngineViewId) {
    let mut config = config;
    config.inspector.buffer = true;
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    (engine, view)
}

pub(crate) fn records(engine: &Engine, view: EngineViewId) -> Vec<InspectorRecord> {
    engine.inspector(view).expect("view").snapshot_since(0).records
}

/// One line per record, with what the tests check (no times, no ports).
pub(crate) fn describe(record: &InspectorRecord, port: u16) -> String {
    let local = |s: &str| s.replace(&format!("http://127.0.0.1:{port}"), "");
    let at = |l: &Option<SourceLocation>| match l {
        Some(l) => format!(" @{}:{}:{}", local(&l.url), l.line, l.column),
        None => String::new(),
    };
    match &record.event {
        InspectorEvent::NavigationStart { url } => format!("navigation_start {}", local(url.as_deref().unwrap_or("-"))),
        InspectorEvent::NavigationCommit { url, status } => {
            format!("navigation_commit {} {}", local(url.as_deref().unwrap_or("-")), status.unwrap_or(0))
        }
        InspectorEvent::LoadFinished { scripts_over_budget, .. } => {
            format!("load_finished over_budget={scripts_over_budget}")
        }
        InspectorEvent::LoadEndedEarly { reason, .. } => format!("load_ended_early {reason:?}"),
        InspectorEvent::Console { level, message, location, .. } => {
            format!("console {level:?} {message}{}", at(location))
        }
        InspectorEvent::ScriptError { name, message, location, unhandled_rejection, script, .. } => format!(
            "script_error {name}: {message}{} script={} rejection={unhandled_rejection}",
            at(location),
            local(script)
        ),
        InspectorEvent::UnsupportedApi { interface, member, location } => {
            format!("unsupported_api {interface}.{member}{}", at(location))
        }
        InspectorEvent::FirstLayout => "first_layout".into(),
        InspectorEvent::FirstPaint => "first_paint".into(),
        InspectorEvent::Relayout { cause, .. } => format!("relayout {cause}"),
    }
}

/// The default config, with console messages kept as text (for the tests
/// that follow what a page logged).
fn console_text() -> EngineConfig {
    let mut config = EngineConfig::default();
    config.inspector.keep_console_text = true;
    config
}

fn load_page(config: EngineConfig, routes: Vec<Route>) -> (Engine, EngineViewId, TestServer) {
    let server = serve(routes);
    let (mut engine, view) = inspected_engine(config);
    runtime().block_on(engine.load_url(view, server.url("/"))).expect("load_url");
    (engine, view, server)
}

fn described(engine: &Engine, view: EngineViewId, port: u16) -> Vec<String> {
    records(engine, view).iter().map(|r| describe(r, port)).collect()
}

/// Only the records of the kinds named.
fn only(lines: &[String], kinds: &[&str]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| kinds.iter().any(|k| l.split(' ').next() == Some(*k)))
        .cloned()
        .collect()
}

const CONSOLE_PAGE: &str = "<html><body>hi<script>
console.log('one', 2);
console.info('two');
console.warn('three');
console.error('four');
console.debug('five');
</script></body></html>";

#[test]
fn a_page_load_is_recorded_in_order() {
    let (engine, view, server) = load_page(EngineConfig::default(), vec![Route::html("/", CONSOLE_PAGE)]);
    assert_eq!(
        described(&engine, view, server.port),
        vec![
            "navigation_start /",
            "navigation_commit / 200",
            "first_layout",
            "first_paint",
            "relayout initial_load",
            // By default a line is its arguments' types and lengths.
            "console Log <string len=3> <number> @inline#1:2:9",
            "console Info <string len=3> @inline#1:3:9",
            "console Warn <string len=5> @inline#1:4:9",
            "console Error <string len=4> @inline#1:5:9",
            "console Debug <string len=4> @inline#1:6:9",
            "load_finished over_budget=0",
        ]
    );
    let records = records(&engine, view);
    assert!(records.windows(2).all(|w| w[1].seq == w[0].seq + 1), "sequence numbers are consecutive");
    assert!(records.windows(2).all(|w| w[1].t_ms >= w[0].t_ms), "times only go forward");
}

#[test]
fn a_throw_names_its_error_script_line_and_column() {
    let script = "var a = 1;\nfunction boom() {\n  null.x;\n}\nboom();\n";
    let page = "<html><body><script src=\"/app.js?session=SECRET#frag\"></script></body></html>";
    let (engine, view, server) = load_page(
        EngineConfig::default(),
        vec![Route::html("/", page), Route::ok("/app.js", "text/javascript", script)],
    );
    let lines = described(&engine, view, server.port);
    assert_eq!(
        only(&lines, &["script_error"]),
        vec!["script_error TypeError: cannot convert 'null' or 'undefined' to object @/app.js:3:8 script=/app.js rejection=false"]
    );
    let all = serde_json::to_string(&records(&engine, view)).unwrap();
    assert!(!all.contains("SECRET") && !all.contains("frag"), "{all}");
}

#[test]
fn an_unhandled_rejection_is_a_script_error_and_a_handled_one_is_not() {
    let page = "<html><body><script>
Promise.reject(new RangeError('nobody caught this'));
Promise.reject(new Error('caught')).catch(function () {});
</script></body></html>";
    let (engine, view, server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
    assert_eq!(
        only(&described(&engine, view, server.port), &["script_error"]),
        vec!["script_error RangeError: nobody caught this @inline#1:2:16 script=inline#1 rejection=true"]
    );
}

#[test]
fn a_listener_error_names_the_step_that_ran_it_and_where_it_was_written() {
    let page = "<html><body><script>
window.addEventListener('load', function onLoad() {
  var settings = null;
  settings.theme;
});
</script></body></html>";
    let (engine, view, server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
    assert_eq!(
        only(&described(&engine, view, server.port), &["script_error"]),
        vec!["script_error TypeError: cannot convert 'null' or 'undefined' to object @inline#1:4:12 script=event:load rejection=false"]
    );
    // The stack names the page's frames only, not the engine's own glue.
    let stack = records(&engine, view)
        .into_iter()
        .find_map(|r| match r.event {
            InspectorEvent::ScriptError { stack, .. } => stack,
            _ => None,
        })
        .expect("a stack");
    assert_eq!(stack, "    at onLoad (inline#1:4:12)");
}

#[test]
fn an_unsupported_api_is_recorded_once_per_page_with_its_count() {
    let page = "<html><body><script>
for (var i = 0; i < 3; i++) {
  try { new XMLHttpRequest().open('GET', '/x', false); } catch (e) {}
}
try { new DOMParser().parseFromString('<a/>', 'text/xml'); } catch (e) {}
</script></body></html>";
    let (engine, view, server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
    assert_eq!(
        only(&described(&engine, view, server.port), &["unsupported_api"]),
        vec![
            "unsupported_api XMLHttpRequest.open(async=false) @inline#1:3:34",
            "unsupported_api DOMParser.parseFromString(XML) @inline#1:5:38",
        ]
    );
    let counts: Vec<(String, u64)> = engine
        .inspector(view)
        .unwrap()
        .unsupported_apis()
        .into_iter()
        .map(|u| (format!("{}.{}", u.interface, u.member), u.count))
        .collect();
    assert_eq!(
        counts,
        vec![
            ("XMLHttpRequest.open(async=false)".to_string(), 3),
            ("DOMParser.parseFromString(XML)".to_string(), 1)
        ]
    );
}

#[test]
fn scripts_over_the_budget_are_counted_on_load_finished() {
    // The first script spends the budget; the two after it are not started.
    let page = "<html><body><script>var t = Date.now(); while (Date.now() - t < 150) {}</script>\
        <script>console.log('a')</script><script>console.log('b')</script></body></html>";
    let config = EngineConfig { script_budget_ms: 50, ..EngineConfig::default() };
    let (engine, view, server) = load_page(config, vec![Route::html("/", page)]);
    let lines = described(&engine, view, server.port);
    assert_eq!(only(&lines, &["load_finished", "console"]), vec!["load_finished over_budget=2"]);
}

#[test]
fn a_new_navigation_starts_a_new_page_in_the_same_log() {
    let server = serve(vec![
        Route::html("/", "<html><body><script>console.log('first')</script></body></html>"),
        Route::html("/next", "<html><body><script>console.log('second')</script></body></html>"),
    ]);
    let (mut engine, view) = inspected_engine(console_text());
    let rt = runtime();
    rt.block_on(engine.load_url(view, server.url("/"))).unwrap();
    rt.block_on(engine.load_url(view, server.url("/next"))).unwrap();
    let lines = described(&engine, view, server.port);
    assert_eq!(
        only(&lines, &["navigation_start", "console", "load_finished"]),
        vec![
            "navigation_start /",
            "console Log first @inline#1:1:9",
            "load_finished over_budget=0",
            "navigation_start /next",
            "console Log second @inline#1:1:9",
            "load_finished over_budget=0",
        ]
    );
}

#[test]
fn a_document_that_cannot_be_fetched_ends_the_load_early() {
    // A port nobody listens on.
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    let url = Url::parse(&format!("http://127.0.0.1:{closed}/")).unwrap();
    assert!(runtime().block_on(engine.load_url(view, url)).is_err());
    let lines = described(&engine, view, closed);
    assert_eq!(lines, vec!["navigation_start /", "load_ended_early Failed"]);
}

#[test]
fn an_empty_error_response_ends_the_load_early() {
    let server = serve(vec![Route::ok("/", "text/html", "").status(404)]);
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    assert!(runtime().block_on(engine.load_url(view, server.url("/"))).is_err());
    assert_eq!(
        described(&engine, view, server.port),
        vec!["navigation_start /", "load_ended_early Failed"]
    );
}

#[test]
fn load_html_is_a_navigation_with_no_url() {
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    engine.load_html(view, "<html><body>hi</body></html>").unwrap();
    assert_eq!(
        described(&engine, view, 0),
        vec![
            "navigation_start -",
            "navigation_commit - 0",
            "first_layout",
            "first_paint",
            "relayout initial_load",
            "load_finished over_budget=0",
        ]
    );
}

// ---- Off by default, and free when off ----

#[test]
fn with_nobody_listening_nothing_is_recorded() {
    let server = serve(vec![Route::html("/", CONSOLE_PAGE)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let inspector = engine.inspector(view).unwrap();
    assert!(!inspector.is_active());
    runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
    let snapshot = inspector.snapshot_since(0);
    assert!(snapshot.records.is_empty());
    assert_eq!(snapshot.next_seq, 0, "no event was even numbered");
}

#[test]
fn a_sink_alone_turns_recording_on() {
    let server = serve(vec![Route::html("/", CONSOLE_PAGE)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let sink = CollectingSink::default();
    engine.add_inspector_sink(view, Box::new(sink.clone())).unwrap();
    runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
    let streamed: Vec<String> = sink.records().iter().map(|r| describe(r, server.port)).collect();
    assert_eq!(only(&streamed, &["console"]).len(), 5, "{streamed:?}");
    // No buffer was asked for: the sink got them, the buffer kept none.
    assert!(engine.inspector(view).unwrap().snapshot_since(0).records.is_empty());
}

// ---- Reading while the page's script runs ----

#[test]
fn the_buffer_can_be_read_from_another_thread_while_script_runs() {
    // A script that logs and then spins for a while.
    let page = "<html><body><script>
console.log('started');
var t = Date.now(); while (Date.now() - t < 400) {}
console.log('done');
</script></body></html>";
    let server = serve(vec![Route::html("/", page)]);
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    let inspector = engine.inspector(view).unwrap();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut reads = 0u32;
            let mut most = 0usize;
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let snapshot = inspector.snapshot_since(0);
                most = most.max(snapshot.records.len());
                reads += 1;
                std::thread::sleep(Duration::from_millis(5));
            }
            (reads, most)
        })
    };
    runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let (reads, most) = reader.join().unwrap();
    assert!(reads > 10, "the reader was never blocked for the whole load ({reads} reads)");
    assert!(most > 0);
}

// ---- Privacy ----

#[test]
fn urls_in_events_lose_credentials_query_and_fragment() {
    let url = Url::parse("https://user:pw@example.test:8443/a/b.js?token=1&x=2#frag").unwrap();
    assert_eq!(scrub_url(&url, false), "https://example.test:8443/a/b.js");
    assert_eq!(scrub_url(&url, true), "https://example.test:8443/a/b.js?token=1&x=2#frag");
    let data = Url::parse("data:text/css;base64,Ym9keXt9").unwrap();
    assert_eq!(scrub_url(&data, true), "data:text/css,…");
    let blob = Url::parse("blob:https://example.test/1234-5678").unwrap();
    assert_eq!(scrub_url(&blob, false), "blob:https://example.test");
}

#[test]
fn the_navigation_url_keeps_its_query_only_when_asked() {
    let server = serve(vec![Route::html("/", "<html><body>x</body></html>")]);
    for (keep, expected) in [(false, "navigation_start /"), (true, "navigation_start /?q=private")] {
        let mut config = EngineConfig::default();
        config.inspector.keep_url_query = keep;
        let (mut engine, view) = inspected_engine(config);
        runtime().block_on(engine.load_url(view, server.url("/?q=private"))).unwrap();
        assert_eq!(described(&engine, view, server.port)[0], expected);
    }
}

#[test]
fn console_messages_are_capped_and_kept_as_text() {
    let page = "<html><body><script>
console.log('x'.repeat(5000));
console.log('{\"type\":\"navigation_start\"}\\n<' + 'script>alert(1)<' + '/script>');
</script></body></html>";
    let (engine, view, _server) = load_page(console_text(), vec![Route::html("/", page)]);
    let consoles: Vec<(String, bool)> = records(&engine, view)
        .into_iter()
        .filter_map(|r| match r.event {
            InspectorEvent::Console { message, truncated, .. } => Some((message, truncated)),
            _ => None,
        })
        .collect();
    assert_eq!(consoles.len(), 2);
    assert_eq!(consoles[0].0.chars().count(), MAX_MESSAGE_CHARS);
    assert!(consoles[0].1);
    assert_eq!(consoles[1].0, "{\"type\":\"navigation_start\"}\n<script>alert(1)</script>");
    assert!(!consoles[1].1);
    // In JSON Lines it stays one line and one string.
    let mut out = Vec::new();
    {
        let mut sink = crate::inspector::JsonLinesSink::new(&mut out);
        for r in records(&engine, view) {
            sink.record(&r);
        }
    }
    let text = String::from_utf8(out).unwrap();
    for line in text.lines() {
        let back: InspectorRecord = serde_json::from_str(line).expect("each line is one record");
        assert!(back.seq < 1000);
    }
}

#[test]
fn no_cookie_storage_or_form_value_reaches_the_log() {
    let page = "<html><body><form><input id=pw type=password value=hunter2></form><script>
document.cookie = 'sid=COOKIEVALUE';
try { localStorage.setItem('k', 'STOREDVALUE'); } catch (e) {}
document.getElementById('pw').value = 'TYPEDVALUE';
console.log(document.cookie);
console.log(location.href);
console.log(document.getElementById('pw').value);
console.info('stored', localStorage.getItem('k'));
console.log(new Error('see https://x.test/p?token=ERRQUERY'));
</script></body></html>";
    let server = serve(vec![Route::html("/", page).header("Set-Cookie", "server=SETCOOKIEVALUE")]);
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    runtime().block_on(engine.load_url(view, server.url("/?token=HREFQUERY"))).expect("load_url");
    let records = records(&engine, view);
    assert_eq!(
        records.iter().filter(|r| r.event.kind() == "console").count(),
        5,
        "the calls are still recorded"
    );
    let all = serde_json::to_string(&records).unwrap();
    for secret in ["COOKIEVALUE", "STOREDVALUE", "TYPEDVALUE", "hunter2", "SETCOOKIEVALUE", "HREFQUERY", "ERRQUERY"] {
        assert!(!all.contains(secret), "{secret} leaked: {all}");
    }
}


/// A console line is its arguments' types (and a string's length) unless
/// console text is asked for; even then a URL in it loses its query.
#[test]
fn console_text_is_kept_only_when_asked_and_its_urls_are_scrubbed() {
    let page = "<html><body><script>
console.log('https://h.test/p?token=Q', 'x', 12, true, null, undefined, {a: 1}, [1], new TypeError('a secret'));
</script></body></html>";
    let server = serve(vec![Route::html("/", page)]);
    let mut lines = Vec::new();
    for config in [EngineConfig::default(), console_text()] {
        let (mut engine, view) = inspected_engine(config);
        runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
        lines.push(only(&described(&engine, view, server.port), &["console"]));
    }
    assert_eq!(
        lines,
        vec![
            vec!["console Log <string len=24> <string len=1> <number> true null undefined [object Object] [object Array] <TypeError message len=8> @inline#1:2:9"],
            vec!["console Log https://h.test/p x 12 true null undefined [object Object] [object Array] TypeError: a secret @inline#1:2:9"],
        ]
    );
}

// ---- Neighbouring behaviour: what naming the page's scripts must not change ----

/// Page scripts now run under a name (`inline#n`, `script#n`) so positions
/// can name them. The name is deliberately not a URL: a classic script's
/// dynamic `import()` must keep resolving against the document, with the
/// Inspector on or off.
#[test]
fn a_classic_scripts_dynamic_import_still_resolves_against_the_document() {
    let routes = vec![
        Route::html("/", "<html><body><script src=\"/js/app.js\"></script></body></html>"),
        Route::ok("/js/app.js", "text/javascript", "import('./mod.js').then(function (m) { window.got = m.v; });"),
        Route::ok("/mod.js", "text/javascript", "export const v = 'document-relative';"),
        Route::ok("/js/mod.js", "text/javascript", "export const v = 'script-relative';"),
    ];
    for buffer in [false, true] {
        let server = serve(routes.clone());
        let mut config = EngineConfig::default();
        config.inspector.buffer = buffer;
        let mut engine = Engine::new(config).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
            .expect("view");
        runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
        assert_eq!(
            engine.execute_script(view, "String(window.got)").unwrap(),
            r#"String("document-relative")"#,
            "inspector buffer {buffer}"
        );
    }
}

/// A listener that throws while a script dispatches to it (`click()`) is
/// that script's in the Inspector, but the script log records it under the
/// lifecycle step that drains next, as before: the script itself ran, so the
/// load did not lose it.
#[test]
fn a_listener_error_during_a_script_is_not_the_script_throwing() {
    let page = "<html><body><button>b</button><script>
var b = document.querySelector('button');
b.addEventListener('click', function () { throw new Error('cb'); });
b.click();
</script></body></html>";
    let (engine, view, _server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
    let log: Vec<(String, ScriptOutcome)> = engine
        .script_log(view)
        .unwrap()
        .iter()
        .map(|r| (r.source.clone(), r.outcome.clone()))
        .collect();
    assert_eq!(log[0], ("inline#1".to_string(), ScriptOutcome::Ran), "{log:?}");
    assert!(
        log.iter().any(|(source, outcome)| source == "event:DOMContentLoaded"
            && matches!(outcome, ScriptOutcome::Threw(text) if text.starts_with("Error: cb"))),
        "{log:?}"
    );
    let scripts: Vec<String> = records(&engine, view)
        .into_iter()
        .filter_map(|r| match r.event {
            InspectorEvent::ScriptError { script, .. } => Some(script),
            _ => None,
        })
        .collect();
    assert_eq!(scripts, vec!["inline#1".to_string()]);
}

/// The script log's exception text now names the script where Boa used to
/// print `unknown`; the board's counts are unchanged (unhandled rejections
/// and unsupported APIs are Inspector events only, never log records).
#[test]
fn the_script_log_counts_are_the_same_with_the_inspector_on_or_off() {
    let page = "<html><body><script>
null.x;
</script><script>
Promise.reject(new Error('r'));
try { new XMLHttpRequest().open('GET', '/x', false); } catch (e) {}
</script><script src=\"/ext.js\"></script></body></html>";
    let mut logs = Vec::new();
    let server = serve(vec![Route::html("/", page), Route::ok("/ext.js", "text/javascript", "undefined.boom;")]);
    for buffer in [false, true] {
        let mut config = EngineConfig::default();
        config.inspector.buffer = buffer;
        let mut engine = Engine::new(config).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
            .expect("view");
        runtime().block_on(engine.load_url(view, server.url("/"))).unwrap();
        let log: Vec<(String, ScriptOutcome)> = engine
            .script_log(view)
            .unwrap()
            .iter()
            .map(|r| (r.source.clone(), r.outcome.clone()))
            .collect();
        logs.push(log);
    }
    assert_eq!(logs[0], logs[1]);
    assert_eq!(
        logs[0][..2],
        vec![
            (
                "inline#1".to_string(),
                ScriptOutcome::Threw(
                    "TypeError: cannot convert 'null' or 'undefined' to object (inline#1:2:6)\n    at <main> (inline#1:2:6)".into()
                )
            ),
            ("inline#2".to_string(), ScriptOutcome::Ran),
        ]
    );
    // An external script runs as `script#n` either way.
    let (_, ext) = &logs[0][2];
    assert!(matches!(ext, ScriptOutcome::Threw(text) if text.contains("(script#1:1:")), "{ext:?}");
}

// ---- Review R1 (#676): modules, handlers, eval, page code, scrubbing ----

/// A module's exception is one `script_error`, with its stack, after what
/// the module logged, and never an unhandled rejection (the module's
/// evaluation promise is the engine's own). An inline module is `inline#n`.
#[test]
fn a_module_exception_is_one_error_after_its_console_lines() {
    let page = "<html><body><script type=\"module\">
var c = 3;
  console.log('mod-inline-log');
null.modinl;
</script><script type=\"module\" src=\"/mod.js\"></script></body></html>";
    let (engine, view, server) = load_page(
        console_text(),
        vec![
            Route::html("/", page),
            Route::ok("/mod.js", "text/javascript", "var a = 1;\nconsole.log('ext-mod-log');\nvar b = 2;\n  null.modext;\n"),
        ],
    );
    let lines = described(&engine, view, server.port);
    assert_eq!(
        only(&lines, &["console", "script_error"]),
        vec![
            "console Log mod-inline-log @inline#1:3:11",
            "script_error TypeError: cannot convert 'null' or 'undefined' to object @inline#1:4:6 script=inline#1 rejection=false",
            "console Log ext-mod-log @/mod.js:2:9",
            "script_error TypeError: cannot convert 'null' or 'undefined' to object @/mod.js:4:8 script=/mod.js rejection=false",
        ]
    );
    let summary = InspectorSummary::from_records(&records(&engine, view), &[]);
    assert_eq!((summary.script_errors, summary.unhandled_rejections), (2, 0));
    let stacks: Vec<Option<String>> = records(&engine, view)
        .into_iter()
        .filter_map(|r| match r.event {
            InspectorEvent::ScriptError { stack, .. } => Some(stack),
            _ => None,
        })
        .collect();
    assert!(stacks.iter().all(|s| s.is_some()), "every module error has its stack: {stacks:?}");
}

/// An `on*` attribute's handler and `eval` code are in no named script:
/// their console calls and errors have no position (never the caller's),
/// and a handler's error is the script's whose `click()` ran it, recorded
/// before the next script runs (after that script's own console lines).
#[test]
fn handler_and_eval_code_are_never_given_their_callers_position() {
    let page = "<html><body><button onclick=\"var k=1;
  console.log('onclick-log'); null.clickboom;\">b</button><script>var btn = document.querySelector('button');
  btn.click();
console.log('after-click');
eval(\"console.log('in-eval')\");
</script><script>console.log('second-script');</script></body></html>";
    let (engine, view, server) = load_page(console_text(), vec![Route::html("/", page)]);
    let lines = only(&described(&engine, view, server.port), &["console", "script_error"]);
    assert_eq!(
        lines,
        vec![
            "console Log onclick-log",
            "console Log after-click @inline#1:3:9",
            "console Log in-eval",
            "script_error TypeError: cannot convert 'null' or 'undefined' to object script=inline#1 rejection=false",
            "console Log second-script @inline#2:1:9",
        ]
    );
    let all = serde_json::to_string(&records(&engine, view)).unwrap();
    assert!(!all.contains("eval at"), "{all}");
}

/// A throw inside `eval` code has no place (Boa names no script for it),
/// and is not given the `eval` call's; its stack keeps the page's frames.
#[test]
fn an_error_thrown_in_eval_code_has_no_borrowed_place() {
    let page = "<html><body><script>
var x = 1;
  eval('null.inEval');
</script></body></html>";
    let (engine, view, server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
    let errors = only(&described(&engine, view, server.port), &["script_error"]);
    assert_eq!(
        errors,
        vec!["script_error TypeError: cannot convert 'null' or 'undefined' to object script=inline#1 rejection=false"]
    );
    let stack = records(&engine, view)
        .into_iter()
        .find_map(|r| match r.event {
            InspectorEvent::ScriptError { stack, .. } => stack,
            _ => None,
        })
        .unwrap();
    // The `eval(` call, as Boa places calls (at the `(`).
    assert_eq!(stack, "    at <main> (inline#1:3:7)");
}

/// Logging an object runs none of the page's code: the next script sees
/// what it would have with the Inspector off (and in Chrome).
#[test]
fn logging_runs_no_page_code() {
    let page = "<html><body><script>var n = 0; console.log({ toString() { n += 100; return 'side'; } });
var p = new Proxy({}, { get() { n += 1; throw new Error('trap'); } }); console.log(p);
Promise.reject({ toString() { n += 1000; return 'r'; } });</script>
<script>console.info('n seen by script 2 = ' + n);</script></body></html>";
    let (engine, view, server) = load_page(console_text(), vec![Route::html("/", page)]);
    let lines = only(&described(&engine, view, server.port), &["console", "script_error"]);
    assert_eq!(
        lines,
        vec![
            "console Log [object Object] @inline#1:1:20",
            "console Log [object Object] @inline#1:2:80",
            "script_error : [object Object] script=inline#1 rejection=true",
            "console Info n seen by script 2 = 0 @inline#2:1:9",
        ]
    );
}

/// Every URL in error text loses its credentials, query and fragment,
/// wherever it sits: after a quote, inside brackets, after `=`.
#[test]
fn urls_anywhere_in_error_text_are_scrubbed() {
    let page = "<html><body><script>
try { throw new Error(\"fetch failed 'https://u:p@h.test/a?token=SECRET1#f' (https://h.test/b?token=SECRET2) url=https://h.test/c?token=SECRET3.\"); } catch (e) { setTimeout(function () { throw e; }, 0); }
Promise.reject(new Error('reason https://u:p@h.test/d?token=SECRET4'));
</script></body></html>";
    let (engine, view, _server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
    let messages: Vec<String> = records(&engine, view)
        .into_iter()
        .filter_map(|r| match r.event {
            InspectorEvent::ScriptError { message, .. } => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(
        messages,
        vec![
            "reason https://h.test/d",
            "fetch failed 'https://h.test/a' (https://h.test/b) url=https://h.test/c.",
        ]
    );
    let all = serde_json::to_string(&records(&engine, view)).unwrap();
    assert!(!all.contains("SECRET") && !all.contains("u:p@"), "{all}");
}

/// `responseXML` is null in every browser for a response that is not a
/// document: reading it there is not an unsupported API.
#[test]
fn reading_response_xml_of_a_non_document_is_not_unsupported() {
    let page = "<html><body><script>
var x = new XMLHttpRequest(); typeof x.responseXML; x.responseXML === null;
x.open('GET', '/data.json'); x.onload = function () { for (var i = 0; i < 5; i++) x.responseXML; };
x.send();
var y = new XMLHttpRequest(); y.open('GET', '/doc.xml'); y.onload = function () { y.responseXML; y.responseXML; }; y.send();
</script></body></html>";
    let (engine, view, server) = load_page(
        EngineConfig::default(),
        vec![
            Route::html("/", page),
            Route::ok("/data.json", "application/json", "{}"),
            Route::ok("/doc.xml", "application/xml", "<a/>"),
        ],
    );
    assert_eq!(
        only(&described(&engine, view, server.port), &["unsupported_api"]),
        vec!["unsupported_api XMLHttpRequest.responseXML @inline#1:5:100"]
    );
    let counts: Vec<u64> = engine.inspector(view).unwrap().unsupported_apis().iter().map(|u| u.count).collect();
    assert_eq!(counts, vec![2]);
}
