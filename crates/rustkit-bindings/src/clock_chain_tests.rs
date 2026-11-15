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
const ZERO_DELAY: &str = include_str!("../../../docs/diagnostics/2026-11-15/zero_delay_chain.html");

/// A fixture's script: its one `<script>`.
fn script(page: &'static str) -> &'static str {
    let start = page.find("<script>").unwrap() + "<script>".len();
    let end = page.rfind("</script>").unwrap();
    &page[start..end]
}

/// Turns a fixture's timers live, as the engine's live turn on the real clock does
/// (`advance_real_timers_for_frame` with the real time since the last turn, a 16 ms frame budget),
/// and waits between turns as the app's content thread does (the next timer, 2 to 50 ms), until the
/// page is done or 5 s pass. Returns `rec`. `real` false turns it on the stepped clock instead
/// (`advance_timers_for_frame`), as every live turn did before CLOCK3.
fn run_live(page: &'static str, real: bool) -> serde_json::Value {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html("<html><head></head><body></body></html>").unwrap(),
    ))
    .unwrap();
    b.evaluate(&format!(
        "(0, eval)({})",
        serde_json::to_string(script(page)).unwrap()
    ))
    .unwrap();
    // A fixture starts in `go()` (at the load event in Chrome).
    b.evaluate("go()").unwrap();
    let began = std::time::Instant::now();
    let mut clock = began;
    while began.elapsed() < std::time::Duration::from_secs(5) {
        let elapsed_ms = clock.elapsed().as_millis() as u64;
        clock += std::time::Duration::from_millis(elapsed_ms);
        if real {
            b.advance_real_timers_for_frame(elapsed_ms, 1000, 16)
                .unwrap();
        } else {
            b.advance_timers_for_frame(elapsed_ms, 1000, 16).unwrap();
        }
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
    let rec = run_live(PAGE, true);
    eprintln!("CLOCK3-RESULT {rec}");
    assert_eq!(rec["done"], true, "the page did not finish: {rec}");
    assert!(rec["longMs"].as_f64().unwrap() >= 500.0, "{rec}");
    // The one link that was overdue runs; the next is set from then. At most one in the 5 ms after
    // the long task (the overdue link may land just past them on a loaded machine), and a link
    // after it.
    assert!(
        rec["chainBurst"].as_u64().unwrap() <= 1,
        "chain links run at once after the long task: {rec}"
    );
    assert!(
        rec["chainAfter"].as_u64().unwrap() >= 1,
        "no chain link after the long task: {rec}"
    );
    // Chrome 10. Up to 1 ms less here: the live loop's turn delta is whole milliseconds, so the
    // virtual clock a link is set on trails the real clock by under a millisecond (0 before CLOCK3).
    assert!(
        rec["chainMinGapMs"].as_f64().unwrap() >= 9.0,
        "two chain links less than their 10 ms apart after the long task: {rec}"
    );
    // No replay: at most a link per 10 ms in the 300 ms after it (Chrome 28).
    assert!(
        rec["chainAfter"].as_u64().unwrap() <= 31,
        "chain links in the 300 ms after: {rec}"
    );
    // An interval: one overdue run, then its period on its grid (Chrome 1 to 2, 30 to 31).
    assert!(rec["intervalBurst"].as_u64().unwrap() <= 2, "{rec}");
    assert!(rec["intervalAfter"].as_u64().unwrap() <= 32, "{rec}");
}

/// A stepped turn (a capture's settle, a test) moves virtual time the page has not lived through, so
/// a chain still runs a link per period of it: five 1000 ms links in a 5000 ms step, as before
/// CLOCK3. On the real clock the same turn is 5000 ms the page was held back: the overdue link runs
/// once, and the next is at least a period after the turn's time (plus the real time the turn had
/// taken when it was set). The period is long so the count cannot depend on the machine: only a
/// stall of a whole second inside the turn would let the next link into it.
#[test]
fn a_stepped_turn_still_runs_a_chain_link_per_period_of_virtual_time() {
    let count = |real: bool| {
        let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
        b.evaluate("var n = 0; (function link() { n++; setTimeout(link, 1000); })();")
            .unwrap();
        if real {
            b.advance_real_timers_for_frame(5000, 1000, 0).unwrap();
        } else {
            b.advance_timers_for_frame(5000, 1000, 0).unwrap();
        }
        let n = match b.evaluate("String(n)").unwrap() {
            JsValue::String(s) => s,
            other => panic!("{other:?}"),
        };
        (n, b.next_timer_delay())
    };
    // The first link runs at once, then one per 1000 ms: 1000, 2000, 3000, 4000, 5000.
    assert_eq!(count(false), ("6".to_string(), Some(1000)));
    let (n, next) = count(true);
    assert_eq!(n, "2");
    assert!(
        next.is_some_and(|d| d >= 1000),
        "the next link is due in {next:?} ms"
    );
}

