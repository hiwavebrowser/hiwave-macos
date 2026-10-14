//! The input queue: what the content view's AppKit handlers record, drained
//! by the app each turn of its loop (threads plan M2, #575).
//!
//! A queue, not a callback: the handlers run inside AppKit's event dispatch,
//! and calling back into app/engine state from there is the re-entrancy trap
//! #108 exists to prevent. Push under a short lock, drain on the main loop.
//!
//! The types here are plain data with no AppKit in them, so the queue and
//! its tests build on every OS; `macos.rs` fills it from the view's
//! selectors.

use crate::ViewId;
use std::sync::Mutex;
use std::time::Instant;

/// A content-view click, in VIEW-LOCAL TOP-LEFT coordinates: exactly the
/// viewport space the engine's hit testing speaks, no chrome/sidebar math.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingClick {
    pub x: f64,
    pub y: f64,
    pub down: bool,
    pub input: PointerInput,
}

/// What a [`PendingClick`] records: a press or release of the primary
/// button (`down` says which), the pointer moving over the view, or the
/// pointer leaving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerInput {
    Button,
    Move,
    Leave,
}

/// A key event captured by the content view while it is first responder.
///
/// `text` carries the typed characters (empty for pure control keys);
/// `mac_keycode` is the hardware-independent macOS keyCode for specials;
/// `up` is a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingKey {
    pub text: String,
    pub mac_keycode: u16,
    pub up: bool,
    pub ctrl: bool,
    pub cmd: bool,
    pub shift: bool,
    pub alt: bool,
}

/// A wheel turn or trackpad swipe over the content view.
///
/// `dx`, `dy` are in CSS pixels with the sign hiwave-app's window-loop arm
/// always used (tao's): `dy < 0` moves the page down, `dx > 0` moves it
/// right. A wheel that reports lines is counted at 40px a line, as the
/// window-loop arm counts it. `x`, `y` are where the pointer was, view-local
/// top-left, for an inner scroller one day; a page scroll does not read them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingScroll {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
}

/// One recorded input.
#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    Pointer(PendingClick),
    Key(PendingKey),
    Wheel(PendingScroll),
}

/// An input as the app drains it: `seq` is its place in arrival order
/// across every kind and every view, `at` when the view recorded it, and
/// `view` the content view it was delivered to.
#[derive(Debug, Clone, PartialEq)]
pub struct InputEvent {
    pub seq: u64,
    pub at: Instant,
    pub view: ViewId,
    pub kind: InputKind,
}

/// The queue itself; one process-wide instance behind [`push_input`] and
/// [`drain_input`], and a value for tests.
#[derive(Debug, Default)]
pub struct InputQueue {
    events: Vec<InputEvent>,
    next_seq: u64,
}

impl InputQueue {
    /// Record `kind` for `view`. Returns the event's `seq`.
    ///
    /// Coalescing is adjacent-only, within one view: a pointer move that
    /// follows a pointer move replaces it (the page needs where the pointer
    /// is, not every point it crossed since the last turn), and a wheel that
    /// follows a wheel is summed into it (a trackpad flick is ~50 events a
    /// second and the page needs where it ends up). Anything else in
    /// between keeps both, so a click after a burst stays after it.
    pub fn push(&mut self, view: ViewId, kind: InputKind) -> u64 {
        if let Some(last) = self.events.last_mut() {
            if last.view == view {
                match (&mut last.kind, &kind) {
                    (InputKind::Pointer(prev), InputKind::Pointer(next))
                        if prev.input == PointerInput::Move && next.input == PointerInput::Move =>
                    {
                        *prev = *next;
                        last.at = Instant::now();
                        return last.seq;
                    }
                    (InputKind::Wheel(prev), InputKind::Wheel(next)) => {
                        prev.dx += next.dx;
                        prev.dy += next.dy;
                        prev.x = next.x;
                        prev.y = next.y;
                        last.at = Instant::now();
                        return last.seq;
                    }
                    _ => {}
                }
            }
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.events.push(InputEvent { seq, at: Instant::now(), view, kind });
        seq
    }

    /// Take everything recorded so far.
    ///
    /// At this commit the drain hands the kinds out grouped, pointer events
    /// first, then wheels, then keys, which is the order the app drained the
    /// three queues it replaced; the next commit drains in `seq` order.
    pub fn drain(&mut self) -> Vec<InputEvent> {
        let events = std::mem::take(&mut self.events);
        let rank = |e: &InputEvent| match e.kind {
            InputKind::Pointer(_) => 0,
            InputKind::Wheel(_) => 1,
            InputKind::Key(_) => 2,
        };
        let mut grouped = events;
        grouped.sort_by_key(rank);
        grouped
    }

    /// Take the recorded events of one kind, leaving the others queued (for
    /// a caller that only handles that kind; the app drains everything).
    pub fn drain_kind(&mut self, keep: impl Fn(&InputKind) -> bool) -> Vec<InputEvent> {
        let (taken, kept): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.events).into_iter().partition(|e| keep(&e.kind));
        self.events = kept;
        taken
    }

