//! Incremental layout, the engine side (`docs/design/INCREMENTAL_LAYOUT_PLAN.md`):
//! the `RUSTKIT_INCREMENTAL_LAYOUT` flag, the state a view keeps between
//! layouts, and what each layout did with it. The layout itself is
//! `rustkit_layout::incremental`.

use rustkit_layout::LayoutBox;
use std::cell::Cell;
use std::collections::BTreeMap;

/// `RUSTKIT_INCREMENTAL_LAYOUT`: `off` (default), `verify` (lay out in full
/// as today, run the incremental path beside it and count the box rects
/// that differ), `on` (paint the incremental result when it applies).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Off,
    Verify,
    On,
}

pub(crate) fn mode_from(value: Option<&str>) -> Mode {
    match value {
        Some("on") | Some("1") => Mode::On,
        Some("verify") => Mode::Verify,
        _ => Mode::Off,
    }
}

thread_local! {
    static TEST_MODE: Cell<Option<Mode>> = const { Cell::new(None) };
    #[cfg(test)]
    static TEST_COUNT_ONLY: Cell<bool> = const { Cell::new(false) };
}

/// Under `cfg(test)` a verify mismatch panics, so any engine test run with
/// `RUSTKIT_INCREMENTAL_LAYOUT=verify` checks the incremental path; a test
/// that sums mismatches over a corpus turns that off for its thread.
#[cfg(test)]
pub(crate) fn panic_on_mismatch() -> bool {
    !TEST_COUNT_ONLY.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn set_count_only_for_test(on: bool) {
    TEST_COUNT_ONLY.with(|c| c.set(on));
}

pub(crate) fn mode() -> Mode {
    if let Some(mode) = TEST_MODE.with(Cell::get) {
        return mode;
    }
    static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| mode_from(std::env::var("RUSTKIT_INCREMENTAL_LAYOUT").ok().as_deref()))
}

/// Run the current thread's layouts under `mode` (tests); `None` goes back
/// to the environment's.
#[cfg(test)]
pub(crate) fn set_mode_for_test(mode: Option<Mode>) {
    TEST_MODE.with(|m| m.set(mode));
}

/// What a layout is keyed on besides the box tree: a change drops the
/// retained state.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Key {
    pub(crate) bounds: (u32, u32),
    pub(crate) cb_width: f32,
    pub(crate) fonts: usize,
}

/// A view's last layout: the tree it started from and the tree it made,
/// before the root post-passes.
pub(crate) struct Retained {
    pub(crate) key: Key,
    pub(crate) unlaid: LayoutBox,
    pub(crate) laid: LayoutBox,
}

/// What the incremental path did, per view.
#[derive(Default, Debug, Clone)]
pub(crate) struct Stats {
    /// Layouts that tried the path (the flag on or verify).
    pub(crate) attempts: u32,
    /// One box laid out again and the rest moved.
    pub(crate) relaid: u32,
    /// Nothing changed: the last layout stood.
    pub(crate) unchanged: u32,
    /// Full layouts, by reason.
    pub(crate) fallbacks: BTreeMap<&'static str, u32>,
    /// Verify: boxes whose rects differed from the full layout's, in all.
    pub(crate) mismatches: u32,
    /// Verify: layouts with at least one mismatch.
    pub(crate) mismatched_layouts: u32,
    /// The last layout's boxes laid out (incremental or full).
    pub(crate) last_visits: u64,
    /// The last incremental relayout's boxes moved by the delta.
    pub(crate) last_moved: u64,
    /// The last layout's outcome, for the relayout line.
    pub(crate) last: String,
    /// Stage 2 (next commit): counted there.
    pub(crate) visits: u64,
    pub(crate) roots: u32,
    pub(crate) hoists: u32,
    pub(crate) bounded: u32,
    pub(crate) skipped_builds: u32,
    pub(crate) skip_violations: u32,
    pub(crate) reused_trees: u32,
    pub(crate) cloned_trees: u32,
}

impl Stats {
    pub(crate) fn fallback(&mut self, why: &'static str) {
        *self.fallbacks.entry(why).or_insert(0) += 1;
        self.last = format!("fallback({why})");
    }
}
