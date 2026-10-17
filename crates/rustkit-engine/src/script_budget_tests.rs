//! The script budget counts JavaScript execution only. A network wait has
//! its own cap: it never arms the watchdog, never refuses a host call, and
//! a script whose body arrived still runs. Pending async work is not
//! rejected when the page-script phase ends.

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

struct Server {
    port: u16,
}

impl Server {
    fn url(&self, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{}{path}", self.port)).unwrap()
    }
}

/// `routes` are `(path, delay, content-type, body)`. Anything else is a 404.
/// Each connection is its own thread, so the delays overlap.
fn serve(routes: Vec<(&'static str, Duration, &'static str, String)>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let routes = std::sync::Arc::new(routes);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let routes = routes.clone();
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&request);
                let path = head
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .split('?')
                    .next()
                    .unwrap_or("/");
                let (status, content_type, body) =
                    match routes.iter().find(|(p, _, _, _)| *p == path) {
                        Some((_, delay, content_type, body)) => {
                            if !delay.is_zero() {
                                std::thread::sleep(*delay);
                            }
                            ("200 OK", *content_type, body.clone())
                        }
                        None => ("404 Not Found", "text/plain", String::new()),
                    };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    Server { port }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn load(config: EngineConfig, server: &Server) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 300,
            height: 200,
        })
        .expect("view");
    runtime()
        .block_on(engine.load_url(view, server.url("/")))
        .expect("load");
    (engine, view)
}

fn config(budget_ms: u64) -> EngineConfig {
    EngineConfig {
        script_budget_ms: budget_ms,
        interrupt_scripts_at_budget: true,
        ..EngineConfig::default()
    }
}

/// An inserted script that arrives after 8s, then does 50ms of JavaScript,
/// still runs and its element hears `load`. The wait is not the budget.
#[test]
fn an_inserted_script_that_arrives_after_8s_still_runs_and_fires_load() {
    let server = serve(vec![
        (
            "/",
            Duration::ZERO,
            "text/html",
            r#"<p>x</p><script>var heard = [];
var s = document.createElement('script'); s.src = '/slow.js';
s.onload = function () { heard.push('load'); };
s.onerror = function () { heard.push('error'); };
document.head.appendChild(s);</script>"#
                .into(),
        ),
        (
            "/slow.js",
            Duration::from_millis(8_100),
            "text/javascript",
            "var t = Date.now(); while (Date.now() - t < 50) {} window.slowRan = true;".into(),
        ),
    ]);
    let (mut engine, view) = load(config(1_000), &server);
    assert_eq!(
        engine
            .execute_script(view, "String(window.slowRan) + ' ' + heard.join()")
            .unwrap(),
        r#"String("true load")"#
    );
    let log = engine.script_log(view).unwrap();
    let record = log
        .iter()
        .find(|r| r.source.ends_with("/slow.js"))
        .expect("inserted script");
    assert_eq!(record.outcome, ScriptOutcome::Ran, "{log:#?}");
    assert!(
        record.elapsed_ms < 2_000,
        "the network wait was charged as script time: {} ms",
        record.elapsed_ms
    );
    assert!(
        log.iter().all(|r| r.outcome != ScriptOutcome::OverBudget),
        "{log:#?}"
    );
}

/// Fast scripts behind slow responses: every one runs. A wall clock on the
/// fetches would spend the budget before the last of them started.
#[test]
fn fast_scripts_behind_slow_responses_all_run() {
    let body = "window.ran = (window.ran || 0) + 1;".to_string();
    let delay = Duration::from_millis(1_500);
    let server = serve(vec![
        (
            "/",
            Duration::ZERO,
            "text/html",
            r#"<html><head>
<script src="/a.js"></script>
<script src="/b.js"></script>
<script src="/c.js"></script>
<script src="/d.js"></script>
</head><body>hi</body></html>"#
                .into(),
        ),
        ("/a.js", delay, "text/javascript", body.clone()),
        ("/b.js", delay, "text/javascript", body.clone()),
        ("/c.js", delay, "text/javascript", body.clone()),
        ("/d.js", delay, "text/javascript", body),
    ]);
    let (mut engine, view) = load(config(1_000), &server);
    assert_eq!(
        engine.execute_script(view, "window.ran").unwrap(),
        "Number(4.0)"
    );
    let log = engine.script_log(view).unwrap();
    assert!(
        log.iter().all(|r| r.outcome == ScriptOutcome::Ran),
        "{log:#?}"
    );
    assert_eq!(log.len(), 4, "{log:#?}");
}

/// An async function that awaits a fetch is still pending when the script
/// that started it returns. The page-script phase ending must not reject
/// it: the rejection used to surface as "async function was interrupted"
/// because the phase was one armed turn.
#[test]
fn a_pending_async_fetch_survives_the_page_script_phase() {
    let server = serve(vec![
        (
            "/",
            Duration::ZERO,
            "text/html",
            r#"<html><body><script>
window.canary = 'pending';
window.canaryMsg = '';
async function STICKY_CANARY() {
  try {
    var response = await fetch('/slow');
    window.canary = 'ok ' + response.status;
  } catch (e) {
    window.canary = 'err';
    window.canaryMsg = String(e);
  }
}
STICKY_CANARY();
</script></body></html>"#
                .into(),
        ),
        (
            "/slow",
            Duration::from_millis(800),
            "text/plain",
            "ok".into(),
        ),
    ]);
    let (mut engine, view) = load(config(200), &server);
    assert_eq!(
        engine
            .execute_script(view, "window.canary + '|' + window.canaryMsg")
            .unwrap(),
        r#"String("ok 200|")"#,
        "{:#?}",
        engine.script_log(view)
    );
    let log = engine.script_log(view).unwrap();
    let inline = log
        .iter()
        .find(|r| r.source.starts_with("inline"))
        .expect("inline script");
    assert_eq!(inline.outcome, ScriptOutcome::Ran, "{log:#?}");
    assert!(
        inline.elapsed_ms < 500,
        "the fetch wait was charged to the script that started it: {} ms",
        inline.elapsed_ms
    );
    let rendered = format!("{log:#?}");
    assert!(!rendered.contains("interrupted"), "{rendered}");
}

