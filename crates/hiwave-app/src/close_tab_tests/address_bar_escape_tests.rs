//! M1f: Escape in the address bar is the address bar's, never the sidebar's.
//!
//! #1015's real-window run: with the sidebar open, Cmd+L during a load and
//! one Escape closed the sidebar; the load stopped only on the second
//! Escape. In Chrome the omnibox takes an Escape pressed in it
//! (`OmniboxViewViews::SkipDefaultKeyEventProcessing` returns true for
//! Escape, so no browser accelerator sees it) and
//! `OmniboxEditModel::OnEscapeKeyPressed` runs: typed text is reverted to
//! the page's URL and selected, and the load is left alone ("If the user
//! was in the midst of editing, don't cancel any underlying page load");
//! with nothing typed it leaves the omnibox. No side panel closes.
//!
//! These press Escape through the engine's own key path
//! (`Engine::handle_text_key`, as the window delivers it) on chrome.html
//! with the address bar focused, and read what the chrome posted.

use super::App;
use crate::ChromeUi;

impl App {
    /// The `cmd` of every message the chrome posted since the last read,
    /// oldest first.
    fn posted_cmds(&self) -> Vec<String> {
        let json = self.chrome.read("JSON.stringify(window.__posted.splice(0))");
        let json = json.replace("\\\"", "\"").replace("\\\\", "\\");
        let posted: Vec<String> = serde_json::from_str(&json).expect("posted messages");
        posted
            .iter()
            .filter_map(|m| serde_json::from_str::<serde_json::Value>(m).ok())
            .filter_map(|v| v.get("cmd").and_then(|c| c.as_str()).map(str::to_string))
            .collect()
    }

    fn open_sidebar(&self) {
        self.chrome
            .eval("window.hiwaveChrome.setSidebarState(220, true);")
            .expect("open the sidebar");
        self.posted_cmds();
        assert!(self.sidebar_open(), "the sidebar opened");
    }

    fn sidebar_open(&self) -> bool {
        self.chrome.read("!document.getElementById('sidebar').classList.contains('collapsed')") == "true"
    }

    fn set_loading(&self, loading: bool) {
        crate::chrome_loading(&self.chrome, loading);
        self.end_turn();
    }

    /// Cmd+L's effect: the bar focused with its text selected.
    fn cmd_l(&self) {
        self.chrome
            .eval("var u = document.getElementById('urlInput'); u.focus(); u.select();")
            .expect("focus the address bar");
        assert!(self.bar_focused(), "the address bar took focus");
    }

    fn bar_focused(&self) -> bool {
        self.chrome.read("document.activeElement === document.getElementById('urlInput')") == "true"
    }

    fn bar_text(&self) -> String {
        self.chrome.read("document.getElementById('urlInput').value")
    }

    /// Keys typed into the focused field, one by one, through the engine.
    fn type_text(&self, text: &str) {
        for ch in text.chars() {
            let s = ch.to_string();
            self.chrome
                .engine
                .borrow_mut()
                .handle_text_key(self.chrome.view, 0, &s, false, false, false);
        }
    }

    fn escape(&self) {
        self.chrome
            .engine
            .borrow_mut()
            .handle_text_key(self.chrome.view, 0x1B, "", false, false, false);
    }
}

/// The #1015 case: sidebar open, page loading, Cmd+L, one Escape stops
/// the load. The sidebar stays open.
#[test]
fn one_escape_in_the_address_bar_stops_a_load_with_the_sidebar_open() {
    let app = App::with_tabs(1);
    app.open_sidebar();
    app.set_loading(true);
    app.posted_cmds();
    app.cmd_l();

    app.escape();

    let cmds = app.posted_cmds();
    assert_eq!(cmds.iter().filter(|c| *c == "stop").count(), 1, "one Escape posts one stop: {cmds:?}");
    assert!(app.sidebar_open(), "the sidebar stays open: {cmds:?}");
    assert!(!cmds.iter().any(|c| c == "set_sidebar_width"), "nothing resized the sidebar: {cmds:?}");
    assert!(!app.bar_focused(), "the address bar let go of the focus");
}

/// Typed text in the bar on a loaded page: Escape puts the page's URL
/// back, selected, and the bar keeps the focus (Chrome's
/// kClearUserInput). No stop, the sidebar stays open.
#[test]
fn escape_reverts_typed_text_to_the_page_url_first() {
    let app = App::with_tabs(1);
    app.open_sidebar();
    app.set_loading(false);
    let url = app.bar_text();
    assert_eq!(url, "http://127.0.0.1/p0", "the bar shows the page");
    app.cmd_l();
    app.type_text("typed");
    assert_eq!(app.bar_text(), "typed", "typing replaced the selection");
    app.posted_cmds();

    app.escape();

    let cmds = app.posted_cmds();
    assert_eq!(app.bar_text(), url, "the page's URL is back");
    assert!(app.bar_focused(), "the bar keeps the focus");
    assert_eq!(
        app.chrome.read("(function(u){return u.selectionStart + ',' + u.selectionEnd;})(document.getElementById('urlInput'))"),
        format!("0,{}", url.len()),
        "the URL is selected, ready to be typed over"
    );
    assert!(app.sidebar_open(), "the sidebar stays open: {cmds:?}");
    assert!(!cmds.iter().any(|c| c == "stop"), "no stop: {cmds:?}");

    // The second Escape has nothing typed to revert: it leaves the bar.
    app.escape();
    assert!(!app.bar_focused(), "the second Escape leaves the bar");
    assert!(app.sidebar_open(), "the sidebar still stays open");
}

