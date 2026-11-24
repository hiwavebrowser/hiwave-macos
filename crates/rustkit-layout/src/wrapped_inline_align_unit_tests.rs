//! YT-CARD-SUBTITLE (#1407): portable pins for per-line center/right of a wrapped inline.
//!
//! The engine companion `wrapped_inline_align_tests` is `macos`+`headless` and pins Menlo
//! line rects against Chrome 148. Default Linux `--lib` never exercised
//! `align_wrapped_inline_records` / `wrapped_text_path` — the path that seats each visual
//! line of a `<span>` (or `<b><span>`) on its own under `text-align: center|right`, and that
//! keeps a shared last line (` B`, a link) aligned as one with the inline's tail.
//!
//! These tests hand-build a synthetic IFC (no font oracle): line widths and records are set
//! directly, so the arithmetic is deterministic on every platform.

use super::*;
use rustkit_css::Display;

fn line(width: f32) -> TextLine {
    TextLine {
        text: String::new(),
        width,
        x_offset: 0.0,
        justify_space: 0.0,
    }
}

fn wrapped_text(widths: &[f32]) -> LayoutBox {
    let mut run = LayoutBox::new(BoxType::Text("…".into()), ComputedStyle::new());
    let max_w = widths.iter().copied().fold(0.0_f32, f32::max);
    run.text_lines = Some(widths.iter().copied().map(line).collect());
    run.dimensions.content = Rect::new(0.0, 0.0, max_w, 20.0 * widths.len() as f32);
    run
}

fn inline_holding(run: LayoutBox) -> LayoutBox {
    let mut style = ComputedStyle::new();
    style.display = Display::Inline;
    let mut span = LayoutBox::new(BoxType::Inline, style);
    span.dimensions.content = run.dimensions.content;
    span.children.push(run);
    span
}

/// Nested `<b><span>…</span></b>` — the path through two non-atomic inlines.
fn nested_inline_holding(run: LayoutBox) -> LayoutBox {
    let mut inner_style = ComputedStyle::new();
    inner_style.display = Display::Inline;
    let mut inner = LayoutBox::new(BoxType::Inline, inner_style);
    inner.dimensions.content = run.dimensions.content;
    inner.children.push(run);

    let mut outer_style = ComputedStyle::new();
    outer_style.display = Display::Inline;
    let mut outer = LayoutBox::new(BoxType::Inline, outer_style);
    outer.dimensions.content = inner.dimensions.content;
    outer.children.push(inner);
    outer
}

fn follower(x: f32, width: f32) -> LayoutBox {
    let mut style = ComputedStyle::new();
    style.display = Display::Inline;
    let mut b = LayoutBox::new(BoxType::Inline, style);
    b.dimensions.content = Rect::new(x, 0.0, width, 20.0);
    b
}

#[test]
fn wrapped_text_path_names_a_single_run_under_nested_inlines() {
    let widths = [100.0_f32, 80.0, 60.0];
    let span = inline_holding(wrapped_text(&widths));
    assert_eq!(LayoutBox::wrapped_text_path(&span), Some(vec![0]));

    let nested = nested_inline_holding(wrapped_text(&widths));
    assert_eq!(LayoutBox::wrapped_text_path(&nested), Some(vec![0, 0]));

    // One line, or a mid-line split, is not this path.
    let mut single = inline_holding(wrapped_text(&[100.0]));
    assert_eq!(LayoutBox::wrapped_text_path(&single), None);
    single.children[0].text_lines = Some(vec![line(50.0), line(40.0)]);
    single.children[0].text_flow_first_offset = Some(12.0);
    assert_eq!(LayoutBox::wrapped_text_path(&single), None);

    // Bare text / atomic inline: not taken.
    assert_eq!(LayoutBox::wrapped_text_path(&wrapped_text(&widths)), None);
    let mut atomic = inline_holding(wrapped_text(&widths));
    atomic.style.display = Display::InlineBlock;
    assert_eq!(LayoutBox::wrapped_text_path(&atomic), None);
}

