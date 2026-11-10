//! M1: Cmd+W closes the active tab every time.
//!
//! The E35 tab-cycle run (Cmd+T, type a URL, Cmd+W, 30 times) closed a tab
//! only on the first Cmd+W of the app's life: every later press logged the
//! menu event and the chrome sent no `close_tab`. These run the app's own
//! pieces against chrome.html on a headless RustKit view, as the RustKit
//! chrome does: the shell and the IPC handler (`ipc::commands`), the tab
//! sync (`sync_tabs_to_chrome`) through the per-turn state coalescing
//! (`CoalescedChrome`), and the menu's Cmd+W (`close_active_tab`).

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use rustkit_engine::{Engine, EngineConfig, EngineViewId};
use rustkit_viewhost::Bounds;

use crate::chrome_state::CoalescedChrome;
use crate::ipc::{self, IpcMessage};
use crate::{AppState, ChromeUi};

// M1c: tab order through the shell, the strip and Cmd+1..9 (uses `App`).
mod tab_order_tests;
// M1d: the tab shortcuts from the menu and the chrome, exactly once.
mod tab_shortcut_tests;
// The history lists keep a visited URL as data (no markup, no script).
mod history_markup_tests;

/// chrome.html on a headless engine view, as the app's chrome view.
struct HeadlessChrome {
    engine: RefCell<Engine>,
    view: EngineViewId,
}

impl ChromeUi for HeadlessChrome {
    fn eval(&self, js: &str) -> Result<(), String> {
        self.engine
            .borrow_mut()
            .execute_script(self.view, js)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn place(&self, _rect: wry::Rect) {}
}

impl HeadlessChrome {
    fn read(&self, expr: &str) -> String {
        let value = self
            .engine
            .borrow_mut()
            .execute_script(self.view, &format!("String({expr})"))
            .expect("read");
        value
            .strip_prefix("String(\"")
            .and_then(|v| v.strip_suffix("\")"))
            .unwrap_or(&value)
            .to_string()
    }

    /// One 16 ms live turn of the chrome: its animation frame (where its
    /// state applier runs), on a test clock moved as far.
    fn frame(&self) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut engine = self.engine.borrow_mut();
        engine
            .execute_script(self.view, "window.__clockMs = (window.__clockMs || 0) + 16;")
            .expect("clock");
        rt.block_on(engine.pump_live(self.view, 16));
    }

    /// The `close_tab`/`activate_tab` messages the chrome posted since the
    /// last call, oldest first.
    fn take_posted(&self) -> Vec<IpcMessage> {
        let json = self.read("JSON.stringify(window.__posted.splice(0))");
        let json = json.replace("\\\"", "\"").replace("\\\\", "\\");
        let posted: Vec<String> = serde_json::from_str(&json).expect("posted messages");
        posted
            .iter()
            .filter_map(|m| serde_json::from_str::<IpcMessage>(m).ok())
            .filter(|m| matches!(m, IpcMessage::CloseTab { .. } | IpcMessage::ActivateTab { .. }))
            .collect()
    }
}

struct App {
    state: Arc<Mutex<AppState>>,
    chrome: CoalescedChrome<HeadlessChrome>,
    _dir: tempfile::TempDir,
}

impl App {
    /// The app with `n` tabs open on local pages (the last one active, as
    /// Cmd+T leaves it), and the chrome showing them.
    fn with_tabs(n: usize) -> App {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = crate::state::AppConfig {
            data_dir: dir.path().to_path_buf(),
            download_dir: dir.path().join("downloads"),
            ..Default::default()
        };
        let state = Arc::new(Mutex::new(AppState::new(config).expect("app state")));

        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds::new(0, 0, 1280, 120))
            .expect("view");
        engine.set_app_owned(view, true);
        engine.load_html(view, crate::CHROME_HTML).expect("chrome.html");
        engine
            .execute_script(view, "window.__clockMs = 0; window.__hiwaveStateClock = function () { return window.__clockMs; };")
            .expect("clock");
        let chrome = HeadlessChrome { engine: RefCell::new(engine), view };
        for _ in 0..40 {
            chrome.frame();
        }
        // What the chrome posts to the app, kept for the test to read.
        chrome
            .eval("window.__posted = []; window.ipc = { postMessage: function (m) { window.__posted.push(m); } };")
            .expect("ipc recorder");

