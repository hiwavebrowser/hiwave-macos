//! TEXT-CACHE-STEP: a page with more distinct strings than a text memo
//! holds must not re-shape all of them on every pass.
//!
//! Both per-thread text memos (measurement in `measure_text_with_spacing`,
//! shaping in `TextShaper::shape`) were wiped completely when full. A pass
//! over `cap + n` distinct strings filled the memo, wiped it, refilled it,
//! and the next pass found almost nothing: every string was shaped again.
//! Forced layout per element stepped from ~7 to ~11 us between 16,300 and
//! 16,500 text rows (Windows measurement seat; Mac numbers in the PR).
//!
//! The counts here are shapes, not time. The web-font generation is
//! process-wide and other tests install web fonts on their threads, which
//! drops every memo; a round is only judged when the generation held still.

use crate::text::{shape_memo_misses, FontFamilyChain, TextShaper, SHAPED_TEXT_MEMO_CAP};
use crate::{measure_text_with_spacing, TEXT_MEASURE_MEMO_CAP, TEXT_SHAPES};
use rustkit_css::{FontStretch, FontStyle, FontWeight};

/// Distinct strings beyond the cap.
const OVER: usize = 1000;

fn row_text(i: usize) -> String {
    format!("row {}", 100_000 + i)
}

/// Runs `round` (one pass over the strings) until a pass happens with the
/// web-font generation unchanged; returns that pass's shape count.
fn steady_pass(mut round: impl FnMut() -> usize) -> usize {
    for _ in 0..10 {
        let generation = rustkit_text::webfonts::generation();
        let n = round();
        if rustkit_text::webfonts::generation() == generation {
            return n;
        }
    }
    panic!("the web-font generation never held still");
}

#[test]
fn a_second_measure_pass_over_more_strings_than_the_cap_reshapes_only_the_overflow() {
    let n = TEXT_MEASURE_MEMO_CAP + OVER;
    let pass = || {
        let before = TEXT_SHAPES.with(std::cell::Cell::get);
        for i in 0..n {
            measure_text_with_spacing(&row_text(i), "sans-serif", 16.0, FontWeight(400), FontStyle::Normal, 0.0, 0.0);
        }
        TEXT_SHAPES.with(std::cell::Cell::get) - before
    };
    let second = steady_pass(|| {
        pass();
        pass()
    });
    // Clear-when-full re-measured all of them (about `n`); the memo keeps
    // the `cap` it holds and measures only what does not fit.
    assert!(
        second <= OVER + OVER / 10,
        "second pass over {n} distinct strings measured {second} of them again (cap {TEXT_MEASURE_MEMO_CAP})"
    );
}

#[test]
fn a_second_shape_pass_over_more_strings_than_the_cap_reshapes_only_the_overflow() {
    let n = SHAPED_TEXT_MEMO_CAP + OVER;
    let shaper = TextShaper::new();
    let chain = FontFamilyChain::from_css_value("sans-serif");
    let pass = || {
        let before = shape_memo_misses();
        for i in 0..n {
            shaper
                .shape(&row_text(i), &chain, FontWeight(400), FontStyle::Normal, FontStretch::Normal, 16.0)
                .expect("shape");
        }
        shape_memo_misses() - before
    };
    let second = steady_pass(|| {
        pass();
        pass()
    });
    assert!(
        second <= OVER + OVER / 10,
        "second pass over {n} distinct strings shaped {second} of them again (cap {SHAPED_TEXT_MEMO_CAP})"
    );
}