#[test]
fn a_wrapped_span_centers_each_line_on_its_own() {
    // Container 200. Line widths 100 / 80 / 60 → centered origins 50 / 60 / 70.
    let widths = [100.0_f32, 80.0, 60.0];
    let mut children = vec![inline_holding(wrapped_text(&widths))];
    let records = [(0, 1, 200.0), (0, 1, 200.0), (0, 1, 200.0)];
    let aligned = LayoutBox::align_wrapped_inline_records(&mut children, &records, TextAlign::Center);
    assert_eq!(aligned, vec![true, true, true]);

    let span = &children[0];
    assert!(
        (span.dimensions.content.x - 50.0).abs() < 1e-5,
        "line 0 origin (span shift): got {}",
        span.dimensions.content.x
    );
    let lines = span.children[0].text_lines.as_ref().expect("lines");
    assert!((lines[0].x_offset - 0.0).abs() < 1e-5, "line 0 keeps flow offset");
    assert!(
        (lines[1].x_offset - 10.0).abs() < 1e-5,
        "line 1 relative: got {}",
        lines[1].x_offset
    );
    assert!(
        (lines[2].x_offset - 20.0).abs() < 1e-5,
        "line 2 relative: got {}",
        lines[2].x_offset
    );
    // Visual origins: shift + x_offset.
    let visual: Vec<f32> = lines.iter().map(|tl| span.dimensions.content.x + tl.x_offset).collect();
    assert!(
        (visual[0] - 50.0).abs() < 1e-5 && (visual[1] - 60.0).abs() < 1e-5 && (visual[2] - 70.0).abs() < 1e-5,
        "visual origins {visual:?}"
    );
}

#[test]
fn a_wrapped_span_rights_each_line_on_its_own() {
    let widths = [100.0_f32, 80.0, 60.0];
    let mut children = vec![inline_holding(wrapped_text(&widths))];
    let records = [(0, 1, 200.0), (0, 1, 200.0), (0, 1, 200.0)];
    let aligned = LayoutBox::align_wrapped_inline_records(&mut children, &records, TextAlign::Right);
    assert_eq!(aligned, vec![true, true, true]);

    let span = &children[0];
    // Right: origins 100 / 120 / 140 → shift 100, relatives 0 / 20 / 40.
    assert!((span.dimensions.content.x - 100.0).abs() < 1e-5);
    let lines = span.children[0].text_lines.as_ref().unwrap();
    assert!((lines[1].x_offset - 20.0).abs() < 1e-5);
    assert!((lines[2].x_offset - 40.0).abs() < 1e-5);
}

#[test]
fn a_shared_last_line_centers_the_inline_tail_with_what_follows() {
    // R1 on #1407: last record also holds a follower (` B`). The last line
    // (60) + follower (20) = 80 → centered at 60; follower lands after the tail.
    let widths = [100.0_f32, 80.0, 60.0];
    let mut children = vec![
        inline_holding(wrapped_text(&widths)),
        follower(60.0, 20.0), // flow place: after the last fragment
    ];
    let records = [(0, 1, 200.0), (0, 1, 200.0), (0, 2, 200.0)];
    let aligned = LayoutBox::align_wrapped_inline_records(&mut children, &records, TextAlign::Center);
    assert_eq!(aligned, vec![true, true, true]);

    assert!(
        (children[0].dimensions.content.x - 50.0).abs() < 1e-5,
        "line 0 still centers the longest line"
    );
    let lines = children[0].children[0].text_lines.as_ref().unwrap();
    // o_last = (200 - 80) / 2 = 60; relative last = 60 - 50 = 10.
    assert!((lines[2].x_offset - 10.0).abs() < 1e-5, "shared last relative: {}", lines[2].x_offset);
    assert!(
        (children[1].dimensions.content.x - 120.0).abs() < 1e-5,
        "follower after centered tail: got {}",
        children[1].dimensions.content.x
    );
}

#[test]
fn left_and_a_wrapping_follower_leave_the_records_alone() {
    let widths = [100.0_f32, 80.0, 60.0];
    let mut children = vec![inline_holding(wrapped_text(&widths))];
    let records = [(0, 1, 200.0), (0, 1, 200.0), (0, 1, 200.0)];
    let left = LayoutBox::align_wrapped_inline_records(&mut children, &records, TextAlign::Left);
    assert_eq!(left, vec![false, false, false]);
    assert_eq!(children[0].dimensions.content.x, 0.0);

    // A follower that itself wraps makes the group ineligible.
    let mut wrapping_follow = follower(60.0, 40.0);
    wrapping_follow.children.push(wrapped_text(&[30.0, 20.0]));
    let mut kids = vec![inline_holding(wrapped_text(&widths)), wrapping_follow];
    let shared = [(0, 1, 200.0), (0, 1, 200.0), (0, 2, 200.0)];
    let skipped = LayoutBox::align_wrapped_inline_records(&mut kids, &shared, TextAlign::Center);
    assert_eq!(skipped, vec![false, false, false]);
    assert_eq!(kids[0].dimensions.content.x, 0.0);
}