        let app = App { state, chrome: CoalescedChrome::new(chrome, true), _dir: dir };
        for i in 0..n {
            app.handle(IpcMessage::CreateTab { url: Some(format!("http://127.0.0.1/p{i}")) });
        }
        app.end_turn();
        app
    }

    /// The app's answer to a chrome message: the IPC handler, then the tab
    /// sync it queues (main.rs, `IpcMessage::CloseTab`/`ActivateTab`).
    fn handle(&self, msg: IpcMessage) {
        let _ = ipc::commands::handle_message(&self.state, msg);
        crate::sync_tabs_to_chrome(&self.state, &self.chrome);
    }

    /// The end of the event loop's turn (`MainEventsCleared`): the turn's
    /// state goes to the chrome, and the chrome's frames apply it.
    fn end_turn(&self) {
        self.chrome.flush_state();
        for _ in 0..16 {
            self.chrome.frame();
        }
    }

    /// The chrome's messages since the last call, answered as the app
    /// answers them. Returns them.
    fn answer_chrome(&self) -> Vec<String> {
        let posted = self.chrome.take_posted();
        let mut seen = Vec::new();
        for msg in posted {
            seen.push(match &msg {
                IpcMessage::CloseTab { id } => format!("close_tab {id}"),
                IpcMessage::ActivateTab { id } => format!("activate_tab {id}"),
                _ => unreachable!(),
            });
            self.handle(msg);
        }
        self.end_turn();
        seen
    }

    /// Cmd+W, as the menu arm runs it, and the app's answer.
    fn cmd_w(&self) -> Vec<String> {
        crate::close_active_tab(&self.chrome);
        self.answer_chrome()
    }

    /// The open tabs' ids, as a set: `list_tabs` reads a `HashMap`, so its
    /// order is not creation order and varies between runs. Pick tabs by
    /// role (`active`, `background_tab`), never by position here.
    fn tab_ids(&self) -> Vec<u64> {
        let s = self.state.lock().unwrap();
        let ws = s.shell.get_active_workspace().map(|w| w.id);
        let mut ids: Vec<u64> = s.shell.list_tabs(ws).iter().map(|t| t.id.0).collect();
        ids.sort_unstable();
        ids
    }

    /// An open tab that is not the active one.
    fn background_tab(&self) -> u64 {
        let active = self.active();
        self.tab_ids()
            .into_iter()
            .find(|id| Some(*id) != active)
            .expect("a background tab")
    }

    fn active(&self) -> Option<u64> {
        self.state.lock().unwrap().shell.get_active_tab().map(|t| t.id.0)
    }

    /// A click on one part (`.tab-title`, `.tab-close`) of tab `id` in the
    /// chrome's strip. The tab element is found first and the part inside
    /// it: RustKit's `querySelector` drops an attribute filter on an
    /// ancestor compound (`.tab[data-id="0"] .tab-title` matches the first
    /// strip tab's title, whatever its id), so a one-selector lookup clicks
    /// the wrong tab unless that tab happens to lead the strip.
    fn click_tab_part(&self, id: u64, part: &str) {
        self.chrome
            .eval(&format!(
                "document.querySelector('#tabs .tab[data-id=\"{id}\"]').querySelector('{part}').click()"
            ))
            .expect("click");
    }

    /// The tab ids the chrome's strip shows, and the one it marks active.
    fn strip(&self) -> String {
        self.chrome.read(
            "Array.from(document.querySelectorAll('#tabs .tab')).map(function (t) { return t.dataset.id + (t.classList.contains('active') ? '*' : ''); }).join(',')",
        )
    }
}

/// Three tabs, Cmd+W three times: each press closes the active tab, and
/// no tab is left.
#[test]
fn cmd_w_three_times_closes_three_tabs() {
    let app = App::with_tabs(3);
    let opened = app.tab_ids();
    assert_eq!(opened.len(), 3, "tabs at the start: {opened:?}");
    for press in 1..=3 {
        let active = app.active().expect("an active tab");
        let sent = app.cmd_w();
        assert_eq!(
            sent,
            vec![format!("close_tab {active}")],
            "Cmd+W #{press}: the chrome (strip {}) sent {sent:?}",
            app.strip()
        );
    }
    assert_eq!(app.tab_ids(), Vec::<u64>::new(), "tabs left after three Cmd+W");
}

/// A background tab clicked in the strip becomes the active tab, and
/// Cmd+W closes it, then the next active one.
#[test]
fn cmd_w_closes_a_clicked_background_tab() {
    let app = App::with_tabs(3);
    let background = app.background_tab();
    assert_ne!(app.active(), Some(background));
    app.click_tab_part(background, ".tab-title");
    assert_eq!(app.answer_chrome(), vec![format!("activate_tab {background}")]);
    assert_eq!(app.active(), Some(background));
    assert_eq!(app.cmd_w(), vec![format!("close_tab {background}")], "strip {}", app.strip());
    let next = app.active().expect("an active tab after the close");
    assert_eq!(app.cmd_w(), vec![format!("close_tab {next}")], "strip {}", app.strip());
    assert_eq!(app.tab_ids().len(), 1);
}

/// A background tab closed with its close button, then Cmd+W: the press
/// closes the active tab of what is left.
#[test]
fn cmd_w_after_a_close_button() {
    let app = App::with_tabs(3);
    let closed = app.background_tab();
    app.click_tab_part(closed, ".tab-close");
    assert_eq!(app.answer_chrome(), vec![format!("close_tab {closed}")]);
    let active = app.active().expect("an active tab");
    assert_eq!(app.cmd_w(), vec![format!("close_tab {active}")], "strip {}", app.strip());
    let active = app.active().expect("an active tab");
    assert_eq!(app.cmd_w(), vec![format!("close_tab {active}")], "strip {}", app.strip());
    assert_eq!(app.tab_ids(), Vec::<u64>::new());
}
