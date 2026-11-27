//! SPARE-COST (#1481): portable table for `spare_is_wanted`.
//!
//! Integration pins in `frames_sync_blank_tests` cover unread URL frames and
//! blank nesting end-to-end, but default Linux `--lib` had no pure assertion of
//! the Speedometer shape (nested URL, no child frames → no spare).

use super::spare_is_wanted;

#[test]
fn spare_is_wanted_table() {
    // A document that already has frames always wants a spare.
    assert!(spare_is_wanted(true, 1, false));
    assert!(spare_is_wanted(true, 2, false));
    assert!(spare_is_wanted(true, 2, true));

    // Top-level empty: no frames yet → no spare until one is inserted.
    assert!(!spare_is_wanted(false, 1, true));
    assert!(!spare_is_wanted(false, 1, false));

    // Nested about:blank (JetStream / spare's spare) wants one before frames.
    assert!(spare_is_wanted(false, 2, true));

    // Speedometer workload: nested URL document with no frames → no spare.
    assert!(!spare_is_wanted(false, 2, false));
    assert!(!spare_is_wanted(false, 3, false));
}
