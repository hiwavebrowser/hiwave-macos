//! M1d: Cmd+1..9, Cmd+Shift+[ ] and Ctrl+Tab switch tabs wherever the
//! keyboard is, exactly once.
//!
//! The menu's arms run `tab_shortcuts::select_tab` (as Cmd+W runs
//! `close_active_tab`); a Cmd chord reaches the menu only when the focused
//! view's page did not claim it (`rustkit_viewhost::finish_key_equivalent`),
//! which `cmd_chord` models with the engine's own answer.

use rustkit_engine::{Engine, EngineConfig};
use rustkit_viewhost::Bounds;

use super::App;
use crate::tab_shortcuts::{select_tab, TabShortcut};
use crate::ChromeUi;

impl App {
    fn ids_in_strip_order(&self) -> Vec<u64> {
        let s = self.state.lock().unwrap();
        let ws = s.shell.get_active_workspace().map(|w| w.id);
        s.shell.list_tabs(ws).iter().map(|t| t.id.0).collect()
    }

    /// A menu item's shortcut, as the menu arm runs it, and the app's answer.
    fn menu(&self, shortcut: TabShortcut) -> Vec<String> {
        select_tab(&self.state, &self.chrome, shortcut);
        self.answer_chrome()
    }

    /// Cmd+`key` with the chrome focused, as the window delivers it: the
    /// chrome's page hears the keydown first; one it did not claim goes on
    /// to the menu (`shortcut`). Everything the app did, in order.
    fn cmd_chord_in_chrome(&self, key: &str, shortcut: TabShortcut) -> Vec<String> {
        let claimed = self
            .chrome
            .engine
            .borrow_mut()
            .handle_command_key(self.chrome.view, 0, key, false, false);
        let mut seen = self.answer_chrome();
        if !claimed {
            seen.extend(self.menu(shortcut));
        }
        seen
    }

    fn focus_address_bar(&self) {
        self.chrome
            .eval("document.getElementById('urlInput').focus();")
            .expect("focus the address bar");
    }
}

fn activated(id: u64) -> Vec<String> {
    vec![format!("activate_tab {id}")]
}

/// Five tabs, the page focused: Cmd+3 shows the third tab, Cmd+9 the
/// last, Cmd+1 the first (the menu's path, which the page leaves to it).
#[test]
fn cmd_n_from_the_menu_picks_the_nth_tab() {
    let app = App::with_tabs(5);
    let ids = app.ids_in_strip_order();
    assert_eq!(app.menu(TabShortcut::Nth(3)), activated(ids[2]), "Cmd+3, strip {}", app.strip());
    assert_eq!(app.active(), Some(ids[2]));
    assert_eq!(app.menu(TabShortcut::Last), activated(ids[4]), "Cmd+9");
    assert_eq!(app.active(), Some(ids[4]));
    assert_eq!(app.menu(TabShortcut::Nth(1)), activated(ids[0]), "Cmd+1");
    assert_eq!(app.active(), Some(ids[0]));
}

/// Cmd+7 and Cmd+8 with five tabs: nothing is sent, the active tab stays.
#[test]
fn cmd_n_past_the_last_tab_does_nothing() {
    let app = App::with_tabs(5);
    let before = app.active();
    assert_eq!(app.menu(TabShortcut::Nth(7)), Vec::<String>::new());
    assert_eq!(app.menu(TabShortcut::Nth(8)), Vec::<String>::new());
    assert_eq!(app.active(), before);
}

/// Next and previous (Cmd+Shift+] / [, Ctrl+Tab / Ctrl+Shift+Tab) step
/// through the strip and wrap.
#[test]
fn next_and_previous_step_through_the_strip_and_wrap() {
    let app = App::with_tabs(3);
    let ids = app.ids_in_strip_order();
    assert_eq!(app.active(), Some(ids[2]), "the last opened is active");
    assert_eq!(app.menu(TabShortcut::Next), activated(ids[0]), "next wraps to the first");
    assert_eq!(app.menu(TabShortcut::Next), activated(ids[1]));
    assert_eq!(app.menu(TabShortcut::Previous), activated(ids[0]));
    assert_eq!(app.menu(TabShortcut::Previous), activated(ids[2]), "previous wraps to the last");
}

/// The address bar focused, Cmd+2: the chrome's listener switches and
/// claims the chord, so the menu does not switch a second time.
#[test]
fn cmd_n_in_the_address_bar_switches_exactly_once() {
    let app = App::with_tabs(5);
    let ids = app.ids_in_strip_order();
    app.focus_address_bar();
    assert_eq!(
        app.cmd_chord_in_chrome("2", TabShortcut::Nth(2)),
        activated(ids[1]),
        "Cmd+2 in the address bar, strip {}",
        app.strip()
    );
    assert_eq!(app.active(), Some(ids[1]));
}

/// Cmd+9 in the chrome is the last tab, as from the menu (it was the
/// ninth tab, so with five tabs it did nothing, and with twelve it was
/// not the last).
#[test]
fn cmd_9_in_the_chrome_is_the_last_tab() {
    let app = App::with_tabs(12);
    let ids = app.ids_in_strip_order();
    app.chrome.eval("document.getElementById('urlInput').blur();").expect("blur");
    assert_eq!(app.cmd_chord_in_chrome("1", TabShortcut::Nth(1)), activated(ids[0]));
    assert_eq!(app.cmd_chord_in_chrome("9", TabShortcut::Last), activated(ids[11]), "Cmd+9 of 12");
    let app = App::with_tabs(5);
    let ids = app.ids_in_strip_order();
    app.focus_address_bar();
    assert_eq!(app.cmd_chord_in_chrome("1", TabShortcut::Nth(1)), activated(ids[0]));
    assert_eq!(app.cmd_chord_in_chrome("9", TabShortcut::Last), activated(ids[4]), "Cmd+9 of 5");
}

/// A page with its text field focused does not claim Cmd+1..9, so the
/// window hands the chord on to the menu (the bug: the menu had no item
/// for it, and the chord went nowhere).
#[test]
fn a_page_text_field_leaves_cmd_digits_to_the_menu() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds::new(0, 0, 800, 600)).expect("view");
    engine
        .load_html(view, "<!doctype html><input id=q value=hello>")
        .expect("page");
    engine.execute_script(view, "document.getElementById('q').focus();").expect("focus");
    for digit in 1..=9 {
        let claimed = engine.handle_command_key(view, 0, &digit.to_string(), false, false);
        assert!(!claimed, "the page claimed Cmd+{digit}");
    }
}
