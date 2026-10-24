//! How long the window thread goes without turning its event loop.
//!
//! A measurement, off unless `HIWAVE_STALL_LOG=1`. A helper thread posts a
//! ping to the event loop every [`PERIOD_MS`]; the loop answers it the next
//! time it turns. The time a ping waits is time in which the window could not
//! answer a click, a key, a scroll or a resize either, so a wait of
//! [`THRESHOLD_MS`] or more is written to the log:
//!
//! ```text
//! Window thread holding ms=1200      (once a second while it lasts)
//! Window thread held ms=15230        (when the loop turns again)
//! ```
//!
//! Limits: a hold that starts just after a ping was answered is measured from
//! the next ping, so a figure can be up to one period short. The pings wake
//! the loop, so with the flag on the page's timers are polled ten times a
//! second even when nothing else happens.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// How often a ping is posted, in milliseconds.
pub const PERIOD_MS: u64 = 100;
/// The shortest wait that is reported, in milliseconds.
pub const THRESHOLD_MS: u64 = 200;
/// How often a hold that has not ended is reported, in milliseconds.
const REPEAT_MS: u64 = 1000;

/// What the helper thread should do after a tick.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Tick {
    /// Post a ping now.
    Ping,
    /// The last ping is still waiting; nothing to say yet.
    Wait,
    /// The last ping has waited this long and is still waiting.
    Holding(u64),
    /// The last ping waited this long before the loop answered it. A new
    /// ping is posted on the next tick.
    Held(u64),
}

/// The bookkeeping of the helper thread, kept apart from the clock and the
/// event loop so it can be tested.
#[derive(Debug, Default)]
pub struct StallMeter {
    /// When the ping that has not been seen answered was posted.
    sent_at: Option<u64>,
    /// When the current hold was last reported.
    reported_at: Option<u64>,
}

impl StallMeter {
    /// `now` and `answered_at` are milliseconds on one clock. `answered_at`
    /// is when the loop last answered a ping (0: never).
    pub fn tick(&mut self, now: u64, answered_at: u64) -> Tick {
        let Some(sent) = self.sent_at else {
            self.sent_at = Some(now);
            return Tick::Ping;
        };
        if answered_at >= sent {
            self.sent_at = None;
            self.reported_at = None;
            let waited = answered_at - sent;
            if waited >= THRESHOLD_MS {
                return Tick::Held(waited);
            }
            self.sent_at = Some(now);
            return Tick::Ping;
        }
        let waited = now.saturating_sub(sent);
        if waited >= THRESHOLD_MS && self.reported_at.map_or(true, |at| now - at >= REPEAT_MS) {
            self.reported_at = Some(now);
            return Tick::Holding(waited);
        }
        Tick::Wait
    }
}

/// The loop's side: the time it last answered a ping.
#[derive(Debug, Default)]
pub struct Answered(AtomicU64);

impl Answered {
    /// Called on the window thread when a ping arrives.
    pub fn now(&self, clock: &std::time::Instant) {
        // Never 0, which means "never answered".
        self.0.store(
            (clock.elapsed().as_millis() as u64).max(1),
            Ordering::Release,
        );
    }
}

/// True when the measurement was asked for.
pub fn enabled() -> bool {
    std::env::var("HIWAVE_STALL_LOG")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Starts the helper thread. `ping` posts one ping to the event loop and
/// returns false once the loop is gone. The returned value and clock go to
/// the loop, which calls [`Answered::now`] for each ping.
pub fn start(ping: impl Fn() -> bool + Send + 'static) -> (Arc<Answered>, std::time::Instant) {
    let answered = Arc::new(Answered::default());
    let clock = std::time::Instant::now();
    let seen = Arc::clone(&answered);
    std::thread::Builder::new()
        .name("window-stall".into())
        .spawn(move || {
            let mut meter = StallMeter::default();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(PERIOD_MS));
                let now = (clock.elapsed().as_millis() as u64).max(1);
                match meter.tick(now, seen.0.load(Ordering::Acquire)) {
                    Tick::Ping => {
                        if !ping() {
                            return;
                        }
                    }
                    Tick::Wait => {}
                    Tick::Holding(ms) => tracing::warn!(ms, "Window thread holding"),
                    Tick::Held(ms) => tracing::warn!(ms, "Window thread held"),
                }
            }
        })
        .expect("window-stall thread");
    tracing::info!(
        period_ms = PERIOD_MS,
        threshold_ms = THRESHOLD_MS,
        "Window thread stall log on"
    );
    (answered, clock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_that_answers_every_ping_reports_nothing() {
        let mut m = StallMeter::default();
        let mut answered = 0;
        for step in 1..50u64 {
            let now = step * PERIOD_MS;
            let t = m.tick(now, answered);
            assert_eq!(t, Tick::Ping, "at {now}");
            answered = now + 3; // the loop turns 3 ms later
        }
    }

    #[test]
    fn a_ping_answered_just_under_the_threshold_is_not_a_hold() {
        let mut m = StallMeter::default();
        assert_eq!(m.tick(100, 0), Tick::Ping);
        assert_eq!(m.tick(200, 0), Tick::Wait);
        assert_eq!(m.tick(300, 100 + THRESHOLD_MS - 1), Tick::Ping);
    }

    #[test]
    fn a_long_hold_is_reported_while_it_lasts_and_once_when_it_ends() {
        let mut m = StallMeter::default();
        assert_eq!(m.tick(100, 0), Tick::Ping);
        assert_eq!(m.tick(200, 0), Tick::Wait);
        assert_eq!(m.tick(300, 0), Tick::Holding(200));
        // not again until a second has passed
        for now in (400..1300).step_by(100) {
            assert_eq!(m.tick(now, 0), Tick::Wait, "at {now}");
        }
        assert_eq!(m.tick(1300, 0), Tick::Holding(1200));
        // the loop turns at 3150: the ping posted at 100 waited 3050 ms
        assert_eq!(m.tick(3200, 3150), Tick::Held(3050));
        // and the meter starts again
        assert_eq!(m.tick(3300, 3150), Tick::Ping);
        assert_eq!(m.tick(3400, 3301), Tick::Ping);
    }

    #[test]
    fn an_answer_older_than_the_ping_does_not_count_for_it() {
        let mut m = StallMeter::default();
        assert_eq!(m.tick(100, 0), Tick::Ping);
        // 105 answers the first ping; the second is posted at 200
        assert_eq!(m.tick(200, 105), Tick::Ping);
        // and 105 does not count for that one
        assert_eq!(m.tick(300, 105), Tick::Wait);
        assert_eq!(m.tick(400, 105), Tick::Holding(200));
        assert_eq!(m.tick(500, 450), Tick::Held(250));
    }

    #[test]
    fn two_holds_in_a_row_are_two_reports() {
        let mut m = StallMeter::default();
        assert_eq!(m.tick(100, 0), Tick::Ping);
        assert_eq!(m.tick(600, 550), Tick::Held(450));
        assert_eq!(m.tick(700, 550), Tick::Ping);
        assert_eq!(m.tick(900, 550), Tick::Holding(200));
        assert_eq!(m.tick(1100, 1050), Tick::Held(350));
    }
}
