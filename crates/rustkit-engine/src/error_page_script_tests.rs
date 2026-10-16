//! An HTTP error response's page runs like any other document: the body of a
//! 403, a 404 or a 503 gets the same script, network, cookie and navigation
//! behaviour as a 200's. Only the status it was served with differs.
//!
//! Until 2026-10-16 a script such a page inserted (`<script src>` appended
//! by an inline script) was never fetched, `location.reload()` did nothing,
//! `<meta http-equiv=refresh>` was ignored and the engine kept no cookies,
//! so a page that checks the browser and then reloads itself stayed on its
//! first answer. None of that was specific to error pages; the fixtures
//! here are error pages because that is where it showed.

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;

/// What the server answers for a path, given the request's Cookie header.
pub(crate) type Handler = fn(path: &str, cookie: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String);

pub(crate) struct Server {
    port: u16,
    /// (request target, Cookie header) in arrival order.
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl Server {
    pub(crate) fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{path}", self.port)).unwrap()
    }
    pub(crate) fn requests(&self) -> Vec<(String, String)> {
        self.seen.lock().unwrap().clone()
    }
}

pub(crate) fn serve(handler: Handler) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let log = log.clone();
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&request).to_string();
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let cookie = head
                    .lines()
                    .find_map(|l| l.split_once(':').filter(|(n, _)| n.eq_ignore_ascii_case("cookie")))
                    .map(|(_, v)| v.trim().to_string())
                    .unwrap_or_default();
                log.lock().unwrap().push((target.clone(), cookie.clone()));
                let path = target.split('?').next().unwrap_or("/");
                let (status, content_type, headers, body) = handler(path, &cookie);
                let mut out = format!(
                    "HTTP/1.1 {status} Status\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
                    body.len()
                );
                for (name, value) in headers {
                    out.push_str(&format!("{name}: {value}\r\n"));
                }
                out.push_str("\r\n");
                let _ = stream.write_all(out.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            });
        }
    });
    Server { port, seen }
}

pub(crate) fn engine() -> (Engine, EngineViewId, tokio::runtime::Runtime) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    (engine, view, rt)
}

pub(crate) fn text(engine: &mut Engine, view: EngineViewId, js: &str) -> String {
    engine.execute_script(view, js).unwrap()
}

/// A 403 whose inline script inserts `<script src>`. That script sets a
/// cookie and reloads; the server answers the reload, which carries the
/// cookie (and the one the 403 set), with the real page.
fn checking_site(path: &str, cookie: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
    match path {
        "/" if cookie.contains("passed=1") => (
            200,
            "text/html",
            vec![],
            "<html><head><title>Welcome</title></head><body><p>the site</p></body></html>".into(),
        ),
        "/" => (
            403,
            "text/html",
            vec![("Set-Cookie", "visit=abc; Path=/; HttpOnly")],
            r#"<html><head><title>Just a moment</title></head><body><p>checking</p>
<script>
  window.ranInline = true;
  var s = document.createElement('script');
  s.src = '/check.js';
  s.onload = function () { window.loaded = true; };
  document.getElementsByTagName('head')[0].appendChild(s);
</script></body></html>"#
                .into(),
        ),
        "/check.js" => (
            200,
            "text/javascript",
            vec![],
            "window.checkRan = document.currentScript && document.currentScript.src;\n\
             document.cookie = 'passed=1; path=/';\n\
             location.reload();"
                .into(),
        ),
        _ => (404, "text/plain", vec![], "not found".into()),
    }
}

