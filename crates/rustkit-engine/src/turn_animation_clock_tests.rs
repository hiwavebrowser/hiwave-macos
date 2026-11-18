//! #1272 / #1270: the turn's animation clock never steps back from the
//! time animations were already drawn at before the round.
//!
//! The same four asserts live in headless `clock_at_present_tests`. That
//! module is `feature = "headless"`; this pin keeps them on every
//! `cargo test -p rustkit-engine --lib`.

#[test]
fn the_turns_animation_clock_never_steps_back_from_the_presampled_frame() {
    let f = crate::turn_animation_clock_for_tests;
    assert_eq!(
        f(100.4, Some(100.0)),
        100.0,
        "a stamp within a millisecond keeps the drawn time"
    );
    assert_eq!(f(120.0, Some(100.0)), 120.0, "a stamp past it is the clock");
    assert_eq!(
        f(95.0, Some(100.0)),
        100.0,
        "a stamp below it does not step the painted time back"
    );
    assert_eq!(f(95.0, None), 95.0, "without a presample the stamp");
}