/// Typed text while the page loads: the first Escape reverts and leaves
/// the load alone (Chrome: "don't cancel any underlying page load"); the
/// next one stops it.
#[test]
fn escape_with_typed_text_during_a_load_reverts_before_it_stops() {
    let app = App::with_tabs(1);
    app.open_sidebar();
    app.set_loading(true);
    let url = app.bar_text();
    app.cmd_l();
    app.type_text("abc");
    app.posted_cmds();

    app.escape();
    let first = app.posted_cmds();
    assert_eq!(app.bar_text(), url, "the first Escape reverts");
    assert!(!first.iter().any(|c| c == "stop"), "the first Escape does not stop: {first:?}");

    app.escape();
    let second = app.posted_cmds();
    assert_eq!(second.iter().filter(|c| *c == "stop").count(), 1, "the second Escape stops: {second:?}");
    assert!(app.sidebar_open(), "the sidebar stays open throughout");
}

/// Outside the address bar, Escape still closes the sidebar as before.
#[test]
fn escape_outside_the_address_bar_still_closes_the_sidebar() {
    let app = App::with_tabs(1);
    app.open_sidebar();
    app.set_loading(true);
    app.chrome
        .eval("document.getElementById('urlInput').blur();")
        .expect("blur");
    app.posted_cmds();

    app.escape();

    let cmds = app.posted_cmds();
    assert!(!app.sidebar_open(), "Escape closed the sidebar: {cmds:?}");
    assert!(!cmds.iter().any(|c| c == "stop"), "and did not stop as well: {cmds:?}");
}

/// Nothing typed on a page that is not loading: Escape leaves the bar and
/// sends no stop (#1022 R1 nit 3: no `Stop: nothing loading`).
#[test]
fn escape_on_a_loaded_page_leaves_the_bar_without_a_stop() {
    let app = App::with_tabs(1);
    app.open_sidebar();
    app.set_loading(false);
    app.cmd_l();
    app.posted_cmds();

    app.escape();

    let cmds = app.posted_cmds();
    assert!(!cmds.iter().any(|c| c == "stop"), "nothing loading, no stop: {cmds:?}");
    assert!(!app.bar_focused(), "the bar let go of the focus");
    assert!(app.sidebar_open(), "the sidebar stays open: {cmds:?}");
}

/// A caret placed in the bar, nothing typed (not all selected): Chrome's
/// OnEscapeKeyPressed reverts and selects all, then (not editing) leaves
/// the omnibox. Here too, and a loading page is stopped.
#[test]
fn escape_with_a_caret_and_nothing_typed_leaves_the_bar_and_stops_a_load() {
    let app = App::with_tabs(1);
    app.set_loading(true);
    app.cmd_l();
    app.chrome
        .eval("document.getElementById('urlInput').setSelectionRange(3, 3);")
        .expect("caret");
    app.posted_cmds();

    app.escape();

    let cmds = app.posted_cmds();
    assert_eq!(cmds.iter().filter(|c| *c == "stop").count(), 1, "{cmds:?}");
    assert!(!app.bar_focused());
    assert_eq!(app.bar_text(), "http://127.0.0.1/p0");
}

/// The find bar open, focus in the address bar: Escape is the address
/// bar's (Chrome: the omnibox takes Escape ahead of IDC_CLOSE_FIND_OR_STOP);
/// the find bar stays open.
#[test]
fn escape_in_the_address_bar_leaves_the_find_bar_open() {
    let app = App::with_tabs(1);
    app.set_loading(true);
    app.chrome.eval("openFindModal();").expect("open the find bar");
    // Its input takes the focus on a timer; let that run first.
    app.end_turn();
    let find_open = || app.chrome.read("document.getElementById('findModal').classList.contains('open')") == "true";
    assert!(find_open(), "the find bar is open");
    app.cmd_l();
    app.type_text("abc");
    app.posted_cmds();

    app.escape();
    assert_eq!(app.bar_text(), "http://127.0.0.1/p0", "the first Escape reverts the bar");
    assert!(find_open(), "and leaves the find bar open");

    app.escape();
    let cmds = app.posted_cmds();
    assert_eq!(cmds.iter().filter(|c| *c == "stop").count(), 1, "the second stops the load: {cmds:?}");
    assert!(find_open(), "the find bar is still open: {cmds:?}");
}

/// An internal page shows an empty bar; typed text over it reverts to
/// empty, not to the hiwave:// URL (#1022 R1 nit 4).
#[test]
fn escape_on_an_internal_page_reverts_to_an_empty_bar() {
    let app = App::with_tabs(1);
    crate::chrome_url(&app.chrome, "hiwave://about");
    app.end_turn();
    assert_eq!(app.bar_text(), "", "an internal page shows the placeholder");
    app.cmd_l();
    app.type_text("xyz");

    app.escape();

    assert_eq!(app.bar_text(), "", "reverted to what the bar showed");
    assert!(app.bar_focused());
}
