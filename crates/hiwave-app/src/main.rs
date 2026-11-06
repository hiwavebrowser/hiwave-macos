//! HiWave - Main Application Entry Point
//!
//! This is the primary entry point for the HiWave browser application.
//! It uses a three-WebView architecture:
//! - HiWave WebView: Full window (browser UI frame + sidebar)
//! - Content WebView: Right pane (excludes sidebar, below top bar)
//! - Shelf WebView: Bottom (collapsible, aligned to content pane)

mod import;
mod platform;

use std::sync::{Arc, Mutex};
// Used only by the WRY-content IPC/nav block, which is compiled out of the
// macOS build (non-macOS only). Deleting this as an "unused import" breaks
// that build with E0433 at the two should_block call sites: a green macOS
// build says nothing about a cfg arm your target omits. Gate must match
// those sites, not the default target.
#[cfg(not(target_os = "macos"))]
use hiwave_shield::ResourceType;
use muda::{Menu, MenuEvent};
use platform::get_platform_manager;
#[cfg(target_os = "macos")]
use platform::menu_ids;
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Icon, WindowBuilder},
};
use tracing::{error, info, trace, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use wry::{Rect, WebView, WebViewBuilder};

/// Default height of the chrome top bar area (workspace + tabs + toolbar)
const CHROME_HEIGHT_DEFAULT: u32 = 104;
/// Small expansion for tab actions panel
const CHROME_HEIGHT_SMALL: u32 = 148;
/// Expanded height for chrome UI overlays
const CHROME_HEIGHT_EXPANDED: u32 = 460;
/// Default height of the shelf (hidden)
const SHELF_HEIGHT_DEFAULT: u32 = 0;
/// Expanded height when command palette/shelf is open
const SHELF_HEIGHT_EXPANDED: u32 = 280;
const SIDEBAR_WIDTH: f64 = 220.0;
/// While a content load is in progress the loop wakes this soon after each
/// slice, so the load keeps its pace and tao still takes events in between.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
const LOAD_WAKE: std::time::Duration = std::time::Duration::from_millis(1);

mod ipc;
mod state;
mod webview;
mod window_stall;

#[cfg(all(target_os = "macos", feature = "rustkit"))]
mod webview_rustkit;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
use hiwave_shield_intercept as shield_adapter;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
mod webview_rustkit_chrome;
#[cfg(all(target_os = "macos", feature = "rustkit"))]
mod devtools_panel;

/// Run `$body` with the RustKit content view bound to `$view` and give
/// `true`; give `false` for WebKit content, whose Inspector is the injected
/// script (`ui/inspector_inject.js`).
macro_rules! with_rustkit_content {
    ($content:expr, $view:ident => $body:expr) => {{
        #[cfg(all(target_os = "macos", feature = "rustkit"))]
        let handled = match *$content {
            UnifiedContentWebView::RustKit(ref $view) => {
                $body;
                true
            }
            _ => false,
        };
        #[cfg(not(all(target_os = "macos", feature = "rustkit")))]
        let handled = false;
        handled
    }};
}

mod content_webview_trait;
#[cfg(target_os = "macos")]
mod content_webview_enum;
mod chrome_webview_enum;

use content_webview_trait::ContentWebViewOps;
#[cfg(target_os = "macos")]
use content_webview_enum::ContentWebView as UnifiedContentWebView;

use ipc::{IpcMessage, JS_BRIDGE};
use state::AppState;

/// The HTML content for the browser chrome
const CHROME_HTML: &str = include_str!("ui/chrome.html");
/// The HTML content for the bottom shelf
const SHELF_HTML: &str = include_str!("ui/shelf.html");
/// The HTML content for the new tab landing page (now shows about page)
const NEW_TAB_URL: &str = "hiwave://newtab";
/// The HTML content for the about page
const ABOUT_HTML: &str = include_str!("ui/about.html");
const ABOUT_URL: &str = "hiwave://about";
/// The HTML content for the settings window
const SETTINGS_HTML: &str = include_str!("ui/settings.html");
/// The HTML content for the analytics report page
const REPORT_HTML: &str = include_str!("ui/report.html");
const REPORT_URL: &str = "hiwave://report";
const FIND_IN_PAGE_HELPER: &str = include_str!("ui/find_in_page.js");
const CONTEXT_MENU_HELPER: &str = include_str!("ui/context_menu.js");
const AUDIO_DETECTOR: &str = include_str!("ui/audio_detector.js");
const AUTOFILL_HELPER: &str = include_str!("ui/autofill.js");
const CONSOLE_INJECT: &str = include_str!("ui/console_inject.js");
const CHART_JS: &str = include_str!("ui/chart.umd.min.js");
/// The HTML content for the inspector panel
const INSPECTOR_HTML: &str = include_str!("ui/inspector.html");
/// The JavaScript to inject into content WebView for DOM inspection
const INSPECTOR_INJECT: &str = include_str!("ui/inspector_inject.js");
/// Default height of the inspector (hidden)
const INSPECTOR_HEIGHT_DEFAULT: u32 = 0;
/// Expanded height when inspector is open
const INSPECTOR_HEIGHT_EXPANDED: u32 = 300;

fn create_window_icon() -> Option<Icon> {
    const SIZE: u32 = 32;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let ratio = x as f32 / (SIZE as f32 - 1.0);
            let mut r = 110.0 + 60.0 * ratio;
            let mut g = 30.0 + 40.0 * ratio;
            let mut b = 200.0 + 30.0 * ratio;
            let is_z_line = y == x
                || y == (SIZE - 1 - x)
                || (y == SIZE / 2 && (SIZE / 4..SIZE * 3 / 4).contains(&x));
            if is_z_line {
                r = (r * 1.15).min(255.0);
                g = (g * 1.15).min(255.0);
                b = (b * 1.15).min(255.0);
            } else {
                r = (r * 0.72).max(0.0);
                g = (g * 0.72).max(0.0);
                b = (b * 0.72).max(0.0);
            }
            data.push(r as u8);
            data.push(g as u8);
            data.push(b as u8);
            data.push(255);
        }
    }
    Icon::from_rgba(data, SIZE, SIZE).ok()
}

#[derive(Debug, Clone, Copy)]
enum ShelfScope {
    Workspace,
    All,
}

impl ShelfScope {
    fn from_option(scope: Option<&str>) -> Self {
        match scope {
            Some("all") => ShelfScope::All,
            _ => ShelfScope::Workspace,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ShelfScope::Workspace => "workspace",
            ShelfScope::All => "all",
        }
    }
}

/// User events for cross-WebView communication
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum UserEvent {
    Navigate(String),
    GoBack,
    GoForward,
    Reload,
    Stop,
    NewTab,
    UpdateTitle(String),
    UpdateUrl(String),
    UpdateActiveTabUrl(String),
    NavigationStateChanged {
        can_go_back: bool,
        can_go_forward: bool,
    },
    SetLoading(bool),
    RecordVisit {
        url: String,
    },
    SyncTabs,
    SyncWorkspaces,
    SyncDownloads,
    SyncShieldStats,
    FindInPage {
        query: String,
        case_sensitive: bool,
        direction: String,
    },
    FindInPageResult(serde_json::Value),
    SyncShelf(ShelfScope),
    SyncBlocklist,
    SyncHistory,
    ShowCommands(String),
    OpenSettings,
    CloseSettings,
    // Dynamic resize events
    ExpandChrome,
    ExpandChromeSmall,
    CollapseChrome,
    ExpandShelf,
    CollapseShelf,
    SetSidebarOpen(bool),
    SetSidebarWidth(f64),
    SetRightPanelOpen(bool),
    // Import events
    GetBrowserProfiles(String),
    ImportBrowserProfilesResult(String),
    ImportBookmarks { browser: String, profile_path: String },
    ImportBookmarksResult(String),
    // Settings events
    GetSettings,
    SettingsResult(String),
    SaveSettings(serde_json::Value),
    SettingsSaved,
    // Export/Import events
    ExportData { include_settings: bool, include_workspaces: bool },
    ExportDataResult(String),
    SaveExportToFile { data: String, suggested_name: String },
    ImportStateData { data: String, replace: bool },
    ImportStateDataResult(String),
    PickImportFile,
    // Clear data event
    ClearBrowsingData { history: bool, downloads: bool, shelf: bool },
    ClearBrowsingDataResult(String),
    // Cellar events
    GetCellar,
    RestoreFromCellar { id: String },
    ClearCellar,
    // Decay ticker
    DecayTick,
    // The stall log's ping (window_stall.rs); only sent with HIWAVE_STALL_LOG=1
    WindowThreadPing,
    // An inserted script's fetch finished. The event only wakes the loop;
    // MainEventsCleared runs the script on the live turn.
    LiveTurn,
    // Focus mode events
    EnterFocusMode,
    ExitFocusMode,
    ToggleFocusMode,
    SyncFocusModeStatus,
    ShowFocusPeek,
    HideFocusPeek,
    FocusAutoTriggerCheck,
    // Focus mode settings events (from settings webview)
    GetFocusModeConfigForSettings,
    SaveFocusModeConfigFromSettings(serde_json::Value),
    AddToFocusBlocklistFromSettings(String),
    RemoveFromFocusBlocklistFromSettings(String),
    // Script evaluation (for IPC responses that need to send data back to chrome)
    EvaluateScript(String),
    // Script evaluation on content webview
    EvaluateContentScript(String),
    // Internal page loading
    LoadAboutPage,
    LoadReportPage,
    // Print
    PrintPage,
    // Zoom controls
    ZoomIn,
    ZoomOut,
    ResetZoom,
    SyncZoomLevel,
    // Tab audio state
    TabAudioStateChanged { playing: bool },
    ToggleTabMute { id: String },
    // Autofill
    TriggerAutofill,
    SendAutofillCredentials { credentials_json: String },
    // Vault settings events (from settings webview)
    GetVaultStatus,
    UnlockVault(String),
    LockVault,
    GetAllCredentials,
    SaveCredential { url: String, username: String, password: String },
    DeleteCredential(i64),
    // Analytics settings events (from settings webview)
    GetAnalyticsSettings,
    UpdateAnalyticsSettings { enabled: bool, retention_days: i32, weekly_report: bool, report_day: String },
    ClearAnalyticsData,
    ExportAnalyticsData { format: String },
    // Inspector events
    OpenInspector,
    CloseInspector,
    ToggleInspector,
    InspectorSendDomTree(String),
    InspectorSendElementStyles(String),
    InspectorElementPicked { path: String, styles: serde_json::Value },
    InspectorPickerStopped,
    InspectorHighlightElement { path: String },
    InspectorClearHighlight,
    InspectorStartPicker,
    InspectorStopPicker,
    InspectorScrollToElement { path: String },
    InspectElement { path: String },
    /// The panel asks for the DOM tree again.
    InspectorRefreshTree,
    /// The panel selected a node (a tree row, a breadcrumb).
    InspectorSelect { path: String },
    /// The panel expanded a node whose children the tree left out.
    InspectorExpand { path: String },
    // Console events
    InspectorConsoleMessage(String),
    InspectorConsoleClear,
    InspectorConsoleEval { code: String },
}

/// The browser chrome's view: the WebKit view today; a RustKit view behind
/// `HIWAVE_RUSTKIT_CHROME` (CH3.A2). The app only runs script in it and
/// places it.
trait ChromeUi {
    /// Run `js` in the chrome's document, without waiting for its value.
    fn eval(&self, js: &str) -> Result<(), String>;
    /// Move and size the chrome view.
    fn place(&self, rect: Rect);
    /// Route the keyboard to the chrome (the window's first responder),
    /// after a shortcut's script focused one of its fields: the address
    /// bar, find, the command palette. The WebKit view: nothing, as before.
    fn take_keyboard(&self) {}
    /// The page column (x, y, width, height, in points) the chrome must
    /// leave to the page below it, in a `width` x `height` window (CH3.A3).
    /// The WebKit view: nothing, as before.
    fn compose(&self, _width: f64, _height: f64, _hole: (f64, f64, f64, f64)) {}
    /// Stay the frontmost of the window's views, over panels built after
    /// it (CH3.A3b). The WebKit view: nothing, as before.
    fn keep_on_top(&self) {}
}

impl ChromeUi for WebView {
    fn eval(&self, js: &str) -> Result<(), String> {
        wry::WebView::evaluate_script(self, js).map_err(|e| e.to_string())
    }

    fn place(&self, rect: Rect) {
        let _ = wry::WebView::set_bounds(self, rect);
    }
}

/// The chrome's view as built: WebKit, or RustKit with
/// `HIWAVE_RUSTKIT_CHROME=1` (macOS, content thread on; CH3.A2).
enum ChromeView {
    WebKit(WebView),
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    RustKit(webview_rustkit::RustKitView),
}

/// A panel's view: the shelf, the Inspector and the settings window (CH3.A4).
/// The same two backends as the chrome, and the app does the same two
/// things with them: run script in them and place them. Only the chrome
/// has a composition region (`compose`); a panel has none.
type PanelView = ChromeView;

/// How many scripts a panel that is not built yet keeps for when it is
/// (CH3.A4b). Past it the oldest go: a panel shows what it was last told.
const LAZY_PANEL_HELD_MAX: usize = 256;

/// A panel's view, built when the panel first opens (CH3.A4b): with
/// `HIWAVE_RUSTKIT_CHROME=1` the shelf and the Inspector were RustKit views
/// from launch, two engine threads and ~40 MB for panels most sessions never
/// open. Once built it stays, hidden when the panel closes. Flag off the
/// WebKit view is built at launch, as before (`built`).
struct LazyPanel<V = PanelView> {
    view: std::cell::OnceCell<V>,
    /// Scripts run on it before it was built, oldest first.
    held: std::cell::RefCell<std::collections::VecDeque<String>>,
    /// Its document and its name in the log.
    html: &'static str,
    name: &'static str,
}

impl<V: ChromeUi> LazyPanel<V> {
    /// A panel whose view is built now (the WebKit panels).
    fn built(view: V, html: &'static str, name: &'static str) -> Self {
        let panel = Self::deferred(html, name);
        let _ = panel.view.set(view);
        panel
    }

    /// A panel built when it first opens.
    fn deferred(html: &'static str, name: &'static str) -> Self {
        LazyPanel {
            view: std::cell::OnceCell::new(),
            held: std::cell::RefCell::new(std::collections::VecDeque::new()),
            html,
            name,
        }
    }

    /// Its view, if built.
    fn get(&self) -> Option<&V> {
        self.view.get()
    }

    /// The layout gives the panel `rect`: a panel with a height is open,
    /// and one not built yet is built there by `build` (its document and
    /// name), then hears the scripts it was sent meanwhile. A closed one is
    /// only placed.
    fn lay_out(&self, rect: Rect, build: impl FnOnce(Rect, &'static str, &'static str) -> Option<V>) {
        let open = rect.size.to_logical::<f64>(1.0).height > 0.0;
        if open && self.view.get().is_none() {
            if let Some(view) = build(rect, self.html, self.name) {
                for js in self.held.take() {
                    let _ = view.eval(&js);
                }
                let _ = self.view.set(view);
            }
        }
        if let Some(view) = self.view.get() {
            view.place(rect);
        }
    }
}

impl<V: ChromeUi> ChromeUi for LazyPanel<V> {
    fn eval(&self, js: &str) -> Result<(), String> {
        match self.view.get() {
            Some(view) => view.eval(js),
            None => {
                let mut held = self.held.borrow_mut();
                if held.len() == LAZY_PANEL_HELD_MAX {
                    held.pop_front();
                }
                held.push_back(js.to_string());
                Ok(())
            }
        }
    }

    fn place(&self, rect: Rect) {
        if let Some(view) = self.view.get() {
            view.place(rect);
        }
    }

    fn take_keyboard(&self) {
        if let Some(view) = self.view.get() {
            view.take_keyboard();
        }
    }
}

/// The `cmd` of an IPC message, for the log (#907 R1 nit 1): `?` when it
/// has none, or when it is not a command name (letters, digits, `_`; a page
/// can post anything to the content view's handler).
fn ipc_cmd(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|msg| msg.get("cmd").and_then(|c| c.as_str()).map(str::to_string))
        .filter(|cmd| !cmd.is_empty() && cmd.len() <= 64 && cmd.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .unwrap_or_else(|| "?".to_string())
}

/// Whether IPC messages are logged whole: only with `HIWAVE_DEBUG=1` (the
/// app's verbose switch) or `HIWAVE_TEST_HOOKS=1` (the real-window checks,
/// which read them). By default a line says the message's `cmd` and size:
/// a body carries URLs with their queries and find-in-page text. The
/// RustKit panels' messages are never logged whole (`panel_ipc_log_line`).
fn ipc_bodies_logged() -> bool {
    static LOGGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LOGGED.get_or_init(|| {
        ["HIWAVE_DEBUG", "HIWAVE_TEST_HOOKS"]
            .iter()
            .any(|name| std::env::var(name).map(|v| v == "1").unwrap_or(false))
    })
}

/// One IPC message in the log, as `ipc_bodies_logged` allows.
fn log_ipc(what: &str, body: &str) {
    if ipc_bodies_logged() {
        info!(%body, "{}", what);
    } else {
        info!(cmd = %ipc_cmd(body), len = body.len(), "{}", what);
    }
}

/// `HIWAVE_RUSTKIT_CHROME_EVAL`, the script the real-window checks run in
/// the RustKit chrome once it boots: only with the flag on and
/// `HIWAVE_TEST_HOOKS=1` as well (#907 R1 nit 2), so a release build does
/// not run a script from the environment in the privileged chrome.
#[cfg_attr(not(all(target_os = "macos", feature = "rustkit")), allow(dead_code))]
fn chrome_boot_eval(flag_on: bool, test_hooks: Option<&str>, eval: Option<String>) -> Option<String> {
    (flag_on && test_hooks == Some("1"))
        .then_some(eval)
        .flatten()
        .filter(|js| !js.is_empty())
}

impl ChromeUi for ChromeView {
    fn eval(&self, js: &str) -> Result<(), String> {
        match self {
            ChromeView::WebKit(view) => view.eval(js),
            #[cfg(all(target_os = "macos", feature = "rustkit"))]
            ChromeView::RustKit(view) => view.wry_evaluate_script(js),
        }
    }

    fn place(&self, rect: Rect) {
        match self {
            ChromeView::WebKit(view) => view.place(rect),
            #[cfg(all(target_os = "macos", feature = "rustkit"))]
            ChromeView::RustKit(view) => {
                let _ = view.wry_set_bounds(rect);
            }
        }
    }

    fn take_keyboard(&self) {
        #[cfg(all(target_os = "macos", feature = "rustkit"))]
        if let ChromeView::RustKit(view) = self {
            view.grab_keyboard();
        }
    }

    fn compose(&self, width: f64, height: f64, hole: (f64, f64, f64, f64)) {
        #[cfg(all(target_os = "macos", feature = "rustkit"))]
        if let ChromeView::RustKit(view) = self {
            view.compose_region(width, height, rustkit_viewhost::RegionRect::new(hole.0, hole.1, hole.2, hole.3));
        }
        #[cfg(not(all(target_os = "macos", feature = "rustkit")))]
        let _ = (width, height, hole);
    }

    /// Keep the RustKit chrome the frontmost of the window's views (CH3.A3b,
    /// #909 R1 nit 2): a panel (shelf, Inspector) built later is added on
    /// top, and a chrome popup reaching into its rect was drawn under it.
    /// The panels are inside the chrome's hole, so they still show and
    /// take the pointer wherever the chrome paints nothing.
    fn keep_on_top(&self) {
        #[cfg(all(target_os = "macos", feature = "rustkit"))]
        if let ChromeView::RustKit(view) = self {
            if rustkit_viewhost::raise_raw_view_to_front(view.host_view_raw()) {
                info!("RustKit chrome raised over a panel");
            }
        }
    }
}

impl ChromeView {
    /// Whether the RustKit chrome's frame on screen paints over the page
    /// column (an open popup; CH3.A3b). Always false for the WebKit chrome.
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    fn has_overlays(&self) -> bool {
        match self {
            ChromeView::RustKit(view) => view.region_has_overlays(),
            ChromeView::WebKit(_) => false,
        }
    }

}

/// The chrome document's helper for a click on the page while one of its
/// popups is open (CH3.A3): the app tells the chrome, which closes the
/// popup on its own outside-click handler. The chrome no longer reports its
/// popups' boxes (CH3.A3b): the region comes from its frames.
const CHROME_OUTSIDE_CLICK: &str = r#"
window.__hiwaveOutsideClick = function () {
    var ev;
    try { ev = new MouseEvent('click', { bubbles: true }); } catch (e) { ev = new Event('click', { bubbles: true }); }
    document.body.dispatchEvent(ev);
};
"#;

/// `HIWAVE_RUSTKIT_CHROME=1`: the chrome is a RustKit view (CH3.A2). Needs
/// the content thread, which is where a RustKit view's IPC arrives from.
fn rustkit_chrome_enabled() -> bool {
    cfg!(all(target_os = "macos", feature = "rustkit"))
        && std::env::var("HIWAVE_RUSTKIT_CHROME").map(|v| v == "1").unwrap_or(false)
        && rustkit_engine::content_thread_enabled()
}

/// The chrome document for a RustKit view: the bridge and the outside-click
/// helper (CH3.A3), which needs the body, so it runs at the document's end.
fn chrome_html_with_bridge() -> String {
    html_with_bridge(CHROME_HTML, CHROME_OUTSIDE_CLICK)
}

/// Load one of the app's own documents (the chrome, a panel; built with
/// [`html_with_bridge`]) into `view`, and make it an app-owned view first (CH3.A7): only
/// these documents get `window.ipc` / `window.hiwaveChrome`. The content
/// view is never passed here, so web pages have none of it.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn load_app_document(view: &webview_rustkit::RustKitView, document: &str) -> hiwave_core::HiWaveResult<()> {
    view.set_app_owned();
    view.load_html_internal(document)
}

/// A document for a RustKit view: `JS_BRIDGE` (the WebKit views run it as
/// an initialization script) as the head's first script, and `tail`, when
/// there is one, as the body's last (CH3.A4: the panels' documents).
fn html_with_bridge(html: &str, tail: &str) -> String {
    let html = if tail.is_empty() {
        html.to_string()
    } else {
        let tail = format!("<script>{}</script>", tail);
        match html.rfind("</body>") {
            Some(at) => format!("{}{}{}", &html[..at], tail, &html[at..]),
            None => format!("{}{}", html, tail),
        }
    };
    let script = format!("<script>{}</script>", JS_BRIDGE);
    match html.find("<head>") {
        Some(at) => {
            let at = at + "<head>".len();
            format!("{}{}{}", &html[..at], script, &html[at..])
        }
        None => format!("{}{}", script, html),
    }
}

/// Which view an input event from the viewhost queue is for (CH3.A4): the
/// RustKit chrome, one of the RustKit panels (by its index in `panels`), or
/// the page.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputRoute {
    Chrome,
    Panel(usize),
    Page,
}

#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn route_input(
    view: rustkit_viewhost::ViewId,
    chrome: Option<rustkit_viewhost::ViewId>,
    panels: &[Option<rustkit_viewhost::ViewId>],
) -> InputRoute {
    if chrome == Some(view) {
        return InputRoute::Chrome;
    }
    match panels.iter().position(|id| *id == Some(view)) {
        Some(at) => InputRoute::Panel(at),
        None => InputRoute::Page,
    }
}

/// `HIWAVE_RUSTKIT_PANEL_PROBE=<selector>,<selector>...` (real-window checks,
/// flag on; CH3.A4): appended to each RustKit panel's document. Posts the
/// panel's size and, for each selector, its first match's box, how many
/// elements match, a checkbox's `checked` and a field's value length,
/// whenever they change; the app logs that (`RustKit panel probe`, through
/// `panel_probe_log_line`) and does not hand it to the panel's handler.
/// Shapes only: no text, no values (#910 R1 blocker 2). Compiled into debug
/// builds and `--features test-hooks` builds only.
#[cfg(any(debug_assertions, feature = "test-hooks"))]
const PANEL_PROBE: &str = r#"
(function (selectors) {
    var last = null;
    function report() {
        var boxes = {};
        selectors.forEach(function (sel) {
            var all = document.querySelectorAll(sel);
            var el = all[0];
            if (!el) { boxes[sel] = null; return; }
            var r = el.getBoundingClientRect();
            boxes[sel] = {
                box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
                n: all.length,
                checked: el.type === 'checkbox' ? !!el.checked : null,
                len: typeof el.value === 'string' ? el.value.length : null
            };
        });
        var s = JSON.stringify({ cmd: 'panel_probe', size: [innerWidth, innerHeight], boxes: boxes });
        if (s === last) return;
        last = s;
        window.ipc.postMessage(s);
    }
    setInterval(report, 500);
    report();
})(SELECTORS);
"#;

/// A RustKit panel's `window.ipc` message as the log carries it (#910 R1
/// blocker 1): its `cmd` and size, never its body, whatever the log level
/// or switches. The settings page posts `unlock_vault {password}`,
/// `save_credential {url, username, password}` and the whole state export;
/// the Inspector the console's code; the shelf search queries and URLs.
#[cfg_attr(not(all(target_os = "macos", feature = "rustkit")), allow(dead_code))]
fn panel_ipc_log_line(panel: &str, body: &str) -> String {
    format!("panel={panel:?} cmd={} len={}", ipc_cmd(body), body.len())
}

/// Input method text as the log carries it (CH2.E3, #918 R1 must-fix 2):
/// the variant and the text's size, never the text, whatever the log
/// level. A dead key's, an IME's, the accent popover's and the emoji
/// picker's commits are typed text, password fields included; the same
/// shapes-only rule as the panel IPC lines (#910).
#[cfg_attr(not(all(target_os = "macos", feature = "rustkit")), allow(dead_code))]
#[cfg(target_os = "macos")]
fn ime_input_log_line(input: &rustkit_viewhost::ImeInput) -> String {
    use rustkit_viewhost::ImeInput;
    match input {
        ImeInput::SetMarked(text) => format!("ime=SetMarked len={}", text.len()),
        ImeInput::Insert { text, replace } => {
            format!("ime=Insert len={} replace={}", text.len(), replace.is_some())
        }
        ImeInput::Unmark => "ime=Unmark".to_string(),
    }
}

/// A panel probe report as the log carries it (#910 R1 blocker 2), or
/// `None` for one that is not a report: rebuilt from its shapes only (the
/// size, and for each of `selectors` the box, the match count, `checked`
/// and the value length), so nothing else the document put in it is
/// logged.
#[cfg_attr(
    not(all(target_os = "macos", feature = "rustkit", any(debug_assertions, feature = "test-hooks"))),
    allow(dead_code)
)]
fn panel_probe_log_line(body: &str, selectors: &[&str]) -> Option<String> {
    use serde_json::{json, Map, Value};
    let report: Value = serde_json::from_str(body).ok()?;
    if report.get("cmd").and_then(Value::as_str) != Some("panel_probe") {
        return None;
    }
    let numbers = |v: Option<&Value>| -> Value {
        match v.and_then(Value::as_array) {
            Some(items) if items.iter().all(Value::is_number) => Value::Array(items.clone()),
            _ => Value::Null,
        }
    };
    let mut boxes = Map::new();
    for &sel in selectors {
        let shape = match report.get("boxes").and_then(|b| b.get(sel)) {
            Some(Value::Object(el)) => json!({
                "box": numbers(el.get("box")),
                "n": el.get("n").filter(|v| v.is_u64()).cloned().unwrap_or(Value::Null),
                "checked": el.get("checked").filter(|v| v.is_boolean()).cloned().unwrap_or(Value::Null),
                "len": el.get("len").filter(|v| v.is_u64()).cloned().unwrap_or(Value::Null),
            }),
            _ => Value::Null,
        };
        boxes.insert(sel.to_string(), shape);
    }
    Some(json!({ "size": numbers(report.get("size")), "boxes": boxes }).to_string())
}

