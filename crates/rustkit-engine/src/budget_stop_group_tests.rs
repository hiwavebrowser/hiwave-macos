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
    let started = std::time::Instant::now();
    for i in 0..100 {
        engine.mouse_move_at_point(view, 10.0 + (i % 2) as f32, 20.0);
    }
    let elapsed = started.elapsed();
    let stops = engine
        .script_log(view)
        .unwrap()
        .iter()
        .filter(|r| r.source == "event:pointermove" && r.outcome == ScriptOutcome::OverBudget)
        .count();
    assert_eq!(stops, 100, "every stop is still recorded");

    let console = engine.devtools_console(view, since).unwrap();
    let rows: Vec<(&str, &str, u32)> = console.entries.iter().map(|e| (e.kind, e.message.as_str(), e.repeat)).collect();
    let fixed = "Uncaught Error: stopped at the script budget [event:pointermove]";
    let errors: Vec<_> = console.entries.iter().filter(|e| e.kind == "script_error" && e.message.ends_with("[event:pointermove]")).collect();
    // "Script stopped: it ran past 5 ms (event:pointermove)", as a shape.
    let notes = console.entries.iter().filter(|e| e.kind == "console" && e.message == "<string len=52>").count();
    // The 5 ms budget is the page's own: on a loaded machine the engine's
    // dispatch of `mousemove` or `pointerover` (no listener) can pass it
    // too. Those are real stops of other sources; each ends the pointermove
    // run in the console, as any other line does.
    let foreign = console.entries.iter().any(|e| e.kind == "script_error" && !e.message.ends_with("[event:pointermove]"));
    // A quiet machine runs the 100 stops well inside one 5 s window: one
    // note, then one row x100. A loaded one may take longer, and then each
    // new window's note starts a new row; the stops still add up to 100.
    let windows = (elapsed.as_secs() / STOP_LINE_EVERY.as_secs()) as usize + 1;
    assert!((1..=windows).contains(&notes), "one stop note per 5 s window: {rows:?} in {elapsed:?}");
    assert_eq!(errors.iter().map(|e| e.repeat).sum::<u32>(), 100, "{rows:?}");
    for e in &errors {
        // One engine line whichever way each stop came (the watchdog's
        // interrupt or the host call refused past the deadline): before
        // T20 the two texts split the run into rows.
        assert_eq!(e.message, fixed, "{rows:?}");
        assert!(e.first_ms <= e.last_ms);
    }
    if !foreign {
        assert_eq!(errors.len(), notes, "one row per window: {rows:?}");
        assert_eq!(console.entries.len(), 2 * notes, "{rows:?}");
        if elapsed < STOP_LINE_EVERY {
            assert_eq!(rows, [("console", "<string len=52>", 1), ("script_error", fixed, 100)]);
        }
    }
}

/// The stop note is page script too (`console.warn` may be the page's own
/// function by now). When it throws, or runs past its own short budget on
/// a loaded machine, its error lands after the stop's in the same drain:
/// the stop is still the one the fixed line replaces, not whatever came
/// last (the T20 suite run under load gave 101 error rows for 100 stops).
#[test]
fn an_error_from_the_stop_note_is_not_taken_for_the_stop() {
    let page = "<html><body style=\"margin:0\"><p id=p style=\"margin:0;height:100px\">alive</p><script>
console.warn = function () { throw new TypeError('note'); };
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
    let started = std::time::Instant::now();
    for i in 0..3 {
        engine.mouse_move_at_point(view, 10.0 + (i % 2) as f32, 20.0);
    }
    if started.elapsed() >= STOP_LINE_EVERY {
        // A second window runs the note twice; the rule is the same.
        return;
    }
    let console = engine.devtools_console(view, since).unwrap();
    let rows: Vec<(&str, &str, u32)> = console.entries.iter().map(|e| (e.kind, e.message.as_str(), e.repeat)).collect();
    let fixed = "Uncaught Error: stopped at the script budget [event:pointermove]";
    // Only pointermove's rows: on a loaded machine the engine's dispatch of
    // `pointerover` and the rest (no listener) can pass the 5 ms budget too,
    // each a real stop of its own source with a note of its own.
    let mine: Vec<_> = console.entries.iter().filter(|e| e.message.ends_with("[event:pointermove]")).collect();
    let stops: u32 = mine.iter().filter(|e| e.message == fixed).map(|e| e.repeat).sum();
    assert_eq!(stops, 3, "each stop is the fixed line once: {rows:?}");
    let others: Vec<_> = mine.iter().filter(|e| e.kind == "script_error" && e.message != fixed).collect();
    assert_eq!(others.len(), 1, "the note's own error, once: {rows:?}");
    assert!(others[0].message.contains("TypeError"), "{rows:?}");
}
