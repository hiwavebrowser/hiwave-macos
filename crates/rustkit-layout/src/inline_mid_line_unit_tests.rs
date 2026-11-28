//! INLINE-MID-LINE (#1422) + PRE-WRAP-NEWLINE-AT-INLINE-END (#1497): portable pins for the
//! mid-line refuse and the preserved-newline break at an inline's end.
//!
//! The engine companions (`inline_mid_line_tests`, `pre_wrap_newline_inline_end_tests`) are
//! `feature = "headless"` only, so default Linux `--lib` never exercised `holds_spaced_text` /
//! `stays_out_of_flow` / `inline_ends_with_preserved_newline`. After PRE-WRAP-NEWLINE-AT-INLINE-END,
//! a preserved newline no longer refuses mid-line (`stays_out_of_flow`); the line loops close via
//! `inline_ends_with_preserved_newline` instead.

use super::*;
use rustkit_css::{Display, Length, WhiteSpace};

fn text_run(text: &str) -> LayoutBox {
    LayoutBox::new(BoxType::Text(text.into()), ComputedStyle::new())
}

fn inline_holding(child: LayoutBox) -> LayoutBox {
    let mut style = ComputedStyle::new();
    style.display = Display::Inline;
    let mut span = LayoutBox::new(BoxType::Inline, style);
    span.children.push(child);
    span
}

fn pre_wrap_text(text: &str) -> LayoutBox {
    let mut run = text_run(text);
    run.style.white_space = WhiteSpace::PreWrap;
    run
}

#[test]
fn letter_or_word_spacing_marks_text_and_its_inline_parent() {
    let mut spaced = text_run("456");
    spaced.style.letter_spacing = Length::Em(1.0);
    assert!(LayoutBox::holds_spaced_text(&spaced));
    assert!(LayoutBox::stays_out_of_flow(&spaced));

    let span = inline_holding(spaced);
    assert!(
        LayoutBox::holds_spaced_text(&span),
        "a nested text run's spacing must refuse the parent inline"
    );
    assert!(LayoutBox::stays_out_of_flow(&span));

    let mut worded = text_run("a b");
    worded.style.word_spacing = Length::Px(4.0);
    assert!(LayoutBox::holds_spaced_text(&worded));

    let plain = text_run("456");
    assert!(
        !LayoutBox::holds_spaced_text(&plain),
        "Auto / zero spacing must stay on the in-flow path"
    );
    assert!(!LayoutBox::stays_out_of_flow(&plain));

    let mut zeroed = text_run("456");
    zeroed.style.letter_spacing = Length::Px(0.0);
    zeroed.style.word_spacing = Length::Auto;
    assert!(!LayoutBox::holds_spaced_text(&zeroed));
}

#[test]
fn a_preserved_newline_flows_mid_line_and_breaks_at_the_inline_end() {
    // PRE-WRAP-NEWLINE-AT-INLINE-END: holding a newline is no longer a mid-line refuse; the break
    // is carried when the inline's content ends with one.
    let mid = pre_wrap_text("a\nb");
    let span_mid = inline_holding(mid);
    assert!(
        !LayoutBox::stays_out_of_flow(&span_mid),
        "a preserved newline must take the in-flow mid-line path"
    );
    assert!(
        !LayoutBox::inline_ends_with_preserved_newline(&span_mid),
        "a mid-run newline does not end the line by itself"
    );

    let ending = pre_wrap_text("4 5\n");
    let span_end = inline_holding(ending);
    assert!(!LayoutBox::stays_out_of_flow(&span_end));
    assert!(
        LayoutBox::inline_ends_with_preserved_newline(&span_end),
        "`1 2 3<span>\\n4 5\\n</span>6 7 8` must close after the span"
    );

    let nested = inline_holding(inline_holding(pre_wrap_text("x\n")));
    assert!(
        LayoutBox::inline_ends_with_preserved_newline(&nested),
        "the ending newline is found at any inline depth"
    );
}

#[test]
fn inline_ends_with_preserved_newline_refuses_collapsed_and_out_of_flow() {
    let mut normal = text_run("a\n");
    normal.style.white_space = WhiteSpace::Normal;
    let span_normal = inline_holding(normal);
    assert!(
        !LayoutBox::inline_ends_with_preserved_newline(&span_normal),
        "a collapsed newline must not end the line"
    );

    let mut abs = inline_holding(pre_wrap_text("a\n"));
    abs.position = Position::Absolute;
    assert!(
        !LayoutBox::inline_ends_with_preserved_newline(&abs),
        "an absolute inline is not in the flow break path"
    );

    let mut fixed = inline_holding(pre_wrap_text("a\n"));
    fixed.position = Position::Fixed;
    assert!(!LayoutBox::inline_ends_with_preserved_newline(&fixed));

    // Bare text is not an inline box: develop's gate is Inline-only (PRE-WRAP-NEWLINE-BEFORE-INLINE
    // on open #1503 / batch 139 widens this to in-flow text; leave that to its own pin after land).
    let bare = pre_wrap_text("a\n");
    assert!(
        !LayoutBox::inline_ends_with_preserved_newline(&bare),
        "a bare text run is not an inline end-break on develop"
    );
}
