//! A script that does not end, started after the load, is stopped (I0(a),
//! step M0 of docs/design/THREAD_OWNERSHIP_2026-10.md).
//!
//! The load's scripts have had a wall-clock budget since 2026-10-08. What
//! runs afterwards had none: a timer callback or an event listener inside
//! nested `forEach` callbacks (no loop statement, so the loop limit does not
//! count it) held the window's thread for as long as it ran. Real window,
//! 2026-10-12: `i0a_script` held the window thread for as long as the check
//! watched. This bounds the hold; it does not remove it.

use super::*;
use crate::script_net_tests::serve_routes;

/// About nine million callbacks, with a host call every 4096: far longer
/// than the budgets below, and stoppable at a host call.
const SPIN: &str = "var spun = 0; var row = new Array(3000).fill(0);
row.forEach(function () { row.forEach(function () { spun++; if (spun % 4096 === 0) { document.getElementById('p'); } }); });";

fn config() -> EngineConfig {
    EngineConfig {
        interrupt_scripts_at_budget: true,
        live_turn_budget_ms: 200,
        ..EngineConfig::default()
    }
}

fn load(config: EngineConfig, page: String) -> (Engine, EngineViewId) {
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    runtime().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
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

/// Both timers are due after the load's own timer horizon, so the load
/// leaves them for the live loop.
fn timer_page() -> String {
    format!(
        "<html><body><p id=p>alive</p><script>
setTimeout(function () {{ {SPIN} }}, 60000);
setTimeout(function () {{ window.after = true; document.title = 'after'; }}, 61000);
</script></body></html>"
    )
}

#[test]
fn a_timer_callback_still_running_at_the_turn_budget_is_stopped() {
    let (mut engine, view) = load(config(), timer_page());
    assert!(over_budget(&engine, view).is_empty(), "the load ran neither timer");
    let started = std::time::Instant::now();
    runtime().block_on(engine.pump_live(view, 60_000));
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_millis(1500), "the callback ran on past the turn budget: {took:?}");
    assert_eq!(over_budget(&engine, view), ["timers"], "{:#?}", engine.script_log(view));
    // The page is still a page: its next timer runs on a later turn, and
    // script runs in it.
    runtime().block_on(engine.pump_live(view, 1_000));
    assert_eq!(engine.execute_script(view, "window.after === true && spun > 0").unwrap(), "Boolean(true)");
    assert_eq!(engine.execute_script(view, "document.title").unwrap(), r#"String("after")"#);
}

#[test]
fn a_click_listener_still_running_at_the_turn_budget_is_stopped() {
    let page = format!(
        "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
document.getElementById('p').addEventListener('click', function () {{ {SPIN} }});
</script></body></html>"
    );
    let (mut engine, view) = load(config(), page);
    let started = std::time::Instant::now();
    engine.click_at_point(view, 20.0, 20.0);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_millis(1500), "the listener ran on past the turn budget: {took:?}");
    assert_eq!(over_budget(&engine, view), ["event:click"], "{:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "document.title = 'x'; spun > 0").unwrap(), "Boolean(true)");
}

/// The board's frames are taken with scripts that run to their end.
#[test]
fn without_the_switch_a_live_turn_has_no_budget() {
    let page = "<html><body><p id=p>alive</p><script>
setTimeout(function () { var n = 0; var row = new Array(150).fill(0);
  row.forEach(function () { row.forEach(function () { n++; document.getElementById('p'); }); }); window.n = n; }, 60000);
</script></body></html>";
    let config = EngineConfig { live_turn_budget_ms: 1, ..EngineConfig::default() };
    assert!(!config.interrupt_scripts_at_budget);
    let (mut engine, view) = load(config, page.to_string());
    runtime().block_on(engine.pump_live(view, 60_000));
    assert!(over_budget(&engine, view).is_empty(), "{:#?}", engine.script_log(view));
    assert_eq!(engine.execute_script(view, "window.n").unwrap(), "Number(22500.0)");
}
