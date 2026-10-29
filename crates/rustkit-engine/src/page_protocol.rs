//! The page/window protocol (threads plan M4, protocol PR).
//!
//! `PageHandle` is what the window side holds. It sends [`PageCmd`] and
//! receives [`PageEvent`], including a [`FrameCommit`] when the page's
//! frame changes. [`PageEndpoint`] is the page side: it owns the [`Engine`]
//! and applies commands.
//!
//! This PR runs both ends on the calling thread. `drain` applies every
//! command already queued before it returns, so a reply is ready as soon
//! as `drain` returns and nothing about a load, an input or a frame
//! changes. The content thread is a later PR: it moves the endpoint onto
//! its own thread and leaves the handle where it is.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;

use rustkit_viewhost::Bounds;
use tokio::sync::oneshot;
use url::Url;

/// A [`PageCmd::DevtoolsEval`] the page reached past its deadline answers
/// this, without running.
pub const EVAL_CANCELLED: &str = "cancelled: the page was busy past the deadline";

use crate::devtools_service::DevtoolsCounters;

use crate::{
    ClickOutcome, DevtoolsConsole, DevtoolsElement, DevtoolsNode, Engine, EngineEvent,
    EngineViewId, FrameCommit, InputMark, LivePump,
};

/// Where a document is in its load, as the page reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Started,
    Complete,
    Failed { error: String },
}

