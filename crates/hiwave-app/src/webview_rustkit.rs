//! RustKit WebView adapter for HiWave macOS
//!
//! This module provides the RustKit engine as the default WebView backend for content rendering.
//! It wraps `rustkit_engine::Engine` and provides a WRY-like interface.


use super::shield_adapter::create_shield_interceptor_with_counter;
use super::webview::IWebContent;
use hiwave_core::{HiWaveError, HiWaveResult};
use rustkit_engine::{
    connect, content_thread_enabled, ContentThread, EngineBuilder, EngineViewId, FrameCommit,
    LoadState, PageCmd, PageEndpoint, PageEvent, PageHandle, PageInput, WindowSurface,
};
use rustkit_viewhost::Bounds;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::window::Window;
use tao::rwh_06::HasWindowHandle;
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};
use url::Url;

/// Shortest gap between two turns of the live loop that a page's timers can
/// ask for (an animation loop asks for 16ms; a `setTimeout(f, 0)` chain
/// asks for none).
const MIN_LIVE_TURN: Duration = Duration::from_millis(4);

/// How soon the loop turns again while a page's XHR or `fetch` is out.
/// Nothing wakes the loop when that answer comes, so it looks. An inserted
/// script's fetch is not one of these: its completion posts `LiveTurn`.
const LIVE_REQUEST_POLL: Duration = Duration::from_millis(10);

/// A RustKit-based WebView that implements IWebContent.
///
/// With `HIWAVE_CONTENT_THREAD` unset, the window talks to the page through
/// a [`PageHandle`] and drains it on this thread. With the variable set,
/// the page is built on the content thread and this view only sends
/// commands, presents commits, and never waits for a load or a script.
///
/// # Thread Safety
///
/// This type is NOT Send or Sync. All operations must be performed on the main thread.
pub struct RustKitView {
    /// Commands in, events out. Applied by [`Self::submit`] on this thread.
    /// `None` when the page runs on the content thread.
    handle: Option<PageHandle>,
    /// Owns the engine and applies the handle's commands. `None` on the
    /// content-thread path: that engine is constructed on the other thread.
    /// Shared with the load in progress ([`PendingLoad`]), which holds it
    /// mutably until the load ends; every other call finds it busy and
    /// does nothing meanwhile.
    endpoint: Option<Rc<RefCell<PageEndpoint>>>,
    /// The page, when `HIWAVE_CONTENT_THREAD` is set. The window does not
    /// join it.
    content: Option<ContentThread>,
    /// NSView, GPU surface and renderer. Created on this thread, and only
    /// when the page is on the content thread. The flag-off engine still
    /// owns its own presenter.
    surface: Option<RefCell<WindowSurface>>,
    /// Navigation the window has asked for. Older events are dropped.
    accepted_nav: Cell<u64>,
    /// A navigation the page decided on, harvested from events:
    /// url, replace, and whether the user activated it.
    script_nav: RefCell<Option<(String, bool, bool)>>,
    /// Newest commit whose navigation is still the one on screen.
    latest_commit: RefCell<Option<FrameCommit>>,
    /// The view ID within the engine. `None` after [`PageCmd::Close`].
    view_id: Option<EngineViewId>,
    /// Current URL (cached).
    current_url: RefCell<Option<String>>,
    /// Current title (cached).
    current_title: RefCell<Option<String>>,
    /// Current zoom level.
    zoom_level: RefCell<f64>,
    /// Visibility state.
    visible: RefCell<bool>,
    /// Loading state.
    loading: RefCell<bool>,
    /// Whether a page element held focus, as the content thread last
    /// reported it (`PageEvent::Focused`). With the page on its own thread,
    /// the window cannot ask and wait for the answer.
    page_focused: std::cell::Cell<bool>,
    /// Counter for blocked requests (shared with shield).
    #[allow(dead_code)]
    blocked_counter: Option<Arc<AtomicU64>>,
    /// Navigation history stack.
    history: RefCell<Vec<Url>>,
    /// Current position in navigation history.
    history_index: RefCell<usize>,
    /// The next `wry_load_url` replaces the current history entry instead
    /// of adding one: the page's script asked for a replace navigation
    /// (`take_script_navigation`).
    replace_next: Cell<bool>,
    /// The page clock: the instant the live loop has run the page's timers
    /// up to. Reset when a page finishes loading.
    live_clock: Cell<Instant>,
    /// The earliest the next live turn may start: the last one's end plus as
    /// long as it took, so the window gets its turn in between.
    next_turn_at: Cell<Option<Instant>>,
    /// Runtime the live loop's network turns block on. `None` if it could
    /// not be built: the page then gets no timers or late fetches, as before.
    live_runtime: Option<tokio::runtime::Runtime>,
    /// The content `NSView`. Kept outside the engine so a load can move the
    /// frame while `engine` is borrowed.
    host_view: isize,
    /// Set once Close has been asked. Later commands are dropped.
    closed: Cell<bool>,
    /// The load in progress, polled a slice at a time by the event loop
    /// ([`RustKitView::step_load`]).
    pending_load: RefCell<Option<PendingLoad>>,
    /// Bounds set while a load held the engine, applied when it ends.
    pending_bounds: Cell<Option<Bounds>>,
    /// A panel attached or detached while a load held the engine, asked
    /// again when it ends (#803 review (2)): the page never heard it, so
    /// it recorded nothing and nothing watched it.
    pending_attach: Cell<Option<bool>>,
    /// Events the flag-off page published, kept for the tests.
    #[cfg(test)]
    seen_events: RefCell<Vec<PageEvent>>,
}

/// A content load the event loop drives: `Engine::load_url` as a future on
/// the live runtime, polled for at most [`LOAD_SLICE`] per wake.
///
/// Until 2026-10-19 the load ran to its end inside tao's event handler
/// (`block_on`), and the window took no click, key or chrome message for
/// as long as the page's network and scripts took (20 s on ebay). A nested
/// run loop pumped from inside the load (#739 as split from #720) re-enters
/// tao's handler lock from `drawRect:` and still dispatches no `NSEvent`.
/// Returning to tao between slices does neither. M4 (#730) replaces this
/// with the page's own thread.
struct PendingLoad {
    future: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
    /// Cancels the load without the engine (M3b): Stop, or a new load.
    canceller: Option<rustkit_engine::NavCanceller>,
    started: Instant,
}

/// The longest one poll of a pending load may wait on the network before
/// the event loop takes its next event. Script that runs inside the poll
/// is not cut by it; the script budgets bound that.
const LOAD_SLICE: Duration = Duration::from_millis(8);

/// How long a load that was cancelled may take to unwind before it is
/// dropped where it stands.
const CANCEL_UNWIND: Duration = Duration::from_secs(2);

/// Script budget for pages in the live app. The engine default (5 s) is
/// what parity-capture and the board keep; a person waits longer than a
/// capture does, and YouTube's 10.8 MB base module alone ran past 5 s and
/// left the page white (Pete, 2026-10-06).
///
/// 15 s, down from 60 s (Pete, 2026-10-08): scripts run on the window's
/// thread, so a script that never yields freezes the window until this
/// budget is spent (#616 stops it at its next host call only then). ebay's
/// new module graph held the window for the full minute. Nothing loaded in
/// the hand tests of 2026-10-08 needed more than about 8 s.
const LIVE_SCRIPT_BUDGET_MS: u64 = 15_000;

/// The longest the script of one turn after the load may run: the timer
/// callbacks and response handlers of one pass of the live loop, or the
/// listeners of one input event. Until 2026-10-12 there was no limit, and a
/// callback that did not end held the window for good.
///
/// It is per event, not per action: a click is `mousedown`, `mouseup` and
/// `click`, each with its own budget, and a resize has one of its own ahead
/// of the turn's.
///
/// 5 s is the figure the thread-ownership design recommends (its open
/// question 2); it is Pete's to set. It bounds the freeze and does not
/// remove it: the script still runs on the window's thread.
const LIVE_TURN_BUDGET_MS: u64 = 5_000;