/// A frame tree is one timer queue on one clock (#1109), so the real clock's turn is the tree's: a
/// chain a same-origin frame set, run by the page's turn, counts from the turn's time as the page's
/// own does (2 links in a 50 ms real turn, 6 stepped), and the page's and the frame's chains, set
/// together, run in step.
#[test]
fn a_frames_chain_on_the_real_clock_counts_from_the_trees_turn() {
    let count = |real: bool| {
        let parent = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
        parent
            .set_document(Rc::new(Document::parse_html("<html><body></body></html>").unwrap()))
            .unwrap();
        let child = parent.new_child().unwrap();
        child
            .set_document(Rc::new(Document::parse_html("<html><body></body></html>").unwrap()))
            .unwrap();
        child
            .evaluate("var n = 0; (function link() { n++; setTimeout(link, 10); })();")
            .unwrap();
        parent
            .evaluate("var m = 0; (function link() { m++; setTimeout(link, 10); })();")
            .unwrap();
        if real {
            parent.advance_real_timers_for_frame(50, 1000, 0).unwrap();
        } else {
            parent.advance_timers_for_frame(50, 1000, 0).unwrap();
        }
        let read = |b: &DomBindings, v: &str| match b.evaluate(&format!("String({v})")).unwrap() {
            JsValue::String(s) => s,
            other => panic!("{other:?}"),
        };
        (read(&child, "n"), read(&parent, "m"), parent.next_timer_delay())
    };
    assert_eq!(count(false), ("6".to_string(), "6".to_string(), Some(10)));
    let (n, m, next) = count(true);
    assert_eq!((n.as_str(), m.as_str()), ("2", "2"));
    assert!(next.is_some_and(|d| d >= 10), "the next link is due in {next:?} ms");
}

/// An interval held back on the real clock runs once and keeps its grid, as Chrome's does (bursts of
/// 1 to 2 and 30 to 31 runs in the 300 ms after a long task): set at 0 with a 10 ms period, held back
/// for 55 ms, it runs once and next at 60, not at 65 (re-based) nor five times (caught up). Its next
/// is the first grid point past the turn's time plus the real time the turn took, so on a loaded
/// machine it can be 70; either way 5 ms off the turn's time modulo 10.
#[test]
fn an_interval_held_back_on_the_real_clock_runs_once_and_keeps_its_grid() {
    // A 1000 ms period held back 5500 ms (the 10 ms and 55 ms of Chrome's case, scaled so that only
    // a whole-second stall inside a turn could change a count).
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.evaluate("var n = 0; setInterval(function () { n++; }, 1000);")
        .unwrap();
    b.advance_real_timers_for_frame(5500, 1000, 0).unwrap();
    assert!(matches!(b.evaluate("String(n)").unwrap(), JsValue::String(s) if s == "1"));
    let next = b.next_timer_delay().expect("the interval is set");
    assert_eq!(
        next % 1000,
        500,
        "the interval left its grid: next in {next} ms"
    );
    // On time, once a period.
    b.advance_real_timers_for_frame(next, 1000, 0).unwrap();
    assert!(matches!(b.evaluate("String(n)").unwrap(), JsValue::String(s) if s == "2"));
}

/// Animation frames are the same on both clocks: one round a turn, and a frame a timer of the turn
/// asks for (due a frame after that timer) is in the round when the turn reaches it.
#[test]
fn animation_frames_are_the_same_on_the_real_clock() {
    let run = |real: bool| {
        let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
        b.evaluate(
            "var frames = 0, log = [];              (function f() { frames++; requestAnimationFrame(f); })();              setTimeout(function () { requestAnimationFrame(function () { log.push('f'); }); }, 50);",
        )
        .unwrap();
        let mut seen = Vec::new();
        for delta in [300, 16, 16] {
            if real {
                b.advance_real_timers_for_frame(delta, 1000, 0).unwrap();
            } else {
                b.advance_timers_for_frame(delta, 1000, 0).unwrap();
            }
            seen.push(match b.evaluate("frames + ' ' + log.join()").unwrap() {
                JsValue::String(s) => s,
                other => panic!("{other:?}"),
            });
        }
        seen
    };
    // The loop's first call is the script's own; then one frame a turn, and the timer's frame in the
    // first turn's round.
    assert_eq!(run(false), ["2 f", "3 f", "4 f"]);
    assert_eq!(run(true), run(false));
}

/// Measurement only (CLOCK3 item 1): the zero-delay page on both clocks.
#[test]
#[ignore]
fn measure_zero_delay_chain() {
    for real in [false, true] {
        for _ in 0..3 {
            eprintln!("ZERO-DELAY real={real} {}", run_live(ZERO_DELAY, real));
        }
    }
}
