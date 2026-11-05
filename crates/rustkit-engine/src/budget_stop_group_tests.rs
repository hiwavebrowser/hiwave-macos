//! Repeated budget stops are one row (T20, Pete 2026-11-04: "thousands of
//! errors reported in the inspector"; 959 identical "Script stopped at the
//! script budget" lines on reddit). A listener stopped on every
//! `pointermove` gets one log line and one stop note per page and source
//! every 5 s, and the Inspector console shows its stops as one row with a
//! count and the first and last time. Every stop is still recorded.

use super::*;
use crate::inspector_engine_tests::{inspected_engine, runtime, serve, Route};

const SECOND: std::time::Duration = std::time::Duration::from_secs(1);

#[test]
fn stop_lines_are_one_per_page_and_source_every_five_seconds() {
    let t0 = std::time::Instant::now();
    let mut lines = StopLines::default();
    let at = |ms: u64| t0 + std::time::Duration::from_millis(ms);
    // The first stop gets a line; 99 more inside the window are folded.
    assert_eq!(lines.admit(1, "event:pointermove", at(0)), Some(0));
    let folded = (1..100).filter(|i| lines.admit(1, "event:pointermove", at(i * 40)).is_some()).count();
    assert_eq!(folded, 0, "stops inside the window get no line of their own");
    // Another source, or another page, is its own line.
    assert_eq!(lines.admit(1, "timers", at(4_000)), Some(0));
    assert_eq!(lines.admit(2, "event:pointermove", at(4_000)), Some(0));
    // Past the window: one line standing for the 99 folded since.
    assert_eq!(lines.admit(1, "event:pointermove", at(0) + 5 * SECOND), Some(99));
    assert_eq!(lines.admit(1, "event:pointermove", at(0) + 5 * SECOND + SECOND), None);
}

#[test]
fn a_hundred_pointermove_stops_are_one_console_row_with_a_count() {
    let page = "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
document.addEventListener('pointermove', function () { for (;;) { document.body.id; } });
</script></body></html>";
    let config = EngineConfig {
        interrupt_scripts_at_budget: true,
        live_turn_budget_ms: 5,
        script_loop_iteration_limit: u64::MAX / 2,
        ..EngineConfig::default()
    };
    let server = serve(vec![Route::html("/", page)]);
    let (mut engine, view) = inspected_engine(config);
    runtime().block_on(engine.load_url(view, server.url("/"))).expect("load_url");
    engine.devtools_attach(view, true).unwrap();
    let since = engine.devtools_console(view, 0).unwrap().next_seq;
    for i in 0..100 {
        engine.mouse_move_at_point(view, 10.0 + (i % 2) as f32, 20.0);
    }
    let stops = engine
        .script_log(view)
        .unwrap()
        .iter()
        .filter(|r| r.source == "event:pointermove" && r.outcome == ScriptOutcome::OverBudget)
        .count();
    assert_eq!(stops, 100, "every stop is still recorded");

    let console = engine.devtools_console(view, since).unwrap();
    let rows: Vec<(&str, &str, u32)> = console.entries.iter().map(|e| (e.kind, e.level, e.repeat)).collect();
    let errors: Vec<_> = console.entries.iter().filter(|e| e.kind == "script_error").collect();
    assert_eq!(errors.len(), 1, "one row for the stops: {rows:?}");
    assert_eq!(errors[0].repeat, 100, "{rows:?}");
    assert!(errors[0].message.ends_with("[event:pointermove]"), "{}", errors[0].message);
    assert!(errors[0].first_ms <= errors[0].last_ms);
    let notes = console.entries.iter().filter(|e| e.kind == "console").count();
    assert_eq!(notes, 1, "one stop note for the burst: {rows:?}");
    assert_eq!(console.entries.len(), 2, "{rows:?}");
}
