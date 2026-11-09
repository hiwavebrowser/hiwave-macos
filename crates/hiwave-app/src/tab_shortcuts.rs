//! M1d: the tab-switching shortcuts work wherever the keyboard is.
//!
//! Cmd+1..9 used to exist only as a keydown listener in chrome.html, so
//! they did nothing while a page had the keyboard (the page does not
//! claim the chord, and no menu item took it). They are now menu items
//! with key equivalents, like Cmd+W/T/L, and pick their tab from the
//! shell's strip order (`BrowserShell::list_tabs`, M1c #1010):
//!
//! - Cmd+1..8: the Nth tab; no Nth tab, nothing happens.
//! - Cmd+9: the last tab (Chrome and Safari).
//! - Cmd+Shift+] / Cmd+Shift+[: the next / previous tab, wrapping.
//! - Ctrl+Tab / Ctrl+Shift+Tab: the same. muda's key equivalent for Tab is
//!   the "⇥" glyph, not a tab character, so a menu item cannot take this
//!   chord; the app takes it from the view's key stream instead
//!   (`tab_cycle_chord`), before the page or the chrome hears it, as
//!   Chromium reserves it. The RustKit view records it from
//!   performKeyEquivalent: (rustkit-viewhost): left unclaimed there, the
//!   window takes it for its key-view loop and keyDown: never sees it.
//!
//! Exactly once: a Cmd chord goes to the focused RustKit view first and
//! on to the menu only when the view's page did not claim it
//! (`rustkit_viewhost::finish_key_equivalent`). The chrome's own Cmd+1..9
//! listener cancels the keydown, so with the chrome focused the chrome
//! switches and the menu does not; with a page focused the page leaves it
//! and the menu switches. Cmd+Shift+[ ] are reserved chords that go
//! straight to the menu and no page hears.

use std::time::{Duration, Instant};

use hiwave_core::types::TabId;

/// A tab shortcut, resolved against the strip's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabShortcut {
    /// Cmd+N, 1-based (1..=8).
    Nth(usize),
    /// Cmd+9.
    Last,
    /// Cmd+Shift+], Ctrl+Tab.
    Next,
    /// Cmd+Shift+[, Ctrl+Shift+Tab.
    Previous,
}

/// The tab `shortcut` selects among `order` (the strip, left to right)
/// with `active` the active tab. `None` when it selects nothing, or the
/// tab that is already active.
pub(crate) fn tab_for_shortcut(order: &[TabId], active: Option<TabId>, shortcut: TabShortcut) -> Option<TabId> {
    let at = active.and_then(|a| order.iter().position(|t| *t == a));
    let target = match shortcut {
        TabShortcut::Nth(n) if n >= 1 => order.get(n - 1).copied(),
        TabShortcut::Nth(_) => None,
        TabShortcut::Last => order.last().copied(),
        TabShortcut::Next => at.map(|i| order[(i + 1) % order.len()]),
        TabShortcut::Previous => at.map(|i| order[(i + order.len() - 1) % order.len()]),
    };
    target.filter(|t| Some(*t) != active)
}

/// How long a pending shortcut target stands in for the active tab: the
/// `activate_tab` round trip is one event-loop turn, so this only bounds
/// a target whose activation never landed.
const PENDING_FOR: Duration = Duration::from_millis(1000);

/// A tab shortcut's target while its `activate_tab` is in flight (#1012
/// R1 nit 3): `select_tab` reads the shell's active tab, which changes
/// only when the chrome's message comes back, so two fast presses of
/// Ctrl+Tab (or key repeat) both stepped from the same tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingSelect {
    /// The shell's active tab when the shortcut was sent.
    pub from: Option<TabId>,
    /// The tab it asked for.
    pub to: TabId,
    pub at: Instant,
}

/// The tab the next shortcut steps from: a pending target while the shell
/// still shows the tab it was sent from, the target is still in the strip
/// and it is recent; otherwise the shell's active tab.
pub(crate) fn effective_active(
    shell_active: Option<TabId>,
    order: &[TabId],
    pending: Option<&PendingSelect>,
    now: Instant,
) -> Option<TabId> {
    match pending {
        Some(p) if p.from == shell_active && order.contains(&p.to) && now.duration_since(p.at) < PENDING_FOR => Some(p.to),
        _ => shell_active,
    }
}

