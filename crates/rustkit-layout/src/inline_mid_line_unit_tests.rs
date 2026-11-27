//! INLINE-MID-LINE (#1422): portable pins for the gates that keep spaced / preserved-newline
//! inlines off the in-flow mid-line path.
//!
//! The engine companion `inline_mid_line_tests` is `feature = "headless"` only, so default
//! Linux `--lib` never exercised `holds_spaced_text` / `holds_preserved_newline` /
//! `stays_out_of_flow` — the refuse that stopped letter-spacing-200 box 3 from starting a
//! spaced `<span>` on a line it did not fit.

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
fn a_preserved_newline_under_pre_wrap_refuses_the_inline() {
    let mut run = text_run("a\nb");
    run.style.white_space = WhiteSpace::PreWrap;
    assert!(LayoutBox::holds_preserved_newline(&run));
    assert!(LayoutBox::stays_out_of_flow(&run));

    let span = inline_holding(run);
    assert!(
        LayoutBox::holds_preserved_newline(&span),
        "a nested preserved newline must refuse the parent inline"
    );
    assert!(LayoutBox::stays_out_of_flow(&span));

    let mut no_break = text_run("ab");
    no_break.style.white_space = WhiteSpace::PreWrap;
    assert!(
        !LayoutBox::holds_preserved_newline(&no_break),
        "pre-wrap without a newline stays on the in-flow path"
    );

    let mut normal_nl = text_run("a\nb");
    normal_nl.style.white_space = WhiteSpace::Normal;
    assert!(
        !LayoutBox::holds_preserved_newline(&normal_nl),
        "a collapsed newline must not refuse mid-line"
    );
    assert!(!LayoutBox::stays_out_of_flow(&normal_nl));
}
