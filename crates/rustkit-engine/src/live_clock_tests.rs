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

/// LINKS-SLOW: the content loop's 2 ms floor is for a page whose timer is always due (a wait of 0):
/// a timer due in 1 ms is waited for 1 ms. Raised to 2, a link due a fraction past a turn came a
/// millisecond or more late.
#[test]
fn only_a_timer_due_now_waits_the_floor() {
    assert_eq!(content_turn_wait(Some(0)), std::time::Duration::from_millis(2));
    assert_eq!(content_turn_wait(Some(1)), std::time::Duration::from_millis(1));
    assert_eq!(content_turn_wait(Some(10)), std::time::Duration::from_millis(10));
    assert_eq!(content_turn_wait(None), std::time::Duration::from_millis(50));
}

/// LINKS-SLOW: the sleep-overshoot estimate, on made-up instants. It starts at 0 (a sleep is aimed
/// at its due instant, as before), follows a steady overshoot, stays at 0 on a machine with none,
/// drops within a few wakes when the overshoot drops (so it does not keep waking early), barely
/// moves for a rare long sleep, and never passes its cap.
#[test]
fn the_overshoot_estimate_follows_this_machines_sleeps() {
    use crate::page_protocol::OvershootEstimate;
    let t0 = std::time::Instant::now();
    let ms = |x: f64| std::time::Duration::from_secs_f64(x / 1000.0);
    let mut e = OvershootEstimate::default();
    assert_eq!(e.aim(t0 + ms(10.0)), t0 + ms(10.0), "starts at 0");
    // A steady 1.5 ms.
    for _ in 0..200 {
        e.observe(t0, t0 + ms(1.5));
    }
    assert!((e.ms() - 1.5).abs() < 0.05, "{}", e.ms());
    // None at all.
    let mut z = OvershootEstimate::default();
    for _ in 0..200 {
        z.observe(t0, t0);
    }
    assert_eq!(z.ms(), 0.0);
    // The overshoot drops from 2 ms to 0: within 6 wakes the estimate is under 0.05 ms.
    let mut j = OvershootEstimate::default();
    for _ in 0..200 {
        j.observe(t0, t0 + ms(2.0));
    }
    for _ in 0..6 {
        j.observe(t0, t0);
    }
    assert!(j.ms() < 0.05, "{}", j.ms());
    // A rare 30 ms sleep (the thread was preempted) moves it by at most a sixteenth of the cap's
    // double, and the cap holds.
    let before = e.ms();
    e.observe(t0, t0 + ms(30.0));
    assert!(e.ms() - before <= 2.0 * OvershootEstimate::CAP_MS / 16.0 + 1e-9 && e.ms() <= OvershootEstimate::CAP_MS);
    for _ in 0..1000 {
        e.observe(t0, t0 + ms(30.0));
    }
    assert!(e.ms() <= OvershootEstimate::CAP_MS);
}

/// LINKS-SLOW: aiming early never runs a timer early. A loop model on made-up instants: a 10 ms
/// timer chain whose sleeps overshoot by a pattern that jumps (1.9 ms, then 0, then 1.2 ms with
/// jitter); every link runs at or after its due instant, and with a steady overshoot the links
/// are on average under 1 ms late (aimed at its due instant, every link was one overshoot late).
#[test]
fn an_aimed_wake_never_runs_a_timer_early() {
    use crate::page_protocol::{after_wake, OvershootEstimate};
    let t0 = std::time::Instant::now();
    let ms = |x: f64| std::time::Duration::from_secs_f64(x / 1000.0);
    let mut e = OvershootEstimate::default();
    let mut now = t0;
    let mut late = Vec::new();
    for i in 0..300 {
        let over = match i {
            0..=99 => 1.9 + 0.3 * ((i % 7) as f64 / 7.0),
            100..=149 => 0.0,
            _ => 1.2 + 0.6 * ((i % 5) as f64 / 5.0),
        };
        let due = now + ms(10.0);
        let aimed = e.aim(due).max(now);
        let woke = aimed + ms(over);
        e.observe(aimed, woke);
        let ran = match after_wake(due, woke) {
            Some(rest) => rest + ms(over),
            None => woke,
        };
        assert!(ran >= due, "link {i} ran before it was due");
        late.push(ran.duration_since(due).as_secs_f64() * 1000.0);
        now = ran;
    }
    let steady: f64 = late[40..100].iter().sum::<f64>() / 60.0;
    assert!(steady < 1.0, "steady lateness {steady:.2} ms");
}