/// Ctrl+Tab (next) and Ctrl+Shift+Tab (previous), from a view's key
/// stream. The chord itself is `rustkit_viewhost::is_tab_cycle_chord`,
/// the one predicate the view also claims it by (#1012 R1 nit 2), so
/// Ctrl+Option+Tab is neither claimed nor taken: the page hears it.
pub(crate) fn tab_cycle_chord(mac_keycode: u16, ctrl: bool, cmd: bool, alt: bool, shift: bool) -> Option<TabShortcut> {
    rustkit_viewhost::is_tab_cycle_chord(mac_keycode, ctrl, cmd, alt)
        .then_some(if shift { TabShortcut::Previous } else { TabShortcut::Next })
}

/// The menu item ids of the tab shortcuts, and the shortcut each runs.
#[cfg(target_os = "macos")]
pub(crate) fn shortcut_for_menu_id(id: &str) -> Option<TabShortcut> {
    use crate::platform::menu_ids;
    match id {
        menu_ids::SELECT_LAST_TAB => Some(TabShortcut::Last),
        menu_ids::SELECT_NEXT_TAB => Some(TabShortcut::Next),
        menu_ids::SELECT_PREVIOUS_TAB => Some(TabShortcut::Previous),
        _ => {
            let n: usize = id.strip_prefix(menu_ids::SELECT_TAB_PREFIX)?.parse().ok()?;
            (1..=8).contains(&n).then_some(TabShortcut::Nth(n))
        }
    }
}

