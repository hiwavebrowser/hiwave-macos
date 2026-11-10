//! JS1: the per-loop iteration cap is a guard for configs without the
//! watchdog, not the thing that decides how long a page's script may run.
//! At 10M iterations an optimized Boa threw `RuntimeLimitError` after
//! 50 to 70 ms, so a legitimate heavy loop (a 12M-iteration hash or a
//! table build) was killed far inside the 5 s turn budget. The time budget
//! (`interrupt_scripts_at_budget`) is what stops a runaway now.

#![cfg(feature = "headless")]

use super::*;

fn view(config: EngineConfig) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    (engine, view)
}

fn threw(engine: &Engine, view: EngineViewId) -> Vec<String> {
    engine
        .script_log(view)
        .unwrap()
        .iter()
        .filter_map(|r| match &r.outcome {
            ScriptOutcome::Threw(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_twelve_million_iteration_loop_completes_in_one_turn() {
    let (mut engine, id) = view(EngineConfig::default());
    engine
        .load_html(id, "<html><body><script>(function () { var n = 0; for (var i = 0; i < 12000000; i++) { n++; } window.done = n; })();</script></body></html>")
        .expect("load_html");
    assert_eq!(engine.execute_script(id, "window.done").unwrap(), "Number(12000000.0)");
    assert!(threw(&engine, id).is_empty(), "{:?}", threw(&engine, id));
}

#[test]
fn while_true_is_still_stopped_by_the_time_budget() {
    let config = EngineConfig {
        interrupt_scripts_at_budget: true,
        script_budget_ms: 300,
        ..EngineConfig::default()
    };
    let (mut engine, id) = view(config);
    let started = std::time::Instant::now();
    engine
        .load_html(id, "<html><body><script>while (true) {}</script></body></html>")
        .expect("load_html");
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
    let log = engine.script_log(id).unwrap();
    assert!(
        log.iter().any(|r| r.outcome == ScriptOutcome::OverBudget),
        "stopped by the budget, not the iteration cap: {log:#?}"
    );
}

#[test]
fn the_microtask_cap_does_not_follow_the_loop_cap() {
    let (mut engine, id) = view(EngineConfig::default());
    let started = std::time::Instant::now();
    engine
        .load_html(id, "<html><body><p>alive</p><script>function again() { Promise.resolve().then(again); } again();</script></body></html>")
        .expect("load_html");
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "{:?}", started.elapsed());
}
