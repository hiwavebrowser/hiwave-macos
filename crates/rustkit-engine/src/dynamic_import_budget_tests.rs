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
