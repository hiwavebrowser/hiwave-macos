//! The chrome's history lists keep a visited URL as data.
//!
//! chrome.html is the app's own page and holds `window.ipc`; the URLs in
//! its history lists come from pages the user visited. A list item built
//! with markup must carry such a URL only as attribute or text data, and a
//! click must hand the app the URL exactly as it was recorded. These run
//! the app's history path (`record_visit` → `sync_history_to_chrome` →
//! chrome.html `updateHistory`) on a headless RustKit chrome view.

use super::App;
use crate::ChromeUi;

impl App {
    /// Record a visit to `url` (as the app's `RecordVisit` arm does) and
    /// send the history to the chrome, as the app does on `SyncHistory`.
    fn visit(&self, url: &str) {
        self.visit_titled(url, "Visited");
    }

    /// The same, with the page title the visit records.
    fn visit_titled(&self, url: &str, title: &str) {
        self.state
            .lock()
            .unwrap()
            .record_visit(url, Some(title.to_string()), None)
            .expect("record visit");
        crate::sync_history_to_chrome(&self.state, &self.chrome);
        self.end_turn();
    }

    /// The `navigate` URLs the chrome posted since the last call. They
    /// cross as `encodeURIComponent` text, which has no quote or backslash
    /// for the script result's own escaping to touch.
    fn take_navigations(&self) -> Vec<String> {
        let joined = self.chrome.read(
            "window.__posted.splice(0).map(function (m) { return JSON.parse(m); }).filter(function (m) { return m.cmd === 'navigate'; }).map(function (m) { return encodeURIComponent(m.url); }).join(' ')",
        );
        joined
            .split(' ')
            .filter(|s| !s.is_empty())
            .map(|s| urlencoding::decode(s).expect("utf-8").into_owned())
            .collect()
    }
}

/// A recorded URL as the URL parser hands it to the history: quotes and
/// parentheses in a path are kept as they are (WHATWG path percent-encode
/// set), so the history can hold them.
fn parsed(raw: &str) -> String {
    url::Url::parse(raw).expect("url").to_string()
}

/// A visited URL with quotes in it, shown in the right sidebar's history
/// list and clicked: no script from the URL runs in the chrome, and the
/// click navigates to the URL exactly as recorded.
#[test]
fn right_sidebar_history_keeps_a_quoted_url_as_data() {
    let app = App::with_tabs(1);
    app.chrome.eval("window.__flag = 0;").expect("flag");

    let url = parsed("http://127.0.0.1/a'+(window.__flag=1)+'");
    assert!(url.contains('\''), "the parser keeps the quote: {url}");
    app.visit(&url);

    app.chrome
        .eval("document.getElementById('rightSidebarHistory').click()")
        .expect("history tab");
    let items = app.chrome.read(
        "document.querySelectorAll('#rightHistoryList .right-panel-item').length",
    );
    assert_eq!(items, "1", "one history item");
    let _ = app.take_navigations();

    app.chrome
        .eval("document.querySelector('#rightHistoryList .right-panel-item .right-panel-item-title').click()")
        .expect("click item");

    assert_eq!(app.chrome.read("window.__flag"), "0", "no script from the URL ran");
    assert_eq!(app.take_navigations(), vec![url], "the click navigates to the URL as recorded");
}

/// The same for the left history panel, with markup in the page title:
/// the URL parser normalizes most of a URL's markup away, but a title
/// reaches the history as the page set it, raw quotes and angle brackets
/// included. Both lists show it as text, and a click on each list's item
/// navigates to the URL as recorded.
#[test]
fn history_lists_keep_a_title_with_markup_as_text() {
    let app = App::with_tabs(1);
    app.chrome.eval("window.__flag = 0;").expect("flag");

    let url = parsed("http://127.0.0.1/b#x\\'\"><b id=injected>(window.__flag=1)");
    let title = "t\"'><b id=\"injected\" onclick=\"window.__flag=1\">x</b>&amp;";
    app.visit_titled(&url, title);

    // No element from the title (or the URL) in either list.
    assert_eq!(
        app.chrome.read("document.querySelectorAll('#injected').length"),
        "0",
        "no markup from the title"
    );
    // Both lists show the title as the page set it, as text.
    let literal = serde_json::to_string(title).expect("title literal");
    for title_sel in [
        "#historyList .history-item .history-item-title",
        "#rightHistoryList .right-panel-item .right-panel-item-title",
    ] {
        assert_eq!(
            app.chrome.read(&format!(
                "document.querySelector('{title_sel}').textContent === {literal}"
            )),
            "true",
            "{title_sel} shows the title as text"
        );
    }

    let _ = app.take_navigations();
    assert_eq!(
        app.chrome.read("document.querySelectorAll('#historyList .history-item').length"),
        "1",
        "the left panel has the item to click"
    );
    app.chrome
        .eval("document.querySelector('#historyList .history-item .history-item-title').click()")
        .expect("click left item");
    app.chrome
        .eval("document.querySelector('#rightHistoryList .right-panel-item .right-panel-item-title').click()")
        .expect("click right item");
    let injected = "var e = document.getElementById('injected'); if (e) e.click();";
    app.chrome.eval(injected).expect("click anything injected");

    assert_eq!(app.chrome.read("window.__flag"), "0", "no script from the title or URL ran");
    assert_eq!(
        app.take_navigations(),
        vec![url.clone(), url],
        "each list's click navigates to the URL as recorded"
    );
}