#[test]
fn a_403_page_runs_the_script_it_inserts_and_its_reload_carries_the_cookies() {
    let server = serve(checking_site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("a 403 with a page is a page");
    assert_eq!(engine.http_status(view), Some(403));
    assert_eq!(text(&mut engine, view, "String(window.ranInline)"), r#"String("true")"#);
    assert_eq!(
        text(&mut engine, view, "String(window.checkRan)"),
        format!(r#"String("{}")"#, server.url("/check.js")),
        "the inserted script was fetched and run, as the current script"
    );
    assert_eq!(text(&mut engine, view, "String(window.loaded)"), r#"String("true")"#, "its element heard load");
    assert_eq!(
        text(&mut engine, view, "document.cookie"),
        r#"String("passed=1")"#,
        "document.cookie reads the jar; the HttpOnly cookie stays out of script's reach"
    );
    let (reload, replace) = engine.take_script_navigation_with_replace(view).expect("location.reload() asks to navigate");
    assert!(replace, "a reload replaces the history entry: Back does not return to the 403");
    assert_eq!(reload, server.url("/").to_string());

    rt.block_on(engine.load_url(view, Url::parse(&reload).unwrap())).expect("the reload");
    assert_eq!(engine.http_status(view), Some(200));
    assert_eq!(engine.views[&view].title.as_deref(), Some("Welcome"));
    assert!(engine.views[&view].layout.is_some(), "the real page is laid out");
    let requests = server.requests();
    let paths: Vec<&str> = requests.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, vec!["/", "/check.js", "/"]);
    assert_eq!(requests[1].1, "visit=abc", "the inserted script's fetch carries the response's cookie");
    assert_eq!(requests[2].1, "visit=abc; passed=1", "the reload carries both cookies");
}

/// The same page served with 200 behaves the same way: none of this is
/// about the status.
#[test]
fn a_200_page_runs_inserted_scripts_the_same_way() {
    fn site(path: &str, cookie: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        let (status, ty, headers, body) = checking_site(path, cookie);
        (if status == 403 { 200 } else { status }, ty, headers, body)
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    assert_eq!(engine.http_status(view), Some(200));
    assert_eq!(text(&mut engine, view, "String(window.loaded)"), r#"String("true")"#);
    assert_eq!(engine.take_script_navigation(view), Some(server.url("/").to_string()));
}

/// A 503 with `<meta http-equiv=refresh>` asks to go where the meta says
/// once the page has loaded.
#[test]
fn a_503_page_with_a_meta_refresh_navigates() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/" => (
                503,
                "text/html",
                vec![],
                r#"<html><head><meta http-equiv="refresh" content="0; url=/later"><title>Busy</title></head><body>try again</body></html>"#.into(),
            ),
            _ => (200, "text/html", vec![], "<html><head><title>Later</title></head><body>ok</body></html>".into()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("a 503 with a page is a page");
    assert_eq!(engine.http_status(view), Some(503));
    assert_eq!(engine.views[&view].title.as_deref(), Some("Busy"));
    let (next, replace) = engine.take_script_navigation_with_replace(view).expect("the refresh asks to navigate");
    assert!(replace, "a declarative refresh replaces the history entry");
    assert_eq!(next, server.url("/later").to_string());
    rt.block_on(engine.load_url(view, Url::parse(&next).unwrap())).expect("follow the refresh");
    assert_eq!(engine.views[&view].title.as_deref(), Some("Later"));
}

/// A refresh with no URL reloads the same document, after its delay.
#[test]
fn a_meta_refresh_without_a_url_reloads_after_its_delay() {
    fn site(_: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        (503, "text/html", vec![], r#"<meta http-equiv="Refresh" content="10"><p>busy</p>"#.into())
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/wait"))).expect("load");
    assert_eq!(engine.take_script_navigation(view), None, "not before its ten seconds");
    rt.block_on(engine.pump_live(view, 10_000));
    assert_eq!(engine.take_script_navigation(view), Some(server.url("/wait").to_string()));
}

/// A 404 page's script fetches JSON from its own origin like any page.
#[test]
fn a_404_page_fetches_json() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/data.json" => (200, "application/json", vec![], r#"{"name":"found it"}"#.into()),
            _ => (
                404,
                "text/html",
                vec![],
                r#"<html><body><p id="out">missing</p><script>
fetch('/data.json').then(function (r) { return r.json(); }).then(function (d) {
  document.getElementById('out').textContent = d.name;
});
</script></body></html>"#
                    .into(),
            ),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/nope"))).expect("a 404 with a page is a page");
    assert_eq!(engine.http_status(view), Some(404));
    assert_eq!(
        text(&mut engine, view, "document.getElementById('out').textContent"),
        r#"String("found it")"#
    );
}

/// What else script navigates with, on an error page: `location.replace`
/// and `assign`, and assigning the document's own URL (a reload, not a
/// fragment navigation).
#[test]
fn location_replace_assign_and_href_to_itself_navigate() {
    fn site(_: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        (404, "text/html", vec![], "<p>not here</p>".into())
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/page"))).expect("load");
    // The URL, and whether it replaces the history entry (Chrome: replace,
    // reload and the document's own URL do; an assign elsewhere adds one).
    for (js, want, replace) in [
        ("location.replace('/a')", "/a", true),
        ("location.assign('/b?x=1')", "/b?x=1", false),
        ("location.href = '/c'", "/c", false),
        ("location.href = location.href", "/page", true),
        ("location.assign(location.href)", "/page", true),
        ("location.reload()", "/page", true),
        ("location.href = '#top'", "", false),
    ] {
        text(&mut engine, view, js);
        let want = (!want.is_empty()).then(|| (server.url(want).to_string(), replace));
        assert_eq!(engine.take_script_navigation_with_replace(view), want, "{js}");
    }
}

/// A script element is run once: moving an inserted script, or inserting
/// one the parser already ran, runs nothing again; a script set to a type
/// that is not JavaScript is not run; one whose fetch fails hears `error`.
#[test]
fn inserted_scripts_run_once_and_only_when_they_are_javascript() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/" => (
                200,
                "text/html",
                vec![],
                r#"<html><head><script id="p">window.runs = (window.runs || 0) + 1;</script></head><body><script>
var log = [];
function add(src, type) {
  var s = document.createElement('script');
  if (type) s.type = type;
  s.src = src;
  s.onload = function () { log.push('load ' + src); };
  s.onerror = function () { log.push('error ' + src); };
  document.body.appendChild(s);
  return s;
}
var a = add('/a.js');
document.head.appendChild(a);
add('/data.txt', 'text/plain');
add('/gone.js');
document.body.appendChild(document.getElementById('p'));
var inline = document.createElement('script');
inline.textContent = 'log.push("inline " + (document.currentScript === inline));';
document.body.appendChild(inline);
</script></body></html>"#
                    .into(),
            ),
            "/a.js" => (200, "text/javascript", vec![], "log.push('ran a');".into()),
            "/data.txt" => (200, "text/plain", vec![], "log.push('ran data');".into()),
            _ => (404, "text/plain", vec![], "".into()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    assert_eq!(
        text(&mut engine, view, "log.join(',') + ' runs=' + runs"),
        r#"String("inline true,ran a,load /a.js,error /gone.js runs=1")"#
    );
    // The two external scripts are fetched together, so the server may see
    // either first. Execution stays in insertion order (asserted above).
    let mut paths: Vec<String> = server.requests().into_iter().map(|(p, _)| p).collect();
    paths.sort();
    assert_eq!(paths, vec!["/", "/a.js", "/gone.js"]);
}

/// A script inserted after the load, from a timer, is fetched and run by
/// the live loop.
#[test]
fn a_script_inserted_after_the_load_runs_in_a_live_turn() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/" => (
                403,
                "text/html",
                vec![],
                r#"<p>wait</p><script>
setTimeout(function () {
  var s = document.createElement('script');
  s.src = '/late.js';
  document.head.appendChild(s);
}, 60000);
</script>"#
                    .into(),
            ),
            "/late.js" => (200, "text/javascript", vec![], "window.late = 'ran';".into()),
            _ => (404, "text/plain", vec![], "".into()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    assert_eq!(text(&mut engine, view, "String(window.late)"), r#"String("undefined")"#);
    // The timer inserts the script on the first turn. A turn does not wait
    // the fetch out, so a later turn may be the one that runs it.
    let started = std::time::Instant::now();
    let mut elapsed = 60_000;
    loop {
        rt.block_on(engine.pump_live(view, elapsed));
        elapsed = 10;
        if text(&mut engine, view, "String(window.late)") == r#"String("ran")"# {
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "the live loop never ran the script inserted after the load"
        );
    }
}

/// A load cancelled while an inserted script is being fetched runs none of
/// it: the M3b cancel reaches the inserted-script fetch like every other.
#[test]
fn a_cancelled_load_does_not_run_the_script_it_inserted() {
    static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/slow.js" => {
                ASKED.store(true, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(400));
                (200, "text/javascript", vec![], "window.slowRan = true;".into())
            }
            _ => (
                403,
                "text/html",
                vec![],
                r#"<p>x</p><script>var s = document.createElement('script'); s.src = '/slow.js'; document.head.appendChild(s);</script>"#.into(),
            ),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    let canceller = engine.nav_canceller(view).expect("canceller");
    // Cancelled once the inserted script's request is out.
    let cancel = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        while !ASKED.load(std::sync::atomic::Ordering::SeqCst) && started.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        canceller.cancel_current();
    });
    let _ = rt.block_on(engine.load_url(view, server.url("/")));
    cancel.join().unwrap();
    // A cancelled page's script may refuse to run at all; it must not have
    // run the inserted one.
    let ran = engine.execute_script(view, "String(window.slowRan)").unwrap_or_default();
    assert_ne!(ran, r#"String("true")"#);
    assert!(
        server.requests().iter().any(|(p, _)| p == "/slow.js"),
        "the cancel came while the inserted script was being fetched"
    );
}

/// An inserted script still arriving when the load's script budget is spent
/// is not dropped. The load finishes without it (no `load`, no `error`,
/// and it is not recorded over budget). A later live turn runs it and the
/// element hears `load`.
#[test]
fn an_inserted_script_past_the_script_budget_runs_on_a_later_live_turn() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/slow.js" => {
                std::thread::sleep(std::time::Duration::from_millis(600));
                (200, "text/javascript", vec![], "window.slowRan = true;".into())
            }
            _ => (
                403,
                "text/html",
                vec![],
                r#"<p>x</p><script>var heard = [];
var s = document.createElement('script'); s.src = '/slow.js';
s.onload = function () { heard.push('load'); }; s.onerror = function () { heard.push('error'); };
document.head.appendChild(s);</script>"#
                    .into(),
            ),
        }
    }
    let server = serve(site);
    let mut engine = Engine::new(EngineConfig { script_budget_ms: 200, ..EngineConfig::default() }).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    assert_eq!(text(&mut engine, view, "document.readyState"), r#"String("complete")"#);
    assert_eq!(text(&mut engine, view, "String(window.slowRan) + ' ' + heard.join()"), r#"String("undefined ")"#);
    assert!(
        engine.script_log(view).unwrap().iter().all(|record| record.outcome != ScriptOutcome::OverBudget),
        "a fetch past the budget stays in flight: {:?}",
        engine.script_log(view).unwrap().iter().map(|r| (&r.source, &r.outcome)).collect::<Vec<_>>()
    );
    // The fetch is not `in_flight`: that would make the window loop poll.
    // Its arrival wakes a turn. The script stays unrun until a turn sees it.
    let turn = rt.block_on(engine.pump_live(view, 0));
    assert_eq!(turn.in_flight, 0, "an inserted fetch must not keep the loop polling: {turn:?}");
    let mut got = text(&mut engine, view, "String(window.slowRan) + ' ' + heard.join()");
    if got != r#"String("true load")"# {
        let started = std::time::Instant::now();
        while engine.inserted_wake_count(view) == 0 {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(3),
                "the finished fetch never woke a turn"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        got = text(&mut engine, view, "String(window.slowRan) + ' ' + heard.join()");
        assert_eq!(got, r#"String("undefined ")"#, "the script ran outside a turn");
        let started = std::time::Instant::now();
        loop {
            rt.block_on(engine.pump_live(view, 20));
            got = text(&mut engine, view, "String(window.slowRan) + ' ' + heard.join()");
            if got == r#"String("true load")"# {
                break;
            }
            assert!(started.elapsed() < std::time::Duration::from_secs(3), "the inserted script never ran: {got}");
        }
    }
}

/// The page is the reduced loader: an inline script inserts chunk A, whose
/// `onload` inserts chunk B, whose `IntersectionObserver` replaces the feed.
fn chunk_chain_page() -> String {
    r#"<!doctype html><html><head>
<script>
function L(m) { var p = document.getElementById('log'); if (p) p.textContent += m + '\n'; }
function insert(src, next) {
  var s = document.createElement('script');
  s.src = src;
  s.async = true;
  s.onload = function () { L(src + ' load'); if (next) next(); };
  s.onerror = function () { L(src + ' error'); };
  document.head.appendChild(s);
  L(src + ' inserted');
}
document.addEventListener('DOMContentLoaded', function () { L('DOMContentLoaded readyState=' + document.readyState); });
window.addEventListener('load', function () { L('window load readyState=' + document.readyState); });
insert('/chunk-a.js', function () { insert('/chunk-b.js'); });
</script></head><body>
<div id="feed"><div class="post">Server-rendered post 1</div><div class="post">Server-rendered post 2</div></div>
<div id="sentinel" style="height:1px"></div>
<pre id="log"></pre>
</body></html>"#
        .into()
}

const CHUNK_A: &str = "\
L('chunk a ran');\n\
document.getElementById('feed').innerHTML = '<div class=\"skeleton\">loading</div>';\n";

const CHUNK_B: &str = "\
L('chunk b ran');\n\
new IntersectionObserver(function (entries) {\n\
  if (!entries[0].isIntersecting) return;\n\
  var feed = document.getElementById('feed');\n\
  feed.innerHTML = '';\n\
  for (var i = 1; i <= 3; i++) {\n\
    var d = document.createElement('div');\n\
    d.className = 'post';\n\
    d.textContent = 'Feed post ' + i;\n\
    feed.appendChild(d);\n\
  }\n\
  L('IO rendered ' + feed.children.length);\n\
}).observe(document.getElementById('sentinel'));\n";

fn chain_engine(budget_ms: u64) -> (Engine, EngineViewId, tokio::runtime::Runtime) {
    let mut engine = Engine::new(EngineConfig { script_budget_ms: budget_ms, ..EngineConfig::default() }).expect("engine");
    // Tall enough that the sentinel is inside the viewport when the feed is short.
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 800 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    (engine, view, rt)
}

fn page_log(engine: &mut Engine, view: EngineViewId) -> String {
    let raw = text(
        engine,
        view,
        r"document.getElementById('log').textContent.replace(/\n/g, '|')",
    );
    raw.trim_start_matches("String(\"").trim_end_matches("\")").to_string()
}

/// A chunk chain that arrives inside the script budget runs after
/// `DOMContentLoaded` and `load`, and the intersection observer renders
/// the feed before the load returns.
#[test]
fn an_inserted_chunk_chain_runs_after_dom_content_loaded() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/chunk-a.js" => (200, "text/javascript", vec![], CHUNK_A.into()),
            "/chunk-b.js" => (200, "text/javascript", vec![], CHUNK_B.into()),
            _ => (200, "text/html", vec![], chunk_chain_page()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = chain_engine(5_000);
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    let log = page_log(&mut engine, view);
    let at = |needle: &str| log.find(needle).unwrap_or_else(|| panic!("{needle} missing from {log}"));
    assert!(at("/chunk-a.js inserted") < at("DOMContentLoaded readyState=interactive"), "{log}");
    assert!(at("DOMContentLoaded readyState=interactive") < at("chunk a ran"), "{log}");
    assert!(at("window load readyState=complete") < at("chunk a ran"), "{log}");
    assert!(at("chunk a ran") < at("/chunk-a.js load"), "{log}");
    assert!(at("/chunk-a.js load") < at("/chunk-b.js inserted"), "{log}");
    assert!(at("/chunk-b.js inserted") < at("chunk b ran"), "{log}");
    assert!(at("chunk b ran") < at("/chunk-b.js load"), "{log}");
    assert!(log.contains("IO rendered 3"), "{log}");
    let feed = text(&mut engine, view, "document.getElementById('feed').textContent");
    assert!(feed.contains("Feed post 1") && feed.contains("Feed post 2") && feed.contains("Feed post 3"), "{feed}");
    assert_eq!(text(&mut engine, view, "document.readyState"), r#"String("complete")"#);
}

/// The same chain, each chunk held longer than the load's script budget.
/// The load still reaches `complete` and does not drop the fetch. Later
/// live turns run both chunks, fire `onload`, and the observer renders
/// the feed. No resize is involved.
#[test]
fn an_inserted_chunk_chain_past_the_budget_renders_on_the_live_loop() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/chunk-a.js" | "/chunk-b.js" => {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let body = if path == "/chunk-a.js" { CHUNK_A } else { CHUNK_B };
                (200, "text/javascript", vec![], body.into())
            }
            _ => (200, "text/html", vec![], chunk_chain_page()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = chain_engine(200);
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    let log = page_log(&mut engine, view);
    assert!(log.contains("/chunk-a.js inserted"), "{log}");
    assert!(log.contains("DOMContentLoaded readyState=interactive"), "{log}");
    assert!(log.contains("window load readyState=complete"), "{log}");
    assert!(!log.contains("chunk a ran"), "the chunk must not block the load, log was {log}");
    assert_eq!(text(&mut engine, view, "document.readyState"), r#"String("complete")"#);
    let feed = text(&mut engine, view, "document.getElementById('feed').textContent");
    assert!(feed.contains("Server-rendered post 1"), "{feed}");
    assert!(!feed.contains("Feed post"), "{feed}");
    assert!(
        engine.script_log(view).unwrap().iter().all(|record| record.outcome != ScriptOutcome::OverBudget),
        "the in-flight chunk was dropped: {:?}",
        engine.script_log(view).unwrap().iter().map(|r| (&r.source, &r.outcome)).collect::<Vec<_>>()
    );

    let started = std::time::Instant::now();
    let mut rendered = false;
    while started.elapsed() < std::time::Duration::from_secs(5) {
        rt.block_on(engine.pump_live(view, 20));
        let log = page_log(&mut engine, view);
        if log.contains("IO rendered 3") {
            rendered = true;
            break;
        }
    }
    assert!(rendered, "the feed never rendered:\n{}", page_log(&mut engine, view));
    let log = page_log(&mut engine, view);
    assert!(log.contains("chunk a ran") && log.contains("/chunk-a.js load"), "{log}");
    assert!(log.contains("chunk b ran") && log.contains("/chunk-b.js load"), "{log}");
    assert!(!log.contains("error"), "{log}");
    let feed = text(&mut engine, view, "document.getElementById('feed').textContent");
    assert!(feed.contains("Feed post 1") && feed.contains("Feed post 2") && feed.contains("Feed post 3"), "{feed}");
}

/// Three delayed chunks, each inserted from the previous one's `onload`.
/// Every chunk is held longer than the load's script budget. The load
/// reaches `complete` with none of them run. Later live turns run them in
/// insertion order and the last one's observer renders the feed.
#[test]
fn an_inserted_chain_of_three_delayed_chunks_runs_in_order() {
    const A: &str = "L('chunk a ran');\n";
    const B: &str = "L('chunk b ran');\n";
    const C: &str = "\
L('chunk c ran');\n\
new IntersectionObserver(function (entries) {\n\
  if (!entries[0].isIntersecting) return;\n\
  var feed = document.getElementById('feed');\n\
  feed.innerHTML = '';\n\
  for (var i = 1; i <= 3; i++) {\n\
    var d = document.createElement('div');\n\
    d.className = 'post';\n\
    d.textContent = 'Feed post ' + i;\n\
    feed.appendChild(d);\n\
  }\n\
  L('IO rendered ' + feed.children.length);\n\
}).observe(document.getElementById('sentinel'));\n";
    fn page() -> String {
        r#"<!doctype html><html><head>
<script>
function L(m) { var p = document.getElementById('log'); if (p) p.textContent += m + '\n'; }
function insert(src, next) {
  var s = document.createElement('script');
  s.src = src;
  s.async = true;
  s.onload = function () { L(src + ' load'); if (next) next(); };
  s.onerror = function () { L(src + ' error'); };
  document.head.appendChild(s);
  L(src + ' inserted');
}
document.addEventListener('DOMContentLoaded', function () { L('DOMContentLoaded readyState=' + document.readyState); });
window.addEventListener('load', function () { L('window load readyState=' + document.readyState); });
insert('/chunk-a.js', function () { insert('/chunk-b.js', function () { insert('/chunk-c.js'); }); });
</script></head><body>
<div id="feed"><div class="post">Server-rendered post 1</div></div>
<div id="sentinel" style="height:1px"></div>
<pre id="log"></pre>
</body></html>"#
            .into()
    }
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/chunk-a.js" | "/chunk-b.js" | "/chunk-c.js" => {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let body = match path {
                    "/chunk-a.js" => A,
                    "/chunk-b.js" => B,
                    _ => C,
                };
                (200, "text/javascript", vec![], body.into())
            }
            _ => (200, "text/html", vec![], page()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = chain_engine(200);
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    let log = page_log(&mut engine, view);
    assert!(log.contains("/chunk-a.js inserted"), "{log}");
    assert!(log.contains("DOMContentLoaded readyState=interactive"), "{log}");
    assert!(log.contains("window load readyState=complete"), "{log}");
    assert!(!log.contains("chunk a ran"), "{log}");
    assert_eq!(text(&mut engine, view, "document.readyState"), r#"String("complete")"#);
    assert!(
        engine.script_log(view).unwrap().iter().all(|record| record.outcome != ScriptOutcome::OverBudget),
        "a delayed chunk was dropped: {:?}",
        engine.script_log(view).unwrap().iter().map(|r| (&r.source, &r.outcome)).collect::<Vec<_>>()
    );

    let started = std::time::Instant::now();
    let mut rendered = false;
    while started.elapsed() < std::time::Duration::from_secs(6) {
        rt.block_on(engine.pump_live(view, 20));
        if page_log(&mut engine, view).contains("IO rendered 3") {
            rendered = true;
            break;
        }
    }
    assert!(rendered, "the feed never rendered:\n{}", page_log(&mut engine, view));
    let log = page_log(&mut engine, view);
    let at = |needle: &str| log.find(needle).unwrap_or_else(|| panic!("{needle} missing from {log}"));
    assert!(at("chunk a ran") < at("/chunk-a.js load"), "{log}");
    assert!(at("/chunk-a.js load") < at("/chunk-b.js inserted"), "{log}");
    assert!(at("/chunk-b.js inserted") < at("chunk b ran"), "{log}");
    assert!(at("chunk b ran") < at("/chunk-b.js load"), "{log}");
    assert!(at("/chunk-b.js load") < at("/chunk-c.js inserted"), "{log}");
    assert!(at("/chunk-c.js inserted") < at("chunk c ran"), "{log}");
    assert!(at("chunk c ran") < at("/chunk-c.js load"), "{log}");
    assert!(at("/chunk-c.js load") < at("IO rendered 3"), "{log}");
    assert!(!log.contains("error"), "{log}");
    let feed = text(&mut engine, view, "document.getElementById('feed').textContent");
    assert!(feed.contains("Feed post 1") && feed.contains("Feed post 2") && feed.contains("Feed post 3"), "{feed}");
}

/// Navigating away while an inserted script is still being fetched drops
/// it. The late body does not run in the next page.
#[test]
fn navigating_away_drops_an_inserted_script_still_fetching() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/slow.js" => {
                std::thread::sleep(std::time::Duration::from_millis(800));
                (200, "text/javascript", vec![], "window.fromFirst = 'ran';".into())
            }
            "/next" => (
                200,
                "text/html",
                vec![],
                "<p id=\"p\">second</p><script>window.page = 'b';</script>".into(),
            ),
            _ => (
                200,
                "text/html",
                vec![],
                r#"<p>first</p><script>
var s = document.createElement('script');
s.src = '/slow.js';
document.head.appendChild(s);
</script>"#
                    .into(),
            ),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = chain_engine(200);
    rt.block_on(engine.load_url(view, server.url("/"))).expect("first load");
    assert_eq!(text(&mut engine, view, "String(window.fromFirst)"), r#"String("undefined")"#);
    assert!(
        server.requests().iter().any(|(p, _)| p == "/slow.js"),
        "the fetch should be in flight when the load returns"
    );
    rt.block_on(engine.load_url(view, server.url("/next"))).expect("second load");
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(1500) {
        rt.block_on(engine.pump_live(view, 50));
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(text(&mut engine, view, "String(window.page)"), r#"String("b")"#);
    assert_eq!(
        text(&mut engine, view, "String(window.fromFirst)"),
        r#"String("undefined")"#,
        "the previous page's script ran after navigation"
    );
    assert!(
        engine.script_log(view).unwrap().iter().all(|record| record.outcome != ScriptOutcome::Ran || !record.source.contains("slow.js")),
        "the dropped script was recorded as run: {:?}",
        engine.script_log(view).unwrap().iter().map(|r| (&r.source, &r.outcome)).collect::<Vec<_>>()
    );
}

/// A fetch that fails after the load's script budget still fires `error`
/// on a later live turn. It is not recorded over budget and it does not
/// vanish.
#[test]
fn a_failed_inserted_fetch_past_the_budget_fires_error() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/missing.js" => {
                std::thread::sleep(std::time::Duration::from_millis(600));
                (404, "text/plain", vec![], String::new())
            }
            _ => (
                200,
                "text/html",
                vec![],
                r#"<p>x</p><script>
var heard = [];
var s = document.createElement('script');
s.src = '/missing.js';
s.onload = function () { heard.push('load'); };
s.onerror = function () { heard.push('error'); };
document.head.appendChild(s);
</script>"#
                    .into(),
            ),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt) = chain_engine(200);
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    assert_eq!(text(&mut engine, view, "heard.join()"), r#"String("")"#);
    assert_eq!(text(&mut engine, view, "document.readyState"), r#"String("complete")"#);
    assert!(
        engine.script_log(view).unwrap().iter().all(|record| record.outcome != ScriptOutcome::OverBudget),
        "a failed fetch was dropped as over budget: {:?}",
        engine.script_log(view).unwrap().iter().map(|r| (&r.source, &r.outcome)).collect::<Vec<_>>()
    );
    let started = std::time::Instant::now();
    loop {
        rt.block_on(engine.pump_live(view, 20));
        let got = text(&mut engine, view, "heard.join()");
        if got == r#"String("error")"# {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "the failed fetch never fired error: {got}");
    }
    assert!(
        engine.script_log(view).unwrap().iter().any(|record| {
            record.source.contains("missing.js") && matches!(record.outcome, ScriptOutcome::FetchFailed(_))
        }),
        "the failure was not recorded: {:?}",
        engine.script_log(view).unwrap().iter().map(|r| (&r.source, &r.outcome)).collect::<Vec<_>>()
    );
}