/// A URL for the log (#910 R1 nit 3): without its query or fragment.
fn url_for_log(url: &str) -> &str {
    url.find(['?', '#']).map_or(url, |at| &url[..at])
}

/// The selectors of `HIWAVE_RUSTKIT_PANEL_PROBE`, in a build with the
/// probe; none otherwise.
#[cfg_attr(not(any(debug_assertions, feature = "test-hooks")), allow(dead_code))]
fn panel_probe_selectors() -> Vec<String> {
    #[cfg(any(debug_assertions, feature = "test-hooks"))]
    if let Ok(list) = std::env::var("HIWAVE_RUSTKIT_PANEL_PROBE") {
        return list.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    }
    Vec::new()
}

/// Says once at launch whether `HIWAVE_RUSTKIT_PANEL_PROBE` is honoured:
/// a release build without `--features test-hooks` has no probe, and the
/// real-window checks that read it say so instead of failing.
fn announce_panel_probe() {
    if std::env::var_os("HIWAVE_RUSTKIT_PANEL_PROBE").is_none() {
        return;
    }
    if cfg!(any(debug_assertions, feature = "test-hooks")) {
        info!("HIWAVE_RUSTKIT_PANEL_PROBE: on (shapes only)");
    } else {
        warn!("HIWAVE_RUSTKIT_PANEL_PROBE ignored: this build has no test hooks");
    }
}

/// The probe for the RustKit panels' documents, or nothing without
/// `HIWAVE_RUSTKIT_PANEL_PROBE` or in a build without the probe.
fn panel_probe_script() -> String {
    #[cfg(any(debug_assertions, feature = "test-hooks"))]
    if std::env::var_os("HIWAVE_RUSTKIT_PANEL_PROBE").is_some() {
        let selectors = panel_probe_selectors();
        return PANEL_PROBE.replace("SELECTORS", &serde_json::to_string(&selectors).unwrap_or_else(|_| "[]".into()));
    }
    String::new()
}

/// With `HIWAVE_RUSTKIT_CHROME=1`, a panel (the shelf, the Inspector, the
/// settings window's page; CH3.A4) is a RustKit view in `window` at `rect`,
/// loading `html` with the bridge first, as the WebKit view ran it. `None`
/// otherwise: the caller builds the WebKit view, as before.
fn rustkit_panel(window: &tao::window::Window, rect: Rect, html: &str, name: &str) -> Option<PanelView> {
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    if rustkit_chrome_enabled() {
        let at = rect.position.to_logical::<f64>(1.0);
        let size = rect.size.to_logical::<f64>(1.0);
        // A hidden panel is 0 pt tall. Its surface is made 1 pt at least;
        // `place` then gives the view the panel's own rect.
        let bounds = rustkit_viewhost::Bounds::new(
            at.x as i32,
            at.y as i32,
            (size.width as u32).max(1),
            (size.height as u32).max(1),
        );
        let view = webview_rustkit::RustKitView::new(window, bounds)
            .unwrap_or_else(|e| panic!("Failed to create the RustKit {name} view: {e}"));
        load_app_document(&view, &html_with_bridge(html, &panel_probe_script()))
            .unwrap_or_else(|e| panic!("Failed to load the {name} into RustKit: {e}"));
        info!("HIWAVE_RUSTKIT_CHROME=1: the {} is a RustKit view", name);
        let panel = PanelView::RustKit(view);
        panel.place(rect);
        return Some(panel);
    }
    let _ = (window, rect, html, name);
    None
}

/// One turn of a RustKit panel (CH3.A4), as the chrome's arm does for the
/// chrome: its clock, its input, its `window.ipc` messages to `handle`
/// (the WebKit view's handler), and a frame. The panel's input view, if it
/// is a RustKit view.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn pump_rustkit_panel(
    panel: &PanelView,
    name: &str,
    handle: &mut dyn FnMut(&str),
    devtools: &mut devtools_panel::Devtools,
    inspector: &dyn ChromeUi,
    click_proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
) -> Option<rustkit_viewhost::ViewId> {
    let PanelView::RustKit(ref view) = *panel else {
        return None;
    };
    view.process_events();
    if let Some(id) = view.input_view() {
        for event in rustkit_viewhost::drain_input_view(id) {
            view.note_input(event.at, event.seq);
            dispatch_view_input(view, InputTarget::Panel, event.kind, devtools, inspector, click_proxy);
        }
    }
    for body in view.take_ipc_messages() {
        #[cfg(any(debug_assertions, feature = "test-hooks"))]
        if body.starts_with("{\"cmd\":\"panel_probe\"") {
            let selectors = panel_probe_selectors();
            let selectors: Vec<&str> = selectors.iter().map(String::as_str).collect();
            if let Some(report) = panel_probe_log_line(&body, &selectors) {
                info!(panel = name, body = %report, "RustKit panel probe");
            }
            continue;
        }
        info!("RustKit panel IPC {}", panel_ipc_log_line(name, &body));
        handle(&body);
    }
    // A link or `location` in a panel would replace its own document.
    if let Some((url, _)) = view.take_script_navigation() {
        warn!(panel = name, url = url_for_log(&url), "RustKit panel navigation not followed");
    }
    view.render();
    view.input_view()
}

/// A RustKit panel that closed gives the keyboard back to the page (CH3.A4).
/// A WebKit panel: nothing, as before.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn panel_closed(panel: &LazyPanel, content: &UnifiedContentWebView) {
    if let (Some(PanelView::RustKit(_)), UnifiedContentWebView::RustKit(page)) = (panel.get(), content) {
        page.grab_keyboard();
    }
}

impl<T: ChromeUi + ?Sized> ChromeUi for Arc<T> {
    fn eval(&self, js: &str) -> Result<(), String> {
        (**self).eval(js)
    }

    fn place(&self, rect: Rect) {
        (**self).place(rect)
    }

    fn take_keyboard(&self) {
        (**self).take_keyboard()
    }

    fn compose(&self, width: f64, height: f64, hole: (f64, f64, f64, f64)) {
        (**self).compose(width, height, hole)
    }

    fn keep_on_top(&self) {
        (**self).keep_on_top()
    }
}

fn apply_layout(
    window: &tao::window::Window,
    chrome: &impl ChromeUi,
    #[cfg(target_os = "macos")] content: &UnifiedContentWebView,
    #[cfg(not(target_os = "macos"))] content: &WebView,
    shelf: &LazyPanel,
    inspector: &LazyPanel,
    chrome_height: f64,
    shelf_height: f64,
    inspector_height: f64,
    sidebar_width: f64,
    right_sidebar_open: bool,
) {
    let scale = window.scale_factor();
    let window_size = window.inner_size();
    let width = window_size.width as f64 / scale;
    let height = window_size.height as f64 / scale;

    // Clamp sidebar width to valid range (0 = closed, 48-400 when open)
    let sidebar_width = sidebar_width.min(width).max(0.0);
    let right_sidebar_width = if right_sidebar_open {
        SIDEBAR_WIDTH.min(width - sidebar_width)
    } else {
        0.0
    };
    let content_width = (width - sidebar_width - right_sidebar_width).max(0.0);
    // Content height now accounts for inspector height as well
    let content_height = (height - chrome_height - shelf_height - inspector_height).max(0.0);

    let chrome_rect = Rect {
        position: LogicalPosition::new(0, 0).into(),
        size: LogicalSize::new(width, height).into(),
    };

    let content_rect = Rect {
        position: LogicalPosition::new(sidebar_width as i32, chrome_height as i32).into(),
        size: LogicalSize::new(content_width, content_height).into(),
    };

    // Inspector appears below content, above shelf
    let inspector_rect = Rect {
        position: LogicalPosition::new(sidebar_width as i32, (chrome_height + content_height) as i32).into(),
        size: LogicalSize::new(content_width, inspector_height).into(),
    };

    let shelf_rect = Rect {
        position: LogicalPosition::new(sidebar_width as i32, (height - shelf_height) as i32).into(),
        size: LogicalSize::new(content_width, shelf_height).into(),
    };

    let _ = chrome.place(chrome_rect);
    // The RustKit chrome is over the page (CH3.A3): it leaves the page
    // column (page, Inspector, shelf) to the views below it.
    chrome.compose(width, height, (sidebar_width, chrome_height, content_width, (height - chrome_height).max(0.0)));
    let _ = content.set_bounds(content_rect);
    // A RustKit panel is built when it first opens (CH3.A4b).
    let build = |rect: Rect, html: &'static str, name: &'static str| rustkit_panel(window, rect, html, name);
    inspector.lay_out(inspector_rect, build);
    shelf.lay_out(shelf_rect, build);
    chrome.keep_on_top();
}

/// The chrome's `window.ipc` messages, as the WebKit chrome view hands
/// them over. A function, not the handler closure, so the RustKit chrome
/// view (CH3.A2) can feed it the same messages.
fn handle_chrome_ipc(
    body: &str,
    chrome_proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
    chrome_state: &Arc<Mutex<AppState>>,
) {

    match serde_json::from_str::<IpcMessage>(body) {
        Ok(msg) => match &msg {
            IpcMessage::Navigate { url } => {
                let _ = chrome_proxy.send_event(UserEvent::Navigate(url.clone()));
            }
            IpcMessage::GoBack => {
                let _ = chrome_proxy.send_event(UserEvent::GoBack);
            }
            IpcMessage::GoForward => {
                let _ = chrome_proxy.send_event(UserEvent::GoForward);
            }
            IpcMessage::Reload => {
                let _ = chrome_proxy.send_event(UserEvent::Reload);
            }
            IpcMessage::Stop => {
                let _ = chrome_proxy.send_event(UserEvent::Stop);
            }
            IpcMessage::FindInPage {
                ref query,
                case_sensitive,
                ref direction,
            } => {
                let _ = chrome_proxy.send_event(UserEvent::FindInPage {
                    query: query.clone(),
                    case_sensitive: *case_sensitive,
                    direction: direction.clone(),
                });
            }
            IpcMessage::CreateTab { ref url } => {
                let nav_url = url.clone();
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("IPC response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                if let Some(u) = nav_url {
                    let _ = chrome_proxy.send_event(UserEvent::Navigate(u));
                } else {
                    let _ = chrome_proxy.send_event(UserEvent::NewTab);
                }
            }
            IpcMessage::CloseTab { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Close tab response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                // Navigate to the new active tab after closing
                if let Ok(s) = chrome_state.lock() {
                    if let Some(active_tab) = s.shell.get_active_tab() {
                        let url = active_tab.url.to_string();
                        info!("Navigating to new active tab after close: {}", url);
                        let _ = chrome_proxy.send_event(UserEvent::Navigate(url));
                    }
                }
            }
            IpcMessage::ActivateTab { ref id } => {
                let _tab_id = id.clone();
                let already_active = chrome_state
                    .lock()
                    .ok()
                    .and_then(|s| s.shell.get_active_tab().map(|tab| tab.id.0))
                    .map(|active_id| id.parse::<u64>().ok() == Some(active_id))
                    .unwrap_or(false);
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Tab activation response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                if !already_active {
                    if let ipc::IpcResponse::Success { data } = response {
                        if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
                            info!("Navigating to tab URL: {}", url);
                            let _ = chrome_proxy
                                .send_event(UserEvent::Navigate(url.to_string()));
                        }
                    }
                    // Sync zoom level for the newly activated tab
                    let _ = chrome_proxy.send_event(UserEvent::SyncZoomLevel);
                }
            }
            IpcMessage::ExecuteCommand { ref id } => {
                let _cmd_id = id.clone();
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Command execution response: {:?}", response);

                if let ipc::IpcResponse::Success { data } = response {
                    if let Some(result) = data.get("result") {
                        if let Some(action) = result.get("action").and_then(|a| a.as_str())
                        {
                            match action {
                                "navigate" => {
                                    if let Some(url) =
                                        result.get("url").and_then(|u| u.as_str())
                                    {
                                        let _ = chrome_proxy.send_event(
                                            UserEvent::Navigate(url.to_string()),
                                        );
                                    } else {
                                        let _ = chrome_proxy.send_event(UserEvent::NewTab);
                                    }
                                }
                                "reload" => {
                                    let _ = chrome_proxy.send_event(UserEvent::Reload);
                                }
                                "go_back" => {
                                    let _ = chrome_proxy.send_event(UserEvent::GoBack);
                                }
                                "go_forward" => {
                                    let _ = chrome_proxy.send_event(UserEvent::GoForward);
                                }
                                "open_vault" => {
                                    // Vault is now accessed via Settings
                                    let _ = chrome_proxy.send_event(UserEvent::OpenSettings);
                                }
                                _ => {
                                    info!("Unhandled action: {}", action);
                                }
                            }
                        }
                    }
                }
            }
            IpcMessage::SearchCommands { ref query } => {
                let _q = query.clone();
                let response = ipc::commands::handle_message(&chrome_state, msg);
                if let ipc::IpcResponse::Success { data } = response {
                    let json =
                        serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string());
                    let _ = chrome_proxy.send_event(UserEvent::ShowCommands(json));
                }
            }
            IpcMessage::GetWorkspaces => {
                let _response = ipc::commands::handle_message(&chrome_state, msg);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
            }
            IpcMessage::CreateWorkspace { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Create workspace response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
            }
            IpcMessage::ActivateWorkspace { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Activate workspace response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let active_url = chrome_state
                    .lock()
                    .ok()
                    .and_then(|s| s.shell.get_active_tab().map(|tab| tab.url.to_string()));
                if let Some(url) = active_url {
                    let _ = chrome_proxy.send_event(UserEvent::Navigate(url));
                }
            }
            IpcMessage::DeleteWorkspace { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Delete workspace response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
            }
            IpcMessage::RenameWorkspace { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Rename workspace response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
            }
            IpcMessage::SetMode { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Set mode response: {:?}", response);
            }
            IpcMessage::OpenSettings => {
                let _ = chrome_proxy.send_event(UserEvent::OpenSettings);
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Open settings response: {:?}", response);
            }
            IpcMessage::CloseSettings => {
                let _ = chrome_proxy.send_event(UserEvent::CloseSettings);
                info!("Close settings requested");
            }
            IpcMessage::GetShelf { scope } => {
                let scope = ShelfScope::from_option(scope.as_deref());
                let _response = ipc::commands::handle_message(&chrome_state, msg);
                let _ = chrome_proxy.send_event(UserEvent::SyncShelf(scope));
            }
            IpcMessage::AddToShelf { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Add to shelf response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
            }
            IpcMessage::RestoreFromShelf { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Restore from shelf response: {:?}", response);
                let _ =
                    chrome_proxy.send_event(UserEvent::SyncShelf(ShelfScope::Workspace));
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                if let ipc::IpcResponse::Success { data } = response {
                    if let Some(restored) = data.get("restored") {
                        if let Some(url) = restored.get("url").and_then(|v| v.as_str()) {
                            let _ = chrome_proxy
                                .send_event(UserEvent::Navigate(url.to_string()));
                        }
                    }
                }
            }
            IpcMessage::MoveTabToWorkspace { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Move tab response: {:?}", response);
                let _ =
                    chrome_proxy.send_event(UserEvent::SyncShelf(ShelfScope::Workspace));
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
            }
            IpcMessage::LockPage { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Lock page response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
            }
            IpcMessage::UnlockPage { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Unlock page response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let _ = chrome_proxy.send_event(UserEvent::SyncWorkspaces);
            }
            IpcMessage::GetStaleLocks { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Get stale locks response: {:?}", response);
            }
            IpcMessage::DeleteFromShelf { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Delete from shelf response: {:?}", response);
            }
            IpcMessage::UpdateActiveTabUrl { ref url } => {
                let _ = chrome_proxy.send_event(UserEvent::UpdateActiveTabUrl(url.clone()));
            }
            IpcMessage::SetSidebarOpen { open } => {
                let _ = chrome_proxy.send_event(UserEvent::SetSidebarOpen(*open));
            }
            IpcMessage::SetSidebarWidth { width } => {
                let _ = chrome_proxy.send_event(UserEvent::SetSidebarWidth(*width));
            }
            IpcMessage::SetRightPanelOpen { open } => {
                let _ = chrome_proxy.send_event(UserEvent::SetRightPanelOpen(*open));
            }
            IpcMessage::ExpandChrome => {
                info!("Expanding chrome (large)");
                let _ = chrome_proxy.send_event(UserEvent::ExpandChrome);
            }
            IpcMessage::ExpandChromeSmall => {
                info!("Expanding chrome (small)");
                let _ = chrome_proxy.send_event(UserEvent::ExpandChromeSmall);
            }
            IpcMessage::CollapseChrome => {
                info!("Collapsing chrome");
                let _ = chrome_proxy.send_event(UserEvent::CollapseChrome);
            }
            IpcMessage::ExpandShelf => {
                info!("Expanding shelf for command palette");
                let _ = chrome_proxy.send_event(UserEvent::ExpandShelf);
            }
            IpcMessage::CollapseShelf => {
                info!("Collapsing shelf");
                let _ = chrome_proxy.send_event(UserEvent::CollapseShelf);
            }
            // Focus mode IPC handlers
            IpcMessage::EnterFocusMode => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Enter focus mode response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::EnterFocusMode);
            }
            IpcMessage::ExitFocusMode => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Exit focus mode response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::ExitFocusMode);
            }
            IpcMessage::ToggleFocusMode => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Toggle focus mode response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::ToggleFocusMode);
            }
            IpcMessage::ToggleInspector => {
                let _ = chrome_proxy.send_event(UserEvent::ToggleInspector);
            }
            IpcMessage::GetFocusModeStatus => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Get focus mode status response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncFocusModeStatus);
            }
            IpcMessage::GetFocusModeConfig => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Get focus mode config response: {:?}", response);
            }
            IpcMessage::SaveFocusModeConfig { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Save focus mode config response: {:?}", response);
            }
            IpcMessage::FocusScrollProgress { .. } => {
                let _ = ipc::commands::handle_message(&chrome_state, msg);
                // No response needed, just update state
            }
            IpcMessage::FocusMediaPlaying { .. } => {
                let _ = ipc::commands::handle_message(&chrome_state, msg);
                // No response needed, just update state
            }
            IpcMessage::FocusPageLoaded { .. } => {
                let _ = ipc::commands::handle_message(&chrome_state, msg);
                // No response needed, just update state
            }
            IpcMessage::ShowFocusPeek => {
                let _ = ipc::commands::handle_message(&chrome_state, msg);
                let _ = chrome_proxy.send_event(UserEvent::ShowFocusPeek);
            }
            IpcMessage::HideFocusPeek => {
                let _ = ipc::commands::handle_message(&chrome_state, msg);
                let _ = chrome_proxy.send_event(UserEvent::HideFocusPeek);
            }
            IpcMessage::AddToFocusBlocklist { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Add to focus blocklist response: {:?}", response);
            }
            IpcMessage::RemoveFromFocusBlocklist { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Remove from focus blocklist response: {:?}", response);
            }
            IpcMessage::BlockAndClose { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Block and close response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncTabs);
                let _ = chrome_proxy.send_event(UserEvent::SyncBlocklist);
            }
            IpcMessage::GetBlocklist => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Get blocklist response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncBlocklist);
            }
            IpcMessage::UnblockDomain { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Unblock domain response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncBlocklist);
            }
            IpcMessage::GetDownloads => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Get downloads response: {:?}", response);
            }
            IpcMessage::ClearDownloads => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Clear downloads response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncDownloads);
            }
            IpcMessage::GetVisitHistory => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Get visit history response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncHistory);
            }
            IpcMessage::ClearVisitHistory => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Clear visit history response: {:?}", response);
                let _ = chrome_proxy.send_event(UserEvent::SyncHistory);
            }
            IpcMessage::OpenDownload { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Open download response: {:?}", response);
            }
            IpcMessage::ShowDownloadInFolder { .. } => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("Show download response: {:?}", response);
            }
            IpcMessage::GetSidebarWidth => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                if let ipc::IpcResponse::Success { data } = response {
                    if let Some(width) = data.get("width").and_then(|w| w.as_u64()) {
                        let open = data
                            .get("default_open")
                            .and_then(|o| o.as_bool())
                            .unwrap_or(false);
                        // Prefer the state-aware entry point; fall back
                        // to the width-only one so an older chrome.html
                        // keeps its previous (closed) behavior instead
                        // of erroring.
                        let script = format!(
                            "if(window.hiwaveChrome && window.hiwaveChrome.setSidebarState) {{ window.hiwaveChrome.setSidebarState({w}, {o}); }} else if(window.hiwaveChrome && window.hiwaveChrome.setSidebarWidth) {{ window.hiwaveChrome.setSidebarWidth({w}); }}",
                            w = width, o = open
                        );
                        let _ = chrome_proxy.send_event(UserEvent::EvaluateScript(script));
                    }
                }
            }
            IpcMessage::PrintPage => {
                info!("Print page requested");
                let _ = chrome_proxy.send_event(UserEvent::PrintPage);
            }
            IpcMessage::ZoomIn => {
                info!("Zoom in requested");
                let _ = chrome_proxy.send_event(UserEvent::ZoomIn);
            }
            IpcMessage::ZoomOut => {
                info!("Zoom out requested");
                let _ = chrome_proxy.send_event(UserEvent::ZoomOut);
            }
            IpcMessage::ResetZoom => {
                info!("Reset zoom requested");
                let _ = chrome_proxy.send_event(UserEvent::ResetZoom);
            }
            IpcMessage::TabAudioStateChanged { playing } => {
                info!("Tab audio state changed: playing={}", playing);
                let _ = chrome_proxy.send_event(UserEvent::TabAudioStateChanged { playing: *playing });
            }
            IpcMessage::ToggleTabMute { ref id } => {
                info!("Toggle tab mute: {}", id);
                let _ = chrome_proxy.send_event(UserEvent::ToggleTabMute { id: id.clone() });
            }
            IpcMessage::TriggerAutofill => {
                info!("Trigger autofill requested");
                let _ = chrome_proxy.send_event(UserEvent::TriggerAutofill);
            }
            _ => {
                let response = ipc::commands::handle_message(&chrome_state, msg);
                info!("IPC response: {:?}", response);
            }
        },
        Err(e) => {
            error!("Failed to parse IPC: {}", e);
        }
    }
}

/// The shelf's `window.ipc` messages (the command palette at the bottom).
/// A function, not the handler closure, so the RustKit shelf (CH3.A4) can
/// feed it the same messages.
fn handle_shelf_ipc(
    body: &str,
    shelf_proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
    shelf_state: &Arc<Mutex<AppState>>,
) {

    match serde_json::from_str::<IpcMessage>(body) {
        Ok(msg) => {
            match &msg {
                IpcMessage::CollapseShelf => {
                    let _ = shelf_proxy.send_event(UserEvent::CollapseShelf);
                }
                IpcMessage::SearchCommands { ref query } => {
                    let _q = query.clone();
                    let response = ipc::commands::handle_message(shelf_state, msg);
                    if let ipc::IpcResponse::Success { data } = response {
                        let json = serde_json::to_string(&data)
                            .unwrap_or_else(|_| "[]".to_string());
                        let _ = shelf_proxy.send_event(UserEvent::ShowCommands(json));
                    }
                }
                IpcMessage::ExecuteCommand { ref id } => {
                    let _cmd_id = id.clone();
                    let response = ipc::commands::handle_message(shelf_state, msg);
                    info!("Command execution response: {:?}", response);
                    // Close shelf after executing command
                    let _ = shelf_proxy.send_event(UserEvent::CollapseShelf);

                    if let ipc::IpcResponse::Success { data } = response {
                        if let Some(result) = data.get("result") {
                            if let Some(action) =
                                result.get("action").and_then(|a| a.as_str())
                            {
                                match action {
                                    "navigate" => {
                                        if let Some(url) =
                                            result.get("url").and_then(|u| u.as_str())
                                        {
                                            let _ = shelf_proxy.send_event(
                                                UserEvent::Navigate(url.to_string()),
                                            );
                                        } else {
                                            let _ =
                                                shelf_proxy.send_event(UserEvent::NewTab);
                                        }
                                    }
                                    "reload" => {
                                        let _ = shelf_proxy.send_event(UserEvent::Reload);
                                    }
                                    "go_back" => {
                                        let _ = shelf_proxy.send_event(UserEvent::GoBack);
                                    }
                                    "go_forward" => {
                                        let _ =
                                            shelf_proxy.send_event(UserEvent::GoForward);
                                    }
                                    "open_vault" => {
                                        // Vault is now accessed via Settings
                                        let _ =
                                            shelf_proxy.send_event(UserEvent::OpenSettings);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                IpcMessage::Navigate { url } => {
                    let _ = shelf_proxy.send_event(UserEvent::Navigate(url.clone()));
                    let _ = shelf_proxy.send_event(UserEvent::CollapseShelf);
                }
                _ => {}
            }
        }
        Err(e) => {
            error!("Failed to parse Shelf IPC: {}", e);
        }
    }
}

/// The shelf view's second `with_ipc_handler`: find-in-page results. wry
/// keeps the last handler a builder is given, so on the WebKit shelf this
/// is the one that runs and [`handle_shelf_ipc`] never does (CH3.A4 survey).
fn handle_shelf_find_ipc(body: &str, content_proxy: &tao::event_loop::EventLoopProxy<UserEvent>) {
    if let Ok(msg) = serde_json::from_str::<IpcMessage>(body) {
        if let IpcMessage::FindInPageResult { result } = msg {
            let _ = content_proxy.send_event(UserEvent::FindInPageResult(result));
        }
    }
}

/// The Inspector panel's `window.ipc` messages. A function, not the
/// handler closure, so the RustKit Inspector (CH3.A4) can feed it the same
/// messages.
fn handle_inspector_ipc(body: &str, inspector_proxy: &tao::event_loop::EventLoopProxy<UserEvent>) {

    match serde_json::from_str::<IpcMessage>(body) {
        Ok(msg) => {
            match &msg {
                IpcMessage::CloseInspector => {
                    let _ = inspector_proxy.send_event(UserEvent::CloseInspector);
                }
                IpcMessage::InspectorGetDomTree | IpcMessage::InspectorRefresh => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorRefreshTree);
                }
                IpcMessage::InspectorGetElementStyles { path } | IpcMessage::InspectorSelectElement { path } => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorSelect { path: path.clone() });
                }
                IpcMessage::InspectorExpandNode { path } => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorExpand { path: path.clone() });
                }
                IpcMessage::InspectorHighlightElement { path } => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorHighlightElement { path: path.clone() });
                }
                IpcMessage::InspectorClearHighlight => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorClearHighlight);
                }
                IpcMessage::InspectorStartPicker => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorStartPicker);
                }
                IpcMessage::InspectorStopPicker => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorStopPicker);
                }
                IpcMessage::InspectorScrollToElement { path } => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorScrollToElement { path: path.clone() });
                }
                IpcMessage::InspectorConsoleEval { code } => {
                    let _ = inspector_proxy.send_event(UserEvent::InspectorConsoleEval { code: code.clone() });
                }
                IpcMessage::InspectorClose => {
                    let _ = inspector_proxy.send_event(UserEvent::CloseInspector);
                }
                _ => {}
            }
        }
        Err(e) => {
            error!("Failed to parse Inspector IPC: {}", e);
        }
    }
}

