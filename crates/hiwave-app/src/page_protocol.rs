//! The page protocol (threads plan, M4 PR1): every message that crosses
//! between the window thread and a page thread, as plain data.
//!
//! `RustKitView` is called from the window thread today and runs the page
//! on it. At M4 the same calls become messages to a `PageThread`, and the
//! view's answers (frames, titles, navigations the page asked for) come
//! back as messages. This module is the protocol alone: what is said in
//! each direction, nothing about who says it. The next PR gives the
//! window thread a `PageHandle` that speaks it on the same thread with
//! `HIWAVE_CONTENT_THREAD` off, and over a channel with it on.
//!
//! Invariant 8 of the plan (`docs/design/THREADS_PLAN_2026-10.md` §4):
//! a cross-thread message is plain data or an `Arc` of an immutable
//! object; no `Rc`, no reference, no `Arc` to page state. The tests at
//! the end make that a compile error here. The one `Arc` in the protocol
//! is the `DisplayList` inside a [`FrameCommit`], which is immutable once
//! built (M3, #704).
//!
//! The table below is the audit that every window-thread entry into
//! `RustKitView` has a message. A request with no reply is fire-and-forget:
//! the window thread must never wait for the page (invariant 2 of the
//! Chrome row in §3: every wait has a timeout). Where the view returns a
//! value today, the reply is listed; the handle PR decides what the window
//! does while it waits (the view's history, title cache and URL cache stay
//! on the window side, so most of what the window needs is already there).
//!
//! | `RustKitView` today | request | reply |
//! |---|---|---|
//! | `wry_load_url`, `navigate`, `nav_back`/`nav_forward`/`reload` (history is the window's) | [`PageRequest::LoadUrl`] | [`PageReply::LoadStarted`], then [`PageReply::Loaded`] or [`PageReply::LoadFailed`] |
//! | `wry_load_html`, `load_html_internal` | [`PageRequest::LoadHtml`] | the same |
//! | `wry_set_bounds`, `set_bounds_internal` | [`PageRequest::SetBounds`] | a [`PageReply::Frame`] once laid out |
//! | `wry_evaluate_script` | [`PageRequest::EvaluateScript`] with `wants_result: false` | none |
//! | `execute_script_sync`, `execute_script` | [`PageRequest::EvaluateScript`] with `wants_result: true` | [`PageReply::ScriptResult`] |
//! | `mouse_down_at_point`, `mouse_move_at_point`, `mouse_leave` | [`PageRequest::Pointer`] | none (a frame follows if paint changed) |
//! | `click_at_point` | [`PageRequest::Pointer`] with [`PointerAction::Click`] | [`PageReply::Click`] |
//! | `scroll_by` | [`PageRequest::Scroll`] | [`PageReply::Frame`] (the Presenter moves the last frame first, T2; owns the offset at M4b) |
//! | `handle_text_key`, `handle_key_up`, `grab_keyboard` | [`PageRequest::Key`], [`PageRequest::GrabKeyboard`] | none |
//! | `focus_at_point`, `link_at_point`, `has_focused_element` | [`PageRequest::Query`] | [`PageReply::Answer`] |
//! | `form_submit_url`, `take_script_navigation` | none: the page sends them unasked | [`PageReply::NavigationRequested`] |
//! | `process_events` (one live turn) | [`PageRequest::Tick`] | [`PageReply::TurnDone`] |
//! | `relayout`, `render` | [`PageRequest::Relayout`]; `render` is the Presenter's, not the page's | [`PageReply::Frame`] |
//! | `title` | none: cached on the window side | [`PageReply::Title`] when it changes |
//! | `note_input` | the `seq` on every input request | the commit's `input` mark |
//! | Stop (M4c) and drop | [`PageRequest::Stop`], [`PageRequest::Close`] | [`PageReply::Stopped`], [`PageReply::Closed`] |

#![allow(dead_code)]

use rustkit_engine::FrameCommit;