/// Run `shortcut`: the chrome sends `activate_tab` for the tab it selects
/// in the active workspace's strip, as a click on that tab does, so the
/// app's one activation path (shell, navigation, zoom, strip sync) runs.
/// Nothing when it selects nothing.
pub(crate) fn select_tab(
    state: &std::sync::Arc<std::sync::Mutex<crate::AppState>>,
    chrome: &impl crate::ChromeUi,
    shortcut: TabShortcut,
) {
    let target = {
        let Ok(mut s) = state.lock() else { return };
        let workspace = s.shell.get_active_workspace().map(|w| w.id);
        let order: Vec<TabId> = s.shell.list_tabs(workspace).iter().map(|t| t.id).collect();
        let shell_active = s.shell.get_active_tab().map(|t| t.id);
        let now = Instant::now();
        let active = effective_active(shell_active, &order, s.tab_select_pending.as_ref(), now);
        let target = tab_for_shortcut(&order, active, shortcut);
        if let Some(to) = target {
            s.tab_select_pending = Some(PendingSelect { from: shell_active, to, at: now });
        }
        target
    };
    tracing::info!(?shortcut, target = ?target.map(|t| t.0), "tab shortcut");
    if let Some(id) = target {
        let _ = chrome.eval(&format!("sendIpc('activate_tab', {{ id: '{}' }});", id.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: u64) -> Vec<TabId> {
        (1..=n).map(TabId).collect()
    }

    #[test]
    fn nth_picks_the_nth_tab_of_the_strip() {
        let order = ids(5);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(5)), TabShortcut::Nth(1)), Some(TabId(1)));
        assert_eq!(tab_for_shortcut(&order, Some(TabId(5)), TabShortcut::Nth(3)), Some(TabId(3)));
        // The strip's order, not the ids' order.
        let shuffled = vec![TabId(9), TabId(2), TabId(7)];
        assert_eq!(tab_for_shortcut(&shuffled, Some(TabId(9)), TabShortcut::Nth(2)), Some(TabId(2)));
    }

    #[test]
    fn nth_past_the_end_does_nothing() {
        let order = ids(5);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(1)), TabShortcut::Nth(6)), None);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(1)), TabShortcut::Nth(8)), None);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(1)), TabShortcut::Nth(0)), None);
        assert_eq!(tab_for_shortcut(&[], None, TabShortcut::Nth(1)), None);
    }

    #[test]
    fn the_active_tab_is_not_selected_again() {
        let order = ids(5);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(3)), TabShortcut::Nth(3)), None);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(5)), TabShortcut::Last), None);
        assert_eq!(tab_for_shortcut(&ids(1), Some(TabId(1)), TabShortcut::Next), None);
    }

    #[test]
    fn nine_is_the_last_tab_however_many() {
        assert_eq!(tab_for_shortcut(&ids(5), Some(TabId(1)), TabShortcut::Last), Some(TabId(5)));
        assert_eq!(tab_for_shortcut(&ids(12), Some(TabId(1)), TabShortcut::Last), Some(TabId(12)));
        assert_eq!(tab_for_shortcut(&[], None, TabShortcut::Last), None);
    }

    #[test]
    fn next_and_previous_wrap() {
        let order = ids(3);
        assert_eq!(tab_for_shortcut(&order, Some(TabId(1)), TabShortcut::Next), Some(TabId(2)));
        assert_eq!(tab_for_shortcut(&order, Some(TabId(3)), TabShortcut::Next), Some(TabId(1)));
        assert_eq!(tab_for_shortcut(&order, Some(TabId(1)), TabShortcut::Previous), Some(TabId(3)));
        assert_eq!(tab_for_shortcut(&order, Some(TabId(2)), TabShortcut::Previous), Some(TabId(1)));
        assert_eq!(tab_for_shortcut(&order, None, TabShortcut::Next), None);
    }

    #[test]
    fn ctrl_tab_is_next_and_ctrl_shift_tab_previous() {
        assert_eq!(tab_cycle_chord(48, true, false, false, false), Some(TabShortcut::Next));
        assert_eq!(tab_cycle_chord(48, true, false, false, true), Some(TabShortcut::Previous));
        assert_eq!(tab_cycle_chord(48, false, false, false, false), None, "plain Tab is the page's");
        assert_eq!(tab_cycle_chord(48, true, true, false, false), None, "Cmd+Ctrl+Tab");
        assert_eq!(tab_cycle_chord(48, true, false, true, false), None, "Ctrl+Option+Tab");
        assert_eq!(tab_cycle_chord(0, true, false, false, false), None, "Ctrl+A");
    }

    #[test]
    fn ctrl_option_tab_is_not_the_chord() {
        // #1012 R1 nit 2: the view and the app use one predicate, so the
        // view does not claim a chord the app then hands to the page.
        assert_eq!(tab_cycle_chord(48, true, false, true, false), None);
        assert_eq!(tab_cycle_chord(48, true, false, true, true), None);
        assert!(!rustkit_viewhost::is_tab_cycle_chord(48, true, false, true));
        assert!(rustkit_viewhost::is_tab_cycle_chord(48, true, false, false));
    }

    #[test]
    fn a_pending_target_is_the_step_base_until_the_shell_moves() {
        let order = ids(5);
        let t0 = Instant::now();
        let pending = PendingSelect { from: Some(TabId(1)), to: TabId(2), at: t0 };
        // In flight: the shell still shows tab 1, the next step is from 2.
        assert_eq!(effective_active(Some(TabId(1)), &order, Some(&pending), t0), Some(TabId(2)));
        // Landed (or another tab was activated): the shell's own.
        assert_eq!(effective_active(Some(TabId(2)), &order, Some(&pending), t0), Some(TabId(2)));
        assert_eq!(effective_active(Some(TabId(4)), &order, Some(&pending), t0), Some(TabId(4)));
        // Stale: an activation that never landed stops counting.
        assert_eq!(
            effective_active(Some(TabId(1)), &order, Some(&pending), t0 + Duration::from_secs(2)),
            Some(TabId(1))
        );
        // Its target closed meanwhile.
        assert_eq!(effective_active(Some(TabId(1)), &[TabId(1), TabId(3)], Some(&pending), t0), Some(TabId(1)));
        assert_eq!(effective_active(Some(TabId(1)), &order, None, t0), Some(TabId(1)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn every_tab_menu_id_runs_its_shortcut() {
        use crate::platform::menu_ids;
        for n in 1..=8 {
            let id = format!("{}{n}", menu_ids::SELECT_TAB_PREFIX);
            assert_eq!(shortcut_for_menu_id(&id), Some(TabShortcut::Nth(n)), "{id}");
        }
        assert_eq!(shortcut_for_menu_id(menu_ids::SELECT_LAST_TAB), Some(TabShortcut::Last));
        assert_eq!(shortcut_for_menu_id(menu_ids::SELECT_NEXT_TAB), Some(TabShortcut::Next));
        assert_eq!(shortcut_for_menu_id(menu_ids::SELECT_PREVIOUS_TAB), Some(TabShortcut::Previous));
        assert_eq!(shortcut_for_menu_id(&format!("{}9", menu_ids::SELECT_TAB_PREFIX)), None);
        assert_eq!(shortcut_for_menu_id(menu_ids::CLOSE_TAB), None);
    }
}