/// The settings window's `window.ipc` messages. A function, not the
/// handler closure, so the RustKit settings view (CH3.A4) can feed it the
/// same messages.
fn handle_settings_ipc(body: &str, settings_proxy: &tao::event_loop::EventLoopProxy<UserEvent>) {
    if let Ok(ipc_msg) = serde_json::from_str::<IpcMessage>(body) {
        match ipc_msg {
            IpcMessage::CloseSettings => {
                let _ = settings_proxy.send_event(UserEvent::CloseSettings);
            }
            IpcMessage::GetBrowserProfiles { browser } => {
                let _ = settings_proxy.send_event(UserEvent::GetBrowserProfiles(browser));
            }
            IpcMessage::ImportBookmarks { browser, profile_path } => {
                let _ = settings_proxy.send_event(UserEvent::ImportBookmarks { browser, profile_path });
            }
            IpcMessage::GetSettings => {
                let _ = settings_proxy.send_event(UserEvent::GetSettings);
            }
            IpcMessage::SaveSettings { settings } => {
                let _ = settings_proxy.send_event(UserEvent::SaveSettings(settings));
            }
            IpcMessage::ExportData { include_settings, include_workspaces } => {
                let _ = settings_proxy.send_event(UserEvent::ExportData { include_settings, include_workspaces });
            }
            IpcMessage::SaveExportToFile { data, suggested_name } => {
                let _ = settings_proxy.send_event(UserEvent::SaveExportToFile { data, suggested_name });
            }
            IpcMessage::ImportData { data, replace } => {
                let _ = settings_proxy.send_event(UserEvent::ImportStateData { data, replace });
            }
            IpcMessage::PickImportFile => {
                let _ = settings_proxy.send_event(UserEvent::PickImportFile);
            }
            IpcMessage::ClearBrowsingData { history, downloads, shelf } => {
                let _ = settings_proxy.send_event(UserEvent::ClearBrowsingData { history, downloads, shelf });
            }
            IpcMessage::GetCellar => {
                let _ = settings_proxy.send_event(UserEvent::GetCellar);
            }
            IpcMessage::RestoreFromCellar { id } => {
                let _ = settings_proxy.send_event(UserEvent::RestoreFromCellar { id });
            }
            IpcMessage::ClearCellar => {
                let _ = settings_proxy.send_event(UserEvent::ClearCellar);
            }
            IpcMessage::GetFocusModeConfig => {
                let _ = settings_proxy.send_event(UserEvent::GetFocusModeConfigForSettings);
            }
            IpcMessage::SaveFocusModeConfig { config } => {
                let _ = settings_proxy.send_event(UserEvent::SaveFocusModeConfigFromSettings(config));
            }
            IpcMessage::AddToFocusBlocklist { domain } => {
                let _ = settings_proxy.send_event(UserEvent::AddToFocusBlocklistFromSettings(domain));
            }
            IpcMessage::RemoveFromFocusBlocklist { domain } => {
                let _ = settings_proxy.send_event(UserEvent::RemoveFromFocusBlocklistFromSettings(domain));
            }
            // Vault IPC commands
            IpcMessage::GetVaultStatus => {
                let _ = settings_proxy.send_event(UserEvent::GetVaultStatus);
            }
            IpcMessage::UnlockVault { password } => {
                let _ = settings_proxy.send_event(UserEvent::UnlockVault(password));
            }
            IpcMessage::LockVault => {
                let _ = settings_proxy.send_event(UserEvent::LockVault);
            }
            IpcMessage::GetAllCredentials => {
                let _ = settings_proxy.send_event(UserEvent::GetAllCredentials);
            }
            IpcMessage::SaveCredential { url, username, password } => {
                let _ = settings_proxy.send_event(UserEvent::SaveCredential { url, username, password });
            }
            IpcMessage::DeleteCredential { id } => {
                let _ = settings_proxy.send_event(UserEvent::DeleteCredential(id));
            }
            // Analytics IPC commands
            IpcMessage::GetAnalyticsSettings => {
                let _ = settings_proxy.send_event(UserEvent::GetAnalyticsSettings);
            }
            IpcMessage::UpdateAnalyticsSettings { enabled, retention_days, weekly_report, report_day } => {
                let _ = settings_proxy.send_event(UserEvent::UpdateAnalyticsSettings { enabled, retention_days, weekly_report, report_day });
            }
            IpcMessage::ClearAnalyticsData => {
                let _ = settings_proxy.send_event(UserEvent::ClearAnalyticsData);
            }
            IpcMessage::ExportAnalyticsData { format } => {
                let _ = settings_proxy.send_event(UserEvent::ExportAnalyticsData { format });
            }
            _ => {
                info!("Settings window received unhandled IPC: {:?}", ipc_msg);
            }
        }
    }
}


fn sync_tabs_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi) {
    if let Ok(s) = state.lock() {
        let active_tab_id = s.shell.get_active_tab().map(|t| t.id);
        let workspace_id = s.shell.get_active_workspace().map(|w| w.id);
        let decay_days = s.user_settings.tab_decay_days;
        let tabs: Vec<serde_json::Value> = s
            .shell
            .list_tabs(workspace_id)
            .iter()
            .map(|tab| {
                let decay_level = hiwave_shell::BrowserShell::calculate_decay_level(
                    tab.last_visited,
                    decay_days,
                );
                serde_json::json!({
                    "id": tab.id.0,
                    "title": tab.title.clone().unwrap_or_else(|| "New Tab".to_string()),
                    "url": tab.url.to_string(),
                    "is_active": Some(tab.id) == active_tab_id,
                    "locked": tab.locked,
                    "last_visited": tab.last_visited,
                    "decay_level": decay_level
                })
            })
            .collect();

        let tabs_json = serde_json::to_string(&tabs).unwrap_or_else(|_| "[]".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.renderTabs({}); }}",
            tabs_json
        );
        let _ = chrome.eval(&script);
    }
}

fn sync_workspaces_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi) {
    if let Ok(s) = state.lock() {
        let active_ws_id = s.shell.get_active_workspace().map(|w| w.id);
        let active_tab_id = s.shell.get_active_tab().map(|t| t.id);
        let decay_days = s.user_settings.tab_decay_days;
        let workspaces: Vec<serde_json::Value> = s.shell.list_workspaces()
            .iter()
            .map(|ws| {
                let tabs: Vec<serde_json::Value> = ws.tabs.iter().filter_map(|tab_id| {
                    s.shell.get_tab(*tab_id).map(|tab| {
                        let decay_level = hiwave_shell::BrowserShell::calculate_decay_level(
                            tab.last_visited,
                            decay_days,
                        );
                        serde_json::json!({
                            "id": tab.id.0.to_string(),
                            "title": tab.title.clone().unwrap_or_else(|| "New Tab".to_string()),
                            "url": tab.url.to_string(),
                            "is_active": active_tab_id == Some(tab.id),
                            "locked": tab.locked,
                            "decay_level": decay_level,
                        })
                    })
                }).collect();
                serde_json::json!({
                    "id": ws.id.0.to_string(),
                    "name": ws.name,
                    "tab_count": ws.tabs.len(),
                    "tabs": tabs,
                    "is_active": Some(ws.id) == active_ws_id,
                    "is_suspended": ws.suspended,
                    "locked_count": s.workspace_locked_count(ws.id),
                })
            })
            .collect();

        let ws_json = serde_json::to_string(&workspaces).unwrap_or_else(|_| "[]".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.renderWorkspaces({}); }}",
            ws_json
        );
        let _ = chrome.eval(&script);
    }
}

fn sync_shield_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi) {
    if let Ok(s) = state.lock() {
        let stats = s.shield.get_stats();
        let payload = serde_json::json!({
            "enabled": s.shield.is_enabled(),
            "requests_blocked": stats.requests_blocked,
            "trackers_blocked": stats.trackers_blocked
        });
        let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.updateShieldStats({}); }}",
            json
        );
        let _ = chrome.eval(&script);
    }
}

fn sync_shelf_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi, scope: ShelfScope) {
    if let Ok(s) = state.lock() {
        let (items, workspace_name) = match scope {
            ShelfScope::All => (s.shelf_items_all(), None),
            ShelfScope::Workspace => (
                s.shelf_items_for_active_workspace(),
                s.shell.get_active_workspace().map(|ws| ws.name.clone()),
            ),
        };

        let payload = serde_json::json!({
            "scope": scope.as_str(),
            "workspace": workspace_name,
            "items": items,
            "global_count": s.shelf_count_all(),
        });
        let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.renderShelf({}); }}",
            json
        );
        let _ = chrome.eval(&script);
    }
}

fn sync_blocklist_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi) {
    if let Ok(s) = state.lock() {
        let domains: Vec<&String> = s.user_blocklist.domains.iter().collect();
        let json = serde_json::to_string(&domains).unwrap_or_else(|_| "[]".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.updateBlocklist({}); }}",
            json
        );
        let _ = chrome.eval(&script);
    }
}

fn sync_downloads_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi) {
    if let Ok(s) = state.lock() {
        let payload = serde_json::json!({
            "downloads": s.downloads_snapshot(),
            "active_count": s.active_download_count(),
        });
        let json = serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.updateDownloads({}); }}",
            json
        );
        let _ = chrome.eval(&script);
    }
}

fn sync_history_to_chrome(state: &Arc<Mutex<AppState>>, chrome: &impl ChromeUi) {
    if let Ok(s) = state.lock() {
        let history = s.visit_history_snapshot();
        let json = serde_json::to_string(&history).unwrap_or_else(|_| "[]".to_string());
        let script = format!(
            "if(window.hiwaveChrome) {{ hiwaveChrome.updateHistory({}); }}",
            json
        );
        let _ = chrome.eval(&script);
    }
}

fn load_new_tab(content: &impl ContentWebViewOps) {
    let _ = content.load_html(ABOUT_HTML);
}

fn load_about_page(content: &impl ContentWebViewOps) {
    let _ = content.load_html(ABOUT_HTML);
}

fn load_report_page(content: &impl ContentWebViewOps) {
    // Embed Chart.js library inline in the report HTML
    let report_with_chartjs = REPORT_HTML.replace(
        "<!-- Chart.js is injected by Rust before page load -->",
        &format!("<script>{}</script>", CHART_JS)
    );
    let _ = content.load_html(&report_with_chartjs);
}

/// True for the first wheel event after a quiet gap, so a flick is visible in
/// a default-level session log without flooding it (a trackpad flick emits
/// ~50 events/s; the rest stay trace). One clock for both wheel paths, the
/// window loop's and the content view's. Restores the diagnosability #100
/// accidentally removed when the #95 diagnostic line was replaced with wiring.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn wheel_burst_started() -> bool {
    // `None` until the first wheel event: a clock started at that event made
    // the first burst of a session look like the middle of one, and it was
    // never logged.
    thread_local! {
        static LAST_WHEEL: std::cell::Cell<Option<std::time::Instant>> =
            const { std::cell::Cell::new(None) };
    }
    LAST_WHEEL.with(|t| {
        let now = std::time::Instant::now();
        t.replace(Some(now))
            .is_none_or(|last| now.duration_since(last) > std::time::Duration::from_millis(500))
    })
}

/// The browser's context menu over the page (CH2.E4), shown where the
/// page let a right press through: Back, Forward and Reload, the items of
/// WebKit's default menu on a page (the content view was WebKit before
/// RustKit). Each item is a main-menu id, so the menu-event arm runs it.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn show_page_context_menu(view: &webview_rustkit::RustKitView, x: f32, y: f32) {
    use crate::webview::IWebContent as _;
    use muda::{ContextMenu as _, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    let items: [&dyn muda::IsMenuItem; 4] = [
        &MenuItem::with_id(menu_ids::GO_BACK, "Back", view.can_go_back(), None),
        &MenuItem::with_id(menu_ids::GO_FORWARD, "Forward", view.can_go_forward(), None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(menu_ids::RELOAD, "Reload Page", true, None),
    ];
    if let Err(e) = menu.append_items(&items) {
        warn!(error = %e, "context menu not built");
        return;
    }
    info!(x, y, "page context menu shown");
    let at = muda::dpi::Position::Logical(muda::dpi::LogicalPosition::new(x as f64, y as f64));
    // SAFETY: the content view is a live NSView in the window.
    unsafe { menu.show_context_menu_for_nsview(view.host_view() as *const std::ffi::c_void, Some(at)) };
}

/// `wheel_burst_started` for a wheel on `target`: the page's only. The
/// h6 check reads the burst line after its first wheel over the page; a
/// wheel over the RustKit chrome or a panel must not take it (#907 R1 nit 4).
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn wheel_starts_burst(target: InputTarget) -> bool {
    target == InputTarget::Page && wheel_burst_started()
}

/// Which RustKit view an input event is for: the page, or the browser's
/// own chrome (`HIWAVE_RUSTKIT_CHROME=1`, CH3.A2).
#[cfg(all(target_os = "macos", feature = "rustkit"))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum InputTarget {
    Page,
    Chrome,
    /// The shelf, the Inspector or the settings page as RustKit views
    /// (CH3.A4): like the chrome, not the page.
    Panel,
}

/// One input event for a RustKit view, as drained from the viewhost queue:
/// the pointer, the wheel and keys. Lifted from the event loop's drain
/// (CH3.A2 step 3b) so the RustKit chrome's input takes the same path.
/// The chrome is not the page: the Inspector's element picker, a link's or
/// a form's navigation and the keys that scroll are the page's only.
#[cfg(all(target_os = "macos", feature = "rustkit"))]
fn dispatch_view_input(
    view: &webview_rustkit::RustKitView,
    target: InputTarget,
    kind: rustkit_viewhost::InputKind,
    devtools: &mut devtools_panel::Devtools,
    inspector_for_events: &dyn ChromeUi,
    click_proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
) {
    match kind {
        rustkit_viewhost::InputKind::Pointer(click) => {
            // The page hears the click before the browser
            // acts on it: mousedown on press; mouseup and click
            // on release, and only then focus and the link,
            // which a listener's preventDefault() cancels. Until
            // 2026-10-03 no event reached the page at all, so
            // every script-driven control was dead (Z lane I0).
            // Where the pointer is, and when it goes: what a
            // hover menu or a drag listens for.
            // The Inspector's select-element mode takes the
            // pointer instead: hover outlines, release picks.
            if target == InputTarget::Page && devtools.picking() {
                let at = (click.x as f32, click.y as f32);
                match click.input {
                    rustkit_viewhost::PointerInput::Move => {
                        devtools.pick_hover(view, inspector_for_events, Some(at))
                    }
                    rustkit_viewhost::PointerInput::Leave => {
                        devtools.pick_hover(view, inspector_for_events, None)
                    }
                    rustkit_viewhost::PointerInput::Button if !click.down => {
                        devtools.pick_at(view, inspector_for_events, at.0, at.1)
                    }
                    rustkit_viewhost::PointerInput::Button | rustkit_viewhost::PointerInput::Secondary => {}
                }
                return;
            }
            match click.input {
                rustkit_viewhost::PointerInput::Move => {
                    view.mouse_move_at_point(click.x as f32, click.y as f32);
                    return;
                }
                rustkit_viewhost::PointerInput::Leave => {
                    view.mouse_leave();
                    return;
                }
                // CH2.E4: `contextmenu`; the browser's menu, if the page
                // lets it through, is shown after the drain.
                rustkit_viewhost::PointerInput::Secondary => {
                    if click.down {
                        view.secondary_down(click.x as f32, click.y as f32);
                    } else {
                        view.secondary_up(click.x as f32, click.y as f32);
                    }
                    return;
                }
                rustkit_viewhost::PointerInput::Button => {}
            }
            if click.down {
                view.mouse_down_with(click.x as f32, click.y as f32, click.clicks, click.shift);
                return;
            }
            match target {
                InputTarget::Page => info!(x = click.x, y = click.y, "content click (view-local)"),
                InputTarget::Chrome => info!(x = click.x, y = click.y, "chrome click (view-local)"),
                InputTarget::Panel => info!(x = click.x, y = click.y, "panel click (view-local)"),
            }
            let outcome = view.click_at_point(click.x as f32, click.y as f32);
            // ENGINE focus is not APPKIT focus: without making
            // the content view the window's first responder,
            // macOS keeps delivering keys to the chrome WebView
            // — observed live as "text entry goes back up to
            // the URL bar". Any click on the page takes the
            // keyboard, focused element or not: the page's own
            // shortcuts are keys with nothing focused.
            view.grab_keyboard();
            if let Some(tag) = outcome.focused {
                info!(%tag, "Focused content element");
                view.relayout();
            }
            if let Some(url) = outcome.navigate.filter(|_| target == InputTarget::Page) {
                info!(%url, "Link clicked");
                let _ = click_proxy.send_event(UserEvent::Navigate(url));
            }
        }
        // The wheel, from the content view's own `scrollWheel:`.
        // The window-loop MouseWheel arm above stays as it was;
        // the view consumes the wheel, so only one of the two
        // fires for an event. Until 2026-10-06 the wheel reached
        // neither in the built app (hand-test item H6). A burst
        // arrives summed by the queue, pointer drift between
        // its steps included, so a drain costs one scroll per
        // burst and one present (#692), not one per step.
        rustkit_viewhost::InputKind::Wheel(scroll) => {
            if wheel_starts_burst(target) {
                info!(
                    dx = scroll.dx,
                    dy = scroll.dy,
                    "wheel burst started (content view)"
                );
            }
            if view.scroll_at(
                scroll.x as f32,
                scroll.y as f32,
                scroll.dx as f32,
                scroll.dy as f32,
            ) {
                trace!(dx = scroll.dx, dy = scroll.dy, "content scrolled");
            }
        }
        // A Cmd chord the content view took before the menu (CH2.E3): the
        // page hears its keydown (metaKey) first, as in Chromium; the page
        // answers whether it claimed it, and one it did not goes on to the
        // menu (`rustkit_viewhost::finish_key_equivalent`). Chromium's
        // reserved chords (Cmd+T/N/W/Q, ...) never reach here.
        rustkit_viewhost::InputKind::KeyEquivalent { key, token } => {
            let (vk, text) = mac_key_to_vk(&key);
            info!(keycode = key.mac_keycode, token, "Cmd chord to the page first");
            view.handle_command_key(vk, text, key.shift, key.alt, token);
        }
        // Marked and committed text from the input method: a dead key, an
        // IME, the accent popover, the emoji picker (CH2.E3).
        rustkit_viewhost::InputKind::Ime(input) => {
            tracing::debug!(input = %ime_input_log_line(&input), "input method text");
            view.ime_input(input);
            view.relayout();
        }
        // An Edit menu item (CH2.E3): the command in the focused text field.
        rustkit_viewhost::InputKind::Edit(action) => {
            use rustkit_engine::EditCommand;
            let command = match action {
                rustkit_viewhost::EditAction::Copy => EditCommand::Copy,
                rustkit_viewhost::EditAction::Cut => EditCommand::Cut,
                rustkit_viewhost::EditAction::Paste => EditCommand::Paste,
                rustkit_viewhost::EditAction::Undo => EditCommand::Undo,
                rustkit_viewhost::EditAction::Redo => EditCommand::Redo,
                rustkit_viewhost::EditAction::SelectAll => EditCommand::SelectAll,
            };
            info!(?action, "Edit menu action");
            view.edit_command(command);
            view.relayout();
        }
        rustkit_viewhost::InputKind::Key(key) => {
            // Cmd chords are menu shortcuts, except the editing
            // ones in a focused text field (CH2.E1): Cmd+A/C/V/X/Z
            // (Shift+Z redo), Cmd+arrows and Cmd+Backspace. A
            // focused link, button or <select> is not one: its
            // Cmd chords stay the menu's (Cmd+arrows Back/Forward).
            // The engine reads Cmd as its command modifier (`ctrl`).
            let edit_chord = key.cmd
                && view.has_focused_text_control()
                && (matches!(
                    key.text.to_ascii_lowercase().as_str(),
                    "a" | "c" | "v" | "x" | "z"
                ) || matches!(key.mac_keycode, 51 | 117 | 123 | 124 | 125 | 126));
            if key.cmd && !edit_chord {
                return; // menu shortcuts are not text
            }
            // Esc leaves select-element mode; the page does not hear it.
            if target == InputTarget::Page && devtools.picking() && key.mac_keycode == 53 {
                if !key.up {
                    devtools.stop_picker(view, inspector_for_events);
                }
                return;
            }
            let (vk, text) = mac_key_to_vk(&key);
            if vk == 0 && text.is_empty() {
                return;
            }
            if key.up {
                view.handle_key_up(vk, text, key.ctrl, key.shift, key.alt);
                return;
            }
            if vk == 0x0D && target == InputTarget::Page {
                if let Some(url) = view.form_submit_url() {
                    info!(%url, "Form submitted");
                    let _ = click_proxy.send_event(UserEvent::Navigate(url));
                    return;
                }
            }
            let focused = view.has_focused_element();
            if view.handle_text_key(vk, text, key.ctrl || key.cmd, key.shift, key.alt) {
                view.relayout();
            } else if !focused && target == InputTarget::Page {
                // Nothing focused and the page did not take
                // the key: the keys that scroll, as in the
                // window-level arm (wheel sign convention).
                let dy: Option<f32> = match (vk, text) {
                    (0x28, _) => Some(-40.0),
                    (0x26, _) => Some(40.0),
                    (0x22, _) | (0, " ") => Some(-600.0),
                    (0x21, _) => Some(600.0),
                    (0x23, _) => Some(-f32::MAX),
                    (0x24, _) => Some(f32::MAX),
                    _ => None,
                };
                if let Some(dy) = dy {
                    view.scroll_by(0.0, dy);
                }
            }
        }
    }
}

/// macOS keyCodes -> the Win32 VK numbering the edit model speaks. Text
/// keys carry their characters.
fn mac_key_to_vk(key: &rustkit_viewhost::PendingKey) -> (u32, &str) {
    match key.mac_keycode {
        51 => (0x08, ""),      // delete (backspace)
        117 => (0x2E, ""),     // forward delete
        123 => (0x25, ""),     // left
        124 => (0x27, ""),     // right
        115 => (0x24, ""),     // home
        119 => (0x23, ""),     // end
        36 | 76 => (0x0D, ""), // return / keypad enter
        48 => (0x09, ""),      // tab
        53 => (0x1B, ""),      // escape
        116 => (0x21, ""),     // page up
        121 => (0x22, ""),     // page down
        126 => (0x26, ""),     // up
        125 => (0x28, ""),     // down
        _ => (0, key.text.as_str()),
    }
}

#[cfg(all(test, target_os = "macos", feature = "rustkit"))]
mod panel_tests {
    use super::*;
    use rustkit_viewhost::ViewId;

    const DOC: &str = "<html><head><title>t</title></head><body><p>x</p></body></html>";

    #[test]
    fn a_panel_document_gets_the_bridge_first_in_its_head_and_the_tail_last_in_its_body() {
        let out = html_with_bridge(DOC, "TAIL()");
        assert!(out.starts_with(&format!("<html><head><script>{}</script><title>", JS_BRIDGE)));
        assert!(out.ends_with("<p>x</p><script>TAIL()</script></body></html>"), "{out}");
    }

    #[test]
    fn no_tail_adds_no_script_to_the_body() {
        let out = html_with_bridge(DOC, "");
        assert!(out.ends_with("<title>t</title></head><body><p>x</p></body></html>"), "{out}");
        assert_eq!(out.matches("<script>").count(), 1);
    }

    #[test]
    fn the_chrome_document_is_the_bridge_and_the_outside_click_helper() {
        assert_eq!(chrome_html_with_bridge(), html_with_bridge(CHROME_HTML, CHROME_OUTSIDE_CLICK));
        // CH3.A3b: no popup reporter; the region comes from the frames.
        assert!(!chrome_html_with_bridge().contains("chrome_overlays"));
    }