/// What the window side asks the page to do.
///
/// Commands that answer something carry a oneshot. On this PR the endpoint
/// sends the answer inside `drain`, on the same thread.
pub enum PageCmd {
    /// Load `url`. `nav_id` is the window side's number for this request
    /// (see [`PageHandle::last_sent_nav_id`]). The engine still assigns the
    /// id the cancel path reads; the handle's counter is the one a later
    /// thread will send.
    Navigate { url: Url, nav_id: u64 },
    /// `nav_id` 0 lets the engine allocate (the same-thread path). A
    /// non-zero id was allocated when the command was sent, and a newer
    /// one already on the counter supersedes this document.
    LoadHtml {
        html: String,
        nav_id: u64,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Cancel the load in progress. `true` when one was in progress.
    Stop {
        reply: oneshot::Sender<bool>,
    },
    /// Load the document's current URL again.
    Reload {
        nav_id: u64,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Input(PageInput),
    Resize {
        bounds: Bounds,
        /// Reserved for the content thread. The engine sizes from `bounds`.
        scale: f64,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Eval {
        script: String,
        reply: oneshot::Sender<Result<String, String>>,
    },
    /// The Inspector console's input line (T6c, R1 condition 5). Reached
    /// past `deadline` (the page was busy and the panel gave up), it is
    /// cancelled, never run late.
    DevtoolsEval {
        script: String,
        deadline: std::time::Instant,
        reply: oneshot::Sender<Result<String, String>>,
    },
    SetVisible {
        visible: bool,
    },
    /// Destroy the view.
    Close,
    /// One live-loop turn.
    PumpLive {
        elapsed_ms: u64,
        reply: oneshot::Sender<LivePump>,
    },
    /// Draw. `every` is `HIWAVE_RENDER_EVERY_WAKE`.
    Render {
        every: bool,
    },
    NoteInput {
        at: std::time::Instant,
        seq: u64,
    },
    GrabKeyboard,
    Relayout,
    /// Test hook: the next JavaScript runtime uses this host re-entry cap.
    /// The frame cap and the operand-stack cap are not changed.
    SetHostReentry {
        limit: usize,
    },
    /// Test hook: install `__rk_reenter` on the current runtime.
    InstallReentryProbe {
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// A developer-tools question (the Inspector, #764). Answered on the
    /// page's own thread, between its script turns, from the document and
    /// layout it last committed. The console's input line is [`PageCmd::Eval`].
    Devtools {
        query: DevtoolsQuery,
        reply: oneshot::Sender<DevtoolsAnswer>,
    },
    /// Test hook: hold the page thread in a host-side wait for this many
    /// ms, as a long script would (T6b: devtools reads are answered
    /// meanwhile).
    #[cfg(test)]
    Block(u64),
    /// Test hook: panic the page thread while it holds the view's
    /// Inspector lock (T6 R1 condition 2).
    #[cfg(test)]
    PanicHoldingInspector,
}

/// What the Inspector asks the page. Node ids are the document's raw
/// `NodeId`s; they do not survive a navigation.
#[derive(Debug, Clone, PartialEq)]
pub enum DevtoolsQuery {
    /// Record the page's console, errors and load events for the panel
    /// (on), or go back to the view's default (off), keeping what was
    /// recorded for the panel's next opening (T6b).
    Attach(bool),
    /// Outline this element on the page, or nothing.
    Highlight(Option<usize>),
    /// The element under viewport point (`x`, `y`), in CSS px.
    NodeAt { x: f32, y: f32 },
    /// The DOM below `root` (the document when `None`), expanded along the
    /// ancestors of `reveal`.
    DomTree { root: Option<usize>, reveal: Option<usize> },
    /// What the panel shows for one element.
    Element(usize),
    /// Console lines recorded since `since`.
    Console { since: u64 },
    /// The current document's requests (T7). Answered by the devtools
    /// thread, like console reads.
    Network,
}

/// The page's answer to a [`DevtoolsQuery`]. `None` and `Unavailable` mean
/// there was nothing to show (no document, no such node, a closed view).
#[derive(Debug, Clone, PartialEq)]
pub enum DevtoolsAnswer {
    Done,
    Node(Option<usize>),
    Tree(Option<DevtoolsNode>),
    Element(Option<DevtoolsElement>),
    Console(DevtoolsConsole),
    Network(crate::DevtoolsNetwork),
    Unavailable,
}

/// Answer one developer-tools question on the thread that owns `engine`.
/// Whether the page's frame changed (an outline was drawn or removed) is
/// the second value.
fn devtools_answer(engine: &mut Engine, view: EngineViewId, query: DevtoolsQuery) -> (DevtoolsAnswer, bool) {
    let answer = match query {
        DevtoolsQuery::Attach(on) => engine.devtools_attach(view, on).map(|_| DevtoolsAnswer::Done),
        DevtoolsQuery::Highlight(node) => {
            return match engine.devtools_highlight(view, node) {
                Ok(changed) => (DevtoolsAnswer::Done, changed),
                Err(_) => (DevtoolsAnswer::Unavailable, false),
            };
        }
        DevtoolsQuery::NodeAt { x, y } => engine.devtools_node_at(view, x, y).map(DevtoolsAnswer::Node),
        DevtoolsQuery::DomTree { root, reveal } => {
            engine.devtools_dom_tree(view, root, reveal).map(DevtoolsAnswer::Tree)
        }
        DevtoolsQuery::Element(node) => engine.devtools_element(view, node).map(DevtoolsAnswer::Element),
        DevtoolsQuery::Console { since } => engine.devtools_console(view, since).map(DevtoolsAnswer::Console),
        DevtoolsQuery::Network => engine.devtools_network(view).map(DevtoolsAnswer::Network),
    };
    (answer.unwrap_or(DevtoolsAnswer::Unavailable), false)
}

/// Whether an answer is worth waking the window for, rather than waiting
/// for its next wake. A console answer with nothing new is not: the panel
/// asks a few times a second on its own clock, and a woken panel used to ask
/// again at once, so an idle page and an open panel woke each other without
/// pause (#785 R1). An acknowledgement nobody reads is not either.
pub(crate) fn devtools_wakes_window(answer: &DevtoolsAnswer, frame_changed: bool) -> bool {
    match answer {
        DevtoolsAnswer::Console(c) => !c.entries.is_empty() || c.page_changed || c.tree_stale,
        DevtoolsAnswer::Done => frame_changed,
        _ => true,
    }
}

/// Input and the synchronous questions the window still asks.
///
/// The ones that return a value keep a oneshot so the window's current
/// callers stay synchronous. A later PR turns the questions the page can
/// answer on its own into events.
pub enum PageInput {
    TextKey {
        key_code: u32,
        key: String,
        ctrl: bool,
        shift: bool,
        alt: bool,
        reply: oneshot::Sender<bool>,
    },
    KeyUp {
        key_code: u32,
        key: String,
        ctrl: bool,
        shift: bool,
        alt: bool,
    },
    MouseDown { x: f32, y: f32 },
    MouseMove { x: f32, y: f32 },
    MouseLeave,
    Click {
        x: f32,
        y: f32,
        reply: oneshot::Sender<ClickOutcome>,
    },
    FocusAt {
        x: f32,
        y: f32,
        reply: oneshot::Sender<Option<String>>,
    },
    LinkAt {
        x: f32,
        y: f32,
        reply: oneshot::Sender<Option<String>>,
    },
    Scroll {
        dx: f32,
        dy: f32,
        reply: oneshot::Sender<bool>,
    },
    FormSubmit {
        reply: oneshot::Sender<Option<String>>,
    },
    TakeNavigation {
        reply: oneshot::Sender<Option<(String, bool)>>,
    },
    HasFocus {
        reply: oneshot::Sender<bool>,
    },
}

/// What the page tells the window.
///
/// `nav_id` is the navigation the page was on when it emitted the event.
/// The window drops one whose id is older than the navigation it has since
/// asked for.
#[derive(Debug, Clone)]
pub enum PageEvent {
    Commit(FrameCommit),
    Title {
        text: String,
        nav_id: u64,
    },
    Url {
        url: Url,
        nav_id: u64,
    },
    LoadState {
        state: LoadState,
        nav_id: u64,
    },
    NavigateRequested {
        url: String,
        replace: bool,
        nav_id: u64,
        /// The user activated a link or submitted a form. A script's
        /// `location.href`, `link.click()` or `form.submit()` is false.
        from_user: bool,
    },
    Focused(bool),
    /// Not produced on this PR. The variant is the protocol's.
    #[allow(dead_code)]
    Cursor(String),
    /// Not produced on this PR. The variant is the protocol's.
    #[allow(dead_code)]
    Unresponsive,
    Closed,
}

/// The window side of one page.
///
/// Send a command, then [`PageEndpoint::drain`] on this thread. Events are
/// read with [`PageHandle::drain_events`].
pub struct PageHandle {
    tx: Sender<PageCmd>,
    events: Receiver<PageEvent>,
    stamp: Arc<rustkit_js::ThreadStamp>,
    control: Arc<rustkit_js::ViewControl>,
    view: EngineViewId,
    last_sent_nav_id: AtomicU64,
}

/// The page side. Owns the engine for `view`.
pub struct PageEndpoint {
    rx: Receiver<PageCmd>,
    events: Sender<PageEvent>,
    engine: Engine,
    engine_events: Option<tokio::sync::mpsc::UnboundedReceiver<EngineEvent>>,
    view: EngineViewId,
    /// Navigation, generation and scroll of the last [`PageEvent::Commit`]
    /// sent. A new document can reuse a generation; the navigation is what
    /// makes that frame a different one. Scroll-only updates keep both.
    last_commit: Option<(u64, u64, (f32, f32))>,
    /// `SetVisible`, recorded for the content thread. Nothing reads it yet.
    #[allow(dead_code)]
    visible: bool,
    /// The content thread settles an unhandled key as a scroll, because the
    /// window does not wait to find out whether the page consumed it.
    settle_keys: bool,
    /// Post the window's live wake when an event is published. The
    /// same-thread drain does not: it is already on the window thread.
    wake_window: bool,
    /// The last [`PageEvent::Focused`] sent. A script's `focus()`, an
    /// autofocus or a new document moves focus without any input from the
    /// window, and the window answers its keys from this report.
    last_focused: Option<bool>,
    /// The newest navigation the window was told failed. A navigation is
    /// reported failed at most once.
    failed_nav: u64,
    /// What devtools costs this page (`devtools_service`).
    devtools_counters: Arc<DevtoolsCounters>,
}

/// Pair a handle with the endpoint that applies its commands.
pub(crate) fn adopt(
    mut engine: Engine,
    view: EngineViewId,
    rx: Receiver<PageCmd>,
    events: Sender<PageEvent>,
) -> PageEndpoint {
    let engine_events = engine.take_event_receiver();
    PageEndpoint {
        rx,
        events,
        engine,
        engine_events,
        view,
        last_commit: None,
        visible: true,
        settle_keys: false,
        wake_window: false,
        last_focused: None,
        failed_nav: 0,
        devtools_counters: Arc::default(),
    }
}

pub fn connect(mut engine: Engine, view: EngineViewId) -> (PageHandle, PageEndpoint) {
    let engine_events = engine.take_event_receiver();
    let control = engine
        .view_control(view)
        .unwrap_or_else(|| panic!("page protocol: view {view:?} is not in the engine"));
    let stamp = rustkit_js::Watchdog::shared().thread_stamp();
    let (tx, rx) = mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    let handle = PageHandle {
        tx,
        events: ev_rx,
        stamp,
        control,
        view,
        last_sent_nav_id: AtomicU64::new(0),
    };
    let endpoint = PageEndpoint {
        rx,
        events: ev_tx,
        engine,
        engine_events,
        view,
        last_commit: None,
        visible: true,
        settle_keys: false,
        wake_window: false,
        last_focused: None,
        failed_nav: 0,
        devtools_counters: Arc::default(),
    };
    (handle, endpoint)
}

impl PageHandle {
    pub fn view(&self) -> EngineViewId {
        self.view
    }

    pub fn stamp(&self) -> Arc<rustkit_js::ThreadStamp> {
        Arc::clone(&self.stamp)
    }

    pub fn control(&self) -> Arc<rustkit_js::ViewControl> {
        Arc::clone(&self.control)
    }

    /// How many navigations this handle has sent. The first is 1.
    pub fn last_sent_nav_id(&self) -> u64 {
        self.last_sent_nav_id.load(Ordering::Relaxed)
    }

    /// Queue `cmd`. It does nothing until [`PageEndpoint::drain`].
    pub fn send(&self, cmd: PageCmd) {
        if matches!(cmd, PageCmd::Navigate { .. } | PageCmd::Reload { .. }) {
            self.last_sent_nav_id.fetch_add(1, Ordering::Relaxed);
        }
        let _ = self.tx.send(cmd);
    }

    /// Events published since the last call, in order.
    pub fn drain_events(&self) -> Vec<PageEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => out.push(event),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
}

impl PageEndpoint {
    /// Apply every command queued so far, on this thread.
    ///
    /// `live` is the runtime the live loop already uses. A navigation builds
    /// its own runtime, as the view did before this protocol.
    pub fn drain(&mut self, live: Option<&tokio::runtime::Runtime>) {
        loop {
            match self.rx.try_recv() {
                Ok(cmd) => self.apply(cmd, live),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    fn apply(&mut self, cmd: PageCmd, live: Option<&tokio::runtime::Runtime>) {
        match cmd {
            PageCmd::Navigate { url, nav_id } => {
                self.load_url(url, nav_id);
            }
            PageCmd::LoadHtml { html, nav_id, reply } => {
                let result = self.load_html_cmd(&html, nav_id);
                let _ = reply.send(result);
                self.after_page_change();
            }
            PageCmd::Stop { reply } => {
                let stopped = self.engine.stop(self.view);
                // `stop_all` left ALL in the word so the script that was
                // running could unwind. That script has returned by the time
                // this command runs. Lift ALL so the next script on this
                // document can run; a turn id left in the word names a turn
                // that has already ended.
                if let Some(control) = self.engine.view_control(self.view) {
                    control.clear_all();
                }
                let _ = reply.send(stopped);
            }
            PageCmd::Reload { reply, .. } => {
                let result = match self.engine.get_url(self.view) {
                    Some(url) => {
                        self.load_url(url, 0);
                        Ok(())
                    }
                    None => Err("No URL to reload".to_string()),
                };
                let _ = reply.send(result);
            }
            PageCmd::Input(input) => self.apply_input(input),
            PageCmd::Resize { bounds, reply, .. } => {
                let result = self
                    .engine
                    .set_view_bounds(self.view, bounds)
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            PageCmd::DevtoolsEval { reply, .. } if reply.is_closed() => {
                // The panel discarded it (navigated, closed): nothing to run for.
            }
            PageCmd::DevtoolsEval { deadline, reply, .. } if std::time::Instant::now() >= deadline => {
                let _ = reply.send(Err(EVAL_CANCELLED.to_string()));
            }
            PageCmd::DevtoolsEval { script, reply, .. } | PageCmd::Eval { script, reply } => {
                let result = self
                    .engine
                    .execute_script(self.view, &script)
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
                self.publish_commit();
                self.report_focus();
                // `location.href =` and `link.click()` from this script.
                // The same-thread window asks with TakeNavigation instead.
                self.report_script_navigation();
            }
            PageCmd::SetVisible { visible } => {
                self.visible = visible;
            }
            PageCmd::Close => {
                if let Err(e) = self.engine.destroy_view(self.view) {
                    tracing::warn!(error = %e, "Failed to destroy RustKit view");
                }
                self.emit(PageEvent::Closed);
            }
            PageCmd::PumpLive { elapsed_ms, reply } => {
                let turn = match live {
                    Some(runtime) => runtime.block_on(self.engine.pump_live(self.view, elapsed_ms)),
                    None => LivePump::default(),
                };
                let _ = reply.send(turn);
                self.after_page_change();
            }
            PageCmd::Render { every } => {
                if let Err(e) = self.engine.flush_pending_resize(self.view) {
                    tracing::debug!(error = %e, "layout after a resize failed");
                }
                if every {
                    self.engine.render_all_views();
                } else {
                    self.engine.render_changed_views();
                }
                self.publish_commit();
            }
            PageCmd::NoteInput { at, seq } => {
                self.engine.note_input(self.view, InputMark { at, seq });
            }
            PageCmd::GrabKeyboard => {
                self.engine.grab_keyboard(self.view);
            }
            PageCmd::Relayout => {
                if let Err(e) = self.engine.relayout_for(self.view, crate::RelayoutCause::Input) {
                    tracing::debug!(error = %e, "relayout after edit failed");
                }
                self.publish_commit();
            }
            PageCmd::SetHostReentry { limit } => {
                self.engine.set_host_reentry_limit(limit);
            }
            PageCmd::InstallReentryProbe { reply } => {
                let result = self
                    .engine
                    .install_reentry_probe(self.view)
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
            }
            PageCmd::Devtools { query, reply } => {
                self.devtools_counters.page_command();
                // The window watches at once when it can reach the page's
                // Inspector; while the page boots or holds its endpoint it
                // cannot, so the page watches too (#803 review (2)).
                if let (DevtoolsQuery::Attach(on), Some(inspector)) = (&query, self.engine.inspector(self.view)) {
                    crate::devtools_service::watch(&inspector, &self.devtools_counters, *on);
                }
                let (answer, frame_changed) = devtools_answer(&mut self.engine, self.view, query);
                let wake = devtools_wakes_window(&answer, frame_changed);
                let _ = reply.send(answer);
                if frame_changed {
                    // On the page's own thread there is no presenter: this
                    // publishes the outlined frame for the window to draw.
                    if let Err(e) = self.engine.render_view(self.view) {
                        tracing::debug!(error = %e, "devtools outline frame failed");
                    }
                    self.after_page_change();
                }
                // No event carries the answer: wake the content thread's
                // window so it reads it now, when there is something to read.
                if self.wake_window && wake {
                    self.devtools_counters.window_wake();
                    crate::script_net::poke_live_turn();
                }
            }
            #[cfg(test)]
            PageCmd::Block(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
            #[cfg(test)]
            PageCmd::PanicHoldingInspector => {
                if let Some(inspector) = self.engine.inspector(self.view) {
                    inspector.panic_holding_lock();
                }
            }
        }
    }

    fn apply_input(&mut self, input: PageInput) {
        let view = self.view;
        match input {
            PageInput::TextKey {
                key_code,
                key,
                ctrl,
                shift,
                alt,
                reply,
            } => {
                let submitted = if key_code == 0x0D {
                    self.engine.submit_focused_form(view)
                } else {
                    None
                };
                if let Some(url) = submitted {
                    let nav_id = self.engine.nav_id(view).unwrap_or(0);
                    self.emit(PageEvent::NavigateRequested {
                        url,
                        replace: false,
                        nav_id,
                        from_user: true,
                    });
                    let _ = reply.send(true);
                } else {
                    let focused_before = self.engine.focused_node(view);
                    let consumed = self
                        .engine
                        .handle_text_key(view, key_code, &key, ctrl, shift, alt);
                    if self.settle_keys && !consumed && focused_before.is_none() {
                        if let Some(dy) = scroll_dy_for_unhandled_key(key_code, &key) {
                            let _ = self.engine.scroll_view(view, 0.0, dy);
                        }
                    }
                    let _ = reply.send(consumed || self.settle_keys);
                }
            }
            PageInput::KeyUp { key_code, key, ctrl, shift, alt } => {
                self.engine.handle_key_up(view, key_code, &key, ctrl, shift, alt);
            }
            PageInput::MouseDown { x, y } => {
                self.engine.mouse_down_at_point(view, x, y);
            }
            PageInput::MouseMove { x, y } => self.engine.mouse_move_at_point(view, x, y),
            PageInput::MouseLeave => self.engine.mouse_leave(view),
            PageInput::Click { x, y, reply } => {
                let outcome = self.engine.click_at_point(view, x, y);
                if let Some(url) = outcome.navigate.clone() {
                    let nav_id = self.engine.nav_id(view).unwrap_or(0);
                    self.emit(PageEvent::NavigateRequested {
                        url,
                        replace: false,
                        nav_id,
                        from_user: true,
                    });
                }
                let _ = reply.send(outcome);
            }
            PageInput::FocusAt { x, y, reply } => {
                let tag = self.engine.focus_at_point(view, x, y);
                self.last_focused = Some(tag.is_some());
                self.emit(PageEvent::Focused(tag.is_some()));
                let _ = reply.send(tag);
            }
            PageInput::LinkAt { x, y, reply } => {
                let url = self.engine.link_at_point(view, x, y);
                let _ = reply.send(url);
            }
            PageInput::Scroll { dx, dy, reply } => {
                let changed = match self.engine.scroll_view(view, dx, dy) {
                    Ok(changed) => changed,
                    Err(e) => {
                        tracing::debug!(error = %e, "scroll_by failed");
                        false
                    }
                };
                let _ = reply.send(changed);
                self.publish_commit();
            }
            PageInput::FormSubmit { reply } => {
                let url = self.engine.submit_focused_form(view);
                if let Some(url) = url.clone() {
                    let nav_id = self.engine.nav_id(view).unwrap_or(0);
                    self.emit(PageEvent::NavigateRequested {
                        url,
                        replace: false,
                        nav_id,
                        from_user: true,
                    });
                }
                let _ = reply.send(url);
            }
            PageInput::TakeNavigation { reply } => {
                let taken = self.engine.take_script_navigation_with_replace(view);
                if let Some((url, replace)) = taken.clone() {
                    let nav_id = self.engine.nav_id(view).unwrap_or(0);
                    self.emit(PageEvent::NavigateRequested {
                        url,
                        replace,
                        nav_id,
                        from_user: false,
                    });
                }
                let _ = reply.send(taken);
            }
            PageInput::HasFocus { reply } => {
                let focused = self.engine.focused_node(view).is_some();
                self.last_focused = Some(focused);
                self.emit(PageEvent::Focused(focused));
                let _ = reply.send(focused);
            }
        }
        self.publish_commit();
        self.report_focus();
        // A listener's `location.href =` or `link.click()` is not the
        // click's own default action. The content thread reports it; the
        // same-thread window still pulls with TakeNavigation.
        self.report_script_navigation();
    }

    fn load_html_cmd(&mut self, html: &str, nav_id: u64) -> Result<(), String> {
        let loaded = if nav_id == 0 {
            self.engine.load_html(self.view, html)
        } else {
            self.engine.load_html_as(self.view, html, Some(nav_id))
        };
        loaded.map_err(|e| e.to_string())
    }

    fn load_url(&mut self, url: Url, nav_id: u64) {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create runtime");
                return;
            }
        };
        rt.block_on(self.load_yielding(url, nav_id));
        self.after_page_change();
    }

    /// Yield after DOMContentLoaded while inserted scripts are still
    /// arriving. A page with none finishes inside this call. `nav_id` 0
    /// lets the engine allocate; any other id is the window's.
    ///
    /// Events the load emits while a fetch is outstanding are forwarded
    /// every couple of milliseconds, including a frame the load published
    /// after layout. The load future borrows only the engine, so the event
    /// channel can be read while it is pending.
    async fn load_yielding(&mut self, url: Url, nav_id: u64) {
        let view = self.view;
        let wake = self.wake_window;
        let engine = &mut self.engine;
        let engine_events = &mut self.engine_events;
        let tx = &self.events;
        let last_commit = &mut self.last_commit;
        let failed_nav = &mut self.failed_nav;
        let mut load = Box::pin(async move {
            if nav_id == 0 {
                engine.load_url_yielding(view, url).await
            } else {
                engine.load_url_yielding_as(view, url, nav_id).await
            }
        });
        loop {
            // Polled on the runtime's waker via the sleep below: a fetch
            // that completes during the sleep is observed on the next poll.
            // `yield_now` inside the load returns Pending on the first poll
            // and Ready on the next, so a frame sent before that yield is
            // forwarded before the load continues into scripts.
            let polled = std::future::Future::poll(
                load.as_mut(),
                &mut std::task::Context::from_waker(futures::task::noop_waker_ref()),
            );
            match polled {
                std::task::Poll::Ready(result) => {
                    let failed = result.as_ref().err().map(|e| e.to_string());
                    if let Some(e) = &failed {
                        tracing::error!(error = %e, "Failed to load URL");
                    }
                    forward_events(engine_events, tx, last_commit, failed_nav, nav_id, wake);
                    drop(load);
                    self.end_uncommitted_nav(nav_id, failed);
                    return;
                }
                std::task::Poll::Pending => {
                    forward_events(engine_events, tx, last_commit, failed_nav, nav_id, wake);
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
            }
        }
    }

    /// A navigation that ended without committing a document: a refused
    /// connection, a TLS or fetch error, an empty error body. The window's
    /// `navigate` raised ALL on the interrupt word so the old document's
    /// running script would stop, and only a commit lowers it. Unless a
    /// newer navigation or a Stop cancelled this one (they lower it
    /// themselves), lower it here, so the document still on screen keeps
    /// its timers, clicks and keys. Tell the window the load is over, so
    /// it stops showing the page as loading: Failed once for an error,
    /// Complete for a load that ended Ok with nothing to commit (a 204).
    fn end_uncommitted_nav(&mut self, nav_id: u64, failed: Option<String>) {
        if nav_id == 0 {
            return;
        }
        let Some(control) = self.engine.view_control(self.view) else {
            return;
        };
        if !control.end_uncommitted(nav_id) {
            return;
        }
        // The address bar went to the URL that failed. The document on
        // screen is still the committed one; show its URL again.
        if let Some(url) = self.engine.get_url(self.view) {
            let _ = self.events.send(PageEvent::Url { url, nav_id });
        }
        // A load that ended Ok without a commit (a 204) is not a failure:
        // the page stays as it is and the load is over. A failure the
        // engine already reported is not reported again.
        let state = match failed {
            None => Some(LoadState::Complete),
            Some(_) if self.failed_nav == nav_id => None,
            Some(error) => {
                self.failed_nav = nav_id;
                Some(LoadState::Failed { error })
            }
        };
        if let Some(state) = state {
            let _ = self.events.send(PageEvent::LoadState { state, nav_id });
        }
        if self.wake_window {
            crate::script_net::poke_live_turn();
        }
    }

    /// Load `url` as [`PageCmd::Navigate`] does, as a future the window
    /// thread polls a slice at a time (#743). It holds the endpoint until
    /// the load ends or yields after `DOMContentLoaded`.
    pub async fn navigate(&mut self, url: Url, nav_id: u64) {
        self.load_yielding(url, nav_id).await;
        self.after_page_change();
    }

    /// Cancels this view's navigations without the endpoint (M3b).
    pub fn nav_canceller(&self) -> Option<crate::NavCanceller> {
        self.engine.nav_canceller(self.view)
    }

    /// Whether a yielded load is parked on the view; see
    /// [`Engine::load_tail_open`].
    pub fn load_tail_open(&self) -> bool {
        self.engine.load_tail_open(self.view)
    }

    fn after_page_change(&mut self) {
        self.translate_engine_events();
        self.publish_commit();
        self.report_focus();
        self.report_script_navigation();
    }

    /// Tell the content thread's window when the page's focus changed on
    /// its own: a script's `focus()` or `blur()`, a timer, a new document
    /// with nothing focused. Only the content thread reports it; the
    /// same-thread window asks with [`PageInput::HasFocus`].
    fn report_focus(&mut self) {
        if !self.wake_window {
            return;
        }
        let focused = self.engine.focused_node(self.view).is_some();
        if self.last_focused == Some(focused) {
            return;
        }
        self.last_focused = Some(focused);
        self.emit(PageEvent::Focused(focused));
    }

    /// A navigation the page's script asked for (`location.href`,
    /// `location.replace`, `link.click()`, `form.submit()`), tagged with
    /// the document that asked. Only the content thread reports it here.
    /// The same-thread window sends [`PageInput::TakeNavigation`] and
    /// would otherwise see an empty queue.
    fn report_script_navigation(&mut self) {
        if !self.wake_window {
            return;
        }
        let Some((url, replace)) = self.engine.take_script_navigation_with_replace(self.view) else {
            return;
        };
        let nav_id = self.engine.nav_id(self.view).unwrap_or(0);
        self.emit(PageEvent::NavigateRequested {
            url,
            replace,
            nav_id,
            from_user: false,
        });
    }

    fn translate_engine_events(&mut self) {
        let nav_id = self.engine.nav_id(self.view).unwrap_or(0);
        let wake = self.wake_window;
        forward_events(
            &mut self.engine_events,
            &self.events,
            &mut self.last_commit,
            &mut self.failed_nav,
            nav_id,
            wake,
        );
    }

    fn publish_commit(&mut self) {
        let Some(commit) = self.engine.latest_commit(self.view).cloned() else {
            return;
        };
        // A commit from before this document was laid out has no display
        // list. Publishing it lets the window present the background and
        // then drop the real frame when the generation matches.
        if commit.list.is_none() {
            return;
        }
        let key = (commit.nav_generation, commit.generation, commit.scroll_offset);
        if self.last_commit == Some(key) {
            return;
        }
        self.last_commit = Some(key);
        self.emit(PageEvent::Commit(commit));
    }

    fn emit(&self, event: PageEvent) {
        let _ = self.events.send(event);
        if self.wake_window {
            crate::script_net::poke_live_turn();
        }
    }

    pub(crate) fn set_settle_keys(&mut self, settle: bool) {
        self.settle_keys = settle;
    }

    pub(crate) fn set_wake_window(&mut self, wake: bool) {
        self.wake_window = wake;
    }

    /// What devtools has cost this page so far.
    pub fn devtools_counters(&self) -> Arc<DevtoolsCounters> {
        Arc::clone(&self.devtools_counters)
    }

    /// The page's Inspector, for reads the page need not answer
    /// (`devtools_service`).
    pub fn inspector(&self) -> Option<Arc<crate::inspector::Inspector>> {
        self.engine.inspector(self.view)
    }

    /// Commands waiting on the channel, with adjacent resizes, renders and
    /// live turns collapsed. Clicks and keys keep their order.
    pub(crate) fn drain_coalesced(&self) -> Vec<PageCmd> {
        let mut batch = Vec::new();
        for _ in 0..64 {
            match self.rx.try_recv() {
                Ok(cmd) => batch.push(cmd),
                Err(_) => break,
            }
        }
        coalesce(batch)
    }

    /// The content thread's loop. One current-thread runtime, already
    /// entered by the caller: timers and fetches progress on every idle wait.
    pub(crate) async fn run(mut self) {
        let mut clock = tokio::time::Instant::now();
        loop {
            let mut cmds = self.drain_coalesced();
            if cmds.is_empty() {
                let elapsed = clock.elapsed().as_millis() as u64;
                clock = tokio::time::Instant::now();
                let turn = self.engine.pump_live(self.view, elapsed).await;
                self.after_page_change();
                // A resize (or any other command) that arrived during the
                // turn is applied before the next timer slice. The frame
                // yield returns between timer tasks; the bounds message is
                // waiting here, and the next pump lays the page out. The
                // sleep is only the wait for the next timer. A single
                // callback that does not return is not a yield: the bounds
                // wait until that callback does.
                cmds = self.drain_coalesced();
                if cmds.is_empty() {
                    let wait_ms = turn.next_timer_ms.unwrap_or(50).clamp(2, 50);
                    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                    continue;
                }
            }
            for cmd in cmds {
                let close = matches!(cmd, PageCmd::Close);
                self.apply_async(cmd).await;
                if close {
                    return;
                }
            }
        }
    }

    async fn apply_async(&mut self, cmd: PageCmd) {
        match cmd {
            PageCmd::Navigate { url, nav_id } => {
                self.load_yielding(url, nav_id).await;
                self.after_page_change();
            }
            PageCmd::Reload { reply, nav_id } => {
                let result = match self.engine.get_url(self.view) {
                    Some(url) => {
                        self.load_yielding(url, nav_id).await;
                        Ok(())
                    }
                    None => Err("No URL to reload".to_string()),
                };
                let _ = reply.send(result);
                self.after_page_change();
            }
            PageCmd::PumpLive { elapsed_ms, reply } => {
                let turn = self.engine.pump_live(self.view, elapsed_ms).await;
                let _ = reply.send(turn);
                self.after_page_change();
            }
            PageCmd::LoadHtml { html, nav_id, reply } => {
                let result = self.load_html_cmd(&html, nav_id);
                let _ = reply.send(result);
                self.after_page_change();
            }
            other => self.apply(other, None),
        }
    }
}

fn forward_events(
    engine_events: &mut Option<tokio::sync::mpsc::UnboundedReceiver<EngineEvent>>,
    tx: &Sender<PageEvent>,
    last_commit: &mut Option<(u64, u64, (f32, f32))>,
    failed_nav: &mut u64,
    nav_id: u64,
    wake: bool,
) {
    let Some(rx) = engine_events.as_mut() else {
        return;
    };
    let mut pending = Vec::new();
    while let Ok(event) = rx.try_recv() {
        let mapped = match event {
            EngineEvent::NavigationStarted { url, .. } => vec![
                PageEvent::LoadState {
                    state: LoadState::Started,
                    nav_id,
                },
                PageEvent::Url { url, nav_id },
            ],
            EngineEvent::NavigationCommitted { url, .. } => {
                vec![PageEvent::Url { url, nav_id }]
            }
            EngineEvent::PageLoaded { url, title, .. } => {
                let mut out = Vec::new();
                if let Some(text) = title {
                    out.push(PageEvent::Title { text, nav_id });
                }
                out.push(PageEvent::Url { url, nav_id });
                out.push(PageEvent::LoadState {
                    state: LoadState::Complete,
                    nav_id,
                });
                out
            }
            EngineEvent::NavigationFailed { url, error, .. } => {
                if *failed_nav == nav_id && nav_id != 0 {
                    Vec::new()
                } else {
                    *failed_nav = nav_id;
                    vec![
                        PageEvent::Url { url, nav_id },
                        PageEvent::LoadState {
                            state: LoadState::Failed { error },
                            nav_id,
                        },
                    ]
                }
            }
            EngineEvent::TitleChanged { title, .. } => {
                vec![PageEvent::Title {
                    text: title,
                    nav_id,
                }]
            }
            EngineEvent::ViewFocused { .. } => vec![PageEvent::Focused(true)],
            EngineEvent::FrameCommitted(commit) => {
                if commit.list.is_none() {
                    Vec::new()
                } else {
                    let key = (commit.nav_generation, commit.generation, commit.scroll_offset);
                    if *last_commit == Some(key) {
                        Vec::new()
                    } else {
                        *last_commit = Some(key);
                        vec![PageEvent::Commit(commit)]
                    }
                }
            }
            EngineEvent::ConsoleMessage { .. }
            | EngineEvent::ViewResized { .. }
            | EngineEvent::DownloadStarted { .. }
            | EngineEvent::ImageLoaded { .. }
            | EngineEvent::ImageError { .. }
            | EngineEvent::FaviconDetected { .. } => Vec::new(),
        };
        pending.extend(mapped);
    }
    if pending.is_empty() {
        return;
    }
    for event in pending {
        let _ = tx.send(event);
    }
    if wake {
        crate::script_net::poke_live_turn();
    }
}

/// Collapse adjacent resizes, renders and live turns. Anything else,
/// including clicks and keys, stays where it was.
pub(crate) fn coalesce(batch: Vec<PageCmd>) -> Vec<PageCmd> {
    let mut out = Vec::new();
    for cmd in batch {
        match cmd {
            PageCmd::Resize {
                bounds,
                scale,
                reply,
            } => {
                if matches!(out.last(), Some(PageCmd::Resize { .. })) {
                    if let Some(PageCmd::Resize {
                        reply: previous, ..
                    }) = out.pop()
                    {
                        let _ = previous.send(Ok(()));
                    }
                }
                out.push(PageCmd::Resize {
                    bounds,
                    scale,
                    reply,
                });
            }
            PageCmd::Render { every } => {
                if let Some(PageCmd::Render { every: previous }) = out.last_mut() {
                    *previous = *previous || every;
                } else {
                    out.push(PageCmd::Render { every });
                }
            }
            PageCmd::PumpLive { elapsed_ms, reply } => {
                if matches!(out.last(), Some(PageCmd::PumpLive { .. })) {
                    if let Some(PageCmd::PumpLive {
                        reply: previous, ..
                    }) = out.pop()
                    {
                        let _ = previous.send(crate::LivePump::default());
                    }
                }
                out.push(PageCmd::PumpLive { elapsed_ms, reply });
            }
            other => out.push(other),
        }
    }
    out
}

fn scroll_dy_for_unhandled_key(key_code: u32, key: &str) -> Option<f32> {
    match (key_code, key) {
        (0x28, _) => Some(-40.0),
        (0x26, _) => Some(40.0),
        (0x22, _) | (0, " ") => Some(-600.0),
        (0x21, _) => Some(600.0),
        (0x23, _) => Some(-f32::MAX),
        (0x24, _) => Some(f32::MAX),
        _ => None,
    }
}

impl std::fmt::Debug for PageHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageHandle")
            .field("view", &self.view)
            .field("last_sent_nav_id", &self.last_sent_nav_id())
            .finish()
    }
}

#[cfg(all(test, feature = "headless"))]
mod tests {
    use super::*;
    use crate::EngineConfig;
    use rustkit_viewhost::Bounds;
    use tokio::sync::oneshot;
    use url::Url;

    fn pair() -> (PageHandle, PageEndpoint) {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
            .expect("view");
        connect(engine, view)
    }

    fn eval(handle: &PageHandle, endpoint: &mut PageEndpoint, script: &str) -> String {
        let (tx, mut rx) = oneshot::channel();
        handle.send(PageCmd::Eval { script: script.to_string(), reply: tx });
        endpoint.drain(None);
        rx.try_recv().expect("eval reply is ready when drain returns").expect("eval")
    }

    /// Commands apply on the caller, in send order, and a changed frame
    /// comes back as a commit. Nothing is deferred to another thread.
    #[test]
    fn commands_apply_on_the_calling_thread_before_drain_returns() {
        let (handle, mut endpoint) = pair();
        let (tx, mut rx) = oneshot::channel();
        handle.send(PageCmd::LoadHtml {
            html: "<p id=\"t\">hi</p>".into(),
            nav_id: 0,
            reply: tx,
        });
        let (tx1, mut rx1) = oneshot::channel();
        handle.send(PageCmd::Eval {
            script: "window.step = 1".into(),
            reply: tx1,
        });
        let (tx2, mut rx2) = oneshot::channel();
        handle.send(PageCmd::Eval {
            script: "window.step = window.step + 1; document.getElementById('t').textContent".into(),
            reply: tx2,
        });
        // No reply exists until drain, and drain does not leave this thread.
        assert!(rx.try_recv().is_err(), "a command ran before drain");
        endpoint.drain(None);
        assert!(rx.try_recv().expect("load reply").is_ok());
        assert!(rx1.try_recv().expect("first eval reply").is_ok());
        let text = rx2.try_recv().expect("second eval reply").expect("eval ok");
        assert!(text.contains("hi"), "{text}");
        // Load, then step = 1, then step + 1, all in that one drain.
        assert_eq!(
            eval(&handle, &mut endpoint, "String(window.step)"),
            r#"String("2")"#,
            "the later command ran after the earlier one"
        );
        handle.send(PageCmd::Render { every: false });
        endpoint.drain(None);
        let events = handle.drain_events();
        assert!(
            events.iter().any(|event| matches!(event, PageEvent::Commit(commit) if commit.generation > 0)),
            "a frame change is a commit: {events:?}"
        );
        assert_eq!(handle.last_sent_nav_id(), 0, "LoadHtml is not a Navigate");
        handle.send(PageCmd::Navigate {
            url: Url::parse("data:text/html,ok").unwrap(),
            nav_id: 1,
        });
        // The counter moves when the command is sent. The engine still
        // assigns the id its cancel path reads.
        assert_eq!(handle.last_sent_nav_id(), 1);
        endpoint.drain(None);
        assert_eq!(handle.last_sent_nav_id(), 1);
        let _ = handle.drain_events();
    }
}