/// A rectangle in the window's coordinate space, in device-independent
/// pixels: `rustkit_viewhost::Bounds` as plain data, so the protocol does
/// not name a viewhost type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// What a pointer did at a point, in the view's CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerAction {
    Down,
    Move,
    /// The pointer left the view: hover state ends.
    Leave,
    /// Press and release at the point: `mousedown`, `mouseup`, `click`,
    /// each with its own turn budget.
    Click,
}

/// A key the window thread took for the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    pub key_code: u32,
    /// The key's name as the window thread reports it (`"a"`, `"Enter"`).
    pub key: String,
    /// The text the key inserts, if any (`handle_text_key`).
    pub text: Option<String>,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Key up rather than key down.
    pub up: bool,
}

/// A question the window thread asks the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageQuery {
    /// `focus_at_point`: focus the element at the point; its tag comes back.
    FocusAt { x: f32, y: f32 },
    /// `link_at_point`: the `href` under the point, for the cursor.
    LinkAt { x: f32, y: f32 },
    /// `has_focused_element`: whether a content element holds focus.
    HasFocusedElement,
}

/// What the page answers a [`PageQuery`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryAnswer {
    /// `FocusAt`: the tag of the element focused; `None` cleared focus.
    Focused(Option<String>),
    /// `LinkAt`: the link under the point.
    Link(Option<String>),
    HasFocusedElement(bool),
}

/// Window thread to page thread.
#[derive(Debug, Clone, PartialEq)]
pub enum PageRequest {
    /// Load a URL. `replace` is what `take_script_navigation` reports
    /// (#707): the window's history entry is replaced, not pushed.
    LoadUrl { url: String, replace: bool },
    LoadHtml { html: String },
    SetBounds(PageBounds),
    /// Run script on the page. With `wants_result`, [`PageReply::ScriptResult`]
    /// carries the value as a string; without it nothing comes back.
    EvaluateScript { script: String, wants_result: bool },
    /// `seq` is the input queue's sequence number (M2); `at_ms` the
    /// arrival time on the window's input clock, for the input-to-present
    /// log (`note_input`).
    Pointer { action: PointerAction, x: f32, y: f32, seq: u64, at_ms: u64 },
    /// A wheel delta in CSS pixels. From M4b the Presenter applies it to
    /// the last frame first; the page still hears it.
    Scroll { dx: f32, dy: f32, seq: u64, at_ms: u64 },
    Key { key: KeyInput, seq: u64, at_ms: u64 },
    /// The content view took keyboard focus from the chrome.
    GrabKeyboard,
    Query(PageQuery),
    /// One turn of the live loop: timers due on the page clock, answers
    /// that arrived, modules that loaded. `elapsed_ms` is how far the page
    /// clock advances (whole milliseconds, as `process_events` does).
    Tick { elapsed_ms: u64 },
    /// Lay out again now (a resize that was deferred, a settings change).
    Relayout,
    /// Stop the script that is running (M1's interrupt word) and the load
    /// if one is in flight (M3b). The page is kept.
    Stop,
    /// Destroy the view; the thread may end once it answers.
    Close,
}

