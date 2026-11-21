//! Portable pins for LPF3 (`4c98f509` / #1226): a one-off miss sits out
//! nothing; misses in a row double to the cap; navigation forgets the run.
//!
//! Headless `script_restyle_tests` covers the short path end-to-end; default
//! Linux `--lib` never saw the cooldown formula itself.

use super::*;
use layout_ctx::ViewLayout;
use rustkit_layout::{BoxType, LayoutBox};

#[test]
fn a_one_off_miss_sits_out_nothing() {
    assert_eq!(script_restyle_cooldown_after_miss(None), 0);
}

#[test]
fn misses_in_a_row_double_to_the_cap() {
    assert_eq!(script_restyle_cooldown_after_miss(Some(0)), 1);
    assert_eq!(script_restyle_cooldown_after_miss(Some(1)), 2);
    assert_eq!(script_restyle_cooldown_after_miss(Some(2)), 4);
    assert_eq!(script_restyle_cooldown_after_miss(Some(30)), 60);
    assert_eq!(script_restyle_cooldown_after_miss(Some(60)), 60);
    assert_eq!(script_restyle_cooldown_after_miss(Some(100)), 60);
}

#[test]
fn forget_script_restyle_clears_the_cooldown_run() {
    let lay = ViewLayout::default();
    *lay.script_unlaid.borrow_mut() =
        Some(LayoutBox::new(BoxType::Block, ComputedStyle::default()));
    lay.want_script_unlaid.set(true);
    lay.script_cooldown.set(8);
    lay.script_last_cooldown.set(Some(4));
    lay.script_idle.set(3);

    lay.forget_script_restyle();

    assert!(lay.script_unlaid.borrow().is_none());
    assert!(!lay.want_script_unlaid.get());
    assert_eq!(lay.script_cooldown.get(), 0);
    assert_eq!(lay.script_last_cooldown.get(), None);
    assert_eq!(lay.script_idle.get(), 0);
}
