//! Cmd chords offered to the page first, and how long they wait for it (M1e).
//!
//! A Cmd chord Chromium does not reserve goes to the page's keydown before
//! the menu (CH2.E3): the view holds the chord, the page answers whether it
//! claimed it, and an unclaimed one goes on to the menu. The page answers
//! from its own thread, which a load can hold for seconds (an image the
//! server held 8 s kept Cmd+L from the address bar for 6.4 s, M1d #1012).
//! So a chord waits [`CHORD_ANSWER_TIMEOUT`] at most: one the page has not
//! answered by then is unclaimed and goes to the menu, and the page's late
//! answer, claimed or not, is dropped. The menu runs once, and a claim
//! that comes too late does not take the chord back.
//!
//! Why 100 ms: it is the response budget a press has before the delay is
//! felt (RAIL: respond to input within 100 ms), and far above what an idle
//! page thread takes to run a keydown listener and answer (the in-time
//! cases in `tests/macos_chord_wait.rs` answer in 10-20 ms). A page whose
//! thread is free keeps every chord it cancels, as in Chrome; only a page
//! whose thread is held past the budget loses it to the menu. The
//! real-window gate (`m1d_cmd_l_while_loading`) allows 250 ms from the
//! press to the address bar, the timeout plus the menu action and the
//! chrome's focus.

use std::time::{Duration, Instant};

/// How long a Cmd chord waits for the page to say whether it claimed it.
pub const CHORD_ANSWER_TIMEOUT: Duration = Duration::from_millis(100);

/// What became of a held chord.
#[derive(Debug, PartialEq, Eq)]
pub enum ChordFate<E> {
    /// The page did not claim it, or did not answer in time: to the menu.
    ToMenu(E),
    /// The page claimed it in time: the page keeps it.
    PageKept(E),
}

/// The chords the view holds for the page, by token, with when each was
/// taken. `E` is the native event (a retained `NSEvent` on macOS).
#[derive(Debug)]
pub struct HeldChords<E> {
    held: Vec<(u64, E, Instant)>,
}

impl<E> Default for HeldChords<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> HeldChords<E> {
    pub const fn new() -> Self {
        Self { held: Vec::new() }
    }

    /// The view took `event` for the page at `at`.
    pub fn hold(&mut self, token: u64, event: E, at: Instant) {
        self.held.push((token, event, at));
    }

    /// When `token`'s wait ends, while it is held.
    pub fn deadline(&self, token: u64) -> Option<Instant> {
        self.held.iter().find(|(t, ..)| *t == token).map(|(_, _, at)| *at + CHORD_ANSWER_TIMEOUT)
    }

    /// The page's answer for `token`, with how long it waited. `None`: the
    /// chord is no longer held (its wait ran out and it went to the menu),
    /// so the answer is dropped.
    pub fn answer(&mut self, token: u64, claimed: bool, now: Instant) -> Option<(ChordFate<E>, Duration)> {
        let at = self.held.iter().position(|(t, ..)| *t == token)?;
        let (_, event, taken) = self.held.remove(at);
        let waited = now.saturating_duration_since(taken);
        Some((if claimed { ChordFate::PageKept(event) } else { ChordFate::ToMenu(event) }, waited))
    }

