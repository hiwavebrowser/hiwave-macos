//! #1064 INC5: portable pins for the incremental helpers that only lived
//! behind headless `incremental_layout_tests`.

use super::*;
use crate::incremental::{compare_rects, nest_free};

#[test]
fn nest_free_drops_children_of_a_kept_prefix() {
    let kept = nest_free(vec![
        vec![0, 1],
        vec![0, 1, 2],
        vec![0, 1, 3],
        vec![2],
        vec![2, 0],
        vec![1],
    ]);
    assert_eq!(kept, vec![vec![0, 1], vec![1], vec![2]]);
}

#[test]
fn nest_free_keeps_equal_roots_once() {
    assert_eq!(
        nest_free(vec![vec![0], vec![0], vec![1, 0]]),
        vec![vec![0], vec![1, 0]]
    );
}

#[test]
fn compare_rects_reports_zero_when_trees_match() {
    let mut a = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    a.dimensions.content = Rect::new(0.0, 0.0, 100.0, 40.0);
    let mut child = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    child.dimensions.content = Rect::new(0.0, 0.0, 100.0, 20.0);
    a.children.push(child);
    let b = a.clone();
    let (count, shown) = compare_rects(&a, &b, 8);
    assert_eq!((count, shown.len()), (0, 0));
}

#[test]
fn compare_rects_reports_a_moved_content_rect() {
    let mut a = LayoutBox::new(BoxType::Block, ComputedStyle::new());
    a.dimensions.content = Rect::new(0.0, 0.0, 100.0, 40.0);
    let mut b = a.clone();
    b.dimensions.content.y = 12.0;
    let (count, shown) = compare_rects(&a, &b, 8);
    assert_eq!(count, 1);
    assert!(
        shown.iter().any(|s| s.contains("content")),
        "shown={shown:?}"
    );
}