/// The engine builder for content views, before the shield is attached.
fn content_engine_builder() -> EngineBuilder {
    // Browser-plausible UA. "HiWave/1.0 RustKit/1.0" alone got us
    // instantly rate-limited as a scraper even by Wikimedia (HTTP 429 on
    // every thumbnail, 2026-08-05 live session) — servers gate on the
    // Mozilla/AppleWebKit shape. Platform is reported honestly; HiWave
    // stays visible as the product token.
    EngineBuilder::new()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15 HiWave/1.0")
        .javascript_enabled(true)
        .cookies_enabled(true)
        .script_budget_ms(LIVE_SCRIPT_BUDGET_MS)
        // A script still running at the budget is stopped: scripts run on
        // the window's thread, and ebay.com's module graph held it for
        // minutes (Pete had to kill the app, 2026-10-08).
        .interrupt_scripts_at_budget(true)
        .live_turn_budget_ms(LIVE_TURN_BUDGET_MS)
        // The event loop loads what a page's script asks for
        // (`take_script_navigation`), so a page that has asked to leave is
        // not kept waiting for images its scripts added.
        .follows_script_navigation(true)
        // What a page asks of the network after its `load` event (beacons,
        // pixels, metrics fetches that may never stop) is the live loop's:
        // it neither holds the window thread inside the load nor delays
        // PageLoaded. 3.3 s of the eBay listing's 17.7 s hold was this.
        .post_load_requests_live(true)
}

/// `HIWAVE_RENDER_EVERY_WAKE=1`: draw at every wake of the event loop,
/// changed or not (see [`RustKitView::render`]).
fn render_every_wake() -> bool {
    static EVERY_WAKE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *EVERY_WAKE.get_or_init(|| std::env::var("HIWAVE_RENDER_EVERY_WAKE").is_ok_and(|v| v == "1"))
}

impl RustKitView {
    /// Create a new RustKit view.
    pub fn new(window: &Window, bounds: Bounds) -> HiWaveResult<Self> {
        Self::with_shield_counter(window, bounds, None)
    }

    /// Create a new RustKit view with a shared blocked request counter.
    pub fn with_shield_counter(
        window: &Window,
        bounds: Bounds,
        blocked_counter: Option<Arc<AtomicU64>>,
    ) -> HiWaveResult<Self> {
        info!("Creating RustKit view");

        // Get raw window handle from TAO window
        let raw_handle = window.window_handle()
            .map_err(|e| HiWaveError::WebView(format!("Failed to get window handle: {}", e)))?
            .as_raw();

        // Create engine builder with shield interceptor if counter provided
        let mut builder = content_engine_builder();

        // Add shield interceptor if counter is provided
        let counter_clone = blocked_counter.clone();
        if let Some(counter) = blocked_counter.clone() {
            info!("RustKit engine with shield ad-blocking enabled");
            let interceptor = create_shield_interceptor_with_counter(counter);
            builder = builder.request_interceptor(interceptor);
        }

        let (handle, endpoint, content, surface, host_view, view_id) = if content_thread_enabled() {
            // The surface is created here, on the window thread. The page
            // engine is created on the content thread, inside spawn.
            let surface = WindowSurface::attach(raw_handle, bounds)
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
            let host_view = surface.raw_view();
            rustkit_viewhost::clip_raw_view(host_view);
            let thread = ContentThread::spawn(builder, bounds).map_err(|e| {
                hiwave_core::HiWaveError::WebView(format!("content thread did not start: {e:?}"))
            })?;
            info!("RustKit view created on the content thread");
            (
                None,
                None,
                Some(thread),
                Some(RefCell::new(surface)),
                host_view,
                None,
            )
        } else {
            let mut engine = builder
                .build()
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
            let view_id = engine
                .create_view(raw_handle, bounds)
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
            let host_view = engine.host_view_raw(view_id);
            let (handle, endpoint) = connect(engine, view_id);
            rustkit_viewhost::clip_raw_view(host_view);
            info!(?view_id, "RustKit view created");
            (
                Some(handle),
                Some(Rc::new(RefCell::new(endpoint))),
                None,
                None,
                host_view,
                Some(view_id),
            )
        };

        Ok(Self::from_parts(
            handle,
            endpoint,
            content,
            surface,
            view_id,
            counter_clone,
            host_view,
        ))
    }

    /// A flag-off content view with no window (tests).
    #[cfg(test)]
    fn headless(bounds: Bounds) -> Self {
        let mut engine = content_engine_builder().build().expect("engine (no GPU?)");
        let view_id = engine.create_headless_view(bounds).expect("headless view");
        let (handle, endpoint) = connect(engine, view_id);
        Self::from_parts(
            Some(handle),
            Some(Rc::new(RefCell::new(endpoint))),
            None,
            None,
            Some(view_id),
            None,
            0,
        )
    }

    /// A content view on the content thread, with no window (tests). There
    /// is no surface, so nothing is presented.
    #[cfg(test)]
    fn headless_content(bounds: Bounds) -> Self {
        let thread = ContentThread::spawn(content_engine_builder(), bounds).expect("content thread");
        Self::from_parts(None, None, Some(thread), None, None, None, 0)
    }

