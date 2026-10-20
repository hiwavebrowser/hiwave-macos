//! RustKit WebView adapter for HiWave macOS
//!
//! This module provides the RustKit engine as the default WebView backend for content rendering.
//! It wraps `rustkit_engine::Engine` and provides a WRY-like interface.


use super::shield_adapter::create_shield_interceptor_with_counter;
use super::webview::IWebContent;
use hiwave_core::{HiWaveError, HiWaveResult};
use rustkit_engine::{Engine, EngineBuilder, EngineEvent, EngineViewId};
use rustkit_viewhost::Bounds;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::window::Window;
use tao::rwh_06::HasWindowHandle;
use tokio::sync::mpsc;
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
/// # Thread Safety
///
/// This type is NOT Send or Sync. All operations must be performed on the main thread.
pub struct RustKitView {
    /// The engine managing this view. Shared with the load in progress
    /// ([`PendingLoad`]), which holds it mutably until the load ends; every
    /// other call takes it with `try_borrow` and does nothing meanwhile.
    engine: Rc<RefCell<Engine>>,
    /// The view ID within the engine.
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
    /// Event receiver for engine events.
    event_rx: Option<mpsc::UnboundedReceiver<EngineEvent>>,
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
    /// Runtime the live loop's network turns block on. `None` if it could
    /// not be built: the page then gets no timers or late fetches, as before.
    live_runtime: Option<tokio::runtime::Runtime>,
    /// The content `NSView`. Kept outside the engine so a load can move the
    /// frame while `engine` is borrowed.
    host_view: isize,
    /// The load in progress, polled a slice at a time by the event loop
    /// ([`RustKitView::step_load`]).
    pending_load: RefCell<Option<PendingLoad>>,
    /// Bounds set while a load held the engine, applied when it ends.
    pending_bounds: Cell<Option<Bounds>>,
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
        // `HIWAVE_REPLAY=http://127.0.0.1:PORT` sends the page's requests at
        // the replay server. Unset, the proxy stays off.
        .replay_proxy_from_env()
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

        let mut engine = builder
            .build()
            .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;

        // Take event receiver for processing events
        let event_rx = engine.take_event_receiver();

        // Create view
        let view_id = engine
            .create_view(raw_handle, bounds)
            .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;

        info!(?view_id, "RustKit view created");
        let host_view = engine.host_view_raw(view_id);
        rustkit_viewhost::clip_raw_view(host_view);

