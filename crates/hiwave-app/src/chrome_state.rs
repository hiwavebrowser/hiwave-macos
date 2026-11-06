//! The chrome's state, one script per turn (CH3.A10).
//!
//! While a page loads the app tells the chrome its URL, title, back/forward
//! state, loading flag, shield counters, tab strip, sidebar, shelf and
//! history. Each used to be its own `evaluate_script` as the event that
//! changed it arrived: ~14 scripts per navigation, each a script task and
//! a full relayout of the RustKit chrome (batch 28: ~66 a second while a
//! page loads). Now each is a keyed value ([`ChromeUi::send_state`]); a
//! turn's values go to the chrome as one script at the end of the event
//! loop's turn (`MainEventsCleared`), a key sent twice in a turn goes once
//! with its last value, and a value equal to the one last sent for its key
//! is not sent again. chrome.html's `__hiwaveApplyState` applies them once
//! per animation frame and writes only what differs from the document.
//!
//! Any other script for the chrome (a shortcut's `openFindModal()`, Cmd+W
//! reading `.tab.active`) still runs at once, after the turn's pending
//! state and the page's own pending frame, so it sees what it saw before.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ChromeUi;

/// Keys whose value is sent only when it differs from the last one sent.
/// Not the URL and the title: the person can type over the address bar,
/// so the same URL committed again must reach it; chrome.html writes them
/// only when the document differs.
const DEDUPED: &[&str] = &["tabs", "workspaces", "shield", "nav", "loading", "shelf", "history", "downloads", "blocklist"];

/// The script that hands `state` (a JSON object) to chrome.html: its
/// applier, or, before the chrome's script defined it, a queue the chrome
/// drains when it boots (`__hiwaveDrainEarlyState`).
pub(crate) fn bridge_script(state: &str) -> String {
    format!(
        "(window.__hiwaveApplyState||function(s){{(window.__hiwaveEarlyState=window.__hiwaveEarlyState||[]).push(s)}})({state})"
    )
}

/// Run before any other chrome script: the page's state pending for its
/// next frame is applied first, as it was before it was deferred.
const FLUSH_PAGE_STATE: &str = "window.__hiwaveFlushState&&window.__hiwaveFlushState();";

/// A chrome view whose state updates are coalesced per turn.
pub(crate) struct CoalescedChrome<V> {
    view: V,
    /// This turn's values, in the order of their last update.
    pending: RefCell<Vec<(&'static str, String)>>,
    /// The last value sent for each deduplicated key.
    sent: RefCell<HashMap<&'static str, String>>,
    /// Whether the event loop flushes at the end of its turn. Without the
    /// RustKit loop's `MainEventsCleared` arm nothing would: each value
    /// goes at once.
    deferred: bool,
}

impl<V: ChromeUi> CoalescedChrome<V> {
    pub(crate) fn new(view: V, deferred: bool) -> Self {
        CoalescedChrome {
            view,
            pending: RefCell::new(Vec::new()),
            sent: RefCell::new(HashMap::new()),
            deferred,
        }
    }

    /// The turn's state as one object, without the values the chrome has
    /// already been sent. Empty when nothing is left.
    fn take_state(&self) -> Option<String> {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        let mut sent = self.sent.borrow_mut();
        let mut parts = Vec::new();
        for (key, json) in pending {
            if DEDUPED.contains(&key) {
                if sent.get(key) == Some(&json) {
                    continue;
                }
                sent.insert(key, json.clone());
            }
            parts.push(format!("{}:{}", serde_json::to_string(key).unwrap_or_default(), json));
        }
        (!parts.is_empty()).then(|| format!("{{{}}}", parts.join(",")))
    }
}

impl<V> std::ops::Deref for CoalescedChrome<V> {
    type Target = V;
    fn deref(&self) -> &V {
        &self.view
    }
}

impl<V: ChromeUi> ChromeUi for CoalescedChrome<V> {
    fn eval(&self, js: &str) -> Result<(), String> {
        // One script: the pending state, the page's pending frame, then `js`.
        let mut script = self.take_state().map(|state| bridge_script(&state) + ";").unwrap_or_default();
        script.push_str(FLUSH_PAGE_STATE);
        script.push_str(js);
        self.view.eval(&script)
    }

    fn send_state(&self, key: &'static str, json: String) {
        {
            let mut pending = self.pending.borrow_mut();
            pending.retain(|(k, _)| *k != key);
            pending.push((key, json));
        }
        if !self.deferred {
            self.flush_state();
        }
    }

    fn flush_state(&self) {
        if let Some(state) = self.take_state() {
            let _ = self.view.eval(&bridge_script(&state));
        }
    }

    fn place(&self, rect: wry::Rect) {
        self.view.place(rect)
    }

    fn take_keyboard(&self) {
        self.view.take_keyboard()
    }

    fn compose(&self, width: f64, height: f64, hole: (f64, f64, f64, f64)) {
        self.view.compose(width, height, hole)
    }