    fn from_parts(
        handle: Option<PageHandle>,
        endpoint: Option<Rc<RefCell<PageEndpoint>>>,
        content: Option<ContentThread>,
        surface: Option<RefCell<WindowSurface>>,
        view_id: Option<EngineViewId>,
        counter_clone: Option<Arc<AtomicU64>>,
        host_view: isize,
    ) -> Self {
        Self {
            handle,
            endpoint,
            content,
            surface,
            accepted_nav: Cell::new(0),
            script_nav: RefCell::new(None),
            latest_commit: RefCell::new(None),
            view_id,
            current_url: RefCell::new(None),
            current_title: RefCell::new(None),
            zoom_level: RefCell::new(1.0),
            visible: RefCell::new(true),
            loading: RefCell::new(false),
            page_focused: std::cell::Cell::new(false),
            blocked_counter: counter_clone,
            history: RefCell::new(Vec::new()),
            history_index: RefCell::new(0),
            replace_next: Cell::new(false),
            live_clock: Cell::new(Instant::now()),
            next_turn_at: Cell::new(None),
            live_runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| warn!(error = %e, "No runtime for the live loop; page timers will not run"))
                .ok(),
            host_view,
            closed: Cell::new(false),
            pending_load: RefCell::new(None),
            pending_bounds: Cell::new(None),
            pending_attach: Cell::new(None),
            #[cfg(test)]
            seen_events: RefCell::new(Vec::new()),
        }
    }

    fn is_open(&self) -> bool {
        !self.closed.get()
    }

    /// Queue `cmd` and apply it on this thread. Events are taken and
    /// dropped: `current_title` and `loading` stay as they were when the
    /// view held the engine and never read its events.
    ///
    /// While a load holds the endpoint ([`PendingLoad`]) the command is
    /// dropped, as the engine calls were before the protocol (#743): a
    /// command queued behind the load would run against the new page.
    fn submit(&self, cmd: PageCmd) {
        if !self.is_open() {
            return;
        }
        if let Some(thread) = &self.content {
            thread.enqueue(cmd);
            return;
        }
        let Ok(mut endpoint) = self
            .endpoint
            .as_ref()
            .expect("inline page endpoint")
            .try_borrow_mut()
        else {
            return;
        };
        let handle = self.handle.as_ref().expect("inline page handle");
        handle.send(cmd);
        endpoint.drain(self.live_runtime.as_ref());
        drop(endpoint);
        self.drop_events();
    }

    /// Take what the flag-off page published since the last call.
    fn drop_events(&self) {
        let Some(handle) = self.handle.as_ref() else { return };
        let events = handle.drain_events();
        #[cfg(test)]
        self.seen_events.borrow_mut().extend(events);
        #[cfg(not(test))]
        drop(events);
    }

    /// Send a command that answers on a oneshot. The answer is ready when
    /// this returns, because [`Self::submit`] drains on this thread.
    fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> PageCmd) -> Option<T> {
        let (tx, mut rx) = oneshot::channel();
        self.submit(make(tx));
        rx.try_recv().ok()
    }

    /// One turn of the live loop for the page (call this in the event
    /// loop): run the timers that came due since the last turn, answer the
    /// page's network requests, and lay out what they changed.
    ///
    /// Returns how long the event loop may sleep before the next turn;
    /// `None` when the page has no timer set and only input needs to wake it.
    ///
    /// Until 2026-10-03 this was an empty function: after the load nothing
    /// ran a page's timers or delivered a fetch, so late content never
    /// showed and a `setTimeout` set by a click never fired (Z lane I0).
    pub fn process_events(&self) -> Option<Duration> {
        if !self.is_open() {
            return None;
        }
        if self.content.is_some() {
            // The content thread pumps itself. This thread only reads what
            // the page has already published.
            self.harvest();
            return None;
        }
        self.live_runtime.as_ref()?;
        let started = Instant::now();
        // Too soon after the last turn: the event loop asks for one before
        // and one after the input of each wake, and until #727's second
        // hold both ran, back to back, with no window turn between them.
        if let Some(at) = self.next_turn_at.get().filter(|at| *at > started) {
            return Some(at - started);
        }
        // Whole milliseconds only; the remainder stays on the clock for the
        // next turn, so frequent turns do not slow the page's time.
        let elapsed_ms = started.duration_since(self.live_clock.get()).as_millis() as u64;
        self.live_clock
            .set(self.live_clock.get() + Duration::from_millis(elapsed_ms));

        let turn = self
            .ask(|reply| PageCmd::PumpLive { elapsed_ms, reply })
            .unwrap_or_default();
        if turn.timers_ran > 0 || turn.requests > 0 {
            debug!(?turn, "Live turn");
        }
        let took = started.elapsed();
        self.next_turn_at.set(Some(Instant::now() + took));
        // A page that keeps the loop busy gets at most half of it: the next
        // turn waits at least as long as this one took.
        let timer = turn.next_timer_ms.map(Duration::from_millis);
        let next = if turn.in_flight > 0 {
            Some(timer.map_or(LIVE_REQUEST_POLL, |t| t.min(LIVE_REQUEST_POLL)))
        } else {
            timer
        };
        next.map(|wait| wait.max(started.elapsed()).max(MIN_LIVE_TURN))
    }

    /// Render the view if its frame would differ from the one on screen
    /// (call this in the event loop).
    ///
    /// The loop wakes for every mouse move and every message from the
    /// browser's own UI. Until 2026-10-08 each wake executed the page's
    /// whole display list (2.2 million commands on the Wikipedia portal),
    /// which held a core long after a page had loaded. A frame is still
    /// drawn again once it is a second old. `HIWAVE_RENDER_EVERY_WAKE=1`
    /// is the old behaviour, for telling a stale frame from a wrong one.
    pub fn render(&self) {
        rustkit_viewhost::clip_raw_view(self.host_view);
        if self.content.is_some() {
            self.harvest();
            self.present_latest();
            return;
        }
        // The live turn lays out a resized view; without a live runtime
        // there is no turn, so the frame does it (`flush_pending_resize`
        // inside [`PageCmd::Render`]).
        self.submit(PageCmd::Render { every: render_every_wake() });
    }

    /// Go back in this view's navigation history.
    ///
    /// Uses the view-local history Vec (every navigate() and load records
    /// into it, including engine link clicks routed through
    /// UserEvent::Navigate). NOT the SessionHistory-canonical shape from
    /// the fleet pin — the full #81-style port is the named follow-up; this
    /// makes the button work tonight without inventing a third stack.
    pub fn nav_back(&self) -> bool {
        let target = {
            let mut index = self.history_index.borrow_mut();
            if *index == 0 {
                return false;
            }
            *index -= 1;
            self.history.borrow()[*index].clone()
        };
        *self.current_url.borrow_mut() = Some(target.to_string());
        self.load_url_blocking(target.as_str());
        true
    }

    /// Go forward in this view's navigation history.
    pub fn nav_forward(&self) -> bool {
        let target = {
            let mut index = self.history_index.borrow_mut();
            let len = self.history.borrow().len();
            if *index + 1 >= len {
                return false;
            }
            *index += 1;
            self.history.borrow()[*index].clone()
        };
        *self.current_url.borrow_mut() = Some(target.to_string());
        self.load_url_blocking(target.as_str());
        true
    }

    /// Reload the current page.
    pub fn nav_reload(&self) -> bool {
        let url = self.current_url.borrow().clone();
        match url {
            Some(u) => {
                self.load_url_blocking(&u);
                true
            }
            None => false,
        }
    }

    /// An input event for this view arrived at the queue at `at` with
    /// sequence number `seq` (W-gate: input-to-present latency). Called
    /// before the event is delivered.
    pub fn note_input(&self, at: Instant, seq: u64) {
        self.submit(PageCmd::NoteInput { at, seq });
    }

    /// Route OS keyboard delivery to the content view (first responder).
    pub fn grab_keyboard(&self) {
        if let Some(surface) = &self.surface {
            surface.borrow().grab_keyboard();
            return;
        }
        self.submit(PageCmd::GrabKeyboard);
    }

    /// Deliver a key to the focused form control, if any.
    ///
    /// Returns true when the control consumed it (value or caret changed),
    /// which tells the caller NOT to fall back to scrolling.
    pub fn handle_text_key(
        &self,
        key_code: u32,
        key: &str,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) -> bool {
        if self.content.is_some() {
            // The page scrolls an unhandled key itself. Returning before
            // the reply arrives tells the window not to scroll as well.
            let (reply, _rx) = oneshot::channel();
            self.submit(PageCmd::Input(PageInput::TextKey {
                key_code,
                key: key.to_string(),
                ctrl,
                shift,
                alt,
                reply,
            }));
            return true;
        }
        self.ask(|reply| {
            PageCmd::Input(PageInput::TextKey {
                key_code,
                key: key.to_string(),
                ctrl,
                shift,
                alt,
                reply,
            })
        })
        .unwrap_or(false)
    }

    /// A key was released: the page hears `keyup`.
    pub fn handle_key_up(&self, key_code: u32, key: &str, ctrl: bool, shift: bool, alt: bool) {
        self.submit(PageCmd::Input(PageInput::KeyUp {
            key_code,
            key: key.to_string(),
            ctrl,
            shift,
            alt,
        }));
    }

    /// Enter in the focused field: the page hears `keydown` and `submit`,
    /// and this is the URL to navigate to when its form submits. `None`
    /// when there is no form, a listener cancelled, or the form is not a
    /// GET.
    pub fn form_submit_url(&self) -> Option<String> {
        if self.content.is_some() {
            // Enter is a key. The page submits the focused form and emits
            // NavigateRequested; this does not wait for that URL.
            return None;
        }
        self.ask(|reply| PageCmd::Input(PageInput::FormSubmit { reply }))
            .flatten()
    }

    /// Where the page's own script asked to go since the last call
    /// (`location.href = url`, `link.click()`, `form.submit()`, a submit
    /// from a timer). The caller navigates.
    /// A reload, `location.replace()` or a meta refresh replaces the
    /// current history entry when it is loaded, as in Chrome.
    /// The URL, and whether the user activated the navigation (a click or
    /// a form submit) rather than the page's script.
    pub fn take_script_navigation(&self) -> Option<(String, bool)> {
        if self.content.is_some() {
            self.harvest();
            let taken = self.script_nav.borrow_mut().take()?;
            self.replace_next.set(taken.1);
            return Some((taken.0, taken.2));
        }
        let (url, replace) =
            self.ask(|reply| PageCmd::Input(PageInput::TakeNavigation { reply }))??;
        self.replace_next.set(replace);
        Some((url, false))
    }

    /// Whether a content element currently holds focus.
    pub fn has_focused_element(&self) -> bool {
        if self.content.is_some() {
            // The reply would come too late for this key. Answer from the
            // page's last report, and ask again so focus that a script moved
            // is known by the next key.
            let (reply, _rx) = oneshot::channel();
            self.submit(PageCmd::Input(PageInput::HasFocus { reply }));
            self.harvest();
            return self.page_focused.get();
        }
        self.ask(|reply| PageCmd::Input(PageInput::HasFocus { reply }))
            .unwrap_or(false)
    }

    /// Rebuild layout and repaint after an edit changed a control's value.
    pub fn relayout(&self) {
        self.submit(PageCmd::Relayout);
    }

    /// Focus whatever focusable element sits at these viewport coordinates,
    /// clearing focus when nothing focusable is there. Returns the focused
    /// element's tag name.
    pub fn focus_at_point(&self, x: f32, y: f32) -> Option<String> {
        if self.content.is_some() {
            // The page reports what it focused as PageEvent::Focused, which
            // has_focused_element reads. The tag is only logged.
            let (reply, _rx) = oneshot::channel();
            self.submit(PageCmd::Input(PageInput::FocusAt { x, y, reply }));
            return None;
        }
        self.ask(|reply| PageCmd::Input(PageInput::FocusAt { x, y, reply }))
            .flatten()
    }

    /// Deliver a mouse press at viewport coordinates to the page
    /// (`mousedown`).
    pub fn mouse_down_at_point(&self, x: f32, y: f32) {
        self.submit(PageCmd::Input(PageInput::MouseDown { x, y }));
    }

    /// Deliver a pointer move to viewport coordinates to the page (the
    /// over/out/enter/leave events when the element under it changed, then
    /// `pointermove` and `mousemove`).
    pub fn mouse_move_at_point(&self, x: f32, y: f32) {
        self.submit(PageCmd::Input(PageInput::MouseMove { x, y }));
    }

    /// Tell the page the pointer left the view.
    pub fn mouse_leave(&self) {
        self.submit(PageCmd::Input(PageInput::MouseLeave));
    }

    /// Deliver a mouse release at viewport coordinates to the page
    /// (`mouseup`, `click`), then report the click's default actions: what
    /// it focused and the link to follow unless a listener cancelled it.
    pub fn click_at_point(&self, x: f32, y: f32) -> rustkit_engine::ClickOutcome {
        if self.content.is_some() {
            // The link, if the page allows it, arrives as
            // NavigateRequested and is taken by `take_script_navigation`.
            // The outcome's navigate is left empty so the caller does not
            // load the same URL twice.
            let (reply, _rx) = oneshot::channel();
            self.submit(PageCmd::Input(PageInput::Click { x, y, reply }));
            return rustkit_engine::ClickOutcome::default();
        }
        self.ask(|reply| PageCmd::Input(PageInput::Click { x, y, reply }))
            .unwrap_or_default()
    }

    /// Resolve a click at viewport coordinates to a link URL, if any.
    pub fn link_at_point(&self, x: f32, y: f32) -> Option<String> {
        self.ask(|reply| PageCmd::Input(PageInput::LinkAt { x, y, reply }))
            .flatten()
    }

    /// Scroll the view by a wheel delta in physical pixels.
    ///
    /// Returns true if the scroll offset changed (a re-render is needed).
    /// Wheel events that reach the tao window loop are exactly the ones the
    /// UI-frame WebView did not consume, i.e. the pointer was over content.
    pub fn scroll_by(&self, delta_x: f32, delta_y: f32) -> bool {
        if self.content.is_some() {
            // The page scrolls on its own thread and publishes the moved
            // frame as a commit. Report a change so the caller presents it.
            let (reply, _rx) = oneshot::channel();
            self.submit(PageCmd::Input(PageInput::Scroll {
                dx: delta_x,
                dy: delta_y,
                reply,
            }));
            return true;
        }
        self.ask(|reply| {
            PageCmd::Input(PageInput::Scroll {
                dx: delta_x,
                dy: delta_y,
                reply,
            })
        })
        .unwrap_or(false)
    }

    /// Load HTML content directly.
    pub fn load_html_internal(&self, html: &str) -> HiWaveResult<()> {
        self.cancel_load();
        if let Some(thread) = &self.content {
            let _rx = thread.load_html(html);
            // Events the old page sent before this (a script navigation just
            // before New Tab) are stale from here, as after `navigate`.
            self.accepted_nav.set(thread.last_nav_id());
            // The new document has nothing focused until it says otherwise.
            self.page_focused.set(false);
            self.live_clock.set(Instant::now());
            return Ok(());
        }
        if self.view_id.is_some() {
            match self.ask(|reply| PageCmd::LoadHtml {
                html: html.to_string(),
                nav_id: 0,
                reply,
            }) {
                Some(Ok(())) => {}
                Some(Err(e)) => return Err(hiwave_core::HiWaveError::WebView(e)),
                None => {
                    return Err(hiwave_core::HiWaveError::WebView(
                        "page did not answer LoadHtml".to_string(),
                    ))
                }
            }
        }
        self.live_clock.set(Instant::now());
        Ok(())
    }

    /// Set the bounds of the view.
    ///
    /// The view and its surface take the size now; the layout and the
    /// page's `resize` event come once, on this turn's live step, however
    /// many sizes a drag delivered since the last one (H9: every size laid
    /// the page out in full before the next was read).
    pub fn set_bounds_internal(&self, bounds: Bounds) -> HiWaveResult<()> {
        if !self.is_open() {
            return Ok(());
        }
        let scale = *self.zoom_level.borrow();
        if let Some(thread) = &self.content {
            if let Some(surface) = &self.surface {
                surface
                    .borrow_mut()
                    .resize(bounds)
                    .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
            }
            thread.resize(bounds, scale);
            // The page lays out when it next yields. Until then the last
            // commit is drawn at the new size: top-left, canvas colour in
            // the area the old layout does not cover.
            self.present_latest();
            return Ok(());
        }
        if self.is_loading() {
            // A load holds the engine: the size waits for its end.
            self.pending_bounds.set(Some(bounds));
            return Ok(());
        }
        match self.ask(|reply| PageCmd::Resize {
            bounds,
            scale,
            reply,
        }) {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(hiwave_core::HiWaveError::WebView(e)),
            None => Err(hiwave_core::HiWaveError::WebView(
                "page did not answer Resize".to_string(),
            )),
        }
    }

    /// Get the current title.
    pub fn title(&self) -> Option<String> {
        self.current_title.borrow().clone()
    }

    /// Load HTML content (WRY compatibility method).
    pub fn wry_load_html(&self, html: &str) -> Result<(), String> {
        self.load_html_internal(html)
            .map_err(|e| format!("Failed to load HTML: {}", e))
    }

    /// Load URL (WRY compatibility method).
    pub fn wry_load_url(&self, url: &str) -> Result<(), String> {
        let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

        // Update cached URL
        *self.current_url.borrow_mut() = Some(url.to_string());

        // Update navigation history
        let mut history = self.history.borrow_mut();
        let mut index = self.history_index.borrow_mut();

        if self.replace_next.take() && *index < history.len() {
            // A replace navigation from the page's script: same entry.
            history[*index] = parsed;
        } else {
            // If we're in the middle of history (user went back), truncate forward history
            if *index < history.len() {
                history.truncate(*index + 1);
            }

            // Add new URL to history
            history.push(parsed);
            *index = history.len() - 1;
        }

        drop(history);
        drop(index);

        self.load_url_blocking(url);
        Ok(())
    }

    /// Evaluate script (WRY compatibility method).
    pub fn wry_evaluate_script(&self, script: &str) -> Result<(), String> {
        self.execute_script_sync(script);
        Ok(())
    }

    /// Set bounds (WRY compatibility method).
    pub fn wry_set_bounds(&self, rect: wry::Rect) -> Result<(), String> {
        use rustkit_viewhost::Bounds;
        let bounds = Bounds::new(
            rect.position.to_logical::<f64>(1.0).x as i32,
            rect.position.to_logical::<f64>(1.0).y as i32,
            rect.size.to_logical::<f64>(1.0).width as u32,
            rect.size.to_logical::<f64>(1.0).height as u32,
        );
        self.set_bounds_internal(bounds)
            .map_err(|e| format!("Failed to set bounds: {}", e))
    }

    /// Get the view ID.
    pub fn view_id(&self) -> Option<EngineViewId> {
        self.view_id
    }

    /// Start loading a URL. On the content thread the command is queued and
    /// this returns at once. Flag off, the event loop drives the load
    /// ([`Self::step_load`]); a load already in progress is cancelled first.
    /// Without a live runtime the load runs to its end here, as before.
    fn load_url_blocking(&self, url: &str) {
        let parsed = match Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                error!(error = %e, url = url, "Invalid URL");
                return;
            }
        };
        self.cancel_load();

        if let Some(thread) = &self.content {
            // Signal cancel and the interrupt before the command is queued.
            let nav_id = thread.navigate(parsed);
            self.accepted_nav.set(nav_id);
            self.page_focused.set(false);
            *self.loading.borrow_mut() = true;
            self.live_clock.set(Instant::now());
            return;
        }
        let Some(endpoint) = self.endpoint.as_ref().filter(|_| self.is_open()) else { return };

        if self.live_runtime.is_none() {
            // Returns after DOMContentLoaded when an inserted script is still
            // arriving. Live turns (`process_events`) run the chain one script
            // at a time and send PageLoaded at the end. A page with no inserted
            // script still finishes inside this call. `nav_id` 0 lets the engine
            // allocate the id its cancel path reads.
            self.submit(PageCmd::Navigate {
                url: parsed,
                nav_id: 0,
            });
            // The new page's clock starts now: the time the load took is not
            // time its timers have waited.
            self.live_clock.set(Instant::now());
            return;
        }

        let canceller = endpoint
            .try_borrow()
            .ok()
            .and_then(|endpoint| endpoint.nav_canceller());
        let endpoint = Rc::clone(endpoint);
        let future = Box::pin(async move {
            let Ok(mut endpoint) = endpoint.try_borrow_mut() else {
                warn!("Load not started: the engine is busy");
                return;
            };
            // `nav_id` 0 lets the engine allocate the id its cancel path
            // reads.
            endpoint.navigate(parsed, 0).await;
        });
        *self.pending_load.borrow_mut() = Some(PendingLoad {
            future,
            canceller,
            started: Instant::now(),
        });
        // The first slice runs now: a page that answers at once loads in
        // the same wake, as it did when the load blocked.
        self.step_load();
    }

    /// Whether a load is in progress on this thread (flag off).
    pub fn is_loading(&self) -> bool {
        self.pending_load.borrow().is_some()
    }

    /// Poll the load in progress for at most [`LOAD_SLICE`] (call this in
    /// the event loop). Returns true while the load goes on: the caller
    /// returns to tao and wakes again soon. Always false on the content
    /// thread, which loads on its own.
    pub fn step_load(&self) -> bool {
        let Some(runtime) = self.live_runtime.as_ref() else { return false };
        let done = {
            let mut slot = self.pending_load.borrow_mut();
            let Some(load) = slot.as_mut() else { return false };
            runtime
                .block_on(async { tokio::time::timeout(LOAD_SLICE, load.future.as_mut()).await })
                .is_ok()
        };
        if !done {
            return true;
        }
        if let Some(load) = self.pending_load.borrow_mut().take() {
            debug!(ms = load.started.elapsed().as_millis() as u64, "Load finished");
        }
        self.load_ended();
        false
    }

    /// Stop the load in progress (the user's Stop). Returns whether there
    /// was one.
    ///
    /// On the content thread the interrupt is stored before this returns,
    /// including while the page is not reading commands.
    ///
    /// Flag off, a load that yielded after `DOMContentLoaded` has no future
    /// here any more, but the engine still finishes it from the live turns:
    /// that one is stopped in the engine, and its `load` never fires (#727
    /// R1 H2).
    pub fn stop_load(&self) -> bool {
        if let Some(thread) = &self.content {
            thread.stop();
            return self.loading.replace(false);
        }
        if self.is_loading() {
            self.cancel_load();
            return true;
        }
        if !self.load_tail_open() {
            return false;
        }
        let _ = self.ask(|reply| PageCmd::Stop { reply });
        true
    }

    /// Whether a flag-off load yielded after `DOMContentLoaded` and its
    /// tail is still open.
    fn load_tail_open(&self) -> bool {
        self.endpoint
            .as_ref()
            .and_then(|endpoint| endpoint.try_borrow().ok().map(|e| e.load_tail_open()))
            .unwrap_or(false)
    }

    /// Cancel the load in progress, if any, and let it unwind: a cancelled
    /// load ends at its next wait. One that has not ended within
    /// [`CANCEL_UNWIND`] is dropped where it stands.
    fn cancel_load(&self) {
        let Some(mut load) = self.pending_load.borrow_mut().take() else { return };
        if let Some(canceller) = load.canceller.as_ref() {
            canceller.cancel_current();
        }
        if let Some(runtime) = self.live_runtime.as_ref() {
            if runtime
                .block_on(async { tokio::time::timeout(CANCEL_UNWIND, load.future.as_mut()).await })
                .is_err()
            {
                warn!("A cancelled load did not end; dropped");
            }
        }
        drop(load);
        self.load_ended();
    }

    /// After a load, finished or cancelled: the size set meanwhile, and a
    /// page clock that starts now (the time the load took is not time its
    /// timers have waited).
    fn load_ended(&self) {
        if let Some(bounds) = self.pending_bounds.take() {
            if let Err(e) = self.set_bounds_internal(bounds) {
                debug!(error = %e, "bounds after the load failed");
            }
        }
        self.drop_events();
        self.live_clock.set(Instant::now());
        if let Some(on) = self.pending_attach.take() {
            let _ = self.devtools_ask(rustkit_engine::DevtoolsQuery::Attach(on));
        }
    }

    /// Execute JavaScript synchronously on the flag-off path. The content
    /// thread queues the script and returns `None` without waiting.
    pub fn execute_script_sync(&self, script: &str) -> Option<String> {
        if self.content.is_some() {
            let _rx = self.content.as_ref().unwrap().eval(script);
            return None;
        }
        match self.ask(|reply| PageCmd::Eval {
            script: script.to_string(),
            reply,
        }) {
            Some(Ok(result)) => Some(result),
            Some(Err(e)) => {
                debug!(error = %e, "Script execution failed");
                None
            }
            None => None,
        }
    }

    // The developer tools (`devtools_panel`): on-demand, bounded questions
    // about the page's last committed DOM and layout, answered on the
    // page's own thread, and console reads, answered by the devtools thread
    // from the view's Inspector without the page (T6a). Nothing here waits
    // for the answer.

    /// Ask a developer-tools question. The answer arrives on the receiver:
    /// from the devtools thread for a console read; otherwise before this
    /// returns on the same-thread path, after the page's current turn on
    /// the content thread. A closed receiver means no answer (a closed
    /// view, or a load holding the same-thread page).
    pub fn devtools_ask(&self, query: rustkit_engine::DevtoolsQuery) -> oneshot::Receiver<rustkit_engine::DevtoolsAnswer> {
        if let Some(thread) = self.content.as_ref().filter(|_| self.is_open()) {
            return thread.devtools_ask(query);
        }
        let page = self
            .endpoint
            .as_ref()
            .filter(|_| self.is_open())
            .and_then(|endpoint| endpoint.try_borrow().ok().and_then(|e| Some((e.inspector()?, e.devtools_counters()))));
        if let (rustkit_engine::DevtoolsQuery::Console { since }, Some((inspector, counters))) = (&query, &page) {
            // The answer arrives on the devtools thread, after this returns:
            // the window is woken for it as on the content thread (#803
            // review (1)), or an idle page's line waits for an unrelated wake.
            return rustkit_engine::devtools_ask_console(Arc::clone(inspector), *since, Some(Arc::clone(counters)));
        }
        // An open panel is woken when the view records (T6b). A load holding
        // the endpoint drops the command, so it is asked again at the end.
        if let rustkit_engine::DevtoolsQuery::Attach(on) = query {
            match &page {
                Some((inspector, counters)) => rustkit_engine::devtools_watch(inspector, counters, on),
                None if self.endpoint.is_some() && self.is_open() => self.pending_attach.set(Some(on)),
                None => {}
            }
        }
        let (reply, rx) = oneshot::channel();
        self.submit(PageCmd::Devtools { query, reply });
        rx
    }

    /// The view's Inspector, for reads that need not ask the page. Only
    /// its atomic [`watermark`](rustkit_engine::Inspector::watermark) is
    /// read on the window thread: sinks run under its lock.
    pub fn devtools_inspector(&self) -> Option<std::sync::Arc<rustkit_engine::Inspector>> {
        if !self.is_open() {
            return None;
        }
        if let Some(thread) = self.content.as_ref() {
            return thread.devtools_inspector();
        }
        self.endpoint
            .as_ref()
            .and_then(|endpoint| endpoint.try_borrow().ok().and_then(|e| e.inspector()))
    }

    /// Run the console's `code` in the page: the value, or the error. A
    /// page that reaches it at or past `deadline` cancels it (T6c).
    pub fn devtools_eval(&self, code: &str, deadline: std::time::Instant) -> oneshot::Receiver<Result<String, String>> {
        let (reply, rx) = oneshot::channel();
        self.submit(PageCmd::DevtoolsEval { script: code.to_string(), deadline, reply });
        rx
    }
}

