//! PACE2: the content thread loop's clock (`page_protocol::LiveClock`), without the wall clock.
//!
//! The loop gives each live turn the whole milliseconds since the last turn began, and sleeps until
//! the next timer or frame is due. Measured on the app's loop (`frame_rate_tests`, the probe page:
//! a requestAnimationFrame loop beside a 10 ms timer chain, Windows with 1 ms sleeps, parity
//! profile, develop c5cfddac): 160 frames in 3000 ms where Chrome for Testing 148.0.7778.216 gets
//! 180-181 (`frame_rate_probe_cft148.json`); per turn the clock dropped 0.46 ms of fraction, the
//! turn and its frame took 0.36 ms before the wait began, and the sleep ran 1.27 ms past its time.
//! The Mac app's probe: 51-54 frames per wall second, debug equal to release.
//!
//! The tests drive the clock with made-up instants. The model loop is the probe's shape: the turn
//! runs what is due on the page's clock (a 10 ms chain on its grid; a frame once 16 ms have passed
//! since the last, as `requestAnimationFrame` asks), the turn takes 0.4 ms, and every sleep ends
//! 1.2 ms late.

use std::time::Duration;

use std::time::Instant;

use crate::page_protocol::{content_turn_wait, LiveClock};

fn us(n: u64) -> Duration {
    Duration::from_micros(n)
}

/// A turn is given whole milliseconds; the fraction left is the next turn's, not dropped. 100 turns
/// 7.4 ms apart are 740 ms of the page's clock (700 when each turn drops its 0.4 ms).
#[test]
fn a_turn_keeps_the_fraction_it_does_not_count() {
    let t0 = Instant::now();
    let mut clock = LiveClock::new(t0);
    let given: u64 = (1..=100).map(|i| clock.turn(t0 + us(7400 * i))).sum();
    assert_eq!(given, 740);
}

/// The next turn is due `wait` after the page's clock reached this turn's time, not `wait` after
/// the turn and its frame ended: a 10 ms wait asked at the end of a 3 ms turn ends 10 ms after the
/// turn began (where the page's next timer is), not 13 ms after.
#[test]
fn the_next_turn_is_due_from_when_this_turn_began() {
    let t0 = Instant::now();
    let mut clock = LiveClock::new(t0);
    // The turn begins at 16.6 ms and is given 16; its time on the page's clock is 16 ms after t0.
    assert_eq!(clock.turn(t0 + us(16_600)), 16);
    let deadline = clock.deadline(t0 + us(19_600), Duration::from_millis(10));
    assert_eq!(deadline, t0 + Duration::from_millis(26));
}

/// A page whose next timer is always due (a turn left work held back, or a `setTimeout(0)` chain)
/// is not turned in a spin: a cheap turn (0.1 ms) still waits until 2 ms after it began, so 1000 ms
/// is at most 500 turns (#1190 R1 follow-up). Only turns that each take 2 ms or more of the page's
/// own work follow one another at once: 3 ms turns come every 3 ms, the loop's commands drained
/// before each.
#[test]
fn an_always_due_page_is_turned_at_most_every_2_ms() {
    let turns_in_a_second = |work: Duration| {
        let t0 = Instant::now();
        let mut clock = LiveClock::new(t0);
        let (mut real, mut turns) = (t0, 0u32);
        while real - t0 < Duration::from_millis(1000) {
            clock.turn(real);
            turns += 1;
            let after = real + work;
            real = clock.deadline(after, content_turn_wait(Some(0))).max(after);
        }
        turns
    };
    assert_eq!(turns_in_a_second(us(100)), 500);
    assert_eq!(turns_in_a_second(us(3000)), 334);
}

/// The probe's loop on the model above, 3000 ms of it: the page's clock keeps up with real time
/// (the 10 ms chain runs 299 times, its last at 2990 ms; 276 when turns drop their fraction and
/// wait after their work) and frames come every 16-17 ms (179; 165 before, and the app measured
/// 160 against Chrome's 180-181).
#[test]
fn the_page_clock_keeps_up_and_frames_come_every_16_to_17_ms() {
    let (work, late, span) = (us(400), us(1200), Duration::from_millis(3000));
    let t0 = Instant::now();
    let mut clock = LiveClock::new(t0);
    let (mut real, mut page) = (t0, 0u64);
    let (mut frame_due, mut timer_due, mut frames, mut timers) = (16u64, 10u64, 0u32, 0u32);
    while real - t0 < span {
        page += clock.turn(real);
        while timer_due <= page {
            timer_due += 10;
            timers += 1;
        }
        if frame_due <= page {
            frames += 1;
            frame_due = page + 16;
        }
        let after = real + work;
        let wait = content_turn_wait(Some(frame_due.min(timer_due).saturating_sub(page)));
        real = clock.deadline(after, wait).max(after) + late;
    }
    assert_eq!(timers, 299, "the page's clock fell behind real time");
    assert!(frames >= 175, "{frames} frames in 3000 ms");
}
