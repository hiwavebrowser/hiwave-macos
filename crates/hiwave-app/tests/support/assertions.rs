//! Custom assertions for integration tests.

use super::test_frame::RGB;

/// Assert that a color matches expected within tolerance.
#[track_caller]
pub fn assert_color_near(actual: RGB, expected: RGB, tolerance: u8) {
    assert!(
        actual.near(expected, tolerance),
        "Color mismatch: expected RGB({}, {}, {}), got RGB({}, {}, {}) (tolerance: {})",
        expected.r,
        expected.g,
        expected.b,
        actual.r,
        actual.g,
        actual.b,
        tolerance
    );
}

/// Assert that a frame is not blank.
#[track_caller]
pub fn assert_not_blank(frame: &super::TestFrame) {
    assert!(!frame.is_blank(), "Frame should not be blank");
}

/// Assert that frames match within tolerance.
#[track_caller]
pub fn assert_frames_match(
    actual: &super::TestFrame,
    expected: &super::TestFrame,
    tolerance: u8,
) {
    let diff = actual.compare(expected, tolerance);

    assert!(
        !diff.size_mismatch,
        "Frame size mismatch: actual {}x{}, expected {}x{}",
        actual.width,
        actual.height,
        expected.width,
        expected.height
    );

    assert_eq!(
        diff.diff_pixels, 0,
        "Frames differ: {} pixels ({:.2}%) are different (tolerance: {})",
        diff.diff_pixels, diff.diff_percent, tolerance
    );
}