impl Drop for RustKitView {
    fn drop(&mut self) {
        self.cancel_load();
        if self.closed.replace(true) {
            return;
        }
        if let Some(thread) = &self.content {
            thread.shutdown();
            self.view_id = None;
            return;
        }
        if self.view_id.is_some() {
            // `submit` refuses a closed view. Close still has to be applied
            // on this thread, and it must not wait on a content thread.
            if let Some(handle) = &self.handle {
                handle.send(PageCmd::Close);
                if let Some(endpoint) = &self.endpoint {
                    endpoint.borrow_mut().drain(self.live_runtime.as_ref());
                }
            }
            self.view_id = None;
        }
    }
}

impl RustKitView {
    fn harvest(&self) {
        let Some(thread) = &self.content else {
            return;
        };
        let accepted = self.accepted_nav.get();
        for event in thread.poll() {
            if stale(accepted, &event) {
                continue;
            }
            match event {
                PageEvent::Commit(commit) => {
                    *self.latest_commit.borrow_mut() = Some(commit);
                }
                PageEvent::Title { text, .. } => {
                    *self.current_title.borrow_mut() = Some(text);
                }
                PageEvent::Url { url, .. } => {
                    *self.current_url.borrow_mut() = Some(url.to_string());
                }
                PageEvent::LoadState { state, .. } => {
                    *self.loading.borrow_mut() = matches!(state, LoadState::Started);
                }
                PageEvent::NavigateRequested {
                    url,
                    replace,
                    from_user,
                    ..
                } => {
                    *self.script_nav.borrow_mut() = Some((url, replace, from_user));
                }
                PageEvent::Focused(focused) => self.page_focused.set(focused),
                PageEvent::Cursor(_) | PageEvent::Unresponsive | PageEvent::Closed => {}
            }
        }
    }

