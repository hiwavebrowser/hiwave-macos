//! SORT1: a numeric `a - b` sort makes no call between comparisons, so the interrupt checks that run at a
//! call's entry never run. The turn budget must still stop a huge one (third_party/boa_engine/PATCHES.md).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use boa_engine::error::{EngineError, RuntimeLimitError};
use boa_engine::{Context, Source};

const ALL: u64 = u64::MAX;

fn stopped_after_set(sort: &str) -> Duration {
    let word = Arc::new(AtomicU64::new(0));
    let mut context = Context::default();
    context.set_interrupt_word(Some(word.clone()));
    context.set_interrupt_turn(ALL);
    // The array is built before the word is set, so the stop can only land in the sort.
    context
        .eval(Source::from_bytes(
            "var a = new Array(3000000); var s = 1; for (var i = 0; i < a.length; i++) { s = (s * 1103515245 + 12345) & 0x7fffffff; a[i] = s; }",
        ))
        .unwrap();
    let setter = {
        let word = word.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            word.fetch_max(ALL, Ordering::Relaxed);
            Instant::now()
        })
    };
    let result = context.eval(Source::from_bytes(sort));
    let ended = Instant::now();
    let set_at = setter.join().unwrap();
    let err = result.expect_err("the sort is stopped");
    assert!(
        matches!(err.as_engine(), Some(EngineError::RuntimeLimit(RuntimeLimitError::Interrupted))),
        "stopped by the interrupt, got {err}"
    );
    ended.saturating_duration_since(set_at)
}

#[test]
fn a_huge_numeric_difference_sort_is_stopped_by_the_turn_budget() {
    let late = stopped_after_set("a.sort(function (x, y) { return x - y; });");
    assert!(late < Duration::from_secs(2), "stopped {late:?} after the word was set");
}

#[test]
fn a_huge_numeric_difference_to_sorted_is_stopped_by_the_turn_budget() {
    let late = stopped_after_set("a.toSorted((x, y) => x - y);");
    assert!(late < Duration::from_secs(2), "stopped {late:?} after the word was set");
}