        Ok(Self::from_engine(engine, view_id, event_rx, counter_clone, host_view))
    }

    /// A content view with no window (tests).
    #[cfg(test)]
    fn headless(bounds: Bounds) -> Self {
        let mut engine = content_engine_builder().build().expect("engine (no GPU?)");
        let event_rx = engine.take_event_receiver();
        let view_id = engine.create_headless_view(bounds).expect("headless view");
        Self::from_engine(engine, view_id, event_rx, None, 0)
    }

    fn from_engine(
        engine: Engine,
        view_id: EngineViewId,
        event_rx: Option<mpsc::UnboundedReceiver<EngineEvent>>,
        counter_clone: Option<Arc<AtomicU64>>,
        host_view: isize,
    ) -> Self {
        Self {
            engine: Rc::new(RefCell::new(engine)),
            view_id: Some(view_id),
            current_url: RefCell::new(None),
            current_title: RefCell::new(None),
            zoom_level: RefCell::new(1.0),
            visible: RefCell::new(true),
            loading: RefCell::new(false),
            event_rx,
            blocked_counter: counter_clone,
            history: RefCell::new(Vec::new()),
            history_index: RefCell::new(0),
            replace_next: Cell::new(false),
            live_clock: Cell::new(Instant::now()),
            live_runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| warn!(error = %e, "No runtime for the live loop; page timers will not run"))
                .ok(),
            host_view,
            pending_load: RefCell::new(None),
            pending_bounds: Cell::new(None),
        }
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
        let view_id = self.view_id?;
        let runtime = self.live_runtime.as_ref()?;
        let started = Instant::now();
        // Whole milliseconds only; the remainder stays on the clock for the
        // next turn, so frequent turns do not slow the page's time.
        let elapsed_ms = started.duration_since(self.live_clock.get()).as_millis() as u64;
        self.live_clock
            .set(self.live_clock.get() + Duration::from_millis(elapsed_ms));

        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        let turn = runtime.block_on(engine.pump_live(view_id, elapsed_ms));
        if turn.timers_ran > 0 || turn.requests > 0 {
            debug!(?turn, "Live turn");
        }
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
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        // The live turn lays out a resized view; without a live runtime
        // there is no turn, so the frame does it.
        if let Some(view_id) = self.view_id {
            if let Err(e) = engine.flush_pending_resize(view_id) {
                debug!(error = %e, "layout after a resize failed");
            }
        }
        if render_every_wake() {
            engine.render_all_views();
        } else {
            engine.render_changed_views();
        }
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
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            engine.note_input(view_id, rustkit_engine::InputMark { at, seq });
        }
    }

    /// Route OS keyboard delivery to the content view (first responder).
    pub fn grab_keyboard(&self) {
        let Ok(engine) = self.engine.try_borrow() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            engine.grab_keyboard(view_id);
        }
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
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        self.view_id
            .map(|view_id| engine.handle_text_key(view_id, key_code, key, ctrl, shift, alt))
            .unwrap_or(false)
    }

    /// A key was released: the page hears `keyup`.
    pub fn handle_key_up(&self, key_code: u32, key: &str, ctrl: bool, shift: bool, alt: bool) {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            engine.handle_key_up(view_id, key_code, key, ctrl, shift, alt);
        }
    }

    /// Enter in the focused field: the page hears `keydown` and `submit`,
    /// and this is the URL to navigate to when its form submits. `None`
    /// when there is no form, a listener cancelled, or the form is not a
    /// GET.
    pub fn form_submit_url(&self) -> Option<String> {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        self.view_id
            .and_then(|view_id| engine.submit_focused_form(view_id))
    }

    /// Where the page's own script asked to go since the last call
    /// (`location.href = url`, `link.click()`, `form.submit()`, a submit
    /// from a timer). The caller navigates.
    /// A reload, `location.replace()` or a meta refresh replaces the
    /// current history entry when it is loaded, as in Chrome.
    pub fn take_script_navigation(&self) -> Option<String> {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        let (url, replace) = self
            .view_id
            .and_then(|view_id| engine.take_script_navigation_with_replace(view_id))?;
        self.replace_next.set(replace);
        Some(url)
    }

    /// Whether a content element currently holds focus.
    pub fn has_focused_element(&self) -> bool {
        let Ok(engine) = self.engine.try_borrow() else { return Default::default() };
        self.view_id
            .and_then(|view_id| engine.focused_node(view_id))
            .is_some()
    }

    /// Rebuild layout and repaint after an edit changed a control's value.
    pub fn relayout(&self) {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            if let Err(e) = engine.relayout_for(view_id, rustkit_engine::RelayoutCause::Input) {
                debug!(error = %e, "relayout after edit failed");
            }
        }
    }

    /// Focus whatever focusable element sits at these viewport coordinates,
    /// clearing focus when nothing focusable is there. Returns the focused
    /// element's tag name.
    pub fn focus_at_point(&self, x: f32, y: f32) -> Option<String> {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        self.view_id
            .and_then(|view_id| engine.focus_at_point(view_id, x, y))
    }

    /// Deliver a mouse press at viewport coordinates to the page
    /// (`mousedown`).
    pub fn mouse_down_at_point(&self, x: f32, y: f32) {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            engine.mouse_down_at_point(view_id, x, y);
        }
    }

    /// Deliver a pointer move to viewport coordinates to the page (the
    /// over/out/enter/leave events when the element under it changed, then
    /// `pointermove` and `mousemove`).
    pub fn mouse_move_at_point(&self, x: f32, y: f32) {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            engine.mouse_move_at_point(view_id, x, y);
        }
    }

    /// Tell the page the pointer left the view.
    pub fn mouse_leave(&self) {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            engine.mouse_leave(view_id);
        }
    }

    /// Deliver a mouse release at viewport coordinates to the page
    /// (`mouseup`, `click`), then report the click's default actions: what
    /// it focused and the link to follow unless a listener cancelled it.
    pub fn click_at_point(&self, x: f32, y: f32) -> rustkit_engine::ClickOutcome {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        self.view_id
            .map(|view_id| engine.click_at_point(view_id, x, y))
            .unwrap_or_default()
    }

    /// Resolve a click at viewport coordinates to a link URL, if any.
    pub fn link_at_point(&self, x: f32, y: f32) -> Option<String> {
        let Ok(engine) = self.engine.try_borrow() else { return Default::default() };
        self.view_id
            .and_then(|view_id| engine.link_at_point(view_id, x, y))
    }

    /// Scroll the view by a wheel delta in physical pixels.
    ///
    /// Returns true if the scroll offset changed (a re-render is needed).
    /// Wheel events that reach the tao window loop are exactly the ones the
    /// UI-frame WebView did not consume, i.e. the pointer was over content.
    pub fn scroll_by(&self, delta_x: f32, delta_y: f32) -> bool {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            match engine.scroll_view(view_id, delta_x, delta_y) {
                Ok(changed) => changed,
                Err(e) => {
                    debug!(error = %e, "scroll_by failed");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Load HTML content directly.
    pub fn load_html_internal(&self, html: &str) -> HiWaveResult<()> {
        self.cancel_load();
        let Ok(mut engine) = self.engine.try_borrow_mut() else {
            return Err(HiWaveError::WebView("engine busy".into()));
        };
        if let Some(view_id) = self.view_id {
            engine
                .load_html(view_id, html)
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
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
        let Ok(mut engine) = self.engine.try_borrow_mut() else {
            // A load holds the engine: the size waits for its end.
            self.pending_bounds.set(Some(bounds));
            return Ok(());
        };
        if let Some(view_id) = self.view_id {
            engine
                .set_view_bounds(view_id, bounds)
                .map_err(|e| hiwave_core::HiWaveError::WebView(e.to_string()))?;
        }
        Ok(())
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

    /// Start loading a URL. The event loop drives the load
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
        let Some(view_id) = self.view_id else { return };
        self.cancel_load();

        if self.live_runtime.is_none() {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    error!(error = %e, "Failed to create runtime");
                    return;
                }
            };
            let Ok(mut engine) = self.engine.try_borrow_mut() else { return };
            rt.block_on(async {
                if let Err(e) = engine.load_url_yielding(view_id, parsed).await {
                    error!(error = %e, "Failed to load URL");
                }
            });
            self.live_clock.set(Instant::now());
            return;
        }

        let canceller = self
            .engine
            .try_borrow()
            .ok()
            .and_then(|engine| engine.nav_canceller(view_id));
        let engine = Rc::clone(&self.engine);
        let future = Box::pin(async move {
            let Ok(mut engine) = engine.try_borrow_mut() else {
                warn!("Load not started: the engine is busy");
                return;
            };
            if let Err(e) = engine.load_url_yielding(view_id, parsed).await {
                error!(error = %e, "Failed to load URL");
            }
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

    /// Whether a load is in progress.
    pub fn is_loading(&self) -> bool {
        self.pending_load.borrow().is_some()
    }

    /// Poll the load in progress for at most [`LOAD_SLICE`] (call this in
    /// the event loop). Returns true while the load goes on: the caller
    /// returns to tao and wakes again soon.
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
    pub fn stop_load(&self) -> bool {
        let loading = self.is_loading();
        self.cancel_load();
        loading
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
        self.live_clock.set(Instant::now());
    }

    /// Execute JavaScript synchronously.
    pub fn execute_script_sync(&self, script: &str) -> Option<String> {
        let Ok(mut engine) = self.engine.try_borrow_mut() else { return Default::default() };
        if let Some(view_id) = self.view_id {
            match engine.execute_script(view_id, script) {
                Ok(result) => Some(result),
                Err(e) => {
                    debug!(error = %e, "Script execution failed");
                    None
                }
            }
        } else {
            None
        }
    }
}

impl Drop for RustKitView {
    fn drop(&mut self) {
        self.cancel_load();
        if let Some(view_id) = self.view_id {
            let Ok(mut engine) = self.engine.try_borrow_mut() else { return };
            if let Err(e) = engine.destroy_view(view_id) {
                warn!(error = %e, "Failed to destroy RustKit view");
            }
        }
    }
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

    #[test]
    fn content_views_stop_a_script_that_outruns_the_budget() {
        assert!(content_engine_builder().config().interrupt_scripts_at_budget);
        // The engine's default (parity-capture, the board) lets it run on.
        assert!(!rustkit_engine::EngineConfig::default().interrupt_scripts_at_budget);
    }
}