    fn present_latest(&self) {
        let Some(commit) = self.latest_commit.borrow().clone() else {
            return;
        };
        let Some(thread) = &self.content else {
            return;
        };
        let Some(images) = thread.images() else {
            return;
        };
        let Some(surface) = &self.surface else {
            return;
        };
        if let Err(error) = surface.borrow_mut().present_commit(&commit, &images) {
            warn!(%error, "content frame was not presented");
        }
    }
}

fn stale(accepted: u64, event: &PageEvent) -> bool {
    let nav_id = match event {
        PageEvent::Title { nav_id, .. }
        | PageEvent::Url { nav_id, .. }
        | PageEvent::LoadState { nav_id, .. }
        | PageEvent::NavigateRequested { nav_id, .. } => *nav_id,
        PageEvent::Commit(commit) => commit.nav_generation,
        PageEvent::Focused(_)
        | PageEvent::Cursor(_)
        | PageEvent::Unresponsive
        | PageEvent::Closed => 0,
    };
    nav_id != 0 && nav_id < accepted
}

// ============================================================================
// IWebContent Implementation
// ============================================================================

impl IWebContent for RustKitView {
    fn navigate(&mut self, url: &Url) -> HiWaveResult<()> {
        // Update cached URL
        *self.current_url.borrow_mut() = Some(url.to_string());

        // Update navigation history
        let mut history = self.history.borrow_mut();
        let mut index = self.history_index.borrow_mut();

        // If we're in the middle of history (user went back), truncate forward history
        if *index < history.len() {
            history.truncate(*index + 1);
        }

        // Add new URL to history
        history.push(url.clone());
        *index = history.len() - 1;

        drop(history);
        drop(index);

        self.load_url_blocking(url.as_str());
        Ok(())
    }

