//! Portable pin for SCRIPT-TIME-DUP (`9ce8475c`): only the first error
//! record of a step carries its wall time. Headless `record_elapsed_tests`
//! covers the drain path; default `--lib` never saw the nth rule.

use super::inspector_glue::report_elapsed_ms;

#[test]
fn the_first_error_carries_the_step_time_and_later_ones_are_zero() {
    assert_eq!(report_elapsed_ms(0, 17), 17);
    assert_eq!(report_elapsed_ms(1, 17), 0);
    assert_eq!(report_elapsed_ms(2, 17), 0);
    assert_eq!(report_elapsed_ms(0, 0), 0);
}