    /// CH3.A7: the chrome is loaded into an app-owned view and keeps its
    /// bridge (its startup `get_sidebar_width` reaches the window); a
    /// content view, built as the app builds it, is not app-owned.
    #[test]
    fn the_chrome_loads_into_an_app_owned_view_and_round_trips_get_sidebar_width() {
        use std::time::{Duration, Instant};
        let content = webview_rustkit::RustKitView::headless_content(rustkit_viewhost::Bounds::new(0, 0, 800, 600));
        assert!(!content.is_app_owned(), "a content view must not be app-owned");
        let chrome = webview_rustkit::RustKitView::headless_content(rustkit_viewhost::Bounds::new(0, 0, 1200, 120));
        load_app_document(&chrome, &chrome_html_with_bridge()).unwrap();
        assert!(chrome.is_app_owned());
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut got = Vec::new();
        while !got.iter().any(|m: &String| m.contains(r#""cmd":"get_sidebar_width""#)) {
            assert!(Instant::now() < deadline, "the chrome posted {got:?}");
            if !chrome.step_load() {
                chrome.process_events();
            }
            got.extend(chrome.take_ipc_messages());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn input_goes_to_the_view_it_was_recorded_on() {
        let (page, chrome, shelf, settings) = (ViewId::new(), ViewId::new(), ViewId::new(), ViewId::new());
        // The Inspector is a WebKit view here: no id.
        let panels = [Some(shelf), None, Some(settings)];
        assert_eq!(route_input(chrome, Some(chrome), &panels), InputRoute::Chrome);
        assert_eq!(route_input(shelf, Some(chrome), &panels), InputRoute::Panel(0));
        assert_eq!(route_input(settings, None, &panels), InputRoute::Panel(2));
        assert_eq!(route_input(page, Some(chrome), &panels), InputRoute::Page);
        assert_eq!(route_input(page, None, &[None, None, None]), InputRoute::Page);
    }
}

// #910 R1: what the log carries of a RustKit panel's messages: shapes only,
// never a body, page text, a field's value or a URL's query.
#[cfg(test)]
mod panel_log_tests {
    use super::*;

    const SECRET: &str = "hunter2-a4-secret";

    #[test]
    fn a_panel_ipc_line_names_the_cmd_and_size_never_the_body() {
        let bodies = [
            format!(r#"{{"cmd":"unlock_vault","password":"{SECRET}"}}"#),
            format!(r#"{{"cmd":"save_credential","url":"https://a.test/login?next={SECRET}","username":"{SECRET}","password":"{SECRET}"}}"#),
            format!(r#"{{"cmd":"inspector_console_eval","code":"{SECRET}"}}"#),
            format!(r#"{{"cmd":"search_commands","query":"{SECRET}"}}"#),
        ];
        for body in &bodies {
            let line = panel_ipc_log_line("settings", body);
            assert!(!line.contains(SECRET), "{line}");
            assert!(!line.contains("password") && !line.contains("https"), "{line}");
            assert!(line.contains(&format!("len={}", body.len())), "{line}");
        }
        assert_eq!(
            panel_ipc_log_line("settings", &bodies[0]),
            format!("panel=\"settings\" cmd=unlock_vault len={}", bodies[0].len())
        );
        // Not JSON, or a cmd that is not a command name: `?`.
        let line = panel_ipc_log_line("shelf", SECRET);
        assert_eq!(line, format!("panel=\"shelf\" cmd=? len={}", SECRET.len()));
        let line = panel_ipc_log_line("shelf", &format!(r#"{{"cmd":"{SECRET} x"}}"#));
        assert!(line.contains("cmd=?") && !line.contains(SECRET), "{line}");
    }

    #[test]
    fn a_probe_report_in_the_log_is_shapes_only() {
        let body = format!(
            r##"{{"cmd":"panel_probe","size":[800,280],"text":"{SECRET}","boxes":{{"#vaultPassword":{{"box":[1,2,3,4],"n":1,"checked":null,"len":17,"value":"{SECRET}"}},"#cb":{{"box":[5,6,7,8],"n":1,"checked":true,"len":2}},"#gone":null,"#{SECRET}":{{"box":[0,0,0,0],"n":1}}}}}}"##
        );
        let line = panel_probe_log_line(&body, &["#vaultPassword", "#cb", "#gone"]).expect("a report");
        assert!(!line.contains(SECRET), "{line}");
        assert!(!line.contains("text") && !line.contains("value"), "{line}");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["size"], serde_json::json!([800, 280]));
        assert_eq!(v["boxes"]["#vaultPassword"], serde_json::json!({"box": [1, 2, 3, 4], "n": 1, "checked": null, "len": 17}));
        assert_eq!(v["boxes"]["#cb"]["checked"], serde_json::json!(true));
        assert_eq!(v["boxes"]["#gone"], serde_json::Value::Null);
        // A text where a shape belongs is dropped, not logged.
        let body = format!(r##"{{"cmd":"panel_probe","size":["{SECRET}",1],"boxes":{{"#cb":{{"box":["{SECRET}"],"len":"{SECRET}"}}}}}}"##);
        let line = panel_probe_log_line(&body, &["#cb"]).expect("a report");
        assert!(!line.contains(SECRET), "{line}");
        assert_eq!(panel_probe_log_line(SECRET, &["#cb"]), None);
    }

    #[test]
    fn a_url_in_the_log_has_no_query_or_fragment() {
        assert_eq!(url_for_log(&format!("https://a.test/p/q?x={SECRET}#{SECRET}")), "https://a.test/p/q");
        assert_eq!(url_for_log(&format!("https://a.test/p#{SECRET}")), "https://a.test/p");
        assert_eq!(url_for_log("about:blank"), "about:blank");
    }
}

// #918 R1 HOLD must-fix 2: input method text (a dead key's, an IME's, the
// accent popover's, the emoji picker's; in a password field too) is never
// logged, at any level: the variant and the size only.
#[cfg(all(test, target_os = "macos", feature = "rustkit"))]
mod ime_log_tests {
    use super::ime_input_log_line;
    use rustkit_viewhost::ImeInput;

    const SECRET: &str = "hunter2-e3-secret";

    #[test]
    fn an_input_method_line_names_the_variant_and_size_never_the_text() {
        let marked = ime_input_log_line(&ImeInput::SetMarked(SECRET.to_string()));
        assert!(!marked.contains(SECRET), "{marked}");
        assert_eq!(marked, format!("ime=SetMarked len={}", SECRET.len()));
        let insert = ime_input_log_line(&ImeInput::Insert { text: SECRET.to_string(), replace: Some((1, 1)) });
        assert!(!insert.contains(SECRET), "{insert}");
        assert_eq!(insert, format!("ime=Insert len={} replace=true", SECRET.len()));
        assert_eq!(ime_input_log_line(&ImeInput::Unmark), "ime=Unmark");
    }
}

// The real-window driver's h6 check reads "wheel burst started" from the app
// log after the first wheel it sends to a freshly launched app.
#[cfg(all(test, target_os = "macos", feature = "rustkit"))]
mod wheel_burst_tests {
    use super::wheel_burst_started;

    #[test]
    fn the_first_wheel_after_launch_starts_a_burst_and_the_next_event_does_not() {
        // The clock is per thread, so this test thread is a fresh launch.
        assert!(wheel_burst_started(), "the first wheel event after launch starts a burst");
        assert!(!wheel_burst_started(), "an event right behind it belongs to the same burst");
    }

    #[test]
    fn a_wheel_over_the_chrome_or_a_panel_leaves_the_page_its_burst() {
        use super::{wheel_starts_burst, InputTarget};
        // A fresh thread: a fresh launch's clock.
        std::thread::spawn(|| {
            assert!(!wheel_starts_burst(InputTarget::Chrome), "the chrome's wheel is not the page's burst");
            assert!(!wheel_starts_burst(InputTarget::Panel), "nor a panel's");
            assert!(wheel_starts_burst(InputTarget::Page), "the page's first wheel still starts one");
            assert!(!wheel_starts_burst(InputTarget::Page));
        })
        .join()
        .unwrap();
    }
}

fn is_new_tab_url(url: &str) -> bool {
    url == "about:blank" || url == NEW_TAB_URL || url.starts_with("data:text/html")
}

fn is_about_url(url: &str) -> bool {
    url == ABOUT_URL
}

fn is_report_url(url: &str) -> bool {
    url == REPORT_URL
}

fn should_record_history_url(url: &str) -> bool {
    if is_new_tab_url(url) || is_about_url(url) || is_report_url(url) {
        return false;
    }
    let lower = url.to_lowercase();
    if lower.starts_with("about:")
        || lower.starts_with("data:")
        || lower.starts_with("javascript:")
        || lower.starts_with("hiwave:")
    {
        return false;
    }
    true
}

// Popup filtering is now handled by platform::is_definitely_popup()
// which provides comprehensive cross-platform URL blocking

// CH3.A4b and the #907 R1 nits: lazy panels, IPC log lines, the eval hook.
#[cfg(test)]
mod lazy_panel_tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A panel view that records what the app did with it.
    struct Fake(Rc<RefCell<Vec<String>>>);

    impl ChromeUi for Fake {
        fn eval(&self, js: &str) -> Result<(), String> {
            self.0.borrow_mut().push(format!("eval {js}"));
            Ok(())
        }
        fn place(&self, rect: Rect) {
            let h = rect.size.to_logical::<f64>(1.0).height;
            self.0.borrow_mut().push(format!("place {h}"));
        }
    }

    fn rect(height: u32) -> Rect {
        Rect {
            position: LogicalPosition::new(0, 600).into(),
            size: LogicalSize::new(800, height).into(),
        }
    }

    #[test]
    fn a_deferred_panel_is_built_when_it_first_opens_and_kept_after_it_closes() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let builds = Rc::new(RefCell::new(0));
        let shelf: LazyPanel<Fake> = LazyPanel::deferred("<p>shelf</p>", "shelf");
        let build = |log: &Rc<RefCell<Vec<String>>>, builds: &Rc<RefCell<i32>>| {
            let (log, builds) = (Rc::clone(log), Rc::clone(builds));
            move |_: Rect, html: &'static str, name: &'static str| {
                assert_eq!((html, name), ("<p>shelf</p>", "shelf"));
                *builds.borrow_mut() += 1;
                Some(Fake(log))
            }
        };
        // Launch: the layout places the panels closed (0 pt tall), and the
        // app sends a hidden panel scripts.
        shelf.lay_out(rect(SHELF_HEIGHT_DEFAULT), build(&log, &builds));
        let _ = shelf.eval("showCommands([])");
        shelf.take_keyboard();
        assert!(shelf.get().is_none(), "no panel view after launch");
        assert_eq!(*builds.borrow(), 0);
        // expand_shelf: the layout gives it its height.
        shelf.lay_out(rect(SHELF_HEIGHT_EXPANDED), build(&log, &builds));
        assert!(shelf.get().is_some(), "the open command built the panel view");
        assert_eq!(*builds.borrow(), 1);
        assert_eq!(
            *log.borrow(),
            vec!["eval showCommands([])".to_string(), format!("place {}", SHELF_HEIGHT_EXPANDED)],
            "it hears what it was sent while not built, then takes its place"
        );
        // Closed and opened again: the same view, not a second one.
        shelf.lay_out(rect(SHELF_HEIGHT_DEFAULT), build(&log, &builds));
        shelf.lay_out(rect(SHELF_HEIGHT_EXPANDED), build(&log, &builds));
        assert_eq!(*builds.borrow(), 1, "kept after close");
        assert_eq!(log.borrow().last().map(String::as_str), Some("place 280"));
    }

    #[test]
    fn a_panel_built_at_launch_is_placed_as_before() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let panel = LazyPanel::built(Fake(Rc::clone(&log)), "", "Inspector");
        panel.lay_out(rect(0), |_, _, _| panic!("a built panel is not built again"));
        let _ = panel.eval("x()");
        assert_eq!(*log.borrow(), vec!["place 0".to_string(), "eval x()".to_string()]);
    }

    #[test]
    fn a_panel_not_built_keeps_only_its_last_scripts() {
        let panel: LazyPanel<Fake> = LazyPanel::deferred("", "Inspector");
        for i in 0..LAZY_PANEL_HELD_MAX + 10 {
            let _ = panel.eval(&format!("line({i})"));
        }
        let held = panel.held.borrow();
        assert_eq!(held.len(), LAZY_PANEL_HELD_MAX);
        assert_eq!(held.front().map(String::as_str), Some("line(10)"));
    }

    #[test]
    fn an_ipc_log_line_names_the_cmd_not_the_body() {
        let body = r#"{"cmd":"navigate","url":"https://example.test/a?q=secret"}"#;
        assert_eq!(ipc_cmd(body), "navigate");
        assert_eq!(ipc_cmd(r#"{"cmd":"find_in_page","query":"secret"}"#), "find_in_page");
        assert_eq!(ipc_cmd("not json, secret"), "?");
        assert_eq!(ipc_cmd(r#"{"url":"secret"}"#), "?");
    }

    #[test]
    fn the_chrome_eval_hook_needs_the_test_hooks_switch() {
        let js = || Some("sendIpc('x')".to_string());
        assert_eq!(chrome_boot_eval(true, None, js()), None, "flag on, no test hooks: not run");
        assert_eq!(chrome_boot_eval(true, Some("0"), js()), None);
        assert_eq!(chrome_boot_eval(false, Some("1"), js()), None, "flag off: not run");
        assert_eq!(chrome_boot_eval(true, Some("1"), Some(String::new())), None);
        assert_eq!(chrome_boot_eval(true, Some("1"), js()), js());
    }
}

#[cfg(test)]
mod rust_log_tests {
    use super::rust_log_filter;
    use tracing_subscriber::filter::LevelFilter;

    #[test]
    fn defaults_to_info_and_honors_a_rust_log_directive() {
        assert_eq!(rust_log_filter(None).max_level_hint(), Some(LevelFilter::INFO));
        assert_eq!(rust_log_filter(Some("")).max_level_hint(), Some(LevelFilter::INFO));
        assert_eq!(rust_log_filter(Some("   ")).max_level_hint(), Some(LevelFilter::INFO));
        assert_eq!(rust_log_filter(Some("===bad===")).max_level_hint(), Some(LevelFilter::INFO));
        assert_eq!(rust_log_filter(Some("debug")).max_level_hint(), Some(LevelFilter::DEBUG));
        assert_eq!(rust_log_filter(Some("warn")).max_level_hint(), Some(LevelFilter::WARN));
        assert_eq!(rust_log_filter(Some("info,rustkit_engine=debug")).max_level_hint(), Some(LevelFilter::DEBUG));
    }
}

/// The settings page's `save_settings` payload applied to `state` and written
/// to settings.json; the value is the reply posted back to the page, whose
/// `ok` says whether the file was written (`error` says why not).
fn save_settings_reply(state: &mut AppState, payload: serde_json::Value) -> serde_json::Value {
    let result = serde_json::from_value::<crate::state::UserSettings>(payload)
        .map_err(|e| format!("invalid settings: {e}"))
        .and_then(|s| state.update_settings(s).map_err(|e| e.to_string()));
    match result {
        Ok(()) => serde_json::json!({ "type": "settings_saved", "ok": true }),
        Err(e) => {
            warn!("Settings not saved: {}", e);
            serde_json::json!({ "type": "settings_saved", "ok": false, "error": e })
        }
    }
}

#[cfg(test)]
mod settings_save_tests {
    use super::*;

    /// The keys `collectSettingsFromUI()` in settings.html puts in the
    /// `save_settings` payload, read from the page the app ships.
    fn page_save_keys() -> Vec<String> {
        let start = SETTINGS_HTML
            .find("function collectSettingsFromUI()")
            .expect("settings.html has collectSettingsFromUI");
        let body = &SETTINGS_HTML[start..];
        let open = body.find("return {").expect("collectSettingsFromUI returns an object literal") + "return {".len();
        let close = open + body[open..].find("};").expect("the object literal ends");
        body[open..close]
            .lines()
            .filter_map(|line| {
                let key = line.trim().split(':').next()?.trim();
                (!key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| key.to_string())
            })
            .collect()
    }

    fn state_in(dir: &std::path::Path) -> AppState {
        let config = crate::state::AppConfig {
            data_dir: dir.to_path_buf(),
            download_dir: dir.join("downloads"),
            ..Default::default()
        };
        AppState::new(config).expect("app state in a temp dir")
    }

    /// CH2.B11: 'Save Settings' with the payload the page sends writes
    /// settings.json (it was rejected for missing focus_* fields while the
    /// page said 'Settings saved!').
    #[test]
    fn the_settings_pages_save_payload_is_written() {
        let keys = page_save_keys();
        assert!(keys.contains(&"sidebar_default_open".to_string()), "page keys {keys:?}");
        // The page's payload: its keys, each with a value of the field's type,
        // and the toggle the person flipped.
        let defaults = serde_json::to_value(crate::state::UserSettings::default()).unwrap();
        let mut payload = serde_json::Map::new();
        for key in &keys {
            if let Some(value) = defaults.get(key) {
                payload.insert(key.clone(), value.clone());
            }
        }
        payload.insert("sidebar_default_open".into(), serde_json::Value::Bool(false));

        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path());
        let _ = std::fs::remove_file(state.settings_path());
        let reply = save_settings_reply(&mut state, serde_json::Value::Object(payload));
        assert_eq!(reply["type"], "settings_saved");
        assert!(reply.get("error").is_none(), "the save was rejected: {reply}");
        assert_eq!(reply["ok"], true);
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(state.settings_path()).expect("settings.json written")).unwrap();
        assert_eq!(written["sidebar_default_open"], false);
    }

    /// A save the app cannot apply is answered with ok:false and the reason,
    /// so the page does not say 'Settings saved!'; nothing is written.
    #[test]
    fn a_rejected_save_replies_not_ok_with_the_reason() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state_in(dir.path());
        let _ = std::fs::remove_file(state.settings_path());
        let reply = save_settings_reply(&mut state, serde_json::json!({ "theme": "dark" }));
        assert_eq!(reply["type"], "settings_saved");
        assert_eq!(reply["ok"], false);
        assert!(reply["error"].as_str().is_some_and(|e| e.contains("missing field")), "{reply}");
        assert!(!state.settings_path().exists());
    }

    /// Every field the app's settings need is in the page's payload, so a
    /// field added to one side and not the other fails here.
    #[test]
    fn the_page_sends_every_settings_field() {
        let keys = page_save_keys();
        let defaults = serde_json::to_value(crate::state::UserSettings::default()).unwrap();
        let fields: Vec<&String> = defaults.as_object().unwrap().keys().collect();
        let missing: Vec<&&String> = fields.iter().filter(|f| !keys.contains(f)).collect();
        let unknown: Vec<&String> = keys.iter().filter(|k| !fields.contains(k)).collect();
        assert!(missing.is_empty() && unknown.is_empty(), "page omits {missing:?}, sends unknown {unknown:?}");
    }
}

/// `RUST_LOG` when it is set to a directive this process can parse.
/// Unset, blank, and unparseable values stay at info, which is what the
/// app logged before `RUST_LOG` was read.
fn rust_log_filter(rust_log: Option<&str>) -> EnvFilter {
    match rust_log.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => EnvFilter::try_new(value).unwrap_or_else(|_| EnvFilter::new("info")),
        None => EnvFilter::new("info"),
    }
}

/// Stack size of the calling thread, as the OS reports it. Logged at startup
/// so a hand-test log shows the linker's main-thread stack took effect.
#[cfg(target_os = "macos")]
fn main_thread_stack_size() -> usize {
    extern "C" {
        fn pthread_self() -> *mut std::ffi::c_void;
        fn pthread_get_stacksize_np(thread: *mut std::ffi::c_void) -> usize;
    }
    // SAFETY: both calls only read the calling thread's own attributes.
    unsafe { pthread_get_stacksize_np(pthread_self()) }
}

/// The shipped app runs the page on the content thread (M4) unless
/// `HIWAVE_CONTENT_THREAD` is set, to `0` for example, which keeps the page on
/// the window thread. The engine's own default stays off, so parity-capture
/// and the engine tests keep their board-comparable arm.
#[cfg(target_os = "macos")]
fn default_content_thread_on() {
    if std::env::var_os("HIWAVE_CONTENT_THREAD").is_none() {
        // main() has started no threads yet.
        std::env::set_var("HIWAVE_CONTENT_THREAD", "1");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    default_content_thread_on();
    // Initialize logging with log compatibility. `RUST_LOG` selects the
    // level when it is set; otherwise info, as before.
    tracing_log::LogTracer::init().expect("Failed to set log tracer");
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(rust_log_filter(std::env::var("RUST_LOG").ok().as_deref()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

    info!("Starting HiWave...");
    #[cfg(target_os = "macos")]
    info!(
        "Main thread stack: {} MiB",
        main_thread_stack_size() / (1024 * 1024)
    );
    #[cfg(target_os = "macos")]
    info!(
        "Content thread: {}",
        if rustkit_engine::content_thread_enabled() {
            "on"
        } else {
            "off (HIWAVE_CONTENT_THREAD)"
        }
    );

    // Initialize application state
    let state = match AppState::with_defaults() {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(e) => {
            error!("Failed to initialize app state: {}", e);
            panic!("Failed to initialize app state: {}", e);
        }
    };

    info!("Application state initialized");

    // Track session start
    {
        let s = state.lock().unwrap();
        if let Err(e) = s.analytics.track_session_start() {
            error!("Failed to track session start: {}", e);
        }
    }

    // Create initial tab in the default workspace
    {
        let mut s = state.lock().unwrap();
        let has_tabs = !s.shell.list_tabs(None).is_empty();
        if !has_tabs {
            if let Some(workspace) = s.shell.get_active_workspace() {
                let workspace_id = workspace.id;
                let tab = hiwave_core::types::TabInfo {
                    id: hiwave_core::types::TabId::new(),
                    url: url::Url::parse(NEW_TAB_URL).unwrap(),
                    title: Some("New Tab".to_string()),
                    favicon: None,
                    workspace_id,
                    suspended: false,
                    loading: false,
                    locked: false,
                    last_visited: None,
                };
                if let Err(e) = s.shell.create_tab(tab) {
                    error!("Failed to create initial tab: {}", e);
                } else {
                    info!("Initial tab created");
                }
                if let Err(e) = s.save_workspace_state() {
                    error!("Failed to persist workspace state: {}", e);
                }
            }
        }
    }

    // Create the event loop with custom user events
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Create the main window
    let mut window_builder = WindowBuilder::new()
        .with_title("HiWave")
        .with_inner_size(LogicalSize::new(1280.0, 800.0));
    if let Some(icon) = create_window_icon() {
        window_builder = window_builder.with_window_icon(Some(icon));
    }
    let window = window_builder
        .build(&event_loop)
        .expect("Failed to create window");

    let main_window_id = window.id();

    // Initialize platform manager and menu
    let platform = get_platform_manager();
    info!("Platform: {}", platform.platform_name());

    // Create and initialize native menu (required for clipboard on macOS)
    let menu_bar = Menu::new();
    if let Err(e) = platform.initialize_menu(&window, &menu_bar) {
        error!("Failed to initialize menu: {}", e);
        // Continue without menu - clipboard may not work on macOS
    }

    info!("Window created successfully");

    // Get initial window size for WebView bounds
    let window_size = window.inner_size();
    let scale_factor = window.scale_factor();
    let width = window_size.width as f64 / scale_factor;
    let height = window_size.height as f64 / scale_factor;

    // Track current heights (will be modified dynamically)
    let chrome_height = Arc::new(Mutex::new(CHROME_HEIGHT_DEFAULT as f64));
    let shelf_height = Arc::new(Mutex::new(SHELF_HEIGHT_DEFAULT as f64));
    let inspector_height = Arc::new(Mutex::new(INSPECTOR_HEIGHT_DEFAULT as f64));
    // Sidebar width: 0.0 = closed, >0 = open with that width.
    // The content view starts at the full window. A page with no inserted
    // scripts reads that width, then hears one `resize` when chrome opens
    // the sidebar. Creating it already inset made that first read the
    // narrowed width, and chrome's initial width of 0 then dispatched a
    // stale full-window resize. Bounds set while a load holds the engine
    // are applied when it ends.
    let sidebar_width_state = Arc::new(Mutex::new(0.0_f64));
    let right_sidebar_open = Arc::new(Mutex::new(false));

    // Calculate initial bounds
    let sidebar_width = *sidebar_width_state.lock().unwrap();
    let content_width = (width - sidebar_width).max(0.0);
    let content_height =
        (height - CHROME_HEIGHT_DEFAULT as f64 - SHELF_HEIGHT_DEFAULT as f64).max(0.0);
    let chrome_bounds = Rect {
        position: LogicalPosition::new(0, 0).into(),
        size: LogicalSize::new(width, height).into(),
    };

    let content_bounds = Rect {
        position: LogicalPosition::new(sidebar_width as i32, CHROME_HEIGHT_DEFAULT as i32).into(),
        size: LogicalSize::new(content_width, content_height).into(),
    };

    let shelf_bounds = Rect {
        position: LogicalPosition::new(
            sidebar_width as i32,
            (height - SHELF_HEIGHT_DEFAULT as f64) as i32,
        )
        .into(),
        size: LogicalSize::new(content_width, SHELF_HEIGHT_DEFAULT as f64).into(),
    };

    // Inspector starts hidden (0 height)
    let inspector_bounds = Rect {
        position: LogicalPosition::new(
            sidebar_width as i32,
            (height - SHELF_HEIGHT_DEFAULT as f64 - INSPECTOR_HEIGHT_DEFAULT as f64) as i32,
        )
        .into(),
        size: LogicalSize::new(content_width, INSPECTOR_HEIGHT_DEFAULT as f64).into(),
    };

    // === CHROME WEBVIEW (created first, at top) ===
    let chrome_state = Arc::clone(&state);
    let chrome_proxy = proxy.clone();
    let settings_proxy_for_handler = proxy.clone();

    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    let rustkit_chrome = if rustkit_chrome_enabled() {
        info!("HIWAVE_RUSTKIT_CHROME=1: the chrome is a RustKit view");
        let bounds = rustkit_viewhost::Bounds::new(
            chrome_bounds.position.to_logical::<f64>(1.0).x as i32,
            chrome_bounds.position.to_logical::<f64>(1.0).y as i32,
            chrome_bounds.size.to_logical::<f64>(1.0).width as u32,
            chrome_bounds.size.to_logical::<f64>(1.0).height as u32,
        );
        let view = webview_rustkit::RustKitView::new(&window, bounds)
            .expect("Failed to create the RustKit chrome view");
        load_app_document(&view, &chrome_html_with_bridge())
            .expect("Failed to load the chrome into RustKit");
        Some(ChromeView::RustKit(view))
    } else {
        None
    };
    #[cfg(not(all(target_os = "macos", feature = "rustkit")))]
    let rustkit_chrome: Option<ChromeView> = None;
    let chrome_ipc_proxy = proxy.clone();
    let chrome_ipc_state = Arc::clone(&state);

    let chrome_webview = if let Some(view) = rustkit_chrome { view } else { ChromeView::WebKit(WebViewBuilder::new()
        .with_html(CHROME_HTML)
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true)
        .with_initialization_script(JS_BRIDGE)
        .with_bounds(chrome_bounds)
        .with_ipc_handler(move |message| {
            // Logged here, not in the handler: the RustKit chrome logs its
            // own (#907 R1 nit 1: once, without the body by default).
            log_ipc("Chrome IPC", message.body());
            handle_chrome_ipc(message.body(), &chrome_proxy, &chrome_state)
        })
        .build_as_child(&window)
        .expect("Failed to create HiWave WebView")) };

    info!(
        "HiWave WebView created (full window, top bar height {}px)",
        CHROME_HEIGHT_DEFAULT
    );

    // === CONTENT WEBVIEW (created second, below chrome) ===
    let _content_proxy = proxy.clone();
    let _content_proxy2 = proxy.clone();
    let _content_proxy3 = proxy.clone();
    let _content_proxy4 = proxy.clone();
    let content_proxy5 = proxy.clone();
    let _content_proxy_new_window = proxy.clone();
    let _content_proxy_downloads = proxy.clone();
    let _content_proxy_downloads_complete = proxy.clone();
    let _content_proxy_history = proxy.clone();
    let _content_proxy_ipc = proxy.clone();
    let _content_state = Arc::clone(&state);
    let _content_state2 = Arc::clone(&state);
    let _content_state_ipc = Arc::clone(&state);
    let _content_state_download_start = Arc::clone(&state);
    let _content_state_download_complete = Arc::clone(&state);
    let initial_url = {
        let s = state.lock().unwrap();
        s.shell
            .get_active_tab()
            .map(|tab| tab.url.to_string())
            .unwrap_or_else(|| NEW_TAB_URL.to_string())
    };
    // RustKit content webview (macOS with rustkit feature, default)
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    let content_webview = {
        use webview_rustkit::RustKitView;
        use rustkit_viewhost::Bounds;
        
        info!("Creating RustKit content webview (engine-level ad blocking enabled)");
        let bounds = Bounds::new(
            content_bounds.position.to_logical::<f64>(1.0).x as i32,
            content_bounds.position.to_logical::<f64>(1.0).y as i32,
            content_bounds.size.to_logical::<f64>(1.0).width as u32,
            content_bounds.size.to_logical::<f64>(1.0).height as u32,
        );
        
        // Create blocked counter for shield integration
        use std::sync::atomic::AtomicU64;
        let blocked_counter = Arc::new(AtomicU64::new(0));
        
        let rustkit_view = RustKitView::with_shield_counter(&window, bounds, Some(blocked_counter))
            .expect("Failed to create RustKit view");
        // An inserted script's fetch finishes off this thread. Posting
        // here wakes `MainEventsCleared`, which runs the script. The loop
        // does not poll for it.
        let wake_proxy = Arc::new(Mutex::new(proxy.clone()));
        rustkit_engine::set_live_wake(Arc::new(move || {
            if let Ok(proxy) = wake_proxy.lock() {
                let _ = proxy.send_event(UserEvent::LiveTurn);
            }
        }));
        // A restored tab is queued below as `UserEvent::Navigate`. With the
        // content thread both loads return immediately, and a new-tab
        // document queued first used to be applied after the restore had
        // already taken the newer navigation id (10/26 launches). The
        // restore is the first command; a late default document carries an
        // older id and is dropped.
        if is_new_tab_url(&initial_url) {
            rustkit_view.load_html_internal(ABOUT_HTML)
                .expect("Failed to load initial HTML");
        }
        
        UnifiedContentWebView::RustKit(Arc::new(rustkit_view))
    };
    
    #[cfg(not(target_os = "macos"))]
    let wry_content_webview = {
        let content_proxy = _content_proxy;
        let content_proxy2 = _content_proxy2;
        let content_proxy3 = _content_proxy3;
        let content_proxy4 = _content_proxy4;
        let content_proxy_new_window = _content_proxy_new_window;
        let content_proxy_downloads = _content_proxy_downloads;
        let content_proxy_downloads_complete = _content_proxy_downloads_complete;
        let content_proxy_history = _content_proxy_history;
        let content_proxy_ipc = _content_proxy_ipc;
        let content_state = _content_state;
        let content_state2 = _content_state2;
        let content_state_ipc = _content_state_ipc;
        let content_state_download_start = _content_state_download_start;
        let content_state_download_complete = _content_state_download_complete;

        WebViewBuilder::new()
        .with_html(ABOUT_HTML)
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true)
        .with_bounds(content_bounds)
        .with_navigation_handler(move |url| {
            info!("Content navigating to: {}", url);

            // Check if current tab URL is an internal URL before processing data URLs
            // This prevents overwriting hiwave://about when loading the about page via load_html()
            let current_tab_is_internal = content_state
                .lock()
                .ok()
                .and_then(|s| s.shell.get_active_tab().map(|tab| {
                    let url_str = tab.url.to_string();
                    is_about_url(&url_str) || url_str == NEW_TAB_URL
                }))
                .unwrap_or(false);

            if is_new_tab_url(&url) {
                // Only update tab URL if current tab isn't already an internal URL
                // This prevents data:text/html from overwriting hiwave://about
                if !current_tab_is_internal || url == NEW_TAB_URL {
                    let _ = content_proxy
                        .send_event(UserEvent::UpdateActiveTabUrl(NEW_TAB_URL.to_string()));
                }
                let _ = content_proxy.send_event(UserEvent::UpdateUrl(String::new()));
                let _ = content_proxy.send_event(UserEvent::SyncShieldStats);
                return true;
            }
            if is_about_url(&url) {
                let _ = content_proxy
                    .send_event(UserEvent::UpdateActiveTabUrl(ABOUT_URL.to_string()));
                let _ = content_proxy.send_event(UserEvent::UpdateUrl(ABOUT_URL.to_string()));
                let _ = content_proxy.send_event(UserEvent::LoadAboutPage);
                return true;
            }
            if is_report_url(&url) {
                let _ = content_proxy
                    .send_event(UserEvent::UpdateActiveTabUrl(REPORT_URL.to_string()));
                let _ = content_proxy.send_event(UserEvent::UpdateUrl(REPORT_URL.to_string()));
                let _ = content_proxy.send_event(UserEvent::LoadReportPage);
                return true;
            }
            if let Ok(s) = content_state.lock() {
                if s.shield.is_enabled() {
                    if let Ok(parsed_url) = url::Url::parse(&url) {
                        if s.shield
                            .should_block(&parsed_url, &parsed_url, ResourceType::Document)
                        {
                            // Track blocked navigation as both tracker and ad
                            if let Some(domain) = parsed_url.host_str() {
                                let workspace_id = s.shell.get_active_workspace()
                                    .map(|w| w.id.0.to_string());
                                let _ = s.analytics.track_tracker_blocked(
                                    domain,
                                    workspace_id.as_deref()
                                );
                                let _ = s.analytics.track_ad_blocked(
                                    domain,
                                    workspace_id.as_deref()
                                );
                            }
                            let _ = content_proxy.send_event(UserEvent::SyncShieldStats);
                            return false;
                        }
                    }
                }
            }
            let _ = content_proxy.send_event(UserEvent::SetLoading(true));
            let _ = content_proxy.send_event(UserEvent::UpdateUrl(url.clone()));
            let _ = content_proxy.send_event(UserEvent::UpdateActiveTabUrl(url));
            let _ = content_proxy.send_event(UserEvent::SyncShieldStats);
            true
        })
        .with_on_page_load_handler(move |event, url| match event {
            wry::PageLoadEvent::Started => {
                let _ = content_proxy2.send_event(UserEvent::SetLoading(true));
            }
            wry::PageLoadEvent::Finished => {
                let _ = content_proxy2.send_event(UserEvent::SetLoading(false));
                let _ = content_proxy2.send_event(UserEvent::SyncShieldStats);
                let _ = content_proxy_history.send_event(UserEvent::RecordVisit {
                    url: url.to_string(),
                });
            }
        })
        .with_document_title_changed_handler(move |title| {
            let _ = content_proxy3.send_event(UserEvent::UpdateTitle(title));
        })
        .with_download_started_handler(move |url, suggested_path| {
            if let Ok(mut s) = content_state_download_start.lock() {
                let target_path = s.prepare_download_path(&url);
                *suggested_path = target_path.clone();
                s.register_download(&url, &target_path);
                if let Err(err) = s.save_workspace_state() {
                    error!("Failed to persist download state: {}", err);
                }
            }
            let _ = content_proxy_downloads.send_event(UserEvent::SyncDownloads);
            true
        })
        .with_download_completed_handler(move |url, path, success| {
            if let Ok(mut s) = content_state_download_complete.lock() {
                s.finalize_download(&url, path, success);
                if let Err(err) = s.save_workspace_state() {
                    error!("Failed to persist download state: {}", err);
                }
            }
            let _ = content_proxy_downloads_complete.send_event(UserEvent::SyncDownloads);
        })
        .with_new_window_req_handler(move |url| {
            // Allow Cloudflare challenges to stay as iframes (required for verification)
            if url.contains("challenges.cloudflare.com") || url.contains("turnstile") {
                info!("Allowing Cloudflare challenge iframe: {}", url);
                return true; // Allow the iframe/popup to render in-place
            }

            // Allow Proton authentication popups/iframes
            if url.contains("proton.me") || url.contains("protonmail.com") {
                info!("Allowing Proton auth iframe: {}", url);
                return true; // Allow the iframe/popup to render in-place
            }

            // Allow about:srcdoc and about:blank - these are inline documents used by
            // verification widgets (like Cloudflare Turnstile) and don't navigate away
            if url == "about:srcdoc" || url == "about:blank" {
                return true; // Allow inline iframes
            }

            // Filter out unwanted popup/iframe requests using platform layer
            if platform::is_definitely_popup(&url) {
                info!("New window request blocked (filtered): {}", url);
                return false;
            }

            // Check Shield for ad/tracker blocking and user blocklist
            if let Ok(mut s) = content_state2.lock() {
                // Check user blocklist first (hot-editable)
                if s.user_blocklist.should_block(&url) {
                    info!("New window request blocked (user blocklist): {}", url);
                    // Increment Shield counters so all blocks show in UI
                    s.shield.increment_block_count();
                    // Track user blocklist blocks in analytics as popup, tracker, and ad
                    if let Ok(parsed_url) = url::Url::parse(&url) {
                        if let Some(domain) = parsed_url.host_str() {
                            let workspace_id = s.shell.get_active_workspace()
                                .map(|w| w.id.0.to_string());
                            let _ = s.analytics.track_popup_blocked(
                                domain,
                                workspace_id.as_deref()
                            );
                            let _ = s.analytics.track_tracker_blocked(
                                domain,
                                workspace_id.as_deref()
                            );
                            let _ = s.analytics.track_ad_blocked(
                                domain,
                                workspace_id.as_deref()
                            );
                        }
                    }
                    drop(s);
                    let _ = content_proxy_new_window.send_event(UserEvent::SyncShieldStats);
                    return false;
                }

                // Check popup flood protection (rate limiting + duplicate detection)
                if !s.should_allow_popup(&url) {
                    // Warning already logged by should_allow_popup
                    // Increment Shield counters so all blocks show in UI
                    s.shield.increment_block_count();
                    // Track flood protection blocks in analytics as popup, tracker, and ad
                    if let Ok(parsed_url) = url::Url::parse(&url) {
                        if let Some(domain) = parsed_url.host_str() {
                            let workspace_id = s.shell.get_active_workspace()
                                .map(|w| w.id.0.to_string());
                            let _ = s.analytics.track_popup_blocked(
                                domain,
                                workspace_id.as_deref()
                            );
                            let _ = s.analytics.track_tracker_blocked(
                                domain,
                                workspace_id.as_deref()
                            );
                            let _ = s.analytics.track_ad_blocked(
                                domain,
                                workspace_id.as_deref()
                            );
                        }
                    }
                    drop(s);
                    let _ = content_proxy_new_window.send_event(UserEvent::SyncShieldStats);
                    return false;
                }

                // Check Shield (EasyList)
                if s.shield.is_enabled() {
                    if let Ok(parsed_url) = url::Url::parse(&url) {
                        // Use SubDocument type for popups/iframes
                        if s.shield
                            .should_block(&parsed_url, &parsed_url, ResourceType::Other)
                        {
                            // Track blocked popup in analytics as popup, tracker, and ad
                            if let Some(domain) = parsed_url.host_str() {
                                let workspace_id = s.shell.get_active_workspace()
                                    .map(|w| w.id.0.to_string());
                                let _ = s.analytics.track_popup_blocked(
                                    domain,
                                    workspace_id.as_deref()
                                );
                                let _ = s.analytics.track_tracker_blocked(
                                    domain,
                                    workspace_id.as_deref()
                                );
                                let _ = s.analytics.track_ad_blocked(
                                    domain,
                                    workspace_id.as_deref()
                                );
                            }
                            info!("New window request blocked (Shield): {}", url);
                            drop(s);
                            let _ = content_proxy_new_window.send_event(UserEvent::SyncShieldStats);
                            return false;
                        }
                    }
                }
            }

            info!("New window request -> new tab: {}", url);
            let response = ipc::commands::handle_message(
                &content_state2,
                IpcMessage::CreateTab {
                    url: Some(url.clone()),
                },
            );
            info!("New window redirected to tab: {:?}", response);
            let _ = content_proxy4.send_event(UserEvent::SyncTabs);
            let _ = content_proxy4.send_event(UserEvent::Navigate(url));
            false
        })
        .with_ipc_handler(move |message| {
            let body = message.body();
            log_ipc("Content IPC", body);
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(body) {
                match &msg {
                    IpcMessage::GoBack => {
                        let _ = content_proxy_ipc.send_event(UserEvent::GoBack);
                    }
                    IpcMessage::GoForward => {
                        let _ = content_proxy_ipc.send_event(UserEvent::GoForward);
                    }
                    IpcMessage::CreateTab { url } => {
                        // Handle create tab directly
                        let _ = ipc::commands::handle_message(&content_state_ipc, msg.clone());
                        let _ = content_proxy_ipc.send_event(UserEvent::SyncTabs);
                        let _ = content_proxy_ipc.send_event(UserEvent::SyncWorkspaces);
                        if let Some(u) = url {
                            let _ = content_proxy_ipc.send_event(UserEvent::Navigate(u.clone()));
                        }
                    }
                    IpcMessage::AddToShelf { tab_id } => {
                        // If tab_id is "active", use the currently active tab
                        let actual_tab_id = if tab_id == "active" {
                            if let Ok(s) = content_state_ipc.lock() {
                                s.shell.get_active_tab().map(|t| t.id.0.to_string()).unwrap_or_default()
                            } else {
                                String::new()
                            }
                        } else {
                            tab_id.clone()
                        };

                        if !actual_tab_id.is_empty() {
                            let shelf_msg = IpcMessage::AddToShelf { tab_id: actual_tab_id };
                            let _ = ipc::commands::handle_message(&content_state_ipc, shelf_msg);
                            let _ = content_proxy_ipc.send_event(UserEvent::SyncShelf(crate::ShelfScope::Workspace));
                            let _ = content_proxy_ipc.send_event(UserEvent::SyncTabs);
                        }
                    }
                    IpcMessage::FindInPageResult { result } => {
                        let _ = content_proxy_ipc.send_event(UserEvent::FindInPageResult(result.clone()));
                    }
                    IpcMessage::TabAudioStateChanged { playing } => {
                        info!("Tab audio state changed from content: playing={}", playing);
                        let _ = content_proxy_ipc.send_event(UserEvent::TabAudioStateChanged { playing: *playing });
                    }
                    IpcMessage::GetCredentialsForAutofill { ref domain } => {
                        info!("Getting credentials for autofill: {}", domain);
                        let response = ipc::commands::handle_message(&content_state_ipc, msg.clone());
                        info!("Autofill response: {:?}", response);
                        match response {
                            ipc::IpcResponse::Success { data } => {
                                let json = serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string());
                                info!("Sending {} credentials to page", json.len());
                                let _ = content_proxy_ipc.send_event(UserEvent::SendAutofillCredentials {
                                    credentials_json: json,
                                });
                            }
                            ipc::IpcResponse::Error { ref message } => {
                                warn!("Autofill error: {}", message);
                                // Show error alert on content page
                                let script = format!(
                                    "alert('[HiWave Autofill] {}');",
                                    message.replace('\'', "\\'")
                                );
                                let _ = content_proxy_ipc.send_event(UserEvent::EvaluateContentScript(script));
                            }
                        }
                    }
                    // Handle analytics and other commands that need responses sent back to content
                    IpcMessage::GetTodayStats
                    | IpcMessage::GetWeeklyReport
                    | IpcMessage::GetMonthlyReport
                    | IpcMessage::GetCustomReport { .. }
                    | IpcMessage::GetTopDomains { .. }
                    | IpcMessage::GetWorkspaceStats
                    | IpcMessage::ResetAnalyticsData
                    | IpcMessage::ExportAnalyticsData { .. } => {
                        info!("Analytics command from content: {:?}", msg);
                        let response = ipc::commands::handle_message(&content_state_ipc, msg.clone());
                        let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
                        let script = format!(
                            "window.dispatchEvent(new CustomEvent('hiwave-response', {{ detail: {} }}));",
                            response_json
                        );
                        let _ = content_proxy_ipc.send_event(UserEvent::EvaluateContentScript(script));
                    }
                    // Inspector IPC handlers from content webview
                    IpcMessage::InspectorDomTreeResult { tree } => {
                        let json = serde_json::to_string(&tree).unwrap_or_else(|_| "null".to_string());
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectorSendDomTree(json));
                    }
                    IpcMessage::InspectorElementStylesResult { styles } => {
                        let json = serde_json::to_string(&styles).unwrap_or_else(|_| "null".to_string());
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectorSendElementStyles(json));
                    }
                    IpcMessage::InspectorElementPicked { path, styles } => {
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectorElementPicked {
                            path: path.clone(),
                            styles: styles.clone(),
                        });
                    }
                    IpcMessage::InspectorPickerStopped => {
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectorPickerStopped);
                    }
                    IpcMessage::InspectElement { path } => {
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectElement { path: path.clone() });
                    }
                    // Console IPC handlers
                    IpcMessage::InspectorConsoleMessage { entry } => {
                        let json = serde_json::to_string(&entry).unwrap_or_else(|_| "null".to_string());
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectorConsoleMessage(json));
                    }
                    IpcMessage::InspectorConsoleClear => {
                        let _ = content_proxy_ipc.send_event(UserEvent::InspectorConsoleClear);
                    }
                    _ => {}
                }
            }
        })
        .build_as_child(&window)
        .expect("Failed to create Content WebView")
    };