    fn keep_on_top(&self) {
        self.view.keep_on_top()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    /// A chrome view that records the scripts it was sent.
    struct Fake(Rc<RefCell<Vec<String>>>);

    impl ChromeUi for Fake {
        fn eval(&self, js: &str) -> Result<(), String> {
            self.0.borrow_mut().push(js.to_string());
            Ok(())
        }
        fn place(&self, _rect: wry::Rect) {}
    }

    fn state(dir: &std::path::Path) -> Arc<Mutex<crate::AppState>> {
        let config = crate::state::AppConfig {
            data_dir: dir.to_path_buf(),
            download_dir: dir.join("downloads"),
            ..Default::default()
        };
        Arc::new(Mutex::new(crate::AppState::new(config).expect("app state in a temp dir")))
    }

    /// The object a bridge script carries.
    fn payload(script: &str) -> serde_json::Value {
        let start = script.find("})(").expect("a bridge script") + 3;
        let end = script[start..].find(')').map(|_| script.rfind(')').unwrap()).unwrap();
        serde_json::from_str(&script[start..end]).expect("the state is JSON")
    }

    /// What a RustKit page's load sends the chrome in one turn: the load
    /// started and committed its URL, the tab and sidebar follow it, the
    /// title came, and then progress (the loading flag, the shield
    /// counters) many times over.
    fn a_load_turn(state: &Arc<Mutex<crate::AppState>>, chrome: &impl ChromeUi, progress: usize) {
        crate::chrome_loading(chrome, true);
        crate::chrome_url(chrome, "http://127.0.0.1/p");
        crate::sync_shield_to_chrome(state, chrome);
        crate::sync_tabs_to_chrome(state, chrome);
        crate::sync_workspaces_to_chrome(state, chrome);
        crate::chrome_nav_state(chrome, true, false);
        crate::chrome_title(chrome, "It's \"done\"");
        crate::sync_tabs_to_chrome(state, chrome);
        crate::sync_workspaces_to_chrome(state, chrome);
        for _ in 0..progress {
            crate::chrome_loading(chrome, true);
            crate::sync_shield_to_chrome(state, chrome);
        }
    }

    /// N progress events in one turn are one script for the chrome, with
    /// every key's last value.
    #[test]
    fn a_turns_progress_events_are_one_chrome_script() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path());
        let log = Rc::new(RefCell::new(Vec::new()));
        let chrome = CoalescedChrome::new(Fake(Rc::clone(&log)), true);
        a_load_turn(&state, &chrome, 50);
        assert!(log.borrow().is_empty(), "nothing is sent before the turn ends: {:?}", log.borrow());
        chrome.flush_state();
        assert_eq!(log.borrow().len(), 1, "one script for the turn");
        let sent = payload(&log.borrow()[0]);
        for key in ["loading", "url", "shield", "tabs", "workspaces", "nav", "title"] {
            assert!(sent.get(key).is_some(), "{key} missing from {sent}");
        }
        assert_eq!(sent["title"], "It's \"done\"");
        assert_eq!(sent["nav"], serde_json::json!([true, false]));
        assert_eq!(sent["loading"], true);
    }

    /// A turn that sends what the chrome already has sends nothing; the
    /// URL and title, which the person can type over, still go.
    #[test]
    fn values_equal_to_the_last_sent_are_not_sent_again() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path());
        let log = Rc::new(RefCell::new(Vec::new()));
        let chrome = CoalescedChrome::new(Fake(Rc::clone(&log)), true);
        a_load_turn(&state, &chrome, 3);
        chrome.flush_state();
        log.borrow_mut().clear();
        for _ in 0..10 {
            crate::chrome_loading(&chrome, true);
            crate::sync_shield_to_chrome(&state, &chrome);
            crate::sync_tabs_to_chrome(&state, &chrome);
            crate::sync_workspaces_to_chrome(&state, &chrome);
            crate::chrome_nav_state(&chrome, true, false);
        }
        chrome.flush_state();
        assert!(log.borrow().is_empty(), "nothing changed: {:?}", log.borrow());
        crate::chrome_loading(&chrome, false);
        crate::chrome_url(&chrome, "http://127.0.0.1/p");
        chrome.flush_state();
        assert_eq!(log.borrow().len(), 1);
        assert_eq!(payload(&log.borrow()[0]), serde_json::json!({"loading": false, "url": "http://127.0.0.1/p"}));
    }

    /// Another script for the chrome runs at once, after the turn's pending
    /// state and the page's pending frame, in one script.
    #[test]
    fn a_command_script_goes_at_once_after_the_pending_state() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let chrome = CoalescedChrome::new(Fake(Rc::clone(&log)), true);
        crate::chrome_url(&chrome, "");
        let _ = chrome.eval("openFindModal();");
        assert_eq!(log.borrow().len(), 1);
        let script = log.borrow()[0].clone();
        let (state_at, flush_at, cmd_at) = (
            script.find("__hiwaveApplyState").unwrap(),
            script.find("__hiwaveFlushState").unwrap(),
            script.find("openFindModal").unwrap(),
        );
        assert!(state_at < flush_at && flush_at < cmd_at, "{script}");
        chrome.flush_state();
        assert_eq!(log.borrow().len(), 1, "the state went with the command");
    }

    /// Without the RustKit loop's end-of-turn flush, each value goes at once.
    #[test]
    fn a_chrome_not_flushed_by_the_loop_gets_each_value_at_once() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let chrome = CoalescedChrome::new(Fake(Rc::clone(&log)), false);
        crate::chrome_loading(&chrome, true);
        crate::chrome_title(&chrome, "t");
        assert_eq!(log.borrow().len(), 2);
    }
}