/// LINKS-SLOW R1: the 10 ms chain of `links_lateness_probe.html` through the whole loop model:
/// sleeps end late as the content loop's did on Windows with 1 ms timers (measured: median 1.79,
/// p90 2.84, mean 1.92 ms past the instant asked; drawn here from the lognormal those three fit,
/// median 1.79 and sigma 0.36), the timers' clock is re-anchored each turn up to
/// 2.4 ms off the page clock (measured -0.17 to 2.37) and the loop aims at the later of the two, a
/// link runs only once the page clock has
/// moved its delay (the page-clock hold), and a held link's wait is counted from the turn that held
/// it. What the model pins: never early; about one turn per link, and a wake for each early
/// wake's rest, not a spin (the idle-CPU count: a 10 ms chain is ~100 turns a second, not
/// thousands); the chain's rate in Chrome's range (278-281 links); lateness under develop's
/// measured 1.31-1.40. Chrome's distribution itself (mean 0.64-0.75, p90 1.1-1.5) is pinned by the
/// measured loop (`measure_the_content_threads_link_lateness`: 0.69-0.77), not here: the model
/// charges an early wake's rest a full independent overshoot (23% of links wake early, ~0.4 ms of
/// the model's ~1.0 mean), which the real loop's short sleeps evidently do not pay.
#[test]
fn the_chain_through_the_loop_model_is_never_early_at_one_turn_a_link() {
    use crate::page_protocol::{after_wake, OvershootEstimate};
    let t0 = std::time::Instant::now();
    let ms = |x: f64| std::time::Duration::from_secs_f64(x / 1000.0);
    let at = |i: std::time::Instant| i.duration_since(t0).as_secs_f64() * 1000.0;
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut uniform = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let overshoot = |u: &mut dyn FnMut() -> f64| {
        let z = (-2.0 * (1.0 - u()).ln()).sqrt() * (2.0 * std::f64::consts::PI * u()).cos();
        1.79 * (0.36 * z).exp()
    };
    let mut e = OvershootEstimate::default();
    let mut now = t0;
    let (mut late, mut turns, mut rests) = (Vec::new(), 0u32, 0u32);
    while at(now) < 3000.0 {
        // The link runs at `now` and sets the next: due on the page clock 10 ms on; the timers'
        // clock this turn reads `off` ahead of the page clock.
        let page_due = now + ms(10.0);
        let off = -0.17 + 2.54 * uniform();
        // `nextTimer` reports the later of the timers' due and the page clock's: a timers' clock
        // reading ahead does not pull the wake early; one reading behind (negative `off`) delays it.
        let mut due = page_due + ms((-off).max(0.0));
        loop {
            let aimed = e.aim(due).max(now);
            let mut woke = aimed + ms(overshoot(&mut uniform));
            if aimed > now {
                e.observe(aimed, woke);
            }
            if let Some(rest) = after_wake(due, woke) {
                rests += 1;
                woke = rest + ms(overshoot(&mut uniform));
            }
            turns += 1;
            now = woke;
            if now >= page_due {
                break;
            }
            // Held: the turn reports the page clock's remaining wait, counted from now.
            due = now + page_due.duration_since(now);
        }
        late.push(now.duration_since(page_due).as_secs_f64() * 1000.0);
    }
    let links = late.len();
    late.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = late.iter().sum::<f64>() / links as f64;
    let p90 = late[links * 9 / 10];
    let per_second = turns as f64 / 3.0;
    let wakes_per_second = (turns + rests) as f64 / 3.0;
    eprintln!("MODEL links {links} mean {mean:.2} p90 {p90:.2} min {:.2} turns/s {per_second:.0} rests {rests} est {:.2}", late[0], e.ms());
    assert!(late[0] >= 0.0, "a link ran early");
    assert!(mean < 1.2, "lateness mean {mean:.2} (develop measured 1.31-1.40)");
    assert!(p90 < 2.1, "lateness p90 {p90:.2} (develop measured 2.1-2.4)");
    assert!((270..=290).contains(&links), "{links} links (Chrome 278-281)");
    assert!(per_second <= 105.0, "{per_second:.0} turns a second for a 10 ms chain");
    assert!(wakes_per_second <= 130.0, "{wakes_per_second:.0} wakes a second for a 10 ms chain");
}
