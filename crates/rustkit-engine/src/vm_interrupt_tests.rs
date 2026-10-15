//! Script that never calls the host is stopped at the turn budget (threads
//! plan M1). T0 (#666) stops a live turn at its first host call past the
//! budget; a bare `while (true) {}` makes none, and before the watchdog and
//! the vendored Boa's interrupt word it held the window's thread for good.

use super::*;
use crate::script_net_tests::serve_routes;

/// The live turn budget in these tests; the app's is 5 s.
const BUDGET_MS: u64 = 300;
/// M1's promise on top of the budget.
const SLACK_MS: u64 = 250;

fn config() -> EngineConfig {
    EngineConfig {
        interrupt_scripts_at_budget: true,
        live_turn_budget_ms: BUDGET_MS,
        ..EngineConfig::default()
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn load(config: EngineConfig, page: &str) -> (Engine, EngineViewId) {
    let (engine, view, _) = load_timed(config, page);
    (engine, view)
}

/// [`load`], and how long `load_url` took (the engine's GPU setup aside).
fn load_timed(config: EngineConfig, page: &str) -> (Engine, EngineViewId, std::time::Duration) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
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
    let runtime = runtime();
    let started = std::time::Instant::now();
    runtime
        .block_on(engine.load_url(view, url))
        .expect("load_url");
    (engine, view, started.elapsed())
}

fn over_budget(engine: &Engine, view: EngineViewId) -> Vec<String> {
    engine
        .script_log(view)
        .unwrap()
        .iter()
        .filter(|record| record.outcome == ScriptOutcome::OverBudget)
        .map(|record| record.source.clone())
        .collect()
}

fn within_budget(took: std::time::Duration) -> bool {
    took < std::time::Duration::from_millis(BUDGET_MS + SLACK_MS)
}

#[test]
fn a_pure_spin_in_a_timer_is_stopped_within_the_budget_plus_250_ms() {
    let page = "<html><body><p id=p>alive</p><script>
setTimeout(function () { window.started = true; while (true) {} }, 60000);
setTimeout(function () { window.after = true; }, 61000);
</script></body></html>";
    let (mut engine, view) = load(config(), page);
    let started = std::time::Instant::now();
    runtime().block_on(engine.pump_live(view, 60_000));
    let took = started.elapsed();
    assert!(within_budget(took), "the spin ran {took:?}");
    assert_eq!(
        over_budget(&engine, view),
        ["timers"],
        "{:#?}",
        engine.script_log(view)
    );
    // The next turn is not stopped by the word the last one left, and the
    // page still runs script.
    runtime().block_on(engine.pump_live(view, 1_000));
    assert_eq!(
        engine
            .execute_script(view, "window.started === true && window.after === true")
            .unwrap(),
        "Boolean(true)"
    );
    assert_eq!(over_budget(&engine, view), ["timers"]);
}

#[test]
fn a_pure_spin_in_a_click_listener_cannot_catch_the_stop() {
    let page =
        "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
window.caught = false;
document.getElementById('p').addEventListener('click', function () {
  try { [1].forEach(function () { for (;;) {} }); } catch (e) { window.caught = true; }
});
</script></body></html>";
    let (mut engine, view) = load(config(), page);
    let started = std::time::Instant::now();
    engine.click_at_point(view, 20.0, 20.0);
    let took = started.elapsed();
    assert!(within_budget(took), "the listener ran {took:?}");
    assert_eq!(
        over_budget(&engine, view),
        ["event:click"],
        "{:#?}",
        engine.script_log(view)
    );
    assert_eq!(
        engine.execute_script(view, "window.caught").unwrap(),
        "Boolean(false)"
    );
}

#[test]
fn a_pure_spin_in_a_load_script_is_stopped_at_the_load_budget() {
    // The load's budget (`script_budget_ms`) is kept by the watchdog too.
    let page = "<html><body><p id=p>alive</p>
<script>window.first = true; while (true) {}</script>
<script>window.second = true;</script>
</body></html>";
    let config = EngineConfig {
        script_budget_ms: BUDGET_MS,
        ..config()
    };
    let (mut engine, view, took) = load_timed(config, page);
    // The load itself (a local server, parse and layout) takes some time
    // of its own.
    assert!(
        took < std::time::Duration::from_millis(BUDGET_MS + 2_000),
        "the load took {took:?}"
    );
    let log = engine.script_log(view).unwrap();
    assert!(
        log.iter()
            .any(|record| record.outcome == ScriptOutcome::OverBudget),
        "{log:#?}"
    );
    // The spin ran, the script after it was not started, and the page runs
    // script once the load is over.
    assert_eq!(
        engine
            .execute_script(view, "window.first === true && window.second === undefined")
            .unwrap(),
        "Boolean(true)"
    );
}

/// The board runs with interruption off: no turn is published, and script
/// runs to its end as before.
#[test]
fn without_the_switch_the_watchdog_stops_nothing() {
    let page = "<html><body><script>
setTimeout(function () { var t0 = Date.now(); while (Date.now() - t0 < 400) {} window.done = true; }, 60000);
</script></body></html>";
    let config = EngineConfig {
        live_turn_budget_ms: 1,
        ..EngineConfig::default()
    };
    assert!(!config.interrupt_scripts_at_budget);
    let (mut engine, view) = load(config, page);
    runtime().block_on(engine.pump_live(view, 60_000));
    assert!(over_budget(&engine, view).is_empty());
    assert_eq!(
        engine.execute_script(view, "window.done").unwrap(),
        "Boolean(true)"
    );
}

/// A turn the watchdog stops reaches the Inspector the way T0's host-call
/// refusal does: the listener's console line, the engine's stop note, and
/// one `script_error` under the turn's source.
#[test]
fn a_stopped_listener_is_a_script_error_in_the_inspector() {
    let page = "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
document.getElementById('p').addEventListener('click', function () { console.log('spin'); while (true) {} });
</script></body></html>";
    let mut config = config();
    config.inspector.keep_console_text = true;
    config.inspector.keep_error_text = true;
    let server =
        crate::inspector_engine_tests::serve(vec![crate::inspector_engine_tests::Route::html(
            "/", page,
        )]);
    let (mut engine, view) = crate::inspector_engine_tests::inspected_engine(config);
    runtime()
        .block_on(engine.load_url(view, server.url("/")))
        .expect("load_url");
    engine.click_at_point(view, 20.0, 20.0);
    assert_eq!(over_budget(&engine, view), ["event:click"]);
    let lines: Vec<String> = crate::inspector_engine_tests::records(&engine, view)
        .iter()
        .map(|record| crate::inspector_engine_tests::describe(record, server.port))
        .filter(|line| line.starts_with("console") || line.starts_with("script_error"))
        .collect();
    assert_eq!(
        lines,
        [
            "console Log spin @inline#1:2:78",
            "console Warn Script stopped: it ran past 300 ms (event:click)",
            "script_error Error: the script was interrupted script=event:click rejection=false",
        ],
        "{lines:#?}"
    );
}
