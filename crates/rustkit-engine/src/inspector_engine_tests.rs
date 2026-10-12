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
    pub body: Vec<u8>,
    /// Answer after this long.
    pub delay_ms: u64,
    /// Extra response headers.
    pub headers: Vec<(&'static str, String)>,
}

// Delays, redirects and the request record serve the Network half's tests.
#[allow(dead_code)]
impl Route {
    pub fn ok(path: &'static str, content_type: &'static str, body: impl Into<String>) -> Self {
        Route { path, status: 200, content_type, body: body.into().into_bytes(), delay_ms: 0, headers: Vec::new() }
    }
    pub fn bytes(path: &'static str, content_type: &'static str, body: Vec<u8>) -> Self {
        Route { path, status: 200, content_type, body, delay_ms: 0, headers: Vec::new() }
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
                let _ = stream.write_all(&route.body);
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
        InspectorEvent::NavigationRedirect { from, to, status } => {
            format!("navigation_redirect {} -> {} {status}", local(from), local(to))
        }
        InspectorEvent::Request { id, url, method, resource_type, initiator } => format!(
            "request #{id} {method} {} {} by={}",
            resource_type.as_str(),
            local(url),
            initiator.as_deref().unwrap_or("-")
        ),
        InspectorEvent::Response { id, status, mime, from_cache, .. } => format!(
            "response #{id} {status} {}{}",
            mime.as_deref().unwrap_or("-"),
            if *from_cache { " cache" } else { "" }
        ),
        InspectorEvent::Failure { id, stage, reason, .. } => format!("failure #{id} {} {reason}", stage.as_str()),
    }
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
            "request #1 GET document / by=navigation",
            "response #1 200 text/html",
            "navigation_commit / 200",
            "first_layout",
            "first_paint",
            "relayout initial_load",
            "console Log one 2 @inline#1:2:12",
            "console Info two @inline#1:3:13",
            "console Warn three @inline#1:4:13",
            "console Error four @inline#1:5:14",
            "console Debug five @inline#1:6:14",
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
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    let rt = runtime();
    rt.block_on(engine.load_url(view, server.url("/"))).unwrap();
    rt.block_on(engine.load_url(view, server.url("/next"))).unwrap();
    let lines = described(&engine, view, server.port);
    assert_eq!(
        only(&lines, &["navigation_start", "console", "load_finished"]),
        vec![
            "navigation_start /",
            "console Log first @inline#1:1:12",
            "load_finished over_budget=0",
            "navigation_start /next",
            "console Log second @inline#1:1:12",
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
    assert_eq!(
        lines,
        vec![
            "navigation_start /",
            "request #1 GET document / by=navigation",
            "failure #1 connect could not connect",
            "load_ended_early Failed"
        ]
    );
}

#[test]
fn an_empty_error_response_ends_the_load_early() {
    let server = serve(vec![Route::ok("/", "text/html", "").status(404)]);
    let (mut engine, view) = inspected_engine(EngineConfig::default());
    assert!(runtime().block_on(engine.load_url(view, server.url("/"))).is_err());
    assert_eq!(
        described(&engine, view, server.port),
        vec![
            "navigation_start /",
            "request #1 GET document / by=navigation",
            "response #1 404 text/html",
            "load_ended_early Failed"
        ]
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
    let (engine, view, _server) = load_page(EngineConfig::default(), vec![Route::html("/", page)]);
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
</script></body></html>";
    let (engine, view, _server) = load_page(
        EngineConfig::default(),
        vec![Route::html("/", page).header("Set-Cookie", "server=SETCOOKIEVALUE")],
    );
    let all = serde_json::to_string(&records(&engine, view)).unwrap();
    for secret in ["COOKIEVALUE", "STOREDVALUE", "TYPEDVALUE", "hunter2", "SETCOOKIEVALUE"] {
        assert!(!all.contains(secret), "{secret} leaked: {all}");
    }
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
</script></body></html>";
    let mut logs = Vec::new();
    for buffer in [false, true] {
        let server = serve(vec![Route::html("/", page)]);
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
        logs[0],
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
}
