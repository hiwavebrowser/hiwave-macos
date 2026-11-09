//! M1c: the tab strip is in the order the tabs were opened, and Cmd+N
//! switches to the Nth tab of the strip.
//!
//! The shell used to hand out its tabs in `HashMap` order, so the strip
//! (`sync_tabs_to_chrome` → chrome.html `renderTabs`), `get_tabs`, the
//! workspace summaries and the export each came out in an arbitrary order,
//! and Cmd+N (chrome.html, `currentTabs[N - 1]`) picked an arbitrary tab.
//! Tabs are opened on `http://127.0.0.1/p<k>`, so a URL names its tab's
//! place in the opening order.

use super::App;
use crate::ChromeUi;
use crate::ipc::{self, IpcMessage, IpcResponse};

impl App {
    /// The shell's tabs for the active workspace: (id, url), in its order.
    fn shell_tabs(&self) -> Vec<(u64, String)> {
        let s = self.state.lock().unwrap();
        let ws = s.shell.get_active_workspace().map(|w| w.id);
        s.shell.list_tabs(ws).iter().map(|t| (t.id.0, t.url.to_string())).collect()
    }

    /// The id of the tab opened on `p<k>`.
    fn id_of(&self, k: usize) -> u64 {
        let url = format!("http://127.0.0.1/p{k}");
        self.shell_tabs()
            .into_iter()
            .find(|(_, u)| *u == url)
            .unwrap_or_else(|| panic!("no tab on {url}"))
            .0
    }

    /// The strip's tab ids, in strip order (the active mark dropped).
    fn strip_ids(&self) -> Vec<u64> {
        let strip = self.strip();
        if strip.is_empty() {
            return Vec::new();
        }
        strip.split(',').map(|id| id.trim_end_matches('*').parse().unwrap()).collect()
    }

    /// Cmd+`n` pressed in the chrome, and the app's answer.
    fn cmd_digit(&self, n: u8) -> Vec<String> {
        self.chrome
            .eval(&format!(
                "document.dispatchEvent(new KeyboardEvent('keydown', {{ key: '{n}', metaKey: true, bubbles: true }}))"
            ))
            .expect("keydown");
        self.answer_chrome()
    }

    /// An IPC request's `data`.
    fn ask(&self, msg: IpcMessage) -> serde_json::Value {
        match ipc::commands::handle_message(&self.state, msg) {
            IpcResponse::Success { data } => data,
            IpcResponse::Error { message } => panic!("ipc error: {message}"),
        }
    }
}

fn urls(tabs: &serde_json::Value) -> Vec<String> {
    tabs.as_array()
        .expect("a tab array")
        .iter()
        .map(|t| t["url"].as_str().expect("url").to_string())
        .collect()
}

fn opened(ks: &[usize]) -> Vec<String> {
    ks.iter().map(|k| format!("http://127.0.0.1/p{k}")).collect()
}

/// Five tabs: Cmd+3 shows the third opened; close the second, and Cmd+3
/// shows the fourth opened.
#[test]
fn cmd_n_picks_the_nth_tab_in_opening_order() {
    let app = App::with_tabs(5);
    let shell_urls: Vec<String> = app.shell_tabs().into_iter().map(|(_, u)| u).collect();
    assert_eq!(shell_urls, opened(&[0, 1, 2, 3, 4]), "shell order");
    let shell_ids: Vec<u64> = app.shell_tabs().into_iter().map(|(id, _)| id).collect();
    assert_eq!(app.strip_ids(), shell_ids, "strip order vs the shell");

    let third = app.id_of(2);
    assert_eq!(app.cmd_digit(3), vec![format!("activate_tab {third}")], "strip {}", app.strip());
    assert_eq!(app.active(), Some(third));

    let second = app.id_of(1);
    app.handle(IpcMessage::CloseTab { id: second.to_string() });
    app.end_turn();
    let fourth = app.id_of(3);
    assert_eq!(app.cmd_digit(3), vec![format!("activate_tab {fourth}")], "strip {}", app.strip());
    assert_eq!(app.active(), Some(fourth));

    let first = app.id_of(0);
    assert_eq!(app.cmd_digit(1), vec![format!("activate_tab {first}")], "strip {}", app.strip());
}

/// A hundred random opens and closes through the app's IPC: after each,
/// the shell is in opening order and the strip shows that order.
#[test]
fn strip_order_survives_a_hundred_random_opens_and_closes() {
    let app = App::with_tabs(0);
    let mut seed: u64 = 0x1c0ffee;
    let mut next = |below: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) as usize) % below
    };
    let mut model: Vec<usize> = Vec::new();
    for step in 0..100 {
        if model.is_empty() || next(3) != 0 {
            app.handle(IpcMessage::CreateTab { url: Some(format!("http://127.0.0.1/p{step}")) });
            model.push(step);
        } else {
            let k = model.remove(next(model.len()));
            let id = app.id_of(k);
            app.handle(IpcMessage::CloseTab { id: id.to_string() });
        }
        app.end_turn();
        let shell = app.shell_tabs();
        let shell_urls: Vec<String> = shell.iter().map(|(_, u)| u.clone()).collect();
        assert_eq!(shell_urls, opened(&model), "step {step}: shell order");
        let shell_ids: Vec<u64> = shell.iter().map(|(id, _)| *id).collect();
        assert_eq!(app.strip_ids(), shell_ids, "step {step}: strip order");
    }
}

/// `get_tabs`, the workspace summaries and the export list a workspace's
/// tabs in the order they were opened.
#[test]
fn tab_lists_over_ipc_are_in_opening_order() {
    let app = App::with_tabs(9);
    let want = opened(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(urls(&app.ask(IpcMessage::GetTabs)), want, "get_tabs");
    let workspaces = app.ask(IpcMessage::GetWorkspaces);
    let ws = workspaces.as_array().unwrap().iter().find(|w| w["is_active"] == true).expect("active ws");
    assert_eq!(urls(&ws["tabs"]), want, "get_workspaces");
    assert_eq!(urls(&app.ask(IpcMessage::GetActiveWorkspace)["tabs"]), want, "get_active_workspace");
    let export = app.ask(IpcMessage::ExportData { include_settings: false, include_workspaces: true });
    let export: serde_json::Value = serde_json::from_str(export["data"].as_str().expect("export json")).unwrap();
    let exported = export["workspaces"].as_array().unwrap().iter().find(|w| !w["tabs"].as_array().unwrap().is_empty()).expect("exported ws");
    assert_eq!(urls(&exported["tabs"]), want, "export");
}
