//! Dynamic `import()` and the load's script budget (T0 #666, M1 #699).
//!
//! x.com, 2026-10: the entry bundle ran for most of the 5 s budget, then
//! `import()`ed its chunks; four failed with "Failed to fetch dynamically
//! imported module: script budget spent" and React's error boundary showed
//! "Something went wrong!". The budget is there to stop runaway script, not
//! to refuse code the page fetches once its scripts have run: module fetches
//! are bounded by a network deadline of their own, and the time spent
//! waiting on them is not charged to the budget. Script that runs too long
//! is still stopped.

use super::*;
use crate::script_net_tests::serve_routes;

const JS: &str = "text/javascript";

/// The fixture page, its bundle running `spend_ms`, and its chunks;
/// `chunk_c` is served as the last chunk.
fn routes(spend_ms: u64, chunk_c: &'static str) -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "/",
            "text/html",
            include_str!("../../../fixtures/dynamic-import-chain/index.html")
                .replace("__SPEND_MS__", &spend_ms.to_string()),
        ),
        (
            "/held/chunk-a.js",
            JS,
            include_str!("../../../fixtures/dynamic-import-chain/chunk-a.js").into(),
        ),
        (
            "/held/chunk-b.js",
            JS,
            include_str!("../../../fixtures/dynamic-import-chain/chunk-b.js").into(),
        ),
        ("/held/chunk-c.js", JS, chunk_c.into()),
    ]
}

const CHUNK_C: &str = include_str!("../../../fixtures/dynamic-import-chain/chunk-c.js");
const CHUNK_SPIN: &str = include_str!("../../../fixtures/dynamic-import-chain/chunk-spin.js");

fn load(
    config: EngineConfig,
    routes: Vec<(&'static str, &'static str, String)>,
) -> (Engine, EngineViewId, std::time::Duration) {
    let server = serve_routes(routes);
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 200,
            height: 100,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let started = std::time::Instant::now();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, started.elapsed())
}

fn log(engine: &mut Engine, view: EngineViewId) -> String {
    let value = engine.execute_script(view, "log.join(',')").unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

const LOADED: &str = "bundle,a:app shell,b,c,done:c";

/// The board (scripts run to their end): the bundle runs past the whole
/// budget, and its chunks still load. Before: "failed:Failed to fetch
/// dynamically imported module: script budget spent".
#[test]
fn chunks_imported_after_the_bundle_spent_the_budget_still_load() {
    let config = EngineConfig {
        script_budget_ms: 300,
        ..EngineConfig::default()
    };
    let (mut engine, view, _) = load(config, routes(400, CHUNK_C));
    assert_eq!(log(&mut engine, view), LOADED);
    assert_eq!(
        engine
            .execute_script(view, "document.getElementById('p').textContent")
            .unwrap(),
        r#"String("rendered")"#
    );
}

/// The live app (script stopped at the budget): the bundle leaves budget
/// over, and the three chunk fetches (300 ms each) take longer than what is
/// left. Waiting is not script, so the chunks are fetched and run. Before:
/// the last import failed with "script budget spent".
#[test]
fn waiting_on_chunk_fetches_is_not_charged_to_the_script_budget() {
    let config = EngineConfig {
        script_budget_ms: 800,
        interrupt_scripts_at_budget: true,
        ..EngineConfig::default()
    };
    let (mut engine, view, took) = load(config, routes(200, CHUNK_C));
    assert_eq!(log(&mut engine, view), LOADED);
    assert!(
        took >= std::time::Duration::from_millis(900),
        "the chunks were fetched: {took:?}"
    );
    let log = engine.script_log(view).unwrap();
    assert!(
        log.iter().all(|r| r.outcome != ScriptOutcome::OverBudget),
        "{log:#?}"
    );
}

/// The runaway protection is intact: a chunk that never ends is stopped at
/// the budget, counted in script time.
#[test]
fn a_runaway_chunk_is_still_stopped_at_the_budget() {
    let config = EngineConfig {
        script_budget_ms: 800,
        interrupt_scripts_at_budget: true,
        ..EngineConfig::default()
    };
    let (mut engine, view, took) = load(config, routes(200, CHUNK_SPIN));
    // 200 ms of bundle, 900 ms of fetching, the rest of the budget spinning.
    assert!(
        took < std::time::Duration::from_secs(5),
        "the spin was not stopped: {took:?}"
    );
    let got = log(&mut engine, view);
    assert!(got.starts_with("bundle,a:app shell,b,spin"), "{got}");
    assert!(!got.contains("done"), "{got}");
    // The page is still a page.
    assert_eq!(engine.execute_script(view, "1 + 1").unwrap(), "Number(2.0)");
}

/// How many module scripts the chain below is. Each one is its own graph,
/// so each used to open a fresh module-fetch window.
const HANGING_MODULE_SCRIPTS: usize = 8;

/// `page` at `/`. `/hang/…` accepts the socket and never answers, so the
/// fetch waits until the engine's own deadline drops it.
fn serve_hanging(page: String) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let page = page.clone();
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut request = Vec::new();
                let mut buf = [0u8; 2048];
                let header_end = loop {
                    if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&request[..header_end]).to_string();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                if path.starts_with("/hang/") {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
                    let mut sink = [0u8; 64];
                    while matches!(stream.read(&mut sink), Ok(n) if n > 0) {}
                    return;
                }
                let (status, content_type, body): (&str, &str, &str) = if path == "/" {
                    ("200 OK", "text/html; charset=utf-8", &page)
                } else {
                    ("404 Not Found", "text/plain", "missing")
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body.as_bytes());
            });
        }
    });
    port
}

fn hanging_module_chain(scripts: usize) -> String {
    let mut page = String::from("<!doctype html><html><head>");
    for i in 0..scripts {
        page.push_str(&format!(
            "<script type=\"module\">import '/hang/m{i}.js';</script>"
        ));
    }
    page.push_str("</head><body></body></html>");
    page
}

/// A chain of module scripts whose imports never answer. The load may spend
/// `subresource_budget_ms` in total waiting on those fetches, not that long
/// once per script. Before the shared window, eight scripts at 1s each kept
/// the load busy for about 8s.
#[test]
fn a_chain_of_slow_module_fetches_stays_within_the_subresource_budget() {
    const BUDGET_MS: u64 = 1_000;
    let config = EngineConfig {
        script_budget_ms: 300,
        subresource_budget_ms: BUDGET_MS,
        ..EngineConfig::default()
    };
    let port = serve_hanging(hanging_module_chain(HANGING_MODULE_SCRIPTS));
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 200,
            height: 100,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let started = std::time::Instant::now();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let took = started.elapsed();
    let records = engine.script_log(view).expect("script log").to_vec();
    assert!(
        took < std::time::Duration::from_millis(2_500),
        "chain of module fetches exceeded one subresource budget ({took:?}): {records:#?}"
    );
    assert!(
        took >= std::time::Duration::from_millis(BUDGET_MS - 200),
        "the shared module-fetch window was not waited out ({took:?}): {records:#?}"
    );
    let timed_out = records
        .iter()
        .filter(|record| match &record.outcome {
            ScriptOutcome::Threw(message) => message.contains("module fetch timed out"),
            _ => false,
        })
        .count();
    assert_eq!(
        timed_out, HANGING_MODULE_SCRIPTS,
        "every link should have been attempted and then stopped by the shared window: {records:#?}"
    );
    assert_eq!(engine.execute_script(view, "1 + 1").unwrap(), "Number(2.0)");
}