    /// Drop what is queued for `view`: the view is gone, and an input
    /// recorded for it must not reach whatever view comes next.
    pub fn retire(&mut self, view: ViewId) {
        self.events.retain(|e| e.view != view);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

static INPUT: Mutex<InputQueue> = Mutex::new(InputQueue { events: Vec::new(), next_seq: 0 });

/// Record an input for `view` (called from the content view's handlers).
pub fn push_input(view: ViewId, kind: InputKind) -> u64 {
    INPUT.lock().map(|mut q| q.push(view, kind)).unwrap_or(0)
}

/// Everything recorded since the last drain, for the app's loop turn.
pub fn drain_input() -> Vec<InputEvent> {
    INPUT.lock().map(|mut q| q.drain()).unwrap_or_default()
}

/// The recorded events of one kind, leaving the others queued.
pub fn drain_input_kind(keep: impl Fn(&InputKind) -> bool) -> Vec<InputEvent> {
    INPUT.lock().map(|mut q| q.drain_kind(keep)).unwrap_or_default()
}

/// Drop what is queued for a view that no longer exists.
pub fn retire_view_input(view: ViewId) {
    if let Ok(mut q) = INPUT.lock() {
        q.retire(view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(dy: f64) -> InputKind {
        InputKind::Wheel(PendingScroll { x: 10.0, y: 10.0, dx: 0.0, dy })
    }

    fn click(down: bool) -> InputKind {
        InputKind::Pointer(PendingClick { x: 5.0, y: 5.0, down, input: PointerInput::Button })
    }

    fn mv(x: f64) -> InputKind {
        InputKind::Pointer(PendingClick { x, y: 5.0, down: false, input: PointerInput::Move })
    }

    fn key(text: &str) -> InputKind {
        InputKind::Key(PendingKey {
            text: text.into(),
            mac_keycode: 0,
            up: false,
            ctrl: false,
            cmd: false,
            shift: false,
            alt: false,
        })
    }

    fn kinds(events: &[InputEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match e.kind {
                InputKind::Pointer(_) => "pointer",
                InputKind::Wheel(_) => "wheel",
                InputKind::Key(_) => "key",
            })
            .collect()
    }

    #[test]
    fn a_click_after_a_wheel_burst_is_delivered_after_it() {
        let view = ViewId::new();
        let mut q = InputQueue::default();
        for _ in 0..5 {
            q.push(view, wheel(-10.0));
        }
        q.push(view, click(true));
        q.push(view, click(false));
        let out = q.drain();
        assert_eq!(kinds(&out), ["wheel", "pointer", "pointer"], "the burst is one wheel, then the click");
        assert!(out.windows(2).all(|w| w[0].seq < w[1].seq), "seq order");
        match out[0].kind {
            InputKind::Wheel(w) => assert_eq!(w.dy, -50.0, "the burst was summed"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_key_typed_before_a_click_is_delivered_before_it() {
        let view = ViewId::new();
        let mut q = InputQueue::default();
        q.push(view, key("a"));
        q.push(view, click(true));
        q.push(view, click(false));
        q.push(view, key("b"));
        assert_eq!(kinds(&q.drain()), ["key", "pointer", "pointer", "key"]);
    }

    #[test]
    fn a_wheel_between_two_clicks_keeps_its_place() {
        let view = ViewId::new();
        let mut q = InputQueue::default();
        q.push(view, click(true));
        q.push(view, click(false));
        q.push(view, wheel(-10.0));
        q.push(view, wheel(-10.0));
        q.push(view, click(true));
        q.push(view, click(false));
        q.push(view, wheel(-5.0));
        let out = q.drain();
        assert_eq!(
            kinds(&out),
            ["pointer", "pointer", "wheel", "pointer", "pointer", "wheel"],
            "wheels sum only with the wheel right before them"
        );
    }

    #[test]
    fn moves_coalesce_only_when_adjacent_and_in_the_same_view() {
        let a = ViewId::new();
        let b = ViewId::new();
        let mut q = InputQueue::default();
        q.push(a, mv(1.0));
        q.push(a, mv(2.0));
        q.push(a, click(true));
        q.push(a, mv(3.0));
        q.push(b, mv(4.0));
        let out = q.drain();
        assert_eq!(out.len(), 4, "two moves replaced, one click, one move per view");
        match out[0].kind {
            InputKind::Pointer(p) => assert_eq!(p.x, 2.0, "the later move replaced the earlier"),
            _ => unreachable!(),
        }
        assert_eq!(out[3].view, b);
    }

    #[test]
    fn a_wheel_queued_for_a_gone_view_is_dropped() {
        let gone = ViewId::new();
        let live = ViewId::new();
        let mut q = InputQueue::default();
        q.push(gone, wheel(-10.0));
        q.push(live, wheel(-20.0));
        q.push(gone, click(true));
        q.retire(gone);
        let out = q.drain();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].view, live);
        assert!(q.is_empty());
    }

    #[test]
    fn drain_kind_leaves_the_rest_queued_in_order() {
        let view = ViewId::new();
        let mut q = InputQueue::default();
        q.push(view, key("a"));
        q.push(view, wheel(-10.0));
        q.push(view, key("b"));
        let keys = q.drain_kind(|k| matches!(k, InputKind::Key(_)));
        assert_eq!(keys.len(), 2);
        assert_eq!(q.len(), 1);
        assert_eq!(kinds(&q.drain()), ["wheel"]);
    }

    #[test]
    fn the_process_queue_is_shared_and_sequenced() {
        let view = ViewId::new();
        let first = push_input(view, key("x"));
        let second = push_input(view, click(true));
        assert!(second > first);
        retire_view_input(view);
        assert!(drain_input().iter().all(|e| e.view != view));
    }
}