    /// `token`'s wait ran out: the chord goes to the menu, if the page has
    /// not answered yet. The timer that calls this was armed for the
    /// deadline; it does not check the clock, so a timer that fires a hair
    /// early still releases the chord.
    pub fn expire(&mut self, token: u64) -> Option<E> {
        let at = self.held.iter().position(|(t, ..)| *t == token)?;
        Some(self.held.remove(at).1)
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;

    /// The main thread's loop against a fake page thread that answers
    /// after `after` (or never): page answers are taken as they come, and
    /// the chord's wait runs out at its deadline. What the menu got, and
    /// whether the page kept it.
    fn drive(claimed: bool, after: Option<Duration>) -> (Vec<&'static str>, Vec<&'static str>, Duration) {
        let mut chords = HeldChords::new();
        let start = Instant::now();
        chords.hold(7, "cmd-l", start);
        let deadline = chords.deadline(7).expect("held");
        let (tx, rx) = mpsc::channel::<(u64, bool)>();
        thread::spawn(move || {
            if let Some(after) = after {
                thread::sleep(after);
                let _ = tx.send((7, claimed));
            } else {
                thread::sleep(Duration::from_secs(2));
            }
        });
        let (mut menu, mut kept) = (Vec::new(), Vec::new());
        let mut menu_at = Duration::ZERO;
        let end = start + after.unwrap_or_default().max(CHORD_ANSWER_TIMEOUT) + Duration::from_millis(150);
        while Instant::now() < end {
            while let Ok((token, claimed)) = rx.try_recv() {
                match chords.answer(token, claimed, Instant::now()) {
                    Some((ChordFate::ToMenu(e), _)) => {
                        menu.push(e);
                        menu_at = start.elapsed();
                    }
                    Some((ChordFate::PageKept(e), _)) => kept.push(e),
                    None => {}
                }
            }
            if Instant::now() >= deadline {
                if let Some(e) = chords.expire(7) {
                    menu.push(e);
                    menu_at = start.elapsed();
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(chords.is_empty(), "nothing left held");
        (menu, kept, menu_at)
    }

    #[test]
    fn answered_in_time_unclaimed_goes_to_the_menu_once() {
        let (menu, kept, at) = drive(false, Some(Duration::from_millis(5)));
        assert_eq!(menu, ["cmd-l"]);
        assert!(kept.is_empty());
        assert!(at < CHORD_ANSWER_TIMEOUT, "at the answer, not the deadline: {at:?}");
    }

    #[test]
    fn answered_in_time_claimed_the_page_keeps_it() {
        // Chromium: IDC_FOCUS_LOCATION / IDC_SELECT_TAB_* are not reserved.
        let (menu, kept, _) = drive(true, Some(Duration::from_millis(5)));
        assert!(menu.is_empty());
        assert_eq!(kept, ["cmd-l"]);
    }

    #[test]
    fn never_answered_goes_to_the_menu_at_the_deadline() {
        let (menu, kept, at) = drive(false, None);
        assert_eq!(menu, ["cmd-l"]);
        assert!(kept.is_empty());
        assert!(at >= CHORD_ANSWER_TIMEOUT && at < Duration::from_millis(250), "{at:?}");
    }

    #[test]
    fn a_late_unclaimed_answer_does_not_run_the_menu_again() {
        let (menu, kept, at) = drive(false, Some(Duration::from_millis(300)));
        assert_eq!(menu, ["cmd-l"], "once");
        assert!(kept.is_empty());
        assert!(at < Duration::from_millis(250), "at the deadline, not the answer: {at:?}");
    }

    #[test]
    fn a_late_claim_is_dropped() {
        let (menu, kept, _) = drive(true, Some(Duration::from_millis(300)));
        assert_eq!(menu, ["cmd-l"]);
        assert!(kept.is_empty(), "too late to take it back");
    }

    #[test]
    fn answer_and_expire_each_release_a_chord_once() {
        let mut chords = HeldChords::new();
        let t0 = Instant::now();
        chords.hold(1, 'a', t0);
        chords.hold(2, 'b', t0);
        assert_eq!(chords.deadline(2), Some(t0 + CHORD_ANSWER_TIMEOUT));
        assert_eq!(chords.expire(1), Some('a'));
        assert_eq!(chords.answer(1, false, t0), None, "late answer for 1 dropped");
        assert_eq!(chords.expire(1), None);
        let (fate, waited) = chords.answer(2, true, t0 + Duration::from_millis(30)).expect("held");
        assert_eq!(fate, ChordFate::PageKept('b'));
        assert_eq!(waited, Duration::from_millis(30));
        assert_eq!(chords.expire(2), None, "an answered chord's timer finds nothing");
        assert_eq!(chords.deadline(2), None);
        assert!(chords.is_empty());
    }
}