    #[cfg(not(target_os = "macos"))]
    let content_webview = UnifiedContentWebView::Wry(Arc::new(wry_content_webview));

    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    {
        if !is_new_tab_url(&initial_url) {
            // DEFERRED, not loaded here (2026-08-05). This call used to run
            // synchronously during setup, BEFORE the event loop's first
            // iteration — and RustKit's load path blocks its thread through
            // network + parse + layout. Result, measured live with a process
            // sample while Pete stared at nothing: main thread parked in
            // load_url_blocking -> tokio block_on, zero paints, zero events,
            // a window that never appeared for as long as the restored tab
            // took to load (minutes, on a heavy page in a debug build).
            //
            // Queuing it as the SAME UserEvent::Navigate the URL bar uses
            // means the event loop starts, the window and chrome UI paint,
            // and THEN the restore navigation runs. The engine still blocks
            // its thread during the load — that is the engine-thread
            // refactor, a separate unit — but it now blocks behind a
            // painted, visibly-alive window instead of a void.
            info!(url = %initial_url, "Deferring restored-tab navigation until after first paint");
            let _ = proxy.send_event(UserEvent::Navigate(initial_url.clone()));
        }
        info!("Content WebView created (RustKit)");
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        if !is_new_tab_url(&initial_url) {
            if let UnifiedContentWebView::Wry(ref v) = content_webview {
                if let Err(e) = v.load_url(&initial_url) {
                    error!("Failed to load initial URL: {}", e);
                }
            }
        }
        info!("Content WebView created (WRY)");
    }

