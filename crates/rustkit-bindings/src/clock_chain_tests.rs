//! CLOCK3: a timer set during a live turn counts its delay from the turn's time, not from the
//! virtual due time of the callback that set it. After a long task the live loop's next turn moves
//! the clock by the whole wait; the timers due inside it ran each at its own due time, so a 10 ms
//! `setTimeout` chain set its next link 10 ms after a time already hundreds of ms in the past, and
//! replayed every link it had missed in that one turn (about 200 after a 2 s frame, PACE1's page).
//! Intervals were already re-based on the turn's time; chains were not.
//!
//! The page and the expected relations: the pinned Chrome (CfT 148.0.7778.216),
//! `docs/diagnostics/2026-11-15/chain_after_long_task{.html,_chrome.mjs,_cft148.json}`.

use super::*;

const PAGE: &str = include_str!("../../../docs/diagnostics/2026-11-15/chain_after_long_task.html");

/// The page's script: the one `<script>` of the fixture.
fn script() -> &'static str {
    let start = PAGE.find("<script>").unwrap() + "<script>".len();
    let end = PAGE.rfind("</script>").unwrap();
    &PAGE[start..end]
}

/// Turns the page's timers live, as the engine's live turn does (`advance_timers_for_frame` with the
/// real time since the last turn, a 16 ms frame budget) and waits between turns as the app's content
/// thread does (the next timer, 2 to 50 ms), until the page is done or 5 s pass. Returns `rec`.
fn run_live() -> serde_json::Value {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html("<html><head></head><body></body></html>").unwrap()))
        .unwrap();
    b.evaluate(&format!("(0, eval)({})", serde_json::to_string(script()).unwrap())).unwrap();
    let began = std::time::Instant::now();
    let mut clock = began;
    while began.elapsed() < std::time::Duration::from_secs(5) {
        let elapsed_ms = clock.elapsed().as_millis() as u64;
        clock += std::time::Duration::from_millis(elapsed_ms);
        b.advance_timers_for_frame(elapsed_ms, 1000, 16).unwrap();
        if matches!(b.evaluate("rec.done").unwrap(), JsValue::Boolean(true)) {
            break;
        }
        let wait = b.next_timer_delay().unwrap_or(50).clamp(2, 50);
        std::thread::sleep(std::time::Duration::from_millis(wait));
    }
    match b.evaluate("JSON.stringify(rec)").unwrap() {
        JsValue::String(s) => serde_json::from_str(&s).unwrap(),
        other => panic!("{other:?}"),
    }
}

/// Chrome, five runs: chainBurst 1, chainMinGapMs 10 to 10.1, chainAfter 28, intervalBurst 1 to 2,
/// intervalAfter 30 to 31 (longMs 500).
#[test]
fn a_timeout_chain_resumes_at_its_own_pace_after_a_long_task() {
    let rec = run_live();
    assert_eq!(rec["done"], true, "the page did not finish: {rec}");
    assert!(rec["longMs"].as_f64().unwrap() >= 500.0, "{rec}");
    // The one link that was overdue runs; the next is set from then.
    assert_eq!(rec["chainBurst"], 1, "chain links run at once after the long task: {rec}");
    assert!(
        rec["chainMinGapMs"].as_f64().unwrap() >= 9.5,
        "two chain links less than their 10 ms apart after the long task: {rec}"
    );
    // No replay: at most a link per 10 ms in the 300 ms after it (Chrome 28).
    assert!(rec["chainAfter"].as_u64().unwrap() <= 31, "chain links in the 300 ms after: {rec}");
    // Intervals were already re-based: one overdue run, then their period (Chrome 1 to 2, 30 to 31).
    assert!(rec["intervalBurst"].as_u64().unwrap() <= 2, "{rec}");
    assert!(rec["intervalAfter"].as_u64().unwrap() <= 32, "{rec}");
}
