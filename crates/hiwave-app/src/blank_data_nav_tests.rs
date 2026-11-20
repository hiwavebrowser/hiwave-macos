//! `about:blank` and `data:` URLs in the tab, as Chrome treats them.
//!
//! Chrome for Testing 148.0.7778.216: a navigation to `about:blank` shows an
//! empty document at `about:blank` (from the address bar or from a page).
//! A page's own top-level navigation to a `data:` URL (link, form,
//! `location`, meta refresh) is not followed: the page stays and its console
//! says "Not allowed to navigate top frame to data URL: <url>". Until
//! 2026-11-20 both showed the browser's own new-tab page (`about.html`).
//! Here no `data:` URL is followed, whoever asked: one typed in the address
//! bar (which Chrome opens) needs the navigation's initiator first.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use rustkit_viewhost::Bounds;

use crate::content_webview_trait::ContentWebViewOps;
use crate::ipc::{self, IpcMessage};
use crate::webview_rustkit::RustKitView;
use crate::{navigate_content, AppState, ChromeUi, NavInitiator};

/// A content view that records what it was asked to load.
#[derive(Default)]
struct Loads(RefCell<Vec<String>>);

impl ContentWebViewOps for Loads {
    fn load_url(&self, url: &str) -> Result<(), String> {
        self.0.borrow_mut().push(format!("url {url}"));
        Ok(())
    }
    fn load_html(&self, html: &str) -> Result<(), String> {
        let what = if html == crate::ABOUT_HTML { "new tab page" } else { "html" };
        self.0.borrow_mut().push(what.to_string());
        Ok(())
    }
    fn evaluate_script(&self, _script: &str) -> Result<(), String> {
        Ok(())
    }
    fn set_bounds(&self, _rect: wry::Rect) -> Result<(), String> {
        Ok(())
    }
}

/// A chrome view that records the URL it was told to show.
#[derive(Default)]
struct Bar(RefCell<Vec<String>>);

impl ChromeUi for Bar {
    fn eval(&self, js: &str) -> Result<(), String> {
        self.0.borrow_mut().push(js.to_string());
        Ok(())
    }
    fn place(&self, _rect: wry::Rect) {}
}

impl Bar {
    fn told_url(&self) -> bool {
        self.0.borrow().iter().any(|s| s.contains("\"url\""))
    }
    fn shows(&self, url: &str) -> bool {
        let quoted = serde_json::to_string(url).unwrap();
        self.0.borrow().iter().any(|s| s.contains("\"url\"") && s.contains(&quoted))
    }
}

const PAGE: &str = "http://127.0.0.1/page";

fn state_with_tab(url: &str) -> (Arc<Mutex<AppState>>, tempfile::TempDir) {
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

fn tab_url(state: &Arc<Mutex<AppState>>) -> String {
    let s = state.lock().unwrap();
    s.shell.get_active_tab().map(|tab| tab.url.to_string()).unwrap_or_default()
}

#[test]
fn about_blank_shows_an_empty_page_not_the_new_tab_page() {
    for (url, want) in [
        ("about:blank", "about:blank"),
        ("about:blank#top", "about:blank#top"),
        ("about:blank?q=x", "about:blank?q=x"),
        ("ABOUT:blank", "about:blank"),
        // Chrome's address bar takes these to about:blank, and a page's
        // `about:BLANK` is blank too.
        ("ABOUT:BLANK", "about:blank"),
        ("about:BLANK#top", "about:blank#top"),
    ] {
        let (state, _dir) = state_with_tab(PAGE);
        let content = Loads::default();
        let bar = Bar::default();
        navigate_content(url.to_string(), NavInitiator::Browser, &state, &content, &bar);
        assert_eq!(*content.0.borrow(), vec!["html".to_string()], "{url}");
        assert_eq!(tab_url(&state), want, "{url}: the tab's URL");
        assert!(bar.shows(&want), "{url}: the address bar does not show it: {:?}", bar.0.borrow());
    }
}

/// The document `about:blank` loads is empty, at `about:blank`.
#[test]
fn about_blank_replaces_the_document_with_an_empty_one() {
    let view = RustKitView::headless(Bounds::new(0, 0, 400, 300));
    view.load_html_internal("<p>before-marker</p>").unwrap();
    let (state, _dir) = state_with_tab(PAGE);
    navigate_content("about:blank".to_string(), NavInitiator::Browser, &state, &view, &Bar::default());
    let body = view.execute_script_sync("document.body.innerHTML").unwrap_or_default();
    assert!(!body.contains("before-marker") && !body.contains("HiWave"), "not an empty document: {}", body.chars().take(120).collect::<String>());
    let href = view.execute_script_sync("location.href").unwrap_or_default();
    assert!(href.contains("about:blank"), "{href}");
}

#[test]
fn a_data_url_does_not_open_the_new_tab_page_or_leave_the_page() {
    for url in [
        "data:text/html,<p>x</p>",
        "data:text/html;base64,PHA+eDwvcD4=",
        "data:text/plain,hello",
        "DATA:text/html,<p>x</p>",
        " data:text/html,<p>x</p>",
        "data:image/svg+xml,<svg/>",
    ] {
        let (state, _dir) = state_with_tab(PAGE);
        let content = Loads::default();
        let bar = Bar::default();
        navigate_content(url.to_string(), NavInitiator::Page, &state, &content, &bar);
        assert!(content.0.borrow().is_empty(), "{url:?} loaded {:?}", content.0.borrow());
        assert_eq!(tab_url(&state), PAGE, "{url:?} changed the tab's URL");
        assert!(!bar.told_url(), "{url:?} changed the address bar");
    }
}

/// The browser's own pages are unchanged.
#[test]
fn the_new_tab_and_about_pages_still_open() {
    for (url, load) in [("hiwave://newtab", "new tab page"), ("hiwave://about", "new tab page")] {
        let (state, _dir) = state_with_tab(PAGE);
        let content = Loads::default();
        navigate_content(url.to_string(), NavInitiator::Browser, &state, &content, &Bar::default());
        assert_eq!(*content.0.borrow(), vec![load.to_string()], "{url}");
    }
}

/// The command palette's "New Tab" (`execute_command` `new_tab`, from the
/// shelf's or the chrome's palette) opens the new tab page, as before
/// about:blank became an empty page (#1373 R1).
#[test]
fn the_command_palettes_new_tab_opens_the_new_tab_page() {
    let (state, _dir) = state_with_tab(PAGE);
    let response = ipc::commands::handle_message(&state, IpcMessage::ExecuteCommand { id: "new_tab".to_string() });
    // As main.rs reads it (`handle_chrome_ipc`, `handle_shelf_ipc`).
    let ipc::IpcResponse::Success { data } = response else { panic!("{response:?}") };
    let result = data.get("result").expect("a result");
    assert_eq!(result.get("action").and_then(|a| a.as_str()), Some("navigate"), "{result}");
    let url = result.get("url").and_then(|u| u.as_str()).expect("a URL").to_string();
    let content = Loads::default();
    navigate_content(url.clone(), NavInitiator::Browser, &state, &content, &Bar::default());
    assert_eq!(*content.0.borrow(), vec!["new tab page".to_string()], "{url}");
    assert_ne!(tab_url(&state), "about:blank", "the tab was left at about:blank");
}
