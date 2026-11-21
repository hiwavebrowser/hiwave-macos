//! Pure map for document scroll keys (`scroll_dy_for_unhandled_key`).
//! The content-thread key path that calls it is only exercised through a
//! headed/page turn; these units pin the deltas on default Linux `--lib`.

use crate::page_protocol::scroll_dy_for_unhandled_key;

#[test]
fn arrow_page_and_space_scroll_by_the_pinned_deltas() {
    assert_eq!(scroll_dy_for_unhandled_key(0x28, "ArrowDown"), Some(-40.0));
    assert_eq!(scroll_dy_for_unhandled_key(0x26, "ArrowUp"), Some(40.0));
    assert_eq!(scroll_dy_for_unhandled_key(0x22, "PageDown"), Some(-600.0));
    assert_eq!(scroll_dy_for_unhandled_key(0x21, "PageUp"), Some(600.0));
    assert_eq!(scroll_dy_for_unhandled_key(0, " "), Some(-600.0));
}

#[test]
fn home_and_end_are_plus_or_minus_max() {
    assert_eq!(scroll_dy_for_unhandled_key(0x24, "Home"), Some(f32::MAX));
    assert_eq!(scroll_dy_for_unhandled_key(0x23, "End"), Some(-f32::MAX));
}

#[test]
fn an_unmapped_key_scrolls_nothing() {
    assert_eq!(scroll_dy_for_unhandled_key(0x41, "a"), None);
    assert_eq!(scroll_dy_for_unhandled_key(0x09, "Tab"), None);
    // Space only matches when key_code is 0 (the page's " " key value path).
    assert_eq!(scroll_dy_for_unhandled_key(0x20, " "), None);
}
