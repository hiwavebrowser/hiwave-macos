//! BOX-REBUILD-ON-STYLE (#1501): portable pins for the restyle child-loop cut.
//!
//! The headless companion `box_splice_style_tests` is `feature = "headless"` only. Default Linux
//! `--lib` never exercised `restyle_last_needed_index` — the refuse that stops a paint write on
//! the first of thousands of siblings from recording every later one.

use super::restyle_last_needed_index;
use std::collections::HashSet;

#[test]
fn an_ordinary_build_reaches_the_last_child() {
    let kids = [10, 20, 30];
    assert_eq!(restyle_last_needed_index(1, &kids, None, None), Some(2));
    assert_eq!(
        restyle_last_needed_index(1, &[], None, None),
        None,
        "an empty child list has no last index"
    );
}

#[test]
fn a_restyle_under_a_path_parent_stops_after_the_last_root_or_path_child() {
    let parent = 1;
    // MotionMark Multiply shape: write the first of many siblings.
    let kids: Vec<usize> = (10..3010).collect();
    let path: HashSet<usize> = [parent, 10].into_iter().collect();
    let roots: HashSet<usize> = [10].into_iter().collect();
    assert_eq!(
        restyle_last_needed_index(parent, &kids, Some(&path), Some(&roots)),
        Some(0),
        "a write on the first tile must cut before recording the rest"
    );

    // A later root: stop at that index, not at the end.
    let path_mid: HashSet<usize> = [parent, 100].into_iter().collect();
    let roots_mid: HashSet<usize> = [100].into_iter().collect();
    assert_eq!(
        restyle_last_needed_index(parent, &kids, Some(&path_mid), Some(&roots_mid)),
        Some(90),
        "tile id 100 is index 90 in 10..3010"
    );
}

#[test]
fn a_parent_off_the_path_builds_every_child() {
    let kids = [10, 20, 30];
    let path: HashSet<usize> = [99].into_iter().collect();
    let roots: HashSet<usize> = [10].into_iter().collect();
    assert_eq!(
        restyle_last_needed_index(1, &kids, Some(&path), Some(&roots)),
        Some(2),
        "a parent not on the path is an ordinary full child walk"
    );
}