    // === SHELF WEBVIEW (created third, at bottom) ===
    let shelf_proxy = proxy.clone();
    let shelf_state = Arc::clone(&state);
    // With HIWAVE_RUSTKIT_CHROME=1 the shelf and the Inspector are RustKit
    // views built when they first open (CH3.A4b, `apply_layout`).
    let shelf_webview = if rustkit_chrome_enabled() { LazyPanel::deferred(SHELF_HTML, "shelf") } else { LazyPanel::built(PanelView::WebKit(WebViewBuilder::new()
        .with_html(SHELF_HTML)
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true)
        .with_initialization_script(JS_BRIDGE)
        .with_bounds(shelf_bounds)
        .with_ipc_handler(move |message| {
            log_ipc("Shelf IPC", message.body());
            handle_shelf_ipc(message.body(), &shelf_proxy, &shelf_state)
        })
        .with_ipc_handler(move |message| {
            handle_shelf_find_ipc(message.body(), &content_proxy5)
        })
        .build_as_child(&window)
        .expect("Failed to create Shelf WebView")), SHELF_HTML, "shelf") };

    info!("Shelf WebView created (bottom, starts hidden)");

    // === INSPECTOR WEBVIEW (created fourth, between content and shelf) ===
    let inspector_proxy = proxy.clone();
    let _inspector_state = Arc::clone(&state);
    let inspector_webview = if rustkit_chrome_enabled() { LazyPanel::deferred(INSPECTOR_HTML, "Inspector") } else { LazyPanel::built(PanelView::WebKit(WebViewBuilder::new()
        .with_html(INSPECTOR_HTML)
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true)
        .with_initialization_script(JS_BRIDGE)
        .with_bounds(inspector_bounds)
        .with_ipc_handler(move |message| {
            log_ipc("Inspector IPC", message.body());
            handle_inspector_ipc(message.body(), &inspector_proxy)
        })
        .build_as_child(&window)
        .expect("Failed to create Inspector WebView")), INSPECTOR_HTML, "Inspector") };

    info!("Inspector WebView created (between content and shelf, starts hidden)");
    info!("Four-WebView architecture initialized");
    announce_panel_probe();

    // DIAGNOSTIC: dump the contentView subview stack, bottom to top. Subview
    // array order IS z-order (later = on top). A full session of clicking
    // produced zero window-level click events, which means SOMETHING sits
    // above the RustKit content view and consumes them — this names it
    // instead of us guessing which view wry stacked where.
    #[cfg(target_os = "macos")]
    // Gated: the probe ends with a SYNTHETIC CLICK at (640,350), which on a
    // real launch would click whatever the restored page has there. Set
    // HIWAVE_DIAG=1 to run the stack dump + hitTest + click self-test.
    if std::env::var("HIWAVE_DIAG").is_ok() {
        unsafe {
        use objc::{msg_send, sel, sel_impl};
        use tao::platform::macos::WindowExtMacOS;
        // ns_view IS the TaoView that hosts every WebView — dump ITS
        // children (first probe dumped the window contentView, one level
        // too high, and saw only TaoView itself).
        let ns_view = window.ns_view() as cocoa::base::id;
        let subviews: cocoa::base::id = msg_send![ns_view, subviews];
        let count: usize = msg_send![subviews, count];
        for idx in 0..count {
            let v: cocoa::base::id = msg_send![subviews, objectAtIndex: idx];
            let cls: cocoa::base::id = msg_send![v, className];
            let utf8: *const std::os::raw::c_char = msg_send![cls, UTF8String];
            let name = std::ffi::CStr::from_ptr(utf8).to_string_lossy();
            let frame: cocoa::foundation::NSRect = msg_send![v, frame];
            let hidden: bool = msg_send![v, isHidden];
            info!(
                z = idx,
                class = %name,
                x = frame.origin.x,
                y = frame.origin.y,
                w = frame.size.width,
                h = frame.size.height,
                hidden,
                "subview stack (z asc = bottom to top)"
            );
        }

        // hitTest is PURE — this asks AppKit "who would receive a click at
        // point P" without any click happening. Points in TaoView coords
        // (bottom-left origin). If the content region resolves to a
        // WryWebView instead of our NSView, hit-test is being overridden
        // and that IS the click-eating bug.
        let superview: cocoa::base::id = msg_send![ns_view, superview];
        for (label, x, y) in [
            ("content-middle", 640.0f64, 350.0f64),
            ("content-left-edge", 8.0, 350.0),
            ("toolbar", 640.0, 760.0),
        ] {
            // hitTest: takes a point in the SUPERVIEW's coordinate system.
            let pt = cocoa::foundation::NSPoint::new(x, y);
            let hit: cocoa::base::id = msg_send![ns_view, hitTest: pt];
            let who = if hit == cocoa::base::nil {
                "nil".to_string()
            } else {
                let cls: cocoa::base::id = msg_send![hit, className];
                let utf8: *const std::os::raw::c_char = msg_send![cls, UTF8String];
                std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
            };
            info!(label, x, y, owner = %who, "hitTest probe");
        }
        let _ = superview;

        // Synthesize a click through the REAL dispatch path: window
        // sendEvent -> hitTest -> view -> responder chain. If this does not
        // produce our own "window-level click received" line, clicks die
        // between the content NSView and TaoView — measured, not assumed —
        // and the fix is real event handlers on the viewhost NSView.
        let ns_window: cocoa::base::id = msg_send![ns_view, window];
        let win_num: isize = msg_send![ns_window, windowNumber];
        for (etype, label) in [(1usize, "synthetic mouseDown"), (2usize, "synthetic mouseUp")] {
            let pt = cocoa::foundation::NSPoint::new(640.0, 350.0);
            let ev: cocoa::base::id = msg_send![objc::class!(NSEvent),
                mouseEventWithType: etype
                location: pt
                modifierFlags: 0usize
                timestamp: 0.0f64
                windowNumber: win_num
                context: cocoa::base::nil
                eventNumber: 0isize
                clickCount: 1isize
                pressure: 1.0f32];
            if ev != cocoa::base::nil {
                info!(label, "posting");
                let _: () = msg_send![ns_window, sendEvent: ev];
            } else {
                info!(label, "NSEvent construction returned nil");
            }
        }
        }
    }

    // Check for debug mode via environment variable
    if std::env::var("HIWAVE_DEBUG").map(|v| v == "1").unwrap_or(false) {
        info!("Debug mode enabled via HIWAVE_DEBUG=1");
        let _ = chrome_webview.eval("window.enableDebugMode && window.enableDebugMode();");
    }

    // Store WebViews in Arcs for event loop access
    let chrome_webview = Arc::new(chrome_webview);
    #[cfg(target_os = "macos")]
    let content_webview = Arc::new(content_webview);
    #[cfg(not(target_os = "macos"))]
    let content_webview = Arc::new(content_webview);
    let shelf_webview = Arc::new(shelf_webview);
    let inspector_webview = Arc::new(inspector_webview);

    let chrome_for_events = Arc::clone(&chrome_webview);
    let content_for_events = Arc::clone(&content_webview);
    // CH3.A3: the RustKit chrome goes over the page, so its popups can
    // overlap it; it leaves the page column to the page (its region) until
    // the first layout sets it again.
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    if let (ChromeView::RustKit(ref chrome), UnifiedContentWebView::RustKit(ref page)) =
        (&*chrome_webview, &*content_webview)
    {
        rustkit_viewhost::raise_raw_view_above(chrome.host_view_raw(), page.host_view_raw());
        let scale = window.scale_factor();
        let size = window.inner_size();
        let (width, height) = (size.width as f64 / scale, size.height as f64 / scale);
        let at = content_bounds.position.to_logical::<f64>(1.0);
        let page_size = content_bounds.size.to_logical::<f64>(1.0);
        chrome_webview.compose(width, height, (at.x, at.y, page_size.width, (height - at.y).max(0.0)));
        info!("RustKit chrome stacked over the page");
    }
    let shelf_for_events = Arc::clone(&shelf_webview);
    let inspector_for_events = Arc::clone(&inspector_webview);
    let state_for_events = Arc::clone(&state);
    let chrome_height_for_events = Arc::clone(&chrome_height);
    let shelf_height_for_events = Arc::clone(&shelf_height);
    let inspector_height_for_events = Arc::clone(&inspector_height);
    let sidebar_width_for_events = Arc::clone(&sidebar_width_state);
    let right_sidebar_open_for_events = Arc::clone(&right_sidebar_open);
    let focus_state_for_events = Arc::clone(&state);

    // Trigger initial state sync
    let init_proxy = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = init_proxy.send_event(UserEvent::SyncTabs);
        let _ = init_proxy.send_event(UserEvent::SyncWorkspaces);
        let _ = init_proxy.send_event(UserEvent::SyncDownloads);
        let _ = init_proxy.send_event(UserEvent::SyncShelf(ShelfScope::Workspace));
        let _ = init_proxy.send_event(UserEvent::SyncHistory);
    });

    // Track settings window (created on demand)
    let settings_window: Arc<Mutex<Option<(tao::window::Window, PanelView)>>> =
        Arc::new(Mutex::new(None));
    let settings_window_for_events = Arc::clone(&settings_window);

    // Spawn background decay ticker thread
    let decay_proxy = proxy.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            let _ = decay_proxy.send_event(UserEvent::DecayTick);
        }
    });
    info!("Started background decay ticker");

    // Spawn focus mode auto-trigger checker thread
    let focus_proxy = proxy.clone();
    std::thread::spawn(move || {
        loop {
            // Check every 5 seconds for auto-trigger conditions
            std::thread::sleep(std::time::Duration::from_secs(5));
            let _ = focus_proxy.send_event(UserEvent::FocusAutoTriggerCheck);
        }
    });
    info!("Started focus mode auto-trigger checker");

    // How long the window thread goes without turning this loop, written to
    // the log when asked for. The real-window responsiveness checks read it.
    let stall_log = window_stall::enabled().then(|| {
        let ping_proxy = proxy.clone();
        window_stall::start(move || ping_proxy.send_event(UserEvent::WindowThreadPing).is_ok())
    });

    // Last known cursor position in WINDOW logical coordinates. tao reports
    // MouseInput without a position, so the CursorMoved stream is what makes
    // a click locatable.
    let cursor_position = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    // tao delivers modifiers as their own event rather than on each KeyEvent,
    // so the current state has to be carried between them.
    let modifiers = std::rc::Rc::new(std::cell::Cell::new(tao::keyboard::ModifiersState::empty()));
    let click_proxy = proxy.clone();
    // When the content page's next timer is due. The loop sleeps until
    // input otherwise, and a sleeping loop runs no timers.
    let live_wake = std::rc::Rc::new(std::cell::Cell::new(None::<std::time::Instant>));
    // The Inspector's native path for RustKit content: picking, the
    // selection and the console's place.
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    let mut devtools = devtools_panel::Devtools::default();
    // A script for the RustKit chrome once it has booted (real-window checks).
    #[cfg(all(target_os = "macos", feature = "rustkit"))]
    let mut chrome_boot_eval = chrome_boot_eval(
        rustkit_chrome_enabled(),
        std::env::var("HIWAVE_TEST_HOOKS").ok().as_deref(),
        std::env::var("HIWAVE_RUSTKIT_CHROME_EVAL").ok(),
    );

    // Run the event loop
    event_loop.run(move |event, event_loop_target, control_flow| {
        *control_flow = match live_wake.get() {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        };

        match event {
            Event::WindowEvent {
                event: WindowEvent::MouseWheel { delta, .. },
                ..
            } => {
                // Wheel events that reach the window loop were not consumed
                // by the UI-frame WebView, so the pointer is over the content
                // view (verified in the 2026-08-05 live session: flicks over
                // page content arrive here; flicks over UI chrome do not).
                // Forward them to the engine; render happens on
                // MainEventsCleared, which this event wakes.
                #[cfg(all(target_os = "macos", feature = "rustkit"))]
                if let UnifiedContentWebView::RustKit(ref view) = *content_for_events {
                    if wheel_burst_started() {
                        info!(?delta, "wheel burst started (window loop)");
                    }
                    let (dx, dy) = match delta {
                        tao::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                        // Line deltas (external mice) arrive in rows/columns;
                        // 40px per line matches common browser behavior.
                        tao::event::MouseScrollDelta::LineDelta(x, y) => (x * 40.0, y * 40.0),
                        _ => (0.0, 0.0),
                    };
                    if view.scroll_by(dx, dy) {
                        trace!(dx, dy, "content scrolled");
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(state),
                ..
            } => {
                modifiers.set(state);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                let scale = window.scale_factor();
                cursor_position.set((position.x / scale, position.y / scale));
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        state: tao::event::ElementState::Released,
                        button: tao::event::MouseButton::Left,
                        ..
                    },
                ..
            } => {
                // Clicks reaching the window loop were not consumed by the UI
                // frame, i.e. the pointer is over content — the same delivery
                // rule the wheel path established. Translate window logical
                // coordinates into the content view's viewport space, then
                // ask the engine what link (if any) sits there.
                #[cfg(all(target_os = "macos", feature = "rustkit"))]
                if let UnifiedContentWebView::RustKit(ref view) = *content_for_events {
                    let (cx, cy) = cursor_position.get();
                    let content_x = cx - *sidebar_width_for_events.lock().unwrap();
                    let content_y = cy - *chrome_height_for_events.lock().unwrap();
                    // info-level ON PURPOSE: a click that arrives but resolves
                    // to nothing was previously indistinguishable from a click
                    // eaten by a WebView layer above us. Clicks are rare;
                    // this cannot flood. (Live session 2026-08-06: an entire
                    // session of link-clicking produced zero log evidence
                    // either way.)
                    info!(cx, cy, content_x, content_y, "window-level click received");
                    // Both bounds, not just the origin side (Prometheus, #104
                    // R1): the delivery rule says clicks reaching this loop are
                    // content-directed, but that is an inference about the UI
                    // frame's behavior, not a geometric fact. A half-open gate
                    // turns any leak into a navigation at whatever the hit test
                    // finds under an out-of-bounds coordinate.
                    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
                    let content_w = size.width
                        - *sidebar_width_for_events.lock().unwrap()
                        - if *right_sidebar_open_for_events.lock().unwrap() { SIDEBAR_WIDTH } else { 0.0 };
                    let content_h = size.height
                        - *chrome_height_for_events.lock().unwrap()
                        - *shelf_height_for_events.lock().unwrap()
                        - *inspector_height_for_events.lock().unwrap();
                    if content_x >= 0.0
                        && content_y >= 0.0
                        && content_x < content_w
                        && content_y < content_h
                    {
                        // Focus first, then navigate. A click that lands on a
                        // form control focuses it; a click on anything else
                        // clears focus, which is what makes a later keystroke
                        // unambiguous about where it belongs.
                        if let Some(tag) = view.focus_at_point(content_x as f32, content_y as f32) {
                            info!(%tag, "Focused content element");
                        }
                        // The content view already recorded this click when
                        // the page runs on its own thread. Asking here races
                        // that thread, and a miss is logged as a script
                        // navigation. The click comes back as
                        // NavigateRequested with the user as its origin.
                        if !rustkit_engine::content_thread_enabled() {
                            if let Some(url) = view.link_at_point(content_x as f32, content_y as f32)
                            {
                                info!(%url, "Link clicked");
                                let _ = click_proxy.send_event(UserEvent::Navigate(url));
                            }
                        }
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { event: key_event, .. },
                ..
            } => {
                // Same delivery rule as MouseWheel: keys that reach the
                // window loop were not consumed by a focused WebView text
                // field, so scrolling the content is the right default.
                // (Whether unfocused keys arrive at all on macOS is the
                // diagnostic half — the wheel needed a live session to
                // answer the same question.)
                #[cfg(all(target_os = "macos", feature = "rustkit"))]
                if key_event.state == tao::event::ElementState::Pressed {
                    if let UnifiedContentWebView::RustKit(ref view) = *content_for_events {
                        use tao::keyboard::Key;

                        // A focused form control claims keys first. Win32
                        // virtual-key numbering because rustkit-dom's editing
                        // model already speaks it (it predates any platform
                        // wiring); mapping here keeps that model untouched.
                        if view.has_focused_element() {
                            let mods = modifiers.get();
                            let (vk, text) = match &key_event.logical_key {
                                Key::ArrowLeft => (0x25u32, String::new()),
                                Key::ArrowRight => (0x27, String::new()),
                                // A focused `<select>` opens, moves and
                                // closes its drop-down with these (CH2.E6);
                                // the native key path already maps them.
                                Key::ArrowUp => (0x26, String::new()),
                                Key::ArrowDown => (0x28, String::new()),
                                Key::PageUp => (0x21, String::new()),
                                Key::PageDown => (0x22, String::new()),
                                Key::Escape => (0x1B, String::new()),
                                Key::Home => (0x24, String::new()),
                                Key::End => (0x23, String::new()),
                                Key::Backspace => (0x08, String::new()),
                                Key::Delete => (0x2E, String::new()),
                                Key::Enter => {
                                    // Enter in a form field submits it. Done
                                    // before the edit model sees the key: the
                                    // model reports Submit but cannot
                                    // navigate, and swallowing Enter into a
                                    // single-line field would do nothing
                                    // visible at all.
                                    if let Some(url) = view.form_submit_url() {
                                        info!(%url, "Form submitted");
                                        let _ = click_proxy.send_event(UserEvent::Navigate(url));
                                        return;
                                    }
                                    (0x0D, String::new())
                                }
                                Key::Character(c) => (0, c.to_string()),
                                _ => (0, String::new()),
                            };
                            if vk != 0 || !text.is_empty() {
                                if view.handle_text_key(
                                    vk,
                                    &text,
                                    mods.control_key() || mods.super_key(),
                                    mods.shift_key(),
                                    mods.alt_key(),
                                ) {
                                    // The edit lives in engine state, so
                                    // nothing else would repaint it.
                                    view.relayout();
                                    return;
                                }
                            }
                            // Focused but unhandled (e.g. Tab): swallow rather
                            // than scroll — scrolling the page while a field
                            // has focus is never what was meant.
                            return;
                        }

                        // Nothing focused: the page hears the key first
                        // (its own shortcuts). A cancelled keydown is the
                        // page's key, not a scroll.
                        let mods = modifiers.get();
                        if !mods.super_key() {
                            let (vk, text) = match &key_event.logical_key {
                                Key::Escape => (0x1Bu32, String::new()),
                                Key::Enter => (0x0D, String::new()),
                                Key::PageUp => (0x21, String::new()),
                                Key::PageDown => (0x22, String::new()),
                                Key::End => (0x23, String::new()),
                                Key::Home => (0x24, String::new()),
                                Key::ArrowUp => (0x26, String::new()),
                                Key::ArrowDown => (0x28, String::new()),
                                Key::Space => (0, " ".to_string()),
                                Key::Character(c) => (0, c.to_string()),
                                _ => (0, String::new()),
                            };
                            if (vk != 0 || !text.is_empty())
                                && view.handle_text_key(
                                    vk,
                                    &text,
                                    mods.control_key(),
                                    mods.shift_key(),
                                    mods.alt_key(),
                                )
                            {
                                view.relayout();
                                return;
                            }
                        }

                        // scroll_by uses wheel sign convention: negative dy
                        // advances the page (natural scrolling).
                        let dy: Option<f32> = match key_event.logical_key {
                            Key::ArrowDown => Some(-40.0),
                            Key::ArrowUp => Some(40.0),
                            Key::PageDown | Key::Space => Some(-600.0),
                            Key::PageUp => Some(600.0),
                            Key::End => Some(-f32::MAX),
                            Key::Home => Some(f32::MAX),
                            _ => None,
                        };
                        if let Some(dy) = dy {
                            if view.scroll_by(0.0, dy) {
                                trace!(dy, "content scrolled via keyboard");
                            }
                        }
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                if window_id == main_window_id {
                    // Main window close - save state and exit
                    info!("Main window close requested, shutting down...");
                    if let Ok(s) = state_for_events.lock() {
                        // Track session end
                        let session_duration = s.session_start_time.elapsed().as_secs() as i64;
                        if let Err(e) = s.analytics.track_session_end(session_duration) {
                            error!("Failed to track session end: {}", e);
                        }

                        if let Err(e) = s.save_workspace_state() {
                            error!("Failed to persist workspace state: {}", e);
                        }
                    }
                    *control_flow = ControlFlow::Exit;
                } else {
                    // Secondary window close (settings, vault) - just hide it
                    info!("Secondary window close requested, hiding...");

                    // Check if it's the settings window
                    if let Ok(settings_guard) = settings_window_for_events.lock() {
                        if let Some((ref settings_win, _)) = *settings_guard {
                            if settings_win.id() == window_id {
                                settings_win.set_visible(false);
                                return;
                            }
                        }
                    }

                }
            }
            // The RustKit settings page follows its own window (CH3.A4).
            #[cfg(all(target_os = "macos", feature = "rustkit"))]
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                window_id,
                ..
            } if window_id != main_window_id && rustkit_chrome_enabled() => {
                if let Ok(guard) = settings_window_for_events.lock() {
                    if let Some((ref settings_win, ref settings)) = *guard {
                        if settings_win.id() == window_id {
                            let scale = settings_win.scale_factor();
                            settings.place(Rect {
                                position: LogicalPosition::new(0, 0).into(),
                                size: LogicalSize::new(size.width as f64 / scale, size.height as f64 / scale).into(),
                            });
                        }
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_new_size),
                ..
            } => {
                let ch = *chrome_height_for_events.lock().unwrap();
                let sh = *shelf_height_for_events.lock().unwrap();
                let ih = *inspector_height_for_events.lock().unwrap();
                let sw = *sidebar_width_for_events.lock().unwrap();
                let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                apply_layout(
                    &window,
                    &chrome_for_events,
                    &content_for_events,
                    &shelf_for_events,
                    &inspector_for_events,
                    ch,
                    sh,
                    ih,
                    sw,
                    right_sidebar_open,
                );
            }
            #[cfg(all(target_os = "macos", feature = "rustkit"))]
            Event::MainEventsCleared => {
                // The RustKit chrome (CH3.A2): its page's messages go to the
                // chrome's command handler, as the WebKit view's do.
                let chrome_input = if let ChromeView::RustKit(ref chrome) = *chrome_for_events {
                    chrome.process_events();
                    // Its input (step 3b), through the page's dispatch. Taken
                    // whatever the page is doing: the address bar answers
                    // while a page loads, as the WebKit chrome did. The
                    // page's own input stays queued for it.
                    if let Some(id) = chrome.input_view() {
                        for event in rustkit_viewhost::drain_input_view(id) {
                            chrome.note_input(event.at, event.seq);
                            dispatch_view_input(
                                chrome,
                                InputTarget::Chrome,
                                event.kind,
                                &mut devtools,
                                &inspector_for_events,
                                &click_proxy,
                            );
                        }
                    }
                    let mut booted = false;
                    for body in chrome.take_ipc_messages() {
                        log_ipc("RustKit chrome IPC", &body);
                        handle_chrome_ipc(&body, &chrome_ipc_proxy, &chrome_ipc_state);
                        booted = true;
                    }
                    // `HIWAVE_RUSTKIT_CHROME_EVAL` (with `HIWAVE_TEST_HOOKS=1`):
                    // a script for the chrome once its own script has posted,
                    // for the real-window checks that cannot type
                    // (a2_chrome_navigate).
                    if booted {
                        if let Some(js) = chrome_boot_eval.take() {
                            info!(len = js.len(), "RustKit chrome: running HIWAVE_RUSTKIT_CHROME_EVAL");
                            let _ = chrome.wry_evaluate_script(&js);
                        }
                    }
                    // A link or `location` in the chrome would replace the
                    // chrome's own document; nothing follows it.
                    if let Some((url, _)) = chrome.take_script_navigation() {
                        warn!(%url, "RustKit chrome navigation not followed");
                    }
                    chrome.render();
                    // A right press the chrome's script let through (the
                    // sidebar's) gets no menu: the WebKit chrome's would be
                    // WebKit's own (CH2.E4).
                    if chrome.take_context_menu().is_some() {
                        tracing::debug!("RustKit chrome: contextmenu not cancelled, no menu shown");
                    }
                    chrome.input_view()
                } else {
                    None
                };
                // The RustKit panels (CH3.A4): the same turn as the chrome's,
                // their messages to the WebKit views' handlers.
                // One not opened yet has no view (CH3.A4b) and no turn.
                let panel_inputs = [
                    shelf_for_events.get().and_then(|shelf| pump_rustkit_panel(
                        shelf,
                        "shelf",
                        &mut |body| {
                            handle_shelf_ipc(body, &chrome_ipc_proxy, &chrome_ipc_state);
                            handle_shelf_find_ipc(body, &chrome_ipc_proxy);
                        },
                        &mut devtools,
                        &*inspector_for_events,
                        &click_proxy,
                    )),
                    inspector_for_events.get().and_then(|inspector| pump_rustkit_panel(
                        inspector,
                        "Inspector",
                        &mut |body| handle_inspector_ipc(body, &chrome_ipc_proxy),
                        &mut devtools,
                        &*inspector_for_events,
                        &click_proxy,
                    )),
                    match settings_window_for_events.lock().as_deref() {
                        Ok(Some((_, settings))) => pump_rustkit_panel(
                            settings,
                            "settings",
                            &mut |body| handle_settings_ipc(body, &chrome_ipc_proxy),
                            &mut devtools,
                            &*inspector_for_events,
                            &click_proxy,
                        ),
                        _ => None,
                    },
                ];
                // Process RustKit events and render
                if let UnifiedContentWebView::RustKit(ref view) = *content_for_events {
                    // A load in progress gets one slice, then the loop goes back to
                    // tao, which dispatches the clicks, keys and chrome messages that
                    // came meanwhile (a load used to hold the window to its end). The
                    // page's input waits in the viewhost queue for the new page.
                    if view.step_load() {
                        live_wake.set(Some(std::time::Instant::now() + LOAD_WAKE));
                    } else {
                        // The page's clock catches up before it hears any input:
                        // a timer a click sets counts from the click, not from
                        // the last time the loop woke.
                        view.process_events();
                        // Clicks come from the content NSView's own handlers in
                        // VIEW-LOCAL coordinates — already viewport space, no
                        // chrome-height/sidebar math and none of its staleness
                        // bugs. The old window-loop MouseInput arm never fired:
                        // events delivered to a child NSView do not surface as
                        // tao window events (measured with a synthetic
                        // sendEvent:, 2026-08-07).
                        // One queue, drained in arrival order across clicks,
                        // the wheel and keys (threads plan M2, #575): until
                        // 2026-10-14 three queues were drained one after another,
                        // so a click made after a wheel burst reached the page
                        // before the burst, and a key typed before a click after
                        // it. An event for a view that is gone was dropped by the
                        // viewhost when the view was destroyed.
                        for event in rustkit_viewhost::drain_input() {
                            // Input on the RustKit chrome's view is not the
                            // page's: the chrome's arm above takes it, and
                            // one that came since goes to the chrome too.
                            let route = route_input(event.view, chrome_input, &panel_inputs);
                            if let InputRoute::Panel(at) = route {
                                // A RustKit panel's (CH3.A4), as the chrome's.
                                let settings = settings_window_for_events.lock();
                                let panel = match at {
                                    0 => shelf_for_events.get(),
                                    1 => inspector_for_events.get(),
                                    _ => settings.as_deref().ok().and_then(|s| s.as_ref()).map(|(_, v)| v),
                                };
                                if let Some(PanelView::RustKit(view)) = panel {
                                    view.note_input(event.at, event.seq);
                                    dispatch_view_input(
                                        view,
                                        InputTarget::Panel,
                                        event.kind,
                                        &mut devtools,
                                        &*inspector_for_events,
                                        &click_proxy,
                                    );
                                }
                                continue;
                            }
                            if route == InputRoute::Chrome {
                                if let ChromeView::RustKit(ref chrome) = *chrome_for_events {
                                    chrome.note_input(event.at, event.seq);
                                    dispatch_view_input(
                                        chrome,
                                        InputTarget::Chrome,
                                        event.kind,
                                        &mut devtools,
                                        &inspector_for_events,
                                        &click_proxy,
                                    );
                                }
                                continue;
                            }
                            // When it arrived, for the W-gate's input-to-present
                            // figure (logged by the presenter with the frame).
                            view.note_input(event.at, event.seq);
                            // CH3.A3: a press on the page while a chrome
                            // popup is open is a click outside it, which
                            // the chrome's own handler closes it on.
                            if let rustkit_viewhost::InputKind::Pointer(click) = &event.kind {
                                if click.down
                                    && click.input == rustkit_viewhost::PointerInput::Button
                                    && chrome_for_events.has_overlays()
                                {
                                    let _ = chrome_for_events.eval("window.__hiwaveOutsideClick && __hiwaveOutsideClick();");
                                }
                            }
                            dispatch_view_input(view, InputTarget::Page, event.kind, &mut devtools, &inspector_for_events, &click_proxy);
                        }
                        // Timers that came due, late fetches, and what the
                        // input above started; then sleep until the next timer.
                        let sleep = view.process_events();
                        live_wake.set(sleep.map(|d| std::time::Instant::now() + d));
                        // A navigation the page started itself: `location.href
                        // = url`, a script's `link.click()` or `form.submit()`,
                        // a submit from a timer. Until 2026-10-06 these were
                        // dropped: only the user's own click on a link or a
                        // submit button went anywhere (hand tests H14, H16).
                        if let Some((url, from_user)) = view.take_script_navigation() {
                            if from_user {
                                info!(%url, "Link clicked");
                            } else {
                                info!(%url, "Script navigation");
                            }
                            let _ = click_proxy.send_event(UserEvent::Navigate(url));
                        }
                        // CH2.E9: what the page committed, as the WebKit
                        // content view's handlers report it off macOS. Until
                        // 2026-11-05 nothing read these on macOS: the address
                        // bar kept the typed URL after a redirect, and the tab
                        // never got its title, its spinner's end or a visit.
                        {
                            use crate::webview::IWebContent as _;
                            let updates = view.take_chrome_updates();
                            if let Some(url) = updates.url {
                                let _ = click_proxy.send_event(UserEvent::UpdateUrl(url.clone()));
                                let _ = click_proxy.send_event(UserEvent::UpdateActiveTabUrl(url));
                                let _ = click_proxy.send_event(UserEvent::NavigationStateChanged {
                                    can_go_back: view.can_go_back(),
                                    can_go_forward: view.can_go_forward(),
                                });
                            }
                            if let Some(title) = updates.title {
                                let _ = click_proxy.send_event(UserEvent::UpdateTitle(title));
                            }
                            if updates.started {
                                let _ = click_proxy.send_event(UserEvent::SetLoading(true));
                            }
                            if updates.loading == Some(false) {
                                let _ = click_proxy.send_event(UserEvent::SetLoading(false));
                                let _ = click_proxy.send_event(UserEvent::SyncShieldStats);
                            }
                            if let Some(url) = updates.finished {
                                let _ = click_proxy.send_event(UserEvent::RecordVisit { url });
                            }
                        }
                        // The page's console lines, and a new tree after it
                        // changed. No clock: the devtools thread wakes the
                        // window when the open panel's view records (T6b).
                        devtools.pump(view, &inspector_for_events);
                        view.render();
                        // CH2.E4: a right press the page let through.
                        if let Some((x, y)) = view.take_context_menu() {
                            show_page_context_menu(view, x, y);
                        }
                    }
                }
            }
            Event::UserEvent(user_event) => {
                match user_event {
                    UserEvent::Navigate(url) => {
                        info!("Navigating content to: {}", url);
                        if is_new_tab_url(&url) {
                            #[cfg(target_os = "macos")]
                            load_new_tab(&*content_for_events);
                            #[cfg(not(target_os = "macos"))]
                            load_new_tab(&*content_for_events);
                            let _ = chrome_for_events.eval("if(window.hiwaveChrome) { hiwaveChrome.updateUrl(''); }",
                            );
                            sync_shield_to_chrome(&state_for_events, &chrome_for_events);
                        } else if is_about_url(&url) {
                            // Update tab URL BEFORE loading the about page
                            // This prevents the data URL from overwriting the tab URL
                            if let Ok(mut s) = state_for_events.lock() {
                                if let Some(tab_id) = s.shell.get_active_tab().map(|tab| tab.id) {
                                    if let Ok(parsed_url) = url::Url::parse(ABOUT_URL) {
                                        let _ = s.shell.update_tab_url(tab_id, parsed_url);
                                    }
                                }
                            }
                            #[cfg(target_os = "macos")]
                            load_about_page(&*content_for_events);
                            #[cfg(not(target_os = "macos"))]
                            load_about_page(&*content_for_events);
                            let _ = chrome_for_events.eval("if(window.hiwaveChrome) { hiwaveChrome.updateUrl('hiwave://about'); }",
                            );
                        } else if is_report_url(&url) {
                            // Update tab URL BEFORE loading the report page
                            if let Ok(mut s) = state_for_events.lock() {
                                if let Some(tab_id) = s.shell.get_active_tab().map(|tab| tab.id) {
                                    if let Ok(parsed_url) = url::Url::parse(REPORT_URL) {
                                        let _ = s.shell.update_tab_url(tab_id, parsed_url);
                                    }
                                }
                            }
                            #[cfg(target_os = "macos")]
                            load_report_page(&*content_for_events);
                            #[cfg(not(target_os = "macos"))]
                            load_report_page(&*content_for_events);
                            let _ = chrome_for_events.eval("if(window.hiwaveChrome) { hiwaveChrome.updateUrl('hiwave://report'); }",
                            );
                        } else if !url.starts_with("about:") && !url.starts_with("hiwave://") {
                            // Skip about: URLs (about:blank, about:srcdoc, etc.)
                            // Skip hiwave:// protocol URLs (handled above if recognized, otherwise ignore)
                            let full_url = if url.starts_with("http://") || url.starts_with("https://") {
                                url
                            } else if url.contains('.') {
                                format!("https://{}", url)
                            } else {
                                format!("https://duckduckgo.com/?q={}", urlencoding::encode(&url))
                            };
                            // Update the tab model BEFORE loading, like the
                            // about/report branches above. Without this the
                            // URL bar showed the typed URL only until the
                            // next renderTabs, which rewrote it from the
                            // stale model (live run 2026-08-05: typed
                            // wikipedia, bar snapped back to the old eBay
                            // URL).
                            if let Ok(mut s) = state_for_events.lock() {
                                if let Some(tab_id) = s.shell.get_active_tab().map(|tab| tab.id) {
                                    if let Ok(parsed_url) = url::Url::parse(&full_url) {
                                        let _ = s.shell.update_tab_url(tab_id, parsed_url);
                                    }
                                }
                            }
                            #[cfg(target_os = "macos")]
                            {
                                if let Err(e) = content_for_events.load_url(&full_url) {
                                    error!("Failed to navigate: {}", e);
                                }
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                if let Err(e) = content_for_events.load_url(&full_url) {
                                    error!("Failed to navigate: {}", e);
                                }
                            }
                            let script = format!(
                                "if(window.hiwaveChrome) {{ hiwaveChrome.updateUrl('{}'); }}",
                                full_url.replace("'", "\\'")
                            );
                            let _ = chrome_for_events.eval(&script);
                        }
                    }
                    UserEvent::GoBack => {
                        info!("Going back");
                        #[cfg(target_os = "macos")]
                        {
                            // Real navigation on RustKit; JS only reaches the
                            // WRY fallback (page JS does not execute in
                            // RustKit, so the old evaluate_script stub was a
                            // silent no-op — live finding 2026-08-07).
                            #[cfg(feature = "rustkit")]
                            if let UnifiedContentWebView::RustKit(ref v) = *content_for_events {
                                if !v.nav_back() {
                                    info!("GoBack: nowhere to go");
                                }
                            } else {
                                let _ = content_for_events.evaluate_script("history.back();");
                            }
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = content_for_events.evaluate_script("history.back();");
                        }
                    }
                    UserEvent::GoForward => {
                        info!("Going forward");
                        #[cfg(target_os = "macos")]
                        {
                            // Real navigation on RustKit; JS only reaches the
                            // WRY fallback (page JS does not execute in
                            // RustKit, so the old evaluate_script stub was a
                            // silent no-op — live finding 2026-08-07).
                            #[cfg(feature = "rustkit")]
                            if let UnifiedContentWebView::RustKit(ref v) = *content_for_events {
                                if !v.nav_forward() {
                                    info!("GoForward: nowhere to go");
                                }
                            } else {
                                let _ = content_for_events.evaluate_script("history.forward();");
                            }
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = content_for_events.evaluate_script("history.forward();");
                        }
                    }
                    UserEvent::Reload => {
                        info!("Reloading");
                        #[cfg(target_os = "macos")]
                        {
                            // Real navigation on RustKit; JS only reaches the
                            // WRY fallback (page JS does not execute in
                            // RustKit, so the old evaluate_script stub was a
                            // silent no-op — live finding 2026-08-07).
                            #[cfg(feature = "rustkit")]
                            if let UnifiedContentWebView::RustKit(ref v) = *content_for_events {
                                if !v.nav_reload() {
                                    info!("Reload: nowhere to go");
                                }
                            } else {
                                let _ = content_for_events.evaluate_script("location.reload();");
                            }
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = content_for_events.evaluate_script("location.reload();");
                        }
                    }
                    UserEvent::Stop => {
                        info!("Stopping load");
                        #[cfg(target_os = "macos")]
                        {
                            // The load in progress, cancelled without the
                            // engine it holds (M3b's canceller).
                            #[cfg(feature = "rustkit")]
                            if let UnifiedContentWebView::RustKit(ref v) = *content_for_events {
                                if !v.stop_load() {
                                    info!("Stop: nothing loading");
                                }
                            } else {
                                let _ = content_for_events.evaluate_script("window.stop();");
                            }
                            #[cfg(not(feature = "rustkit"))]
                            let _ = content_for_events.evaluate_script("window.stop();");
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = content_for_events.evaluate_script("window.stop();");
                        }
                    }
                    UserEvent::NewTab => {
                        info!("New tab - navigating to start page");
                        #[cfg(target_os = "macos")]
                        load_new_tab(&*content_for_events);
                        #[cfg(not(target_os = "macos"))]
                        load_new_tab(&*content_for_events);
                        let _ = chrome_for_events.eval("if(window.hiwaveChrome) { hiwaveChrome.updateUrl(''); }",
                        );
                        sync_shield_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::UpdateTitle(title) => {
                        let trimmed = title.trim();
                        // Only update title if non-empty to prevent reverting to "New Tab"
                        // during navigation when the document title temporarily becomes empty
                        if !trimmed.is_empty() {
                            if let Ok(mut s) = state_for_events.lock() {
                                let active_tab_id = s.shell.get_active_tab().map(|tab| tab.id);
                                if let Some(tab_id) = active_tab_id {
                                    if let Some(tab) = s.shell.get_tab_mut(tab_id) {
                                        tab.title = Some(trimmed.to_string());
                                    }
                                }
                                if let Err(e) = s.save_workspace_state() {
                                    error!("Failed to persist workspace state: {}", e);
                                }
                            }
                            let script = format!(
                                "if(window.hiwaveChrome) {{ hiwaveChrome.updateTitle('{}'); }}",
                                title.replace("'", "\\'")
                            );
                            let _ = chrome_for_events.eval(&script);
                            sync_tabs_to_chrome(&state_for_events, &chrome_for_events);
                            // Also sync workspaces so sidebar tab info updates
                            sync_workspaces_to_chrome(&state_for_events, &chrome_for_events);
                        }
                    }
                    UserEvent::UpdateUrl(url) => {
                        let script = format!(
                            "if(window.hiwaveChrome) {{ hiwaveChrome.updateUrl('{}'); }}",
                            url.replace("'", "\\'")
                        );
                        let _ = chrome_for_events.eval(&script);
                        sync_shield_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::UpdateActiveTabUrl(url) => {
                        if let Ok(mut s) = state_for_events.lock() {
                            let active_tab_id = s.shell.get_active_tab().map(|tab| tab.id);
                            if let Some(tab_id) = active_tab_id {
                                if let Ok(parsed_url) = url::Url::parse(&url) {
                            if let Err(e) = s.shell.update_tab_url(tab_id, parsed_url) {
                                error!("Failed to update tab URL: {}", e);
                            }
                            if let Err(e) = s.touch_tab(tab_id) {
                                error!("Failed to record tab visit: {}", e);
                            }
                                }
                                if let Err(e) = s.save_workspace_state() {
                                    error!("Failed to persist workspace state: {}", e);
                                }
                            }
                        }
                        sync_tabs_to_chrome(&state_for_events, &chrome_for_events);
                        // Also sync workspaces so sidebar tab info updates
                        sync_workspaces_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::NavigationStateChanged { can_go_back, can_go_forward } => {
                        let script = format!(
                            "if(window.hiwaveChrome) {{ hiwaveChrome.updateNavState({}, {}); }}",
                            can_go_back, can_go_forward
                        );
                        let _ = chrome_for_events.eval(&script);
                    }
                    UserEvent::SetLoading(loading) => {
                        let script = format!(
                            "if(window.hiwaveChrome) {{ hiwaveChrome.setLoading({}); }}",
                            loading
                        );
                        let _ = chrome_for_events.eval(&script);
                        // Inject context menu handler and audio detector when page finishes loading.
                        // Not on macOS: the content view is RustKit, which never
                        // ran these WebKit helpers (no SetLoading was sent there
                        // before CH2.E9), and they post to the WebKit IPC.
                        #[cfg(not(target_os = "macos"))]
                        if !loading {
                            info!("Injecting context menu handler into content webview");
                            let _ = content_for_events.evaluate_script(CONTEXT_MENU_HELPER);
                            info!("Injecting audio detector into content webview");
                            let _ = content_for_events.evaluate_script(AUDIO_DETECTOR);
                            info!("Injecting autofill helper into content webview");
                            let _ = content_for_events.evaluate_script(AUTOFILL_HELPER);
                            info!("Injecting console interceptor into content webview");
                            let _ = content_for_events.evaluate_script(CONSOLE_INJECT);
                        }
                    }
                    UserEvent::RecordVisit { url } => {
                        if should_record_history_url(&url) {
                            if let Ok(mut s) = state_for_events.lock() {
                                let title = s
                                    .shell
                                    .get_active_tab()
                                    .and_then(|tab| tab.title.clone());
                                let workspace = s
                                    .shell
                                    .get_active_workspace()
                                    .map(|ws| ws.name.clone());
                                if let Err(err) = s.record_visit(&url, title, workspace.clone()) {
                                    error!("Failed to persist history entry: {}", err);
                                }

                                // Track page visit (domain only)
                                if let Ok(parsed_url) = url::Url::parse(&url) {
                                    if let Some(domain) = parsed_url.host_str() {
                                        let workspace_id = s.shell.get_active_workspace()
                                            .map(|w| w.id.0.to_string());
                                        if let Err(e) = s.analytics.track_page_visit(
                                            domain,
                                            workspace_id.as_deref()
                                        ) {
                                            error!("Failed to track page visit: {}", e);
                                        }
                                    }
                                }

                                // Update focus mode page loaded timestamp for auto-trigger
                                s.focus_mode.record_navigation(&url);
                            }
                        }
                        sync_history_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::FindInPageResult(payload) => {
                        if payload.is_null() {
                            return;
                        }
                        if let Ok(json) = serde_json::to_string(&payload) {
                            let update_script = format!(
                                "if(window.hiwaveChrome) {{ hiwaveChrome.updateFindState({}); }}",
                                json
                            );
                            let _ = chrome_for_events.eval(&update_script);
                        }
                    }
                    UserEvent::FindInPage { query, case_sensitive, direction } => {
                        let payload = serde_json::json!({
                            "query": query,
                            "case_sensitive": case_sensitive,
                            "direction": direction,
                        });
                        let script = format!(
                            "{}\n(() => {{\n    const result = window.__hiwaveFind && window.__hiwaveFind.run({});\n    if (window.ipc && result) {{\n        window.ipc.postMessage(JSON.stringify({{ cmd: 'find_in_page_result', result }}));\n    }}\n}})();",
                            FIND_IN_PAGE_HELPER,
                            payload.to_string()
                        );
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    UserEvent::SyncTabs => {
                        info!("Syncing tabs to Chrome UI");
                        sync_tabs_to_chrome(&state_for_events, &chrome_for_events);
                        sync_shield_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::SyncWorkspaces => {
                        info!("Syncing workspaces to Chrome UI");
                        sync_workspaces_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::SyncDownloads => {
                        sync_downloads_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::SyncShelf(scope) => {
                        sync_shelf_to_chrome(&state_for_events, &chrome_for_events, scope);
                    }
                    UserEvent::SyncBlocklist => {
                        sync_blocklist_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::SyncHistory => {
                        sync_history_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::SyncShieldStats => {
                        sync_shield_to_chrome(&state_for_events, &chrome_for_events);
                    }
                    UserEvent::ShowCommands(json) => {
                        let script = format!(
                            "if(window.hiwaveShelf) {{ hiwaveShelf.showCommands({}); }}",
                            json
                        );
                        let _ = shelf_for_events.eval(&script);
                    }
                    UserEvent::ExpandChrome if rustkit_chrome_enabled() => {
                        // CH3.A3: the RustKit chrome is over the page; its
                        // panel is drawn over it, and the frame that shows
                        // the panel brings it into the region (CH3.A3b).
                    }
                    UserEvent::ExpandChrome => {
                        // Update chrome height
                        *chrome_height_for_events.lock().unwrap() = CHROME_HEIGHT_EXPANDED as f64;
                        let ch = CHROME_HEIGHT_EXPANDED as f64;
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sidebar_open = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sidebar_open,
                            right_sidebar_open,
                        );
                    }
                    UserEvent::ExpandChromeSmall => {
                        // Update chrome height (small expansion for tab actions panel)
                        *chrome_height_for_events.lock().unwrap() = CHROME_HEIGHT_SMALL as f64;
                        let ch = CHROME_HEIGHT_SMALL as f64;
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sidebar_open = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sidebar_open,
                            right_sidebar_open,
                        );
                    }
                    UserEvent::CollapseChrome => {
                        // Update chrome height
                        *chrome_height_for_events.lock().unwrap() = CHROME_HEIGHT_DEFAULT as f64;
                        let ch = CHROME_HEIGHT_DEFAULT as f64;
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sidebar_open = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sidebar_open,
                            right_sidebar_open,
                        );
                    }
                    UserEvent::ExpandShelf => {
                        let ch = *chrome_height_for_events.lock().unwrap();

                        // Update shelf height
                        *shelf_height_for_events.lock().unwrap() = SHELF_HEIGHT_EXPANDED as f64;
                        let sh = SHELF_HEIGHT_EXPANDED as f64;
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sidebar_open = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sidebar_open,
                            right_sidebar_open,
                        );

                        // Focus the command palette input and trigger initial search
                        let _ = shelf_for_events.eval(r#"
                            if(window.hiwaveShelf) {
                                hiwaveShelf.clear();
                                hiwaveShelf.focus();
                                // Trigger initial search after a brief delay to ensure IPC is ready
                                setTimeout(() => {
                                    const input = document.getElementById('commandInput');
                                    if (input && window.ipc) {
                                        window.ipc.postMessage(JSON.stringify({ cmd: 'search_commands', query: '' }));
                                    }
                                }, 50);
                            }
                        "#);
                        // The RustKit shelf takes the keyboard (CH3.A4).
                        shelf_for_events.take_keyboard();
                    }
                    UserEvent::CollapseShelf => {
                        let ch = *chrome_height_for_events.lock().unwrap();

                        // Update shelf height
                        *shelf_height_for_events.lock().unwrap() = SHELF_HEIGHT_DEFAULT as f64;
                        let sh = SHELF_HEIGHT_DEFAULT as f64;
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sidebar_open = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sidebar_open,
                            right_sidebar_open,
                        );
                        // The RustKit shelf gives the keyboard back to the page (CH3.A4).
                        #[cfg(all(target_os = "macos", feature = "rustkit"))]
                        panel_closed(&shelf_for_events, &content_for_events);
                    }
                    UserEvent::SetSidebarOpen(open) => {
                        // Get current width or use default when opening
                        let new_width = if open { SIDEBAR_WIDTH } else { 0.0 };
                        *sidebar_width_for_events.lock().unwrap() = new_width;
                        let ch = *chrome_height_for_events.lock().unwrap();
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            new_width,
                            right_sidebar_open,
                        );
                    }
                    UserEvent::SetSidebarWidth(width) => {
                        // Update sidebar width during resize drag
                        *sidebar_width_for_events.lock().unwrap() = width;
                        let ch = *chrome_height_for_events.lock().unwrap();
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            width,
                            right_sidebar_open,
                        );
                    }
                    UserEvent::SetRightPanelOpen(open) => {
                        // Update right sidebar state and relayout
                        *right_sidebar_open_for_events.lock().unwrap() = open;
                        let ch = *chrome_height_for_events.lock().unwrap();
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sw = *sidebar_width_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sw,
                            open,
                        );
                        info!("Right panel: {}", if open { "open" } else { "closed" });
                    }
                    // Focus mode event handlers
                    UserEvent::EnterFocusMode => {
                        info!("Entering focus mode - hiding chrome UI");
                        // In focus mode, keep a small chrome area (10px) for the reveal zone
                        // This allows mouse hover at top to trigger peek mode
                        // Content takes most of the window
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            10.0, // Minimal chrome height for reveal zone hover detection
                            0.0,  // No shelf height
                            0.0,  // No inspector height in focus mode
                            0.0,  // Sidebar hidden (width = 0)
                            false, // Right sidebar hidden
                        );
                        // Get focus mode config to pass to chrome
                        let config = {
                            let state = focus_state_for_events.lock().unwrap();
                            serde_json::json!({
                                "show_progress_bar": state.focus_mode_config.show_progress_bar,
                                "hide_cursor": state.focus_mode_config.hide_cursor,
                                "cursor_hide_delay_secs": state.focus_mode_config.cursor_hide_delay_secs,
                            })
                        };
                        // Notify chrome of focus mode state with config
                        let script = format!(
                            "if(window.hiwaveChrome) {{ hiwaveChrome.setFocusMode(true, {}); }}",
                            config
                        );
                        let _ = chrome_for_events.eval(&script);
                    }
                    UserEvent::ExitFocusMode => {
                        info!("Exiting focus mode - restoring chrome UI");
                        // Restore normal layout
                        let ch = *chrome_height_for_events.lock().unwrap();
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = *inspector_height_for_events.lock().unwrap();
                        let sidebar_open = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sidebar_open,
                            right_sidebar_open,
                        );
                        // Notify chrome of focus mode state - call setFocusMode immediately
                        match chrome_for_events.eval("if(window.hiwaveChrome) { \
                                console.log('[Focus] ExitFocusMode event - calling setFocusMode(false)'); \
                                hiwaveChrome.setFocusMode(false); \
                            } else { \
                                console.error('[Focus] hiwaveChrome not available!'); \
                            }"
                        ) {
                            Ok(_) => info!("Successfully called setFocusMode(false)"),
                            Err(e) => error!("Failed to call setFocusMode(false): {}", e),
                        }
                    }
                    UserEvent::ToggleFocusMode => {
                        // State was already toggled by IPC handler, so check the NEW state
                        // If active is now true, we should enter focus mode
                        // If active is now false, we should exit focus mode
                        let is_focus_mode = {
                            let state = focus_state_for_events.lock().unwrap();
                            state.focus_mode.active
                        };
                        if is_focus_mode {
                            let _ = proxy.send_event(UserEvent::EnterFocusMode);
                        } else {
                            let _ = proxy.send_event(UserEvent::ExitFocusMode);
                        }
                    }
                    UserEvent::SyncFocusModeStatus => {
                        // Sync focus mode status to chrome
                        if let Ok(state) = focus_state_for_events.lock() {
                            let status = serde_json::json!({
                                "active": state.focus_mode.active,
                                "peek_visible": state.focus_mode.peek_visible,
                                "scroll_progress": state.focus_mode.scroll_progress,
                            });
                            let script = format!(
                                "if(window.hiwaveChrome) {{ hiwaveChrome.updateFocusModeStatus({}); }}",
                                status
                            );
                            let _ = chrome_for_events.eval(&script);
                        }
                    }
                    UserEvent::ShowFocusPeek => {
                        info!("Showing focus mode peek UI");
                        // In peek mode, show minimal chrome (60px) at top
                        let peek_height = 60.0;
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            peek_height,
                            0.0,  // No shelf in peek mode
                            0.0,  // No inspector in peek mode
                            0.0,  // No sidebar in peek mode (width = 0)
                            false,  // No right sidebar in peek mode
                        );
                    }
                    UserEvent::HideFocusPeek => {
                        info!("Hiding focus mode peek UI");
                        // Return to full focus mode - no chrome visible
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            0.0,  // No chrome
                            0.0,  // No shelf
                            0.0,  // No inspector
                            0.0,  // No sidebar (width = 0)
                            false,  // No right sidebar
                        );
                    }
                    UserEvent::FocusAutoTriggerCheck => {
                        // Check if auto-focus should trigger
                        let (should_enter, debug_reason) = {
                            let state = focus_state_for_events.lock().unwrap();
                            let result = state.focus_mode.should_auto_enter(&state.focus_mode_config);
                            let reason = if state.focus_mode.active {
                                "already active"
                            } else if state.focus_mode.auto_triggered_for_page {
                                "already triggered for this page"
                            } else if !state.focus_mode_config.auto_enabled {
                                "auto-trigger disabled"
                            } else if state.focus_mode.page_loaded_at.is_none() {
                                "no page loaded"
                            } else {
                                let elapsed = state.focus_mode.page_loaded_at.unwrap().elapsed().as_secs();
                                if elapsed < state.focus_mode_config.activation_delay_secs as u64 {
                                    "waiting for delay"
                                } else {
                                    "ready"
                                }
                            };
                            (result, reason)
                        };

                        // Log every 30 seconds (6 checks) when debug enabled
                        static CHECK_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                        let count = CHECK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if count % 6 == 0 && std::env::var("HIWAVE_DEBUG").is_ok() {
                            info!("Focus auto-trigger check: {} ({})", should_enter, debug_reason);
                        }

                        if should_enter {
                            info!("Auto-triggering focus mode");
                            // Update state and mark as auto-triggered for this page
                            {
                                let mut state = focus_state_for_events.lock().unwrap();
                                state.focus_mode.enter();
                                state.focus_mode.auto_triggered_for_page = true;
                            }
                            // Trigger the UI update
                            let _ = proxy.send_event(UserEvent::EnterFocusMode);
                        }
                    }
                    UserEvent::OpenSettings => {
                        info!("Opening settings window");
                        let mut settings_guard = settings_window_for_events.lock().unwrap();

                        if let Some((ref settings_win, _)) = *settings_guard {
                            settings_win.set_visible(true);
                            settings_win.set_focus();
                        } else {
                            let settings_win = WindowBuilder::new()
                                .with_title("HiWave Settings")
                                .with_inner_size(LogicalSize::new(720.0, 680.0))
                                .with_min_inner_size(LogicalSize::new(600.0, 500.0))
                                .with_resizable(true)
                                .build(event_loop_target)
                                .expect("Failed to create settings window");

                            let settings_proxy = settings_proxy_for_handler.clone();
                            let settings_rect = Rect {
                                position: LogicalPosition::new(0, 0).into(),
                                size: {
                                    let (size, scale) = (settings_win.inner_size(), settings_win.scale_factor());
                                    LogicalSize::new(size.width as f64 / scale, size.height as f64 / scale).into()
                                },
                            };
                            let settings_webview = if let Some(view) = rustkit_panel(&settings_win, settings_rect, SETTINGS_HTML, "settings page") { view } else { PanelView::WebKit(WebViewBuilder::new()
                                .with_html(SETTINGS_HTML)
                                .with_devtools(cfg!(debug_assertions))
                                .with_clipboard(true)
                                .with_initialization_script(JS_BRIDGE)
                                .with_ipc_handler(move |msg: wry::http::Request<String>| {
                                    handle_settings_ipc(msg.body(), &settings_proxy)
                                })
                                .build(&settings_win)
                                .expect("Failed to create settings WebView")) };

                            *settings_guard = Some((settings_win, settings_webview));
                        }
                    }
                    UserEvent::CloseSettings => {
                        info!("Closing settings window");
                        let settings_guard = settings_window_for_events.lock().unwrap();
                        if let Some((ref settings_win, _)) = *settings_guard {
                            settings_win.set_visible(false);
                        }
                    }
                    UserEvent::GetBrowserProfiles(browser) => {
                        info!("Getting browser profiles for: {}", browser);
                        let browser_type = match browser.to_lowercase().as_str() {
                            "chrome" => import::Browser::Chrome,
                            "firefox" => import::Browser::Firefox,
                            "brave" => import::Browser::Brave,
                            _ => {
                                let error_json = serde_json::json!({
                                    "type": "profiles_result",
                                    "error": format!("Unknown browser: {}", browser)
                                }).to_string();
                                if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                                    let script = format!("window.postMessage({}, '*');", error_json);
                                    let _ = settings_wv.eval(&script);
                                }
                                return;
                            }
                        };

                        let profiles = import::get_browser_profiles(browser_type);
                        let profiles_json: Vec<serde_json::Value> = profiles
                            .iter()
                            .map(|p| serde_json::json!({
                                "name": p.name,
                                "path": p.path.to_string_lossy(),
                            }))
                            .collect();

                        let result_json = serde_json::json!({
                            "type": "profiles_result",
                            "profiles": profiles_json,
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ImportBrowserProfilesResult(json) => {
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ImportBookmarks { browser, profile_path } => {
                        info!("Importing bookmarks from {} at {}", browser, profile_path);
                        let browser_type = match browser.to_lowercase().as_str() {
                            "chrome" => import::Browser::Chrome,
                            "firefox" => import::Browser::Firefox,
                            "brave" => import::Browser::Brave,
                            _ => {
                                let error_json = serde_json::json!({
                                    "type": "import_result",
                                    "error": format!("Unknown browser: {}", browser)
                                }).to_string();
                                if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                                    let script = format!("window.postMessage({}, '*');", error_json);
                                    let _ = settings_wv.eval(&script);
                                }
                                return;
                            }
                        };

                        let profile = import::BrowserProfile {
                            name: "Import".to_string(),
                            path: std::path::PathBuf::from(&profile_path),
                            browser: browser_type,
                        };

                        // Parse bookmarks
                        let bookmarks = match import::import_bookmarks(&profile) {
                            Ok(b) => b,
                            Err(e) => {
                                let error_json = serde_json::json!({
                                    "type": "import_result",
                                    "error": format!("Failed to parse bookmarks: {}", e)
                                }).to_string();
                                if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                                    let script = format!("window.postMessage({}, '*');", error_json);
                                    let _ = settings_wv.eval(&script);
                                }
                                return;
                            }
                        };

                        if bookmarks.is_empty() {
                            let result_json = serde_json::json!({
                                "type": "import_result",
                                "stats": {
                                    "workspaces_created": 0,
                                    "tabs_created": 0,
                                },
                                "message": "No bookmarks found to import"
                            }).to_string();
                            if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                                let script = format!("window.postMessage({}, '*');", result_json);
                                let _ = settings_wv.eval(&script);
                            }
                            return;
                        }

                        // Convert to workspaces
                        let config = import::converter::ConversionConfig::for_browser(browser_type);
                        let result = import::converter::convert_to_workspaces(bookmarks, &config);

                        // Add workspaces and tabs
                        let mut state = state.lock().unwrap();
                        let mut workspaces_created = 0;
                        let mut tabs_created = 0;

                        for import_ws in &result.workspaces {
                            let ws_id = state.shell.create_workspace(import_ws.name.clone());
                            workspaces_created += 1;

                            for tab in &import_ws.tabs {
                                let parsed_url = url::Url::parse(&tab.url)
                                    .unwrap_or_else(|_| url::Url::parse("about:blank").unwrap());

                                let tab_info = hiwave_core::types::TabInfo {
                                    id: hiwave_core::types::TabId::new(),
                                    url: parsed_url,
                                    title: Some(tab.title.clone()),
                                    favicon: None,
                                    workspace_id: ws_id,
                                    suspended: false,
                                    loading: false,
                                    locked: false,
                                    last_visited: None,
                                };

                                if state.shell.create_tab(tab_info).is_ok() {
                                    tabs_created += 1;
                                }
                            }
                        }

                        // Save state
                        if let Err(e) = state.save_workspace_state() {
                            warn!("Failed to save state after import: {}", e);
                        }

                        // Update UI
                        let _ = proxy.send_event(UserEvent::SyncWorkspaces);

                        let result_json = serde_json::json!({
                            "type": "import_result",
                            "stats": {
                                "workspaces_created": workspaces_created,
                                "tabs_created": tabs_created,
                            }
                        }).to_string();

                        drop(state); // Release lock before accessing settings window

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }

                        info!("Import complete: {} workspaces, {} tabs", workspaces_created, tabs_created);
                    }
                    UserEvent::ImportBookmarksResult(json) => {
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::GetSettings => {
                        let state = state_for_events.lock().unwrap();
                        let settings = state.get_settings();
                        let result_json = serde_json::json!({
                            "type": "settings_result",
                            "data": settings
                        }).to_string();
                        drop(state);

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::SettingsResult(json) => {
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::SaveSettings(settings_value) => {
                        let result_json = {
                            let mut state = state_for_events.lock().unwrap();
                            save_settings_reply(&mut state, settings_value)
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::SettingsSaved => {
                        let result_json = serde_json::json!({ "type": "settings_saved" }).to_string();
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ExportData { include_settings, include_workspaces } => {
                        let state = state_for_events.lock().unwrap();
                        let export = state.export_data(include_settings, include_workspaces);
                        let result_json = match serde_json::to_string_pretty(&export) {
                            Ok(data) => serde_json::json!({
                                "type": "export_result",
                                "data": data,
                                "workspaces_count": export.workspaces.len()
                            }),
                            Err(e) => serde_json::json!({
                                "type": "export_result",
                                "error": e.to_string()
                            }),
                        }.to_string();
                        drop(state);

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ExportDataResult(json) => {
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::SaveExportToFile { data, suggested_name } => {
                        // Show native save dialog
                        let result = rfd::FileDialog::new()
                            .set_title("Export HiWave Data")
                            .set_file_name(&suggested_name)
                            .add_filter("JSON", &["json"])
                            .save_file();

                        let result_json = match result {
                            Some(path) => {
                                match std::fs::write(&path, &data) {
                                    Ok(_) => {
                                        info!("Exported data to: {:?}", path);
                                        serde_json::json!({
                                            "type": "export_file_result",
                                            "success": true,
                                            "path": path.to_string_lossy()
                                        })
                                    }
                                    Err(e) => {
                                        error!("Failed to write export file: {}", e);
                                        serde_json::json!({
                                            "type": "export_file_result",
                                            "error": format!("Failed to write file: {}", e)
                                        })
                                    }
                                }
                            }
                            None => {
                                // User cancelled
                                serde_json::json!({
                                    "type": "export_file_result",
                                    "cancelled": true
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::PickImportFile => {
                        // Show native open dialog
                        let result = rfd::FileDialog::new()
                            .set_title("Import HiWave Data")
                            .add_filter("JSON", &["json"])
                            .pick_file();

                        let result_json = match result {
                            Some(path) => {
                                match std::fs::read_to_string(&path) {
                                    Ok(contents) => {
                                        info!("Loaded import file from: {:?}", path);
                                        serde_json::json!({
                                            "type": "import_file_picked",
                                            "data": contents,
                                            "path": path.to_string_lossy()
                                        })
                                    }
                                    Err(e) => {
                                        error!("Failed to read import file: {}", e);
                                        serde_json::json!({
                                            "type": "import_file_picked",
                                            "error": format!("Failed to read file: {}", e)
                                        })
                                    }
                                }
                            }
                            None => {
                                // User cancelled
                                serde_json::json!({
                                    "type": "import_file_picked",
                                    "cancelled": true
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ImportStateData { data, replace } => {
                        let result_json = match serde_json::from_str::<crate::state::HiWaveExport>(&data) {
                            Ok(import_data) => {
                                let mut state = state_for_events.lock().unwrap();
                                match state.import_data(&import_data, replace) {
                                    Ok(result) => serde_json::json!({
                                        "type": "import_data_result",
                                        "success": result.success,
                                        "workspaces_created": result.workspaces_created,
                                        "tabs_created": result.tabs_created,
                                        "errors": result.errors
                                    }),
                                    Err(e) => serde_json::json!({
                                        "type": "import_data_result",
                                        "error": e.to_string()
                                    }),
                                }
                            }
                            Err(e) => serde_json::json!({
                                "type": "import_data_result",
                                "error": format!("Invalid import format: {}", e)
                            }),
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ImportStateDataResult(json) => {
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ClearBrowsingData { history, downloads, shelf } => {
                        let mut state = state_for_events.lock().unwrap();
                        let mut cleared = Vec::new();

                        if history {
                            if state.clear_visit_history().is_ok() {
                                cleared.push("history");
                            }
                        }
                        if downloads {
                            state.clear_download_history();
                            cleared.push("downloads");
                        }
                        if shelf {
                            state.clear_shelf();
                            cleared.push("shelf");
                        }

                        let _ = state.save_workspace_state();
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "clear_data_result",
                            "cleared": cleared
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ClearBrowsingDataResult(json) => {
                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::GetCellar => {
                        let state = state_for_events.lock().unwrap();
                        let cellar = state.list_cellar();
                        let items: Vec<serde_json::Value> = cellar
                            .iter()
                            .map(|item| {
                                serde_json::json!({
                                    "id": item.id,
                                    "url": item.url,
                                    "title": item.title,
                                    "workspace": item.workspace,
                                    "originally_shelved": item.originally_shelved,
                                    "cellared_at": item.cellared_at,
                                })
                            })
                            .collect();
                        let result_json = serde_json::json!({
                            "type": "cellar_result",
                            "items": items,
                            "count": items.len()
                        }).to_string();
                        drop(state);

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::RestoreFromCellar { id } => {
                        let mut state = state_for_events.lock().unwrap();
                        let result_json = match state.restore_from_cellar(&id) {
                            Some(shelf_item) => {
                                let _ = state.save_workspace_state();
                                info!("Restored item from cellar: {}", id);
                                serde_json::json!({
                                    "type": "cellar_restore_result",
                                    "restored": true,
                                    "item": {
                                        "id": shelf_item.id,
                                        "url": shelf_item.url,
                                        "title": shelf_item.title,
                                        "workspace": shelf_item.workspace,
                                    }
                                })
                            }
                            None => {
                                serde_json::json!({
                                    "type": "cellar_restore_result",
                                    "error": format!("Cellar item not found: {}", id)
                                })
                            }
                        }.to_string();
                        drop(state);

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ClearCellar => {
                        let mut state = state_for_events.lock().unwrap();
                        state.clear_cellar();
                        let _ = state.save_workspace_state();
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "cellar_cleared"
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::GetFocusModeConfigForSettings => {
                        let state = state_for_events.lock().unwrap();
                        let config = &state.focus_mode_config;
                        let result_json = serde_json::json!({
                            "type": "focus_config_result",
                            "data": config
                        }).to_string();
                        drop(state);

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::SaveFocusModeConfigFromSettings(config_value) => {
                        let config: Result<crate::state::FocusModeConfig, _> = serde_json::from_value(config_value);
                        let result_json = match config {
                            Ok(c) => {
                                let mut state = state_for_events.lock().unwrap();
                                match state.update_focus_config(c) {
                                    Ok(_) => {
                                        info!("Focus mode config saved from settings");
                                        // Reset auto-trigger flag so new settings can take effect
                                        state.focus_mode.auto_triggered_for_page = false;
                                        serde_json::json!({ "type": "focus_config_saved" })
                                    }
                                    Err(e) => serde_json::json!({ "type": "focus_config_error", "error": e.to_string() })
                                }
                            }
                            Err(e) => serde_json::json!({ "type": "focus_config_error", "error": e.to_string() })
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::AddToFocusBlocklistFromSettings(domain) => {
                        let mut state = state_for_events.lock().unwrap();
                        let added = state.focus_mode_config.add_to_blocklist(domain.clone());
                        if added {
                            if let Err(e) = state.save_focus_config() {
                                warn!("Failed to save focus config: {}", e);
                            }
                        }
                        let blocklist = state.focus_mode_config.blocklist.clone();
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "focus_blocklist_updated",
                            "blocklist": blocklist,
                            "added": added,
                            "domain": domain
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::RemoveFromFocusBlocklistFromSettings(domain) => {
                        let mut state = state_for_events.lock().unwrap();
                        let removed = state.focus_mode_config.remove_from_blocklist(&domain);
                        if removed {
                            if let Err(e) = state.save_focus_config() {
                                warn!("Failed to save focus config: {}", e);
                            }
                        }
                        let blocklist = state.focus_mode_config.blocklist.clone();
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "focus_blocklist_updated",
                            "blocklist": blocklist,
                            "removed": removed,
                            "domain": domain
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    // Vault settings events
                    UserEvent::GetVaultStatus => {
                        let state = state_for_events.lock().unwrap();
                        let unlocked = state.vault.is_unlocked();
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "vault_status_result",
                            "unlocked": unlocked
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::UnlockVault(password) => {
                        let mut state = state_for_events.lock().unwrap();
                        let result = state.vault.unlock(&password);
                        drop(state);

                        let result_json = match result {
                            Ok(_) => {
                                info!("Vault unlocked from settings");
                                serde_json::json!({
                                    "type": "vault_unlock_result",
                                    "success": true
                                })
                            }
                            Err(e) => {
                                warn!("Failed to unlock vault: {}", e);
                                serde_json::json!({
                                    "type": "vault_unlock_result",
                                    "success": false,
                                    "error": "Invalid password"
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::LockVault => {
                        let mut state = state_for_events.lock().unwrap();
                        state.vault.lock();
                        info!("Vault locked from settings");
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "vault_lock_result",
                            "success": true
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::GetAllCredentials => {
                        let state = state_for_events.lock().unwrap();
                        let result = state.vault.get_all_credentials();
                        drop(state);

                        let result_json = match result {
                            Ok(creds) => {
                                let cred_list: Vec<serde_json::Value> = creds
                                    .iter()
                                    .map(|c| serde_json::json!({
                                        "id": c.id,
                                        "url": c.url,
                                        "username": c.username,
                                    }))
                                    .collect();
                                serde_json::json!({
                                    "type": "credentials_result",
                                    "credentials": cred_list
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "type": "credentials_result",
                                    "error": format!("{}", e)
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::SaveCredential { url, username, password } => {
                        let mut state = state_for_events.lock().unwrap();
                        let url_parsed = url::Url::parse(&url)
                            .or_else(|_| url::Url::parse(&format!("https://{}", url)));

                        let result = match url_parsed {
                            Ok(parsed_url) => state.vault.save_credential(&parsed_url, &username, &password),
                            Err(e) => Err(hiwave_core::HiWaveError::Vault(format!("Invalid URL: {}", e))),
                        };
                        drop(state);

                        let result_json = match result {
                            Ok(id) => {
                                info!("Credential saved from settings");
                                serde_json::json!({
                                    "type": "credential_saved",
                                    "success": true,
                                    "id": id
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "type": "credential_saved",
                                    "success": false,
                                    "error": format!("{}", e)
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::DeleteCredential(id) => {
                        let mut state = state_for_events.lock().unwrap();
                        let result = state.vault.delete_credential(id);
                        drop(state);

                        let result_json = match result {
                            Ok(_) => {
                                info!("Credential {} deleted from settings", id);
                                serde_json::json!({
                                    "type": "credential_deleted",
                                    "success": true,
                                    "id": id
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "type": "credential_deleted",
                                    "success": false,
                                    "error": format!("{}", e)
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    // Analytics settings events
                    UserEvent::GetAnalyticsSettings => {
                        let state = state_for_events.lock().unwrap();
                        let settings = state.analytics.get_settings();
                        drop(state);

                        let result_json = serde_json::json!({
                            "type": "analytics_settings_result",
                            "data": {
                                "enabled": settings.enabled,
                                "retention_days": settings.retention_days,
                                "weekly_report": settings.weekly_report,
                                "report_day": settings.report_day,
                            }
                        }).to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::UpdateAnalyticsSettings { enabled, retention_days, weekly_report, report_day } => {
                        use hiwave_analytics::ReportSettings;

                        let new_settings = ReportSettings {
                            enabled,
                            retention_days,
                            weekly_report,
                            report_day: report_day.clone(),
                        };

                        let state = state_for_events.lock().unwrap();
                        let result = state.analytics.update_settings(new_settings);
                        drop(state);

                        let result_json = match result {
                            Ok(_) => {
                                info!("Analytics settings updated");
                                serde_json::json!({
                                    "type": "analytics_settings_updated",
                                    "success": true
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "type": "analytics_settings_updated",
                                    "success": false,
                                    "error": format!("{}", e)
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ClearAnalyticsData => {
                        let state = state_for_events.lock().unwrap();
                        let result = state.analytics.clear_all_data();
                        drop(state);

                        let result_json = match result {
                            Ok(_) => {
                                info!("Analytics data cleared");
                                serde_json::json!({
                                    "type": "analytics_data_cleared",
                                    "success": true
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "type": "analytics_data_cleared",
                                    "success": false,
                                    "error": format!("{}", e)
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::ExportAnalyticsData { format } => {
                        let state = state_for_events.lock().unwrap();

                        let result = match format.to_lowercase().as_str() {
                            "json" => {
                                state.analytics.generate_monthly_report()
                                    .and_then(|report| serde_json::to_string_pretty(&report).map_err(|e| {
                                        hiwave_core::error::HiWaveError::analytics(e.to_string())
                                    }))
                                    .map(|data| (data, "json"))
                            }
                            "csv" => {
                                state.analytics.get_last_n_days_stats(30)
                                    .map(|stats| {
                                        let mut csv = String::from("Date,Trackers Blocked,Ads Blocked,Popups Blocked,Pages Visited,Tabs Opened,Tabs Closed,Browsing Time (s),Focus Time (s),Workspace Switches\n");
                                        for stat in stats {
                                            csv.push_str(&format!(
                                                "{},{},{},{},{},{},{},{},{},{}\n",
                                                stat.date,
                                                stat.trackers_blocked,
                                                stat.ads_blocked,
                                                stat.popups_blocked,
                                                stat.pages_visited,
                                                stat.tabs_opened,
                                                stat.tabs_closed,
                                                stat.browsing_time,
                                                stat.focus_time,
                                                stat.workspace_switches
                                            ));
                                        }
                                        (csv, "csv")
                                    })
                            }
                            _ => Err(hiwave_core::error::HiWaveError::analytics(format!("Unsupported format: {}", format)))
                        };
                        drop(state);

                        let result_json = match result {
                            Ok((data, fmt)) => {
                                serde_json::json!({
                                    "type": "analytics_export_result",
                                    "success": true,
                                    "data": data,
                                    "format": fmt
                                })
                            }
                            Err(e) => {
                                serde_json::json!({
                                    "type": "analytics_export_result",
                                    "success": false,
                                    "error": format!("{}", e)
                                })
                            }
                        }.to_string();

                        if let Some((_, ref settings_wv)) = *settings_window_for_events.lock().unwrap() {
                            let script = format!("window.postMessage({}, '*');", result_json);
                            let _ = settings_wv.eval(&script);
                        }
                    }
                    UserEvent::WindowThreadPing => {
                        if let Some((answered, clock)) = stall_log.as_ref() {
                            answered.now(clock);
                        }
                    }
                    UserEvent::LiveTurn => {
                        // MainEventsCleared follows and runs the live turn.
                    }
                    UserEvent::DecayTick => {
                        // Get decay settings and calculate decay levels for all tabs
                        let state = state_for_events.lock().unwrap();
                        let decay_days = state.user_settings.tab_decay_days;
                        let auto_shelf_days = state.user_settings.auto_shelf_days;
                        let mode = state.user_settings.default_mode.clone();

                        // Get active workspace
                        let active_ws = state.shell.get_active_workspace().map(|ws| ws.id);

                        // Calculate decay info for tabs in active workspace
                        if let Some(ws_id) = active_ws {
                            let tabs_with_decay = state.shell.tabs_with_decay(Some(ws_id), decay_days);

                            // Build decay info JSON
                            let decay_info: Vec<serde_json::Value> = tabs_with_decay
                                .iter()
                                .map(|(tab, level)| {
                                    serde_json::json!({
                                        "id": tab.id.0.to_string(),
                                        "url": tab.url.to_string(),
                                        "decay_level": level
                                    })
                                })
                                .collect();

                            drop(state);

                            // Send decay updates to chrome UI
                            let update_json = serde_json::json!({
                                "type": "decay_update",
                                "tabs": decay_info
                            }).to_string();

                            let script = format!(
                                "if(window.hiwave && window.hiwave.updateDecay) {{ window.hiwave.updateDecay({}); }}",
                                update_json
                            );
                            let _ = chrome_for_events.eval(&script);
                        }

                        // Auto-shelf expired tabs in Zen mode
                        if mode == "zen" {
                            if let Some(shelf_days) = auto_shelf_days {
                                if shelf_days > 0 {
                                    let mut state = state_for_events.lock().unwrap();
                                    let expired = state.shell.expired_tabs(None, shelf_days);

                                    for tab in expired {
                                        // Add to shelf
                                        let workspace_name = state.shell
                                            .get_workspace(tab.workspace_id)
                                            .map(|ws| ws.name.clone())
                                            .unwrap_or_else(|| "Default".to_string());

                                        let shelf_item = crate::state::ShelfItem {
                                            id: format!("shelf-{}", crate::state::AppState::current_timestamp_secs()),
                                            url: tab.url.to_string(),
                                            title: tab.title.clone(),
                                            workspace: workspace_name.clone(),
                                            added_at: crate::state::AppState::current_timestamp_secs(),
                                        };

                                        state.shelves
                                            .entry(workspace_name)
                                            .or_default()
                                            .push(shelf_item);

                                        // Close the tab
                                        let _ = state.shell.close_tab(tab.id);
                                    }
                                }
                            }
                        }
                    }
                    UserEvent::EvaluateScript(script) => {
                        let _ = chrome_for_events.eval(&script);
                    }
                    UserEvent::EvaluateContentScript(script) => {
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    UserEvent::LoadAboutPage => {
                        info!("Loading about page");
                        load_about_page(&content_for_events);
                    }
                    UserEvent::LoadReportPage => {
                        info!("Loading analytics report page");
                        load_report_page(&content_for_events);
                    }
                    UserEvent::PrintPage => {
                        info!("Triggering print dialog");
                        let _ = content_for_events.evaluate_script("window.print();");
                    }
                    UserEvent::ZoomIn => {
                        let mut s = state_for_events.lock().unwrap();
                        if let Some(tab) = s.shell.get_active_tab() {
                            let tab_id = tab.id.0.to_string();
                            let level = s.zoom_in(&tab_id);
                            let zoom_pct = (level * 100.0).round() as u32;
                            info!("Zoom in: {}%", zoom_pct);
                            let _ = content_for_events.evaluate_script(&format!(
                                "document.body.style.zoom = '{}%';",
                                zoom_pct
                            ));
                            // Notify chrome of zoom level change
                            let _ = chrome_for_events.eval(&format!(
                                "if(window.updateZoomIndicator) {{ updateZoomIndicator({}); }}",
                                level
                            ));
                        }
                    }
                    UserEvent::ZoomOut => {
                        let mut s = state_for_events.lock().unwrap();
                        if let Some(tab) = s.shell.get_active_tab() {
                            let tab_id = tab.id.0.to_string();
                            let level = s.zoom_out(&tab_id);
                            let zoom_pct = (level * 100.0).round() as u32;
                            info!("Zoom out: {}%", zoom_pct);
                            let _ = content_for_events.evaluate_script(&format!(
                                "document.body.style.zoom = '{}%';",
                                zoom_pct
                            ));
                            let _ = chrome_for_events.eval(&format!(
                                "if(window.updateZoomIndicator) {{ updateZoomIndicator({}); }}",
                                level
                            ));
                        }
                    }
                    UserEvent::ResetZoom => {
                        let mut s = state_for_events.lock().unwrap();
                        if let Some(tab) = s.shell.get_active_tab() {
                            let tab_id = tab.id.0.to_string();
                            let level = s.reset_zoom(&tab_id);
                            let zoom_pct = (level * 100.0).round() as u32;
                            info!("Reset zoom: {}%", zoom_pct);
                            let _ = content_for_events.evaluate_script(&format!(
                                "document.body.style.zoom = '{}%';",
                                zoom_pct
                            ));
                            let _ = chrome_for_events.eval(&format!(
                                "if(window.updateZoomIndicator) {{ updateZoomIndicator({}); }}",
                                level
                            ));
                        }
                    }
                    UserEvent::SyncZoomLevel => {
                        let s = state_for_events.lock().unwrap();
                        if let Some(tab) = s.shell.get_active_tab() {
                            let tab_id = tab.id.0.to_string();
                            let level = s.get_zoom_level(&tab_id);
                            let zoom_pct = (level * 100.0).round() as u32;
                            let _ = content_for_events.evaluate_script(&format!(
                                "document.body.style.zoom = '{}%';",
                                zoom_pct
                            ));
                            let _ = chrome_for_events.eval(&format!(
                                "if(window.updateZoomIndicator) {{ updateZoomIndicator({}); }}",
                                level
                            ));
                        }
                    }
                    UserEvent::TabAudioStateChanged { playing } => {
                        let mut s = state_for_events.lock().unwrap();
                        if let Some(tab) = s.shell.get_active_tab() {
                            let tab_id = tab.id.0.to_string();
                            s.set_tab_audio_playing(&tab_id, playing);
                            info!("Tab {} audio state: playing={}", tab_id, playing);
                            // Sync tabs to update UI
                            drop(s);
                            let _ = proxy.send_event(UserEvent::SyncTabs);
                        }
                    }
                    UserEvent::ToggleTabMute { ref id } => {
                        let mut s = state_for_events.lock().unwrap();
                        let new_muted = s.toggle_tab_mute(id);
                        info!("Tab {} mute toggled: now {}", id, if new_muted { "muted" } else { "unmuted" });
                        let is_active = s.shell.get_active_tab()
                            .map(|t| t.id.0.to_string() == *id)
                            .unwrap_or(false);
                        drop(s);
                        if is_active {
                            let script = if new_muted {
                                "document.querySelectorAll('video, audio').forEach(el => el.muted = true);"
                            } else {
                                "document.querySelectorAll('video, audio').forEach(el => el.muted = false);"
                            };
                            let _ = content_for_events.evaluate_script(script);
                        }
                        let _ = proxy.send_event(UserEvent::SyncTabs);
                    }
                    UserEvent::TriggerAutofill => {
                        info!("Triggering autofill");
                        // Focus the password field if any
                        let _ = content_for_events.evaluate_script(
                            "if(window.hiwaveAutofill) { hiwaveAutofill.trigger(); }"
                        );
                    }
                    UserEvent::SendAutofillCredentials { ref credentials_json } => {
                        info!("Sending autofill credentials to page");
                        // Send credentials to the page via window.postMessage
                        let script = format!(
                            "window.postMessage({{ type: 'hiwave_autofill_credentials', credentials: {} }}, '*');",
                            credentials_json
                        );
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    // Inspector event handlers
                    UserEvent::OpenInspector => {
                        info!("Opening inspector panel");
                        // Close shelf if open
                        *shelf_height_for_events.lock().unwrap() = SHELF_HEIGHT_DEFAULT as f64;
                        // Open inspector
                        *inspector_height_for_events.lock().unwrap() = INSPECTOR_HEIGHT_EXPANDED as f64;
                        let ch = *chrome_height_for_events.lock().unwrap();
                        let sh = SHELF_HEIGHT_DEFAULT as f64;
                        let ih = INSPECTOR_HEIGHT_EXPANDED as f64;
                        let sw = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sw,
                            right_sidebar_open,
                        );
                        // The RustKit Inspector takes the keyboard, as the
                        // shelf does: its console without a click first
                        // (#910 R1 nit 6).
                        inspector_for_events.take_keyboard();
                        if with_rustkit_content!(content_for_events, view => devtools.open(view, &inspector_for_events)) {
                            return;
                        }
                        // Inject inspector script into content webview if not already injected
                        let inject_script = format!(
                            "if (!window.__hiwaveInspector) {{ {} }}",
                            INSPECTOR_INJECT
                        );
                        let _ = content_for_events.evaluate_script(&inject_script);
                        // Trigger DOM tree refresh
                        let _ = content_for_events.evaluate_script(r#"
                            setTimeout(() => {
                                if (window.__hiwaveInspector) {
                                    const tree = window.__hiwaveInspector.getDomTree(10);
                                    window.ipc.postMessage(JSON.stringify({
                                        cmd: 'inspector_dom_tree_result',
                                        tree: tree
                                    }));
                                }
                            }, 100);
                        "#);
                    }
                    UserEvent::CloseInspector => {
                        info!("Closing inspector panel");
                        *inspector_height_for_events.lock().unwrap() = INSPECTOR_HEIGHT_DEFAULT as f64;
                        let ch = *chrome_height_for_events.lock().unwrap();
                        let sh = *shelf_height_for_events.lock().unwrap();
                        let ih = INSPECTOR_HEIGHT_DEFAULT as f64;
                        let sw = *sidebar_width_for_events.lock().unwrap();
                        let right_sidebar_open = *right_sidebar_open_for_events.lock().unwrap();
                        apply_layout(
                            &window,
                            &chrome_for_events,
                            &content_for_events,
                            &shelf_for_events,
                            &inspector_for_events,
                            ch,
                            sh,
                            ih,
                            sw,
                            right_sidebar_open,
                        );
                        // The RustKit Inspector gives the keyboard back to the page (CH3.A4).
                        #[cfg(all(target_os = "macos", feature = "rustkit"))]
                        panel_closed(&inspector_for_events, &content_for_events);
                        if with_rustkit_content!(content_for_events, view => devtools.close(view, &inspector_for_events)) {
                            return;
                        }
                        // Clear highlight in content
                        let _ = content_for_events.evaluate_script(
                            "if (window.__hiwaveInspector) { window.__hiwaveInspector.clearHighlight(); }"
                        );
                    }
                    UserEvent::ToggleInspector => {
                        let current_height = *inspector_height_for_events.lock().unwrap();
                        if current_height > 0.0 {
                            let _ = proxy.send_event(UserEvent::CloseInspector);
                        } else {
                            let _ = proxy.send_event(UserEvent::OpenInspector);
                        }
                    }
                    UserEvent::InspectorSendDomTree(json) => {
                        // Send DOM tree to inspector panel
                        let script = format!(
                            "if (window.hiwaveInspector) {{ window.hiwaveInspector.setDomTree({}); }}",
                            json
                        );
                        let _ = inspector_for_events.eval(&script);
                    }
                    UserEvent::InspectorSendElementStyles(json) => {
                        // Send element styles to inspector panel
                        let script = format!(
                            "if (window.hiwaveInspector) {{ window.hiwaveInspector.setElementStyles({}); }}",
                            json
                        );
                        let _ = inspector_for_events.eval(&script);
                    }
                    UserEvent::InspectorElementPicked { path, styles } => {
                        // Element was picked in content webview, update inspector
                        let styles_json = serde_json::to_string(&styles).unwrap_or_else(|_| "null".to_string());
                        let script = format!(
                            "if (window.hiwaveInspector) {{ window.hiwaveInspector.selectElementByPath('{}'); window.hiwaveInspector.setElementStyles({}); window.hiwaveInspector.setPickerActive(false); }}",
                            path.replace("'", "\\'"),
                            styles_json
                        );
                        let _ = inspector_for_events.eval(&script);
                    }
                    UserEvent::InspectorPickerStopped => {
                        let _ = inspector_for_events.eval(
                            "if (window.hiwaveInspector) { window.hiwaveInspector.setPickerActive(false); }"
                        );
                    }
                    UserEvent::InspectorHighlightElement { path } => {
                        if with_rustkit_content!(content_for_events, view => devtools.highlight(view, Some(&path))) {
                            return;
                        }
                        let script = format!(
                            "if (window.__hiwaveInspector) {{ window.__hiwaveInspector.highlightElement('{}'); }}",
                            path.replace("'", "\\'")
                        );
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    UserEvent::InspectorClearHighlight => {
                        if with_rustkit_content!(content_for_events, view => devtools.highlight(view, None)) {
                            return;
                        }
                        let _ = content_for_events.evaluate_script(
                            "if (window.__hiwaveInspector) { window.__hiwaveInspector.clearHighlight(); }"
                        );
                    }
                    UserEvent::InspectorStartPicker => {
                        if with_rustkit_content!(content_for_events, _view => devtools.start_picker(&inspector_for_events)) {
                            return;
                        }
                        let _ = content_for_events.evaluate_script(
                            "if (window.__hiwaveInspector) { window.__hiwaveInspector.startPicker(); }"
                        );
                        let _ = inspector_for_events.eval(
                            "if (window.hiwaveInspector) { window.hiwaveInspector.setPickerActive(true); }"
                        );
                    }
                    UserEvent::InspectorStopPicker => {
                        if with_rustkit_content!(content_for_events, view => devtools.stop_picker(view, &inspector_for_events)) {
                            return;
                        }
                        let _ = content_for_events.evaluate_script(
                            "if (window.__hiwaveInspector) { window.__hiwaveInspector.stopPicker(); }"
                        );
                        let _ = inspector_for_events.eval(
                            "if (window.hiwaveInspector) { window.hiwaveInspector.setPickerActive(false); }"
                        );
                    }
                    UserEvent::InspectorScrollToElement { path } => {
                        let script = format!(
                            "if (window.__hiwaveInspector) {{ window.__hiwaveInspector.scrollToElement('{}'); }}",
                            path.replace("'", "\\'")
                        );
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    UserEvent::InspectorRefreshTree => {
                        if with_rustkit_content!(content_for_events, view => devtools.send_tree(view)) {
                            return;
                        }
                        let _ = content_for_events.evaluate_script(r#"
                            (function() {
                                if (window.__hiwaveInspector) {
                                    const tree = window.__hiwaveInspector.getDomTree(10);
                                    window.ipc.postMessage(JSON.stringify({
                                        cmd: 'inspector_dom_tree_result',
                                        tree: tree
                                    }));
                                }
                            })();
                        "#);
                    }
                    UserEvent::InspectorSelect { path } => {
                        if with_rustkit_content!(content_for_events, view => devtools.select(view, &path)) {
                            return;
                        }
                        let script = format!(r#"
                            (function() {{
                                if (window.__hiwaveInspector) {{
                                    const styles = window.__hiwaveInspector.getElementStyles('{}');
                                    window.ipc.postMessage(JSON.stringify({{
                                        cmd: 'inspector_element_styles_result',
                                        styles: styles
                                    }}));
                                }}
                            }})();
                        "#, path.replace("\\", "\\\\").replace("'", "\\'"));
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    UserEvent::InspectorExpand { path } => {
                        // The injected-script tree is sent whole; only
                        // RustKit's is cut and expanded on demand.
                        with_rustkit_content!(content_for_events, view => devtools.expand(view, &path));
                    }
                    UserEvent::InspectElement { path } => {
                        // Open inspector and select the element
                        let _ = proxy.send_event(UserEvent::OpenInspector);
                        // Delay the selection to allow inspector to initialize
                        let path_escaped = path.replace("'", "\\'");
                        let script = format!(r#"
                            setTimeout(() => {{
                                if (window.__hiwaveInspector) {{
                                    const styles = window.__hiwaveInspector.getElementStyles('{}');
                                    window.ipc.postMessage(JSON.stringify({{
                                        cmd: 'inspector_element_picked',
                                        path: '{}',
                                        styles: styles
                                    }}));
                                }}
                            }}, 200);
                        "#, path_escaped, path_escaped);
                        let _ = content_for_events.evaluate_script(&script);
                    }
                    // Console events
                    UserEvent::InspectorConsoleMessage(json) => {
                        // Forward console message to inspector panel
                        let script = format!(
                            "if (window.hiwaveInspector) {{ window.hiwaveInspector.addConsoleEntry({}); }}",
                            json
                        );
                        let _ = inspector_for_events.eval(&script);
                    }
                    UserEvent::InspectorConsoleClear => {
                        // Clear console in inspector panel
                        devtools.clear_console(&inspector_for_events);
                    }
                    UserEvent::InspectorConsoleEval { code } => {
                        if with_rustkit_content!(content_for_events, view => devtools.eval(view, &code)) {
                            return;
                        }
                        // Execute JavaScript in content webview
                        let escaped_code = code.replace("\\", "\\\\").replace("'", "\\'").replace("\n", "\\n").replace("\r", "\\r");
                        let script = format!(r#"
                            (function() {{
                                try {{
                                    const result = eval('{}');
                                    const serialized = typeof result === 'object' ? JSON.stringify(result, null, 2) : String(result);
                                    window.ipc.postMessage(JSON.stringify({{
                                        cmd: 'inspector_console_message',
                                        entry: {{
                                            id: Date.now(),
                                            level: 'result',
                                            message: serialized,
                                            timestamp: Date.now(),
                                            stack: []
                                        }}
                                    }}));
                                }} catch (e) {{
                                    window.ipc.postMessage(JSON.stringify({{
                                        cmd: 'inspector_console_message',
                                        entry: {{
                                            id: Date.now(),
                                            level: 'error',
                                            message: e.name + ': ' + e.message,
                                            timestamp: Date.now(),
                                            stack: []
                                        }}
                                    }}));
                                }}
                            }})();
                        "#, escaped_code);
                        let _ = content_for_events.evaluate_script(&script);
                    }
                }
            }
            _ => {}
        }

        // Handle menu events (keyboard shortcuts)
        #[cfg(target_os = "macos")]
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().0.as_str();
            // The real-window driver reads which shortcut the menu took.
            info!(id, "menu event");
            match id {
                id if id == menu_ids::NEW_TAB => {
                    // Send create_tab IPC to properly create tab in shell and sync
                    let _ = chrome_for_events.eval("sendIpc('create_tab');");
                }
                id if id == menu_ids::CLOSE_TAB => {
                    // Send close_tab command to Chrome
                    let _ = chrome_for_events.eval("const activeTab = document.querySelector('.tab.active'); if(activeTab) { sendIpc('close_tab', { id: activeTab.dataset.id }); }"
                    );
                }
                id if id == menu_ids::RELOAD => {
                    let _ = proxy.send_event(UserEvent::Reload);
                }
                id if id == menu_ids::FIND => {
                    let _ = chrome_for_events.eval("openFindModal(); performFind('reset');");
                    chrome_for_events.take_keyboard();
                }
                id if id == menu_ids::COMMAND_PALETTE => {
                    let _ = chrome_for_events.eval("openCommandPalette();");
                    chrome_for_events.take_keyboard();
                }
                id if id == menu_ids::HISTORY => {
                    let _ = chrome_for_events.eval("toggleHistoryPanel();");
                }
                id if id == menu_ids::TOGGLE_SIDEBAR => {
                    let _ = chrome_for_events.eval("toggleSidebar();");
                }
                id if id == menu_ids::FOCUS_URL => {
                    let _ = chrome_for_events.eval("document.getElementById('urlInput').focus(); document.getElementById('urlInput').select();");
                    chrome_for_events.take_keyboard();
                }
                id if id == menu_ids::GO_BACK => {
                    let _ = proxy.send_event(UserEvent::GoBack);
                }
                id if id == menu_ids::GO_FORWARD => {
                    let _ = proxy.send_event(UserEvent::GoForward);
                }
                id if id == menu_ids::TOGGLE_INSPECTOR => {
                    let _ = proxy.send_event(UserEvent::ToggleInspector);
                }
                _ => {}
            }
        }
    });
}
