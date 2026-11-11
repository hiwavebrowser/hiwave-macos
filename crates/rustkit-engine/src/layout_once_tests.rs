//! LAT21: a box must be laid out about once per layout pass. walmart.com ran ~160 visits per box
//! (256,036 visits over 1,598 boxes in one forced layout; 76% of the 665 ms was block layout
//! re-run through `layout_block_children_with_collapse`).
//!
//! Root cause, measured on these pages with `rustkit_layout::take_layout_visits`: a flex container
//! is laid out TWICE per pass, once by the block pre-pass (its items as block children) and once by
//! the flex pass (`flex.rs`, `item.layout_box.layout_block_children_with_collapse`), and each of
//! those lays out every nested flex container the same way. So visits grow as 2^k in the number k
//! of flex containers nested through blocks. Direct flex-in-flex is only quadratic, floats and
//! inline-blocks add a constant factor, plain blocks and abspos are flat.
//!
//! Measured (visits / boxes): flex+block alternation d=4: 3.5, d=8: 9.3, d=12: 27.2; flex+float
//! d=12: 109.9. Each extra nested flex roughly doubles the work.

#![cfg(feature = "headless")]

use super::*;

/// Layout visits for one `load_html` of `html`.
fn visits(html: &str) -> u64 {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 1200, height: 800 })
        .expect("view");
    rustkit_layout::take_layout_visits();
    engine.load_html(id, html).expect("load_html");
    rustkit_layout::take_layout_visits()
}

/// `depth` levels cycling through `parts` (open, close), around the text "text".
fn nest(parts: &[(&str, &str)], depth: usize) -> String {
    let mut open = String::new();
    let mut close = Vec::new();
    for i in 0..depth {
        let (o, c) = parts[i % parts.len()];
        open.push_str(o);
        close.push(c);
    }
    close.reverse();
    format!("<html><body>{open}text{}</body></html>", close.concat())
}

const FLEX: (&str, &str) = ("<div style=\"display:flex\"><span>w</span>", "</div>");
const BLOCK: (&str, &str) = ("<div>", "</div>");
const INLINE_BLOCK: (&str, &str) = ("<div style=\"display:inline-block\"><span>a</span>", "</div>");
const FLOAT: (&str, &str) = ("<div style=\"float:left\">", "</div>");
const ABSPOS: (&str, &str) = ("<div style=\"position:absolute;inset:0\">", "</div>");

/// Elements in the page (a stand-in for boxes: every `div` makes one, plus html and body).
fn boxes(html: &str) -> u64 {
    (html.matches("<div").count() + 2) as u64
}

#[test]
fn plain_blocks_and_abspos_are_laid_out_once_each() {
    for parts in [&[BLOCK][..], &[ABSPOS][..]] {
        let html = nest(parts, 24);
        let (v, b) = (visits(&html), boxes(&html));
        assert!(v <= b + b / 4, "{v} visits for {b} boxes: {html:.80}");
    }
}

#[test]
fn inline_blocks_and_nested_floats_stay_within_a_small_multiple_of_the_boxes() {
    // Inline-blocks holding inline content cost a constant ~3 per box; nested floats grow
    // quadratically but stay small at the depths real pages use.
    let ib = nest(&[INLINE_BLOCK], 12);
    assert!(visits(&ib) <= 4 * boxes(&ib), "inline-block: {} visits for {} boxes", visits(&ib), boxes(&ib));
    let fl = nest(&[FLOAT], 8);
    assert!(visits(&fl) <= 6 * boxes(&fl), "float: {} visits for {} boxes", visits(&fl), boxes(&fl));
}

/// The pin for the fix. Red today: each flex container nested through a block doubles the work,
/// so 12 levels (6 flex containers) cost 27 visits per box and 12 levels of flex+float cost 110.
/// Remove the `ignore` when a flex container's items are laid out once per pass.
#[test]
#[ignore = "LAT21: red until a flex container's items are laid out once per pass, not by both the block pre-pass and the flex pass"]
fn nested_flex_containers_are_laid_out_once_per_pass_not_once_per_nesting_level() {
    for (name, parts) in [("flex/block", &[FLEX, BLOCK][..]), ("flex/float", &[FLEX, FLOAT][..])] {
        let html = nest(parts, 12);
        let (v, b) = (visits(&html), boxes(&html));
        assert!(v <= 4 * b, "{name}: {v} visits for {b} boxes = {:.1} per box (want <= 4)", v as f64 / b as f64);
    }
}

/// What the ignored pin measures, kept as a number so the next change can see the doubling move.
#[test]
fn flex_nesting_doubles_the_work_per_container_today() {
    let (shallow, deep) = (nest(&[FLEX, BLOCK], 8), nest(&[FLEX, BLOCK], 12));
    let (vs, vd) = (visits(&shallow), visits(&deep));
    // Two more flex containers: ~4x the visits today (93 -> 381). If this ever fails because the
    // ratio fell, the fix landed: delete this test and un-ignore the pin above.
    assert!(vd >= 3 * vs, "expected the exponential growth this row documents: {vs} -> {vd}");
}
