//! The browser's own `hiwave://` pages open from the browser's UI only.
//!
//! Chrome opens a `chrome://` page for its address bar, menus and its own
//! pages, and not for a web page: a link, a form, `location`, a meta
//! refresh or `window.open` to one leaves the page where it is, with
//! "Not allowed to load local resource: <url>" in the console. These run
//! the app's navigation handler (`navigate_content`) on recording views;
//! the page paths themselves, on a real content view, are in
//! `webview_rustkit`'s tests.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use crate::content_webview_trait::ContentWebViewOps;
use crate::ipc::{self, IpcMessage};
use crate::{navigate_content, AppState, ChromeUi, NavInitiator, NavOutcome};

/// A content view that records what it was asked to load.
#[derive(Default)]
pub(crate) struct RecordingContent {
    pub(crate) loads: RefCell<Vec<String>>,
}

impl ContentWebViewOps for RecordingContent {
    fn load_url(&self, url: &str) -> Result<(), String> {
        self.loads.borrow_mut().push(format!("url {url}"));
        Ok(())
    }
    fn load_html(&self, html: &str) -> Result<(), String> {
        let page = if html == crate::ABOUT_HTML { "about" } else { "report" };
        self.loads.borrow_mut().push(format!("html {page}"));
        Ok(())
    }
    fn evaluate_script(&self, _script: &str) -> Result<(), String> {
        Ok(())
    }
    fn set_bounds(&self, _rect: wry::Rect) -> Result<(), String> {
        Ok(())
    }
}

/// A chrome view that records the scripts sent to it.
#[derive(Default)]
pub(crate) struct RecordingChrome {
    pub(crate) scripts: RefCell<Vec<String>>,
}

impl ChromeUi for RecordingChrome {
    fn eval(&self, js: &str) -> Result<(), String> {
        self.scripts.borrow_mut().push(js.to_string());
        Ok(())
    }
    fn place(&self, _rect: wry::Rect) {}
}

impl RecordingChrome {
    /// Whether the chrome was told a new address-bar URL.
    pub(crate) fn told_url(&self) -> bool {
        self.scripts.borrow().iter().any(|s| s.contains("\"url\""))
    }
}

/// App state with one tab open on `url`, active.
pub(crate) fn state_with_tab(url: &str) -> (Arc<Mutex<AppState>>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = crate::state::AppConfig {
        data_dir: dir.path().to_path_buf(),
        download_dir: dir.path().join("downloads"),
        ..Default::default()
    };
    let state = Arc::new(Mutex::new(AppState::new(config).expect("app state")));
    let _ = ipc::commands::handle_message(&state, IpcMessage::CreateTab { url: Some(url.to_string()) });
    (state, dir)
}

pub(crate) fn active_tab_url(state: &Arc<Mutex<AppState>>) -> String {
    let s = state.lock().unwrap();
    s.shell.get_active_tab().map(|tab| tab.url.to_string()).unwrap_or_default()
}

const PAGE: &str = "http://127.0.0.1/page";

/// What a page-started navigation to `url` did: the outcome, the loads,
/// whether the bar was told, and the tab's URL after.
fn from_page(url: &str) -> (NavOutcome, Vec<String>, bool, String) {
    let (state, _dir) = state_with_tab(PAGE);
    let content = RecordingContent::default();
    let chrome = RecordingChrome::default();
    let outcome = navigate_content(url.to_string(), NavInitiator::Page, &state, &content, &chrome);
    let loads = content.loads.borrow().clone();
    (outcome, loads, chrome.told_url(), active_tab_url(&state))
}

#[test]
fn a_page_cannot_open_the_browsers_own_pages() {
    for url in [
        "hiwave://report",
        "hiwave://about",
        "hiwave://newtab",
        "hiwave://settings",
        "HIWAVE://report",
        "HiWave://about",
        "hiwave:report",
        " hiwave://report",
        "hiwave://report/",
        "hiwave://report?x=1",
    ] {
        let (outcome, loads, told, tab) = from_page(url);
        assert_eq!(outcome, NavOutcome::Refused, "{url:?} was followed");
        assert!(loads.is_empty(), "{url:?} loaded {loads:?}");
        assert!(!told, "{url:?} changed the address bar");
        assert_eq!(tab, PAGE, "{url:?} changed the tab's URL");
    }
}

#[test]
fn a_page_still_goes_to_web_urls() {
    let (outcome, loads, told, tab) = from_page("http://127.0.0.1/next");
    assert_eq!(outcome, NavOutcome::Followed);
    assert_eq!(loads, vec!["url http://127.0.0.1/next".to_string()]);
    assert!(told);
    assert_eq!(tab, "http://127.0.0.1/next");
}

/// The address bar, a menu, the command palette, the tab strip and session
/// restore send `NavInitiator::Browser`.
#[test]
fn the_browser_ui_opens_its_own_pages() {
    for (url, load) in [
        ("hiwave://report", "html report"),
        ("hiwave://about", "html about"),
        ("hiwave://newtab", "html about"),
    ] {
        let (state, _dir) = state_with_tab(PAGE);
        let content = RecordingContent::default();
        let chrome = RecordingChrome::default();
        let outcome = navigate_content(url.to_string(), NavInitiator::Browser, &state, &content, &chrome);
        assert_eq!(outcome, NavOutcome::Followed, "{url}");
        assert_eq!(*content.loads.borrow(), vec![load.to_string()], "{url}");
        assert!(chrome.told_url(), "{url}: the bar was not told");
        if url != "hiwave://newtab" {
            assert_eq!(active_tab_url(&state), url);
        }
    }
}

/// Only `hiwave:` URLs are the browser's: a page-started navigation to
/// anything else is unchanged by the initiator.
#[test]
fn the_initiator_changes_nothing_for_other_urls() {
    for url in ["http://127.0.0.1/a", "https://example.test/b", "about:blank", "about:srcdoc"] {
        let run = |initiator| {
            let (state, _dir) = state_with_tab(PAGE);
            let content = RecordingContent::default();
            let chrome = RecordingChrome::default();
            let outcome = navigate_content(url.to_string(), initiator, &state, &content, &chrome);
            let loads = content.loads.borrow().clone();
            (outcome, loads, active_tab_url(&state))
        };
        assert_eq!(run(NavInitiator::Page), run(NavInitiator::Browser), "{url}");
    }
}