/// The passing test above delivers the fetch while the budget still has
/// time. Live reddit spends that budget on other scripts, and the canary's
/// fetch comes back as the page-script phase is ending: one microsecond
/// left. Delivering into that turn either never resumes the async function
/// (it stays `pending`, so the feed stays empty) or resumes it under a
/// watchdog that stops it at once ("async function was interrupted").
/// The answer has to wait for a later turn. A resize after that is the
/// live-turn arm (`arm_live_turn`).
#[test]
fn a_fetch_delivered_as_the_budget_runs_out_is_not_interrupted() {
    let server = serve(vec![
        (
            "/",
            Duration::ZERO,
            "text/html",
            r#"<html><body><script>
window.canary = 'pending';
window.canaryMsg = '';
async function STICKY_CANARY() {
  try {
    var response = await fetch('/slow');
    window.canary = 'ok ' + response.status;
  } catch (e) {
    window.canary = 'err';
    window.canaryMsg = String(e);
  }
}
STICKY_CANARY();
</script></body></html>"#
                .into(),
        ),
        (
            "/slow",
            Duration::from_millis(40),
            "text/plain",
            "ok".into(),
        ),
    ]);
    // At the network step the inline script has finished and the budget
    // has one microsecond left: the live page, whose other scripts used
    // the budget, not the test above.
    nav_cancel::LOAD_PHASE_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|phase| {
            if phase == "network" {
                test_leave_script_budget(Duration::from_micros(1));
            }
        }));
    });
    struct ClearPhase;
    impl Drop for ClearPhase {
        fn drop(&mut self) {
            nav_cancel::LOAD_PHASE_HOOK.with(|hook| *hook.borrow_mut() = None);
        }
    }
    let _clear = ClearPhase;
    let (mut engine, view) = load(config(200), &server);
    let after_load = engine
        .execute_script(view, "window.canary + '|' + window.canaryMsg")
        .unwrap();
    runtime().block_on(engine.pump_live(view, 1_000));
    let _ = engine.resize_view(
        view,
        Bounds {
            x: 0,
            y: 0,
            width: 320,
            height: 240,
        },
    );
    runtime().block_on(engine.pump_live(view, 0));
    let after_turn = engine
        .execute_script(view, "window.canary + '|' + window.canaryMsg")
        .unwrap();
    let log = format!("{:#?}", engine.script_log(view));
    assert!(
        !after_load.contains("interrupted") && !after_turn.contains("interrupted") && !log.contains("interrupted"),
        "load={after_load}\nturn={after_turn}\n{log}"
    );
    assert!(
        after_turn.contains("ok 200"),
        "the fetch should arrive on a turn that still has time\nload={after_load}\nturn={after_turn}\n{log}"
    );
}

/// The chrome UI is another turn on the thread that is loading the page.
/// A slow content fetch must still run it, and must not charge it to the
/// content script budget or leave the content watchdog armed.
#[test]
fn the_chrome_ui_turn_runs_during_a_slow_content_load() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let hits = Arc::new(AtomicUsize::new(0));
    let clear = Arc::new(AtomicUsize::new(0));
    let suspended = Arc::new(AtomicUsize::new(0));
    let (hits_h, clear_h, suspended_h) = (hits.clone(), clear.clone(), suspended.clone());
    set_chrome_ui_turn(Some(Box::new(move || {
        hits_h.fetch_add(1, Ordering::SeqCst);
        if chrome_ui_turn_is_clear() {
            clear_h.fetch_add(1, Ordering::SeqCst);
        }
        if chrome_ui_turn_suspended_content_budget() {
            suspended_h.fetch_add(1, Ordering::SeqCst);
        }
    })));
    struct ClearHook;
    impl Drop for ClearHook {
        fn drop(&mut self) {
            set_chrome_ui_turn(None);
        }
    }
    let _clear = ClearHook;
    let server = serve(vec![
        (
            "/",
            Duration::ZERO,
            "text/html",
            r#"<html><body><script>
fetch('/slow').then(function () { window.fetched = true; });
</script></body></html>"#
                .into(),
        ),
        (
            "/slow",
            Duration::from_millis(180),
            "text/plain",
            "ok".into(),
        ),
    ]);
    let (mut engine, view) = load(config(1_000), &server);
    assert!(
        hits.load(Ordering::SeqCst) > 0,
        "the chrome turn never ran while the content fetch was in flight"
    );
    assert_eq!(
        clear.load(Ordering::SeqCst),
        hits.load(Ordering::SeqCst),
        "a chrome turn ran inside the content budget or watchdog turn"
    );
    assert!(
        suspended.load(Ordering::SeqCst) > 0,
        "the content script budget was not installed during the wait, so the turn was not the one a loading page skips"
    );
    assert_eq!(
        engine.execute_script(view, "String(window.fetched)").unwrap(),
        r#"String("true")"#
    );
}