    fn execute_script(&self, script: &str) -> HiWaveResult<String> {
        self.execute_script_sync(script)
            .ok_or_else(|| hiwave_core::HiWaveError::WebView("Script execution failed".to_string()))
    }

    fn get_url(&self) -> Option<Url> {
        // Return cached URL
        if let Some(url) = self.current_url.borrow().clone() {
            return Url::parse(&url).ok();
        }
        None
    }

    fn can_go_back(&self) -> bool {
        *self.history_index.borrow() > 0
    }

    fn can_go_forward(&self) -> bool {
        let index = *self.history_index.borrow();
        let history_len = self.history.borrow().len();
        index < history_len.saturating_sub(1)
    }

    fn go_back(&mut self) -> HiWaveResult<()> {
        if !self.can_go_back() {
            return Err(hiwave_core::HiWaveError::WebView("Cannot go back".to_string()));
        }

        let mut index = self.history_index.borrow_mut();
        *index -= 1;
        let url = self.history.borrow()[*index].clone();
        drop(index);

        self.load_url_blocking(url.as_str());
        *self.current_url.borrow_mut() = Some(url.to_string());
        Ok(())
    }

    fn go_forward(&mut self) -> HiWaveResult<()> {
        if !self.can_go_forward() {
            return Err(hiwave_core::HiWaveError::WebView("Cannot go forward".to_string()));
        }

        let mut index = self.history_index.borrow_mut();
        *index += 1;
        let url = self.history.borrow()[*index].clone();
        drop(index);

        self.load_url_blocking(url.as_str());
        *self.current_url.borrow_mut() = Some(url.to_string());
        Ok(())
    }

