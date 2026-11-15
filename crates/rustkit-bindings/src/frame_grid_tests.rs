//! Animation frames on a 60 Hz grid (PACE2's next row). The pinned Chrome (CfT 148.0.7778.216,
//! headless, five runs of `docs/diagnostics/2026-11-15/frame_grid_probe.html`,
//! `frame_grid_probe_cft148.json`): a requestAnimationFrame loop gets 181 frames in 3000 ms; the
//! timestamps step by 16.6-16.7 ms, all on one 1000/60 ms grid; and after a frame whose callback
//! ran 24 ms the next frame's timestamp is again 16.7 ms on: a late frame is caught up, it does not
//! move the frames after it and no frame is dropped.
//!
//! RustKit's clock is whole milliseconds, so the grid is 1000/60 ms rounded up: 17, 34, 50, 67,
//! 84, 100, ... from the clock's zero. Until this row a frame was due 16 ms after the turn that ran
//! the last one, so every late turn moved every later frame (the content thread's loop wakes
//! ~1.2 ms late: 173 frames of Chrome's 180-181, PACE2 #1190).
//!
//! The tests turn the clock by hand (no wall clock): each turn comes when the next timer is due,
//! plus a lateness.

use super::*;

/// Grid point `k`: 1000/60 ms times `k`, rounded up to the whole millisecond.
fn grid(k: u64) -> u64 {
    (k * 50).div_ceil(3)
}

fn on_grid(t: u64) -> bool {
    grid(t * 3 / 50) == t || grid(t * 3 / 50 + 1) == t
}

fn frames_page() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.evaluate("var frames = 0; (function f() { requestAnimationFrame(function () { frames++; f(); }); })();")
        .unwrap();
    b
}

fn read(b: &DomBindings, v: &str) -> String {
    match b.evaluate(&format!("String({v})")).unwrap() {
        JsValue::String(s) => s,
        other => panic!("{other:?}"),
    }
}

/// Turns the clock from 0, each turn `late(turn)` ms after the next timer is due, while that is due
/// by `span`. Returns when each turn's frame was due on the engine's reading (the clock plus
/// `next_timer_delay`): a grid point, or the clock itself when the frame is already due.
fn drive(b: &DomBindings, span: u64, late: impl Fn(usize) -> u64) -> Vec<u64> {
    let (mut now, mut dues) = (0u64, Vec::new());
    for turn in 0.. {
        let delay = b.next_timer_delay().expect("a frame is asked for");
        if now + delay > span {
            break;
        }
        dues.push(now + delay);
        let step = delay + late(turn);
        b.advance_timers_for_frame(step, 1000, 0).unwrap();
        now += step;
    }
    dues
}

/// Turns that come 1 ms late every time (as the loop's sleeps do) leave the frames on the grid:
/// 180 due in 3000 ms, as Chrome's 180-181. Due 16 ms after the late turn, they came every 17 ms
/// (176) and drifted off it.
#[test]
fn frames_stay_on_the_60hz_grid_when_every_turn_is_late() {
    let b = frames_page();
    let dues = drive(&b, 3000, |_| 1);
    let off: Vec<u64> = dues
        .iter()
        .copied()
        .filter(|&t| !on_grid(t))
        .take(5)
        .collect();
    assert!(off.is_empty(), "frames due off the grid at {off:?}");
    assert_eq!(read(&b, "frames"), "180");
}

/// A turn that comes 24 ms after its frame was due (a long task) runs that frame, and the next is
/// due at its own grid point, already passed, so it runs on the next turn: no frame is dropped and
/// the frames after are on the grid again (Chrome: 181 frames with seven 24 ms frames, every gap
/// 16.6-16.7 ms). Due 16 ms after the late turn, the late frame moved every later one.
#[test]
fn a_late_frame_is_caught_up_not_carried_forward() {
    let b = frames_page();
    // Turn 30 runs frame 31 (due at grid(31)) 24 ms late. Frame 32 (grid(31) + 17) is due already
    // and runs on the next turn, at once; frame 33 is on the grid.
    let dues = drive(&b, 3000, |turn| if turn == 30 { 24 } else { 0 });
    assert_eq!(dues[30], grid(31));
    assert_eq!(
        dues[31],
        grid(31) + 24,
        "the frame after the late one waited: {:?}",
        &dues[29..34]
    );
    assert!(
        dues[32..].iter().all(|&t| on_grid(t)),
        "{:?}",
        &dues[29..40]
    );
    assert_eq!(read(&b, "frames"), "180");
}

/// A frame asked for after the page was idle (here by a timer at 1005 ms) is due at the next grid
/// point (1017), as Chrome's comes at the next vsync, not 16 ms after it was asked for (1021).
#[test]
fn a_frame_asked_for_when_idle_is_due_at_the_next_grid_point() {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.evaluate("setTimeout(function () { requestAnimationFrame(function () {}); }, 1005);")
        .unwrap();
    b.advance_timers_for_frame(1005, 1000, 0).unwrap();
    assert_eq!(b.next_timer_delay(), Some(grid(61) - 1005));
}
