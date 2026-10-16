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
type Handler = fn(path: &str, cookie: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String);

struct Server {
    port: u16,
    /// (request target, Cookie header) in arrival order.
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

impl Server {
    fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{path}", self.port)).unwrap()
    }
    fn requests(&self) -> Vec<(String, String)> {
        self.seen.lock().unwrap().clone()
    }
}

fn serve(handler: Handler) -> Server {
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

fn engine() -> (Engine, EngineViewId, tokio::runtime::Runtime) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    (engine, view, rt)
}

fn text(engine: &mut Engine, view: EngineViewId, js: &str) -> String {
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
    let reload = engine.take_script_navigation(view).expect("location.reload() asks to navigate");
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
    let next = engine.take_script_navigation(view).expect("the refresh asks to navigate");
    assert_eq!(next, server.url("/later").to_string());
    rt.block_on(engine.load_url(view, Url::parse(&next).unwrap())).expect("follow the refresh");
    assert_eq!(engine.views[&view].title.as_deref(), Some("Later"));
}

/// A refresh with no URL reloads the same document, after its delay.
#[test]
fn a_meta_refresh_without_a_url_reloads_after_its_delay() {
    fn site(_: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        (503, "text/html", vec![], r#"<meta http-equiv="Refresh" content="5"><p>busy</p>"#.into())
    }
    let server = serve(site);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, server.url("/wait"))).expect("load");
    assert_eq!(engine.take_script_navigation(view), None, "not before its five seconds");
    rt.block_on(engine.pump_live(view, 5_000));
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
    for (js, want) in [
        ("location.replace('/a')", "/a"),
        ("location.assign('/b?x=1')", "/b?x=1"),
        ("location.href = location.href", "/page"),
        ("location.reload()", "/page"),
        ("location.href = '#top'", ""),
    ] {
        text(&mut engine, view, js);
        let want = (!want.is_empty()).then(|| server.url(want).to_string());
        assert_eq!(engine.take_script_navigation(view), want, "{js}");
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
    let paths: Vec<String> = server.requests().into_iter().map(|(p, _)| p).collect();
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
    rt.block_on(engine.pump_live(view, 60_000));
    assert_eq!(text(&mut engine, view, "String(window.late)"), r#"String("ran")"#);
}

/// A load cancelled while an inserted script is being fetched runs none of
/// it: the M3b cancel reaches the inserted-script fetch like every other.
#[test]
fn a_cancelled_load_does_not_run_the_script_it_inserted() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/slow.js" => {
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
    let cancel = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        canceller.cancel_current();
    });
    let _ = rt.block_on(engine.load_url(view, server.url("/")));
    cancel.join().unwrap();
    // A cancelled page's script may refuse to run at all; it must not have
    // run the inserted one.
    let ran = engine.execute_script(view, "String(window.slowRan)").unwrap_or_default();
    assert_ne!(ran, r#"String("true")"#);
}