    fn reload(&mut self) -> HiWaveResult<()> {
        if let Some(url) = self.current_url.borrow().clone() {
            self.load_url_blocking(&url);
            Ok(())
        } else {
            Err(hiwave_core::HiWaveError::WebView("No URL to reload".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_views_get_the_live_script_budget() {
        let builder = content_engine_builder();
        assert_eq!(builder.config().script_budget_ms, 15_000);
        // Only the script budget is raised: the virtual timer clock and
        // the engine's own default (parity-capture, the board) stay at 5 s.
        assert_eq!(builder.config().timer_horizon_ms, 5_000);
        assert_eq!(rustkit_engine::EngineConfig::default().script_budget_ms, 5_000);
    }

    #[test]
    fn content_views_get_a_budget_for_script_after_the_load() {
        let config = content_engine_builder().config().clone();
        assert!(config.interrupt_scripts_at_budget);
        assert_eq!(config.live_turn_budget_ms, 5_000);
    }

    #[test]
    fn content_views_follow_a_page_s_own_navigation() {
        assert!(content_engine_builder().config().follows_script_navigation);
        assert!(!rustkit_engine::EngineConfig::default().follows_script_navigation);
    }

    #[test]
    fn content_views_leave_requests_after_load_to_the_live_loop() {
        assert!(content_engine_builder().config().post_load_requests_live);
        assert!(!rustkit_engine::EngineConfig::default().post_load_requests_live);
    }

    /// A server that answers every request with `body` after `delay`.
    fn slow_server(delay: Duration, body: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                std::thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 2048];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buf[..n]),
                        }
                    }
                    std::thread::sleep(delay);
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                });
            }
        });
        format!("http://127.0.0.1:{port}/")
    }

    fn view() -> RustKitView {
        RustKitView::headless(Bounds::new(0, 0, 400, 300))
    }

    /// #739 / Pete's item 4: the window took no click while a page loaded,
    /// because the load ran to its end inside the event handler.
    #[test]
    fn a_slow_load_gives_the_event_loop_back_at_once() {
        let url = slow_server(Duration::from_millis(2_000), "<title>slow</title><p>late</p>");
        let view = view();
        let started = Instant::now();
        view.wry_load_url(&url).unwrap();
        let held = started.elapsed();
        assert!(held < Duration::from_millis(500), "the load held its caller for {held:?}");
    }

    #[test]
    fn a_load_driven_a_slice_at_a_time_finishes() {
        let url = slow_server(Duration::from_millis(300), "<title>slow</title><p>late</p>");
        let view = view();
        view.wry_load_url(&url).unwrap();
        assert!(view.is_loading());
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut slices = 0;
        while view.step_load() {
            slices += 1;
            assert!(Instant::now() < deadline, "the load never finished");
        }
        assert!(slices > 1, "the 300 ms wait was not split into slices");
        let title = view.execute_script_sync("document.title").unwrap_or_default();
        assert!(title.contains("slow"), "document.title = {title:?}");
    }

    #[test]
    fn engine_calls_during_a_load_do_nothing_and_do_not_panic() {
        let url = slow_server(Duration::from_millis(1_000), "<p>x</p>");
        let view = view();
        view.wry_load_url(&url).unwrap();
        assert!(view.is_loading());
        assert!(!view.scroll_by(0.0, -40.0));
        view.render();
        assert_eq!(view.process_events(), None);
        let _ = view.click_at_point(10.0, 10.0);
        assert_eq!(view.execute_script_sync("1"), None);
        // A size set meanwhile waits for the load's end.
        view.set_bounds_internal(Bounds::new(0, 0, 500, 300)).unwrap();
        assert!(view.pending_bounds.get().is_some());
        while view.step_load() {}
        assert!(view.pending_bounds.get().is_none());
    }

    #[test]
    fn a_panel_attached_during_a_load_is_watched_once_it_ends() {
        // #803 review (2): the load holds the endpoint, so the attach could
        // neither watch the view nor reach the page. An idle page then never
        // woke the panel for its console.
        let url = slow_server(Duration::from_millis(600), "<p>x</p>");
        let view = view();
        view.wry_load_url(&url).unwrap();
        assert!(view.is_loading());
        let _ = view.devtools_ask(rustkit_engine::DevtoolsQuery::Attach(true));
        while view.step_load() {}
        let counters = view.endpoint.as_ref().unwrap().borrow().devtools_counters();
        let wakes = |budget: Duration| {
            let deadline = Instant::now() + budget;
            let from = counters.read().window_wakes;
            while counters.read().window_wakes == from && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            counters.read().window_wakes - from
        };
        // What the page recorded before: the panel reads it.
        let _ = wakes(Duration::from_millis(500));
        let _ = view.devtools_ask(rustkit_engine::DevtoolsQuery::Console { since: 0 });
        std::thread::sleep(Duration::from_millis(200));
        let _ = view.execute_script_sync("console.log('after the load'); 1");
        assert!(wakes(Duration::from_secs(3)) > 0, "a console line after the load never woke the panel");
    }

    #[test]
    fn a_console_read_on_the_same_thread_path_wakes_the_window() {
        // #803 review (1): the answer arrives on the devtools thread after
        // the ask returns; with no wake an idle page's line waited for an
        // unrelated one.
        let view = view();
        view.load_html_internal("<p>x</p>").unwrap();
        let _ = view.devtools_ask(rustkit_engine::DevtoolsQuery::Attach(true));
        let _ = view.execute_script_sync("console.log('one'); 1");
        let counters = view.endpoint.as_ref().unwrap().borrow().devtools_counters();
        // The watch's own wake for the unread line comes first.
        std::thread::sleep(Duration::from_millis(300));
        let from = counters.read().window_wakes;
        let mut rx = view.devtools_ask(rustkit_engine::DevtoolsQuery::Console { since: 0 });
        let deadline = Instant::now() + Duration::from_secs(3);
        let answer = loop {
            match rx.try_recv() {
                Ok(answer) => break answer,
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => panic!("no console answer: {e:?}"),
            }
        };
        let rustkit_engine::DevtoolsAnswer::Console(console) = answer else { panic!("{answer:?}") };
        assert!(!console.entries.is_empty(), "the line is missing");
        // The wake is posted just after the answer is sent.
        let deadline = Instant::now() + Duration::from_secs(1);
        while counters.read().window_wakes == from && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(counters.read().window_wakes - from, 1, "the answer posted no window wake");
    }

    #[test]
    fn stop_cancels_a_slow_load_without_waiting_for_it() {
        let url = slow_server(Duration::from_secs(10), "<p>never</p>");
        let view = view();
        view.wry_load_url(&url).unwrap();
        assert!(view.is_loading());
        let started = Instant::now();
        assert!(view.stop_load());
        assert!(!view.is_loading());
        let took = started.elapsed();
        assert!(took < Duration::from_secs(1), "stop took {took:?}");
        assert!(!view.stop_load(), "nothing left to stop");
    }

    /// A page whose inserted `/a.js` is answered after `delay`; the page
    /// itself at once. `load` marks `window.loadFired`.
    fn yielding_server(delay: Duration) -> String {
        use std::io::{Read, Write};
        const PAGE: &str = "<script>window.addEventListener('load', function () { window.loadFired = true; });\
var s = document.createElement('script'); s.src = '/a.js'; document.head.appendChild(s);</script>";
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                std::thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 2048];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buf[..n]),
                        }
                    }
                    let (kind, body) = if request.starts_with(b"GET /a.js") {
                        std::thread::sleep(delay);
                        ("text/javascript", "window.aRan = true;")
                    } else {
                        ("text/html", PAGE)
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        format!("http://127.0.0.1:{port}/")
    }

    /// #727 R1 H2: a load that yielded after `DOMContentLoaded` is finished
    /// by the live turns. Stop used to find no `pending_load`, log "nothing
    /// loading" and let the tail run its script, fire `load` and send
    /// `PageLoaded`.
    #[test]
    fn stop_ends_a_load_that_yielded_while_an_inserted_script_fetches() {
        let url = yielding_server(Duration::from_millis(1_500));
        let view = view();
        view.wry_load_url(&url).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while view.step_load() {
            assert!(Instant::now() < deadline, "the load never yielded");
        }
        assert!(!view.is_loading());
        assert!(view.load_tail_open(), "the load did not yield: it waited out /a.js");
        assert!(view.stop_load(), "Stop found nothing to stop");
        assert!(!view.load_tail_open());
        // Past the time /a.js is answered.
        let until = Instant::now() + Duration::from_millis(2_500);
        while Instant::now() < until {
            view.process_events();
            std::thread::sleep(Duration::from_millis(30));
        }
        let fired = view
            .execute_script_sync("String(window.loadFired) + ',' + String(window.aRan)")
            .unwrap_or_default();
        assert!(!fired.contains("true"), "the stopped load went on: {fired}");
        view.drop_events();
        let events = view.seen_events.borrow();
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, PageEvent::LoadState { state: LoadState::Complete, .. })),
            "a stopped load sent PageLoaded"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                PageEvent::LoadState { state: LoadState::Failed { error }, .. } if error == "stopped"
            )),
            "the navigation was not ended as stopped"
        );
        drop(events);
        assert!(!view.stop_load(), "nothing left to stop");
    }

    /// #727 Atlas hold, item 2: a live turn is followed by a window turn.
    /// The event loop asks for a turn twice per wake (before and after the
    /// input it drains), so a page whose interval spins ran two turns back
    /// to back and the window took no event between them.
    #[test]
    fn a_slow_live_turn_is_not_followed_at_once_by_another() {
        let url = slow_server(Duration::ZERO, "<p>x</p>");
        let view = view();
        view.wry_load_url(&url).unwrap();
        while view.step_load() {}
        view.execute_script_sync(
            "window.n = 0; setInterval(function () { window.n++; var t = Date.now(); while (Date.now() - t < 200) {} }, 100);",
        );
        std::thread::sleep(Duration::from_millis(105));
        let first = Instant::now();
        view.process_events();
        assert!(first.elapsed() >= Duration::from_millis(200), "the interval did not run");
        let runs = || view.execute_script_sync("String(window.n)").unwrap_or_default();
        let after_first = runs();
        // The first turn's 200 ms put the interval due again.
        let second = Instant::now();
        let wait = view.process_events();
        let took = second.elapsed();
        let after_second = runs();
        assert_eq!(after_second, after_first, "two turns ran back to back");
        assert!(took < Duration::from_millis(50), "the second call took {took:?}");
        assert!(
            wait.is_some_and(|w| w >= Duration::from_millis(100)),
            "the loop was not told to give the window a turn first: {wait:?}"
        );
    }

    /// #727 Atlas hold, item 3: a click that comes after a live turn the
    /// watchdog stopped (cnn's Agree landed in a 5.03 s turn) reaches its
    /// listener.
    #[test]
    fn a_click_after_a_stopped_live_turn_reaches_its_listener() {
        let url = slow_server(
            Duration::ZERO,
            "<body style='margin:0'><div style='width:300px;height:200px'></div></body>",
        );
        let view = view();
        view.wry_load_url(&url).unwrap();
        while view.step_load() {}
        view.execute_script_sync(
            "window.clicks = 0; document.addEventListener('click', function () { window.clicks++; });\
             setTimeout(function () { while (true) {} }, 1);",
        );
        std::thread::sleep(Duration::from_millis(5));
        let turn = Instant::now();
        view.process_events();
        assert!(turn.elapsed() >= Duration::from_secs(1), "the spin did not run");
        view.mouse_down_at_point(10.0, 10.0);
        let _ = view.click_at_point(10.0, 10.0);
        let clicks = view.execute_script_sync("String(window.clicks)").unwrap_or_default();
        assert!(clicks.contains('1'), "the click was lost: clicks = {clicks}");
    }

    #[test]
    fn a_new_load_replaces_the_one_in_progress() {
        let slow = slow_server(Duration::from_secs(10), "<title>old</title>");
        let fast = slow_server(Duration::ZERO, "<title>new</title>");
        let view = view();
        view.wry_load_url(&slow).unwrap();
        let started = Instant::now();
        view.wry_load_url(&fast).unwrap();
        while view.step_load() {}
        assert!(started.elapsed() < Duration::from_secs(5));
        let title = view.execute_script_sync("document.title").unwrap_or_default();
        assert!(title.contains("new"), "document.title = {title:?}");
    }

    /// Answers every request with `body` at once and logs its path and
    /// when it arrived.
    fn logging_server(body: &'static str) -> (String, Arc<std::sync::Mutex<Vec<(String, Instant)>>>) {
        use std::io::{Read, Write};
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&log);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut buf = [0u8; 2048];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buf) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buf[..n]),
                        }
                    }
                    let path = String::from_utf8_lossy(&request)
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .to_string();
                    seen.lock().unwrap().push((path, Instant::now()));
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                });
            }
        });
        (format!("http://127.0.0.1:{port}/"), log)
    }

    /// The event loop's wake, as `MainEventsCleared` does it: the load's
    /// slice, or the live turn and a frame.
    fn wake(view: &RustKitView) {
        if !view.step_load() {
            view.process_events();
            view.render();
        }
    }

    /// Wake the view until its load has ended (either path).
    fn load_and_settle(view: &RustKitView, url: &str) {
        view.wry_load_url(url).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            wake(view);
            let loading = if view.content.is_some() { *view.loading.borrow() } else { view.is_loading() };
            if !loading && (view.content.is_none() || view.latest_commit.borrow().is_some()) {
                return;
            }
            assert!(Instant::now() < deadline, "the load never ended");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The page's answer to `script`, on either path.
    fn read(view: &RustKitView, script: &str) -> String {
        let Some(thread) = &view.content else {
            return view.execute_script_sync(script).unwrap_or_default();
        };
        let mut rx = thread.eval(script);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(answer) = rx.try_recv() {
                return answer.unwrap_or_default();
            }
            assert!(Instant::now() < deadline, "the page did not answer {script}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Wake the view every 2 ms, as the event loop does, from now until a
    /// page timer has started spinning and the user acts at `at`. Returns
    /// how late after `at` the window was free to take the input.
    fn wake_until(view: &RustKitView, at: Instant) -> Duration {
        while Instant::now() < at {
            wake(view);
            std::thread::sleep(Duration::from_millis(2));
        }
        at.elapsed()
    }

    const SPIN_PAGE: &str = "<body style='margin:0'><div style='width:300px;height:200px'></div>\
<script>window.clicks = 0; document.addEventListener('click', function () { window.clicks++; });</script></body>";

    /// M4 acceptance (Atlas 22:05, 10-21): with a page script spinning, the
    /// URL bar's navigation takes effect within 100 ms: the window is free
    /// to take it, and the new page is requested.
    fn a_url_bar_navigation_lands_within_100_ms_while_a_page_script_spins(view: RustKitView) {
        let (base, log) = logging_server(SPIN_PAGE);
        load_and_settle(&view, &base);
        // A timer the page set spins until the turn budget stops it (5 s).
        let _ = read(&view, "setTimeout(function () { while (true) {} }, 30); 'armed'");
        let at = Instant::now() + Duration::from_millis(250);
        let late = wake_until(&view, at);
        view.wry_load_url(&format!("{base}next")).unwrap();
        let taken = at.elapsed();
        let deadline = at + Duration::from_secs(10);
        let requested = loop {
            if let Some(when) = log.lock().unwrap().iter().find(|(path, _)| path == "/next").map(|(_, when)| *when) {
                break when.saturating_duration_since(at);
            }
            assert!(Instant::now() < deadline, "the new page was never requested");
            wake(&view);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(
            late < Duration::from_millis(100) && taken < Duration::from_millis(100) && requested < Duration::from_millis(100),
            "window free {late:?} late, navigation taken at {taken:?}, page requested at {requested:?}"
        );
    }

    /// M4 acceptance: with a page script spinning, a click is taken by the
    /// window within 100 ms, and the page hears it once the spin is
    /// stopped: queued, never dropped.
    fn a_click_is_taken_within_100_ms_while_a_page_script_spins_and_is_not_dropped(view: RustKitView) {
        let (base, _log) = logging_server(SPIN_PAGE);
        load_and_settle(&view, &base);
        let _ = read(&view, "setTimeout(function () { while (true) {} }, 30); 'armed'");
        let at = Instant::now() + Duration::from_millis(250);
        let late = wake_until(&view, at);
        view.mouse_down_at_point(10.0, 10.0);
        let _ = view.click_at_point(10.0, 10.0);
        view.render();
        let taken = at.elapsed();
        assert!(
            late < Duration::from_millis(100) && taken < Duration::from_millis(100),
            "window free {late:?} late, click taken at {taken:?}"
        );
        // The turn budget (5 s) stops the spin; then the listener runs.
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            wake(&view);
            if read(&view, "String(window.clicks)").contains('1') {
                break;
            }
            assert!(Instant::now() < deadline, "the click was dropped");
            std::thread::sleep(Duration::from_millis(20));
        }
        // Heard only once the budget stopped the spin: it was spinning.
        let heard = at.elapsed();
        assert!(heard > Duration::from_secs(1), "the page was not spinning: heard at {heard:?}");
    }

    #[test]
    fn content_thread_a_url_bar_navigation_lands_within_100_ms_while_a_page_script_spins() {
        a_url_bar_navigation_lands_within_100_ms_while_a_page_script_spins(content_view());
    }

    #[test]
    fn content_thread_a_click_is_taken_within_100_ms_while_a_page_script_spins_and_is_not_dropped() {
        a_click_is_taken_within_100_ms_while_a_page_script_spins_and_is_not_dropped(content_view());
    }

    /// Independent R1 on #730, SHOULD-FIX 3: with the page on its own
    /// thread, the window's focus and scroll questions used to be asked
    /// and read back at once, so they always missed. A focused field then
    /// looked unfocused and Backspace or the arrows were dropped.
    #[test]
    fn content_thread_the_window_learns_a_click_focused_a_field_and_a_scroll_reports_a_change() {
        let view = content_view();
        let (base, _log) = logging_server(
            "<body style='margin:0'><input id=f style='width:200px;height:40px'>\
<div style='height:3000px'></div></body>",
        );
        load_and_settle(&view, &base);
        let _ = view.focus_at_point(20.0, 20.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !view.has_focused_element() {
            assert!(Instant::now() < deadline, "the window never learned the field was focused");
            wake(&view);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(read(&view, "document.activeElement.id").contains('f'));
        assert!(view.scroll_by(0.0, -200.0), "a scroll reported no change");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !read(&view, "String(window.scrollY > 0)").contains("true") {
            assert!(Instant::now() < deadline, "the page never scrolled");
            wake(&view);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// #764 re-cut: the Inspector's questions borrowed an engine the window
    /// no longer has once the page runs on its own thread (the shipped
    /// default). They are asked through the page protocol and answered
    /// without the window waiting.
    #[test]
    fn content_thread_the_inspector_gets_the_dom_tree_and_an_element() {
        use rustkit_engine::{DevtoolsAnswer, DevtoolsQuery};
        let view = content_view();
        let (base, _log) = logging_server("<body><p id=hello class=greeting>hi</p></body>");
        load_and_settle(&view, &base);
        let answer = |query| {
            let mut rx = view.devtools_ask(query);
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match rx.try_recv() {
                    Ok(answer) => return answer,
                    Err(_) => assert!(Instant::now() < deadline, "the Inspector was never answered"),
                }
                wake(&view);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let DevtoolsAnswer::Tree(Some(tree)) = answer(DevtoolsQuery::DomTree { root: None, reveal: None }) else {
            panic!("no DOM tree");
        };
        fn find(node: &rustkit_engine::DevtoolsNode) -> Option<usize> {
            if node.id.as_deref() == Some("hello") {
                return Some(node.node_id);
            }
            node.children.iter().find_map(find)
        }
        let node = find(&tree).expect("the tree has #hello");
        let DevtoolsAnswer::Element(Some(element)) = answer(DevtoolsQuery::Element(node)) else {
            panic!("no element");
        };
        assert_eq!((element.tag.as_str(), element.classes.clone()), ("p", vec!["greeting".to_string()]));
    }

    /// #730 delta R1 NIT: a new document starts with nothing focused. The
    /// window kept the old page's answer until the page next reported, so
    /// the first key on the new page went to the page instead of the window.
    #[test]
    fn content_thread_a_new_document_starts_with_no_focused_field() {
        let view = content_view();
        let (base, _log) = logging_server(
            "<body style='margin:0'><input id=f style='width:200px;height:40px'></body>",
        );
        load_and_settle(&view, &base);
        focus_the_field(&view);
        view.load_url_blocking(&base);
        assert!(!view.page_focused.get(), "a navigation kept the old page's focused field");
        load_and_settle(&view, &base);
        focus_the_field(&view);
        view.load_html_internal("<body><p>new tab</p></body>").unwrap();
        assert!(!view.page_focused.get(), "New Tab kept the old page's focused field");
    }

    fn focus_the_field(view: &RustKitView) {
        let _ = view.focus_at_point(20.0, 20.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !view.has_focused_element() {
            assert!(Instant::now() < deadline, "the window never learned the field was focused");
            wake(view);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Independent R1 on #730, NIT 4: a script navigation the old page
    /// sent just before New Tab replaced it must not pull the new tab page
    /// away.
    #[test]
    fn content_thread_a_script_navigation_from_the_page_a_new_tab_replaced_is_dropped() {
        let view = content_view();
        let (base, _log) = logging_server("<body><p>old</p></body>");
        load_and_settle(&view, &base);
        let _ = view.content.as_ref().unwrap().eval(&format!("location.href = '{base}elsewhere'; 'sent'"));
        view.load_html_internal("<body><p>new tab</p></body>").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            wake(&view);
            assert!(
                view.take_script_navigation().is_none(),
                "the replaced page's navigation pulled the new tab away"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn content_view() -> RustKitView {
        RustKitView::headless_content(Bounds::new(0, 0, 400, 300))
    }

    #[test]
    fn content_views_stop_a_script_that_outruns_the_budget() {
        assert!(content_engine_builder().config().interrupt_scripts_at_budget);
        // The engine's default (parity-capture, the board) lets it run on.
        assert!(!rustkit_engine::EngineConfig::default().interrupt_scripts_at_budget);
    }
}