/// Page thread to window thread.
#[derive(Debug, Clone)]
pub enum PageReply {
    /// A navigation began; `nav_id` names it in every later reply and
    /// commit (M3b).
    LoadStarted { nav_id: u64, url: String },
    /// The document loaded (its scripts and lifecycle steps ran, or the
    /// budget stopped them). `status` is the response status the page saw.
    Loaded { nav_id: u64, url: String, status: Option<u16> },
    LoadFailed { nav_id: u64, url: String, error: String },
    /// A new frame for the Presenter. The commit holds an `Arc` of an
    /// immutable display list and plain numbers; nothing of the page.
    Frame(FrameCommit),
    /// The document's title changed.
    Title { nav_id: u64, title: String },
    /// The page asked to go somewhere: `location`, a link's default
    /// action, a form submit, a meta refresh. `replace` as in
    /// [`PageRequest::LoadUrl`]. The window thread decides and sends the
    /// load; the page does nothing on its own.
    NavigationRequested { nav_id: u64, url: String, replace: bool },
    /// The outcome of a [`PointerAction::Click`]: `rustkit_engine::ClickOutcome`
    /// as plain data.
    Click { seq: u64, focused: Option<String>, navigate: Option<String> },
    Answer { query: PageQuery, answer: QueryAnswer },
    ScriptResult { value: Option<String> },
    /// What the turn did and when the page wants the next one
    /// (`process_events`'s answer): `None` means nothing is due.
    TurnDone { next_turn_ms: Option<u64>, timers_ran: u32, requests: u32, in_flight: u32 },
    /// The page has been busy in one turn for this long; the window shows
    /// the busy bar after 2 s (M4c) and offers Stop.
    Busy { nav_id: u64, turn_ms: u64 },
    /// [`PageRequest::Stop`] took effect; `turn_ms` is how long the stopped
    /// turn had run.
    Stopped { nav_id: u64, turn_ms: u64 },
    Closed,
}

impl From<rustkit_viewhost::Bounds> for PageBounds {
    fn from(b: rustkit_viewhost::Bounds) -> Self {
        PageBounds { x: b.x, y: b.y, width: b.width, height: b.height }
    }
}

impl From<PageBounds> for rustkit_viewhost::Bounds {
    fn from(b: PageBounds) -> Self {
        rustkit_viewhost::Bounds::new(b.x, b.y, b.width, b.height)
    }
}

impl From<rustkit_engine::ClickOutcome> for PageReply {
    fn from(outcome: rustkit_engine::ClickOutcome) -> Self {
        PageReply::Click { seq: 0, focused: outcome.focused, navigate: outcome.navigate }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}

    /// Invariant 8: both directions of the protocol can cross a thread.
    /// An `Rc`, a reference or a page-owned handle in either enum is a
    /// compile error here, on every OS the app's tests build on.
    #[test]
    fn every_message_can_cross_to_the_other_thread() {
        assert_send::<PageRequest>();
        assert_send::<PageReply>();
        assert_send_sync::<PageRequest>();
        assert_send_sync::<PageReply>();
        assert_send_sync::<FrameCommit>();
    }

    /// The protocol is plain data: a request can be cloned and compared,
    /// so the handle PR can queue, coalesce (M2's rule: moves and wheels
    /// commute) and log it without touching the page.
    #[test]
    fn requests_are_plain_data() {
        let a = PageRequest::Pointer { action: PointerAction::Move, x: 1.0, y: 2.0, seq: 7, at_ms: 100 };
        assert_eq!(a.clone(), a);
        let b = PageRequest::Scroll { dx: 0.0, dy: -40.0, seq: 8, at_ms: 101 };
        assert_ne!(a, b);
    }

    /// The viewhost's bounds and the protocol's are the same rectangle.
    #[test]
    fn bounds_round_trip_through_the_protocol() {
        let b = rustkit_viewhost::Bounds::new(10, -20, 1280, 800);
        let p: PageBounds = b.into();
        assert_eq!(p, PageBounds { x: 10, y: -20, width: 1280, height: 800 });
        let back: rustkit_viewhost::Bounds = p.into();
        assert_eq!((back.x, back.y, back.width, back.height), (10, -20, 1280, 800));
    }

    /// A click's outcome carries exactly what the window thread acts on
    /// today (`click_at_point`): the focused tag and the link to follow.
    #[test]
    fn a_click_outcome_becomes_a_reply() {
        let outcome = rustkit_engine::ClickOutcome {
            focused: Some("input".into()),
            navigate: Some("https://example.test/next".into()),
        };
        match PageReply::from(outcome) {
            PageReply::Click { focused, navigate, .. } => {
                assert_eq!(focused.as_deref(), Some("input"));
                assert_eq!(navigate.as_deref(), Some("https://example.test/next"));
            }
            other => panic!("not a click reply: {other:?}"),
        }
    }
}
