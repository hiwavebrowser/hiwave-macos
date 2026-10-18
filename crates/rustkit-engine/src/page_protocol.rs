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

use crate::{
    ClickOutcome, Engine, EngineEvent, EngineViewId, FrameCommit, InputMark, LivePump,
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
    LoadHtml {
        html: String,
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
#[derive(Debug, Clone)]
pub enum PageEvent {
    Commit(FrameCommit),
    Title(String),
    Url(Url),
    LoadState(LoadState),
    NavigateRequested { url: String, replace: bool },
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
    /// Generation of the last [`PageEvent::Commit`] sent. `None` until the
    /// first one, so a frame whose generation is still 0 is not dropped.
    last_commit: Option<u64>,
    /// `SetVisible`, recorded for the content thread. Nothing reads it yet.
    #[allow(dead_code)]
    visible: bool,
}

/// Pair a handle with the endpoint that applies its commands.
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
            PageCmd::Navigate { url, .. } => {
                self.load_url(url);
            }
            PageCmd::LoadHtml { html, reply } => {
                let result = self
                    .engine
                    .load_html(self.view, &html)
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
                self.after_page_change();
            }
            PageCmd::Stop { reply } => {
                let stopped = self.engine.stop(self.view);
                let _ = reply.send(stopped);
            }
            PageCmd::Reload { reply, .. } => {
                let result = match self.engine.get_url(self.view) {
                    Some(url) => {
                        self.load_url(url);
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
            PageCmd::Eval { script, reply } => {
                let result = self
                    .engine
                    .execute_script(self.view, &script)
                    .map_err(|e| e.to_string());
                let _ = reply.send(result);
                self.publish_commit();
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
        }
    }

    fn apply_input(&mut self, input: PageInput) {
        let view = self.view;
        match input {
            PageInput::TextKey { key_code, key, ctrl, shift, alt, reply } => {
                let consumed = self.engine.handle_text_key(view, key_code, &key, ctrl, shift, alt);
                let _ = reply.send(consumed);
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
                let _ = reply.send(outcome);
            }
            PageInput::FocusAt { x, y, reply } => {
                let tag = self.engine.focus_at_point(view, x, y);
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
                let _ = reply.send(url);
            }
            PageInput::TakeNavigation { reply } => {
                let taken = self.engine.take_script_navigation_with_replace(view);
                if let Some((url, replace)) = taken.clone() {
                    self.emit(PageEvent::NavigateRequested { url, replace });
                }
                let _ = reply.send(taken);
            }
            PageInput::HasFocus { reply } => {
                let focused = self.engine.focused_node(view).is_some();
                self.emit(PageEvent::Focused(focused));
                let _ = reply.send(focused);
            }
        }
        self.publish_commit();
    }

    fn load_url(&mut self, url: Url) {
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create runtime");
                return;
            }
        };
        rt.block_on(async {
            // Yield after DOMContentLoaded while inserted scripts are still
            // arriving. A page with none finishes inside this call, as
            // `load_url` did. Later live turns run the tail.
            if let Err(e) = self.engine.load_url_yielding(self.view, url).await {
                tracing::error!(error = %e, "Failed to load URL");
            }
        });
        self.after_page_change();
    }

    fn after_page_change(&mut self) {
        self.translate_engine_events();
        self.publish_commit();
    }

    fn translate_engine_events(&mut self) {
        let raw = {
            let Some(rx) = self.engine_events.as_mut() else {
                return;
            };
            let mut raw = Vec::new();
            while let Ok(event) = rx.try_recv() {
                raw.push(event);
            }
            raw
        };
        let mut pending = Vec::new();
        for event in raw {
            let mapped = match event {
                EngineEvent::NavigationStarted { url, .. } => {
                    Some(PageEvent::LoadState(LoadState::Started))
                        .into_iter()
                        .chain(Some(PageEvent::Url(url)))
                        .collect::<Vec<_>>()
                }
                EngineEvent::NavigationCommitted { url, .. } => vec![PageEvent::Url(url)],
                EngineEvent::PageLoaded { url, title, .. } => {
                    let mut out = Vec::new();
                    if let Some(title) = title {
                        out.push(PageEvent::Title(title));
                    }
                    out.push(PageEvent::Url(url));
                    out.push(PageEvent::LoadState(LoadState::Complete));
                    out
                }
                EngineEvent::NavigationFailed { url, error, .. } => {
                    vec![
                        PageEvent::Url(url),
                        PageEvent::LoadState(LoadState::Failed { error }),
                    ]
                }
                EngineEvent::TitleChanged { title, .. } => vec![PageEvent::Title(title)],
                EngineEvent::ViewFocused { .. } => vec![PageEvent::Focused(true)],
                EngineEvent::ConsoleMessage { .. }
                | EngineEvent::ViewResized { .. }
                | EngineEvent::DownloadStarted { .. }
                | EngineEvent::ImageLoaded { .. }
                | EngineEvent::ImageError { .. }
                | EngineEvent::FaviconDetected { .. } => Vec::new(),
            };
            pending.extend(mapped);
        }
        for event in pending {
            self.emit(event);
        }
    }

    fn publish_commit(&mut self) {
        let Some(commit) = self.engine.latest_commit(self.view).cloned() else {
            return;
        };
        if self.last_commit == Some(commit.generation) {
            return;
        }
        self.last_commit = Some(commit.generation);
        self.emit(PageEvent::Commit(commit));
    }

    fn emit(&self, event: PageEvent) {
        let _ = self.events.send(event);
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
