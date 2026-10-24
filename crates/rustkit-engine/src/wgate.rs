//! The W-gate log (threads plan `THREADS_PLAN_2026-10.md` §4 invariant 9
//! and §5 "The W-gate"): every window-thread hold of 50 ms or more, by
//! stage, and input-to-present latency.
//!
//! The Presenter stays on the window thread until M9, so the window can
//! still stall on the page's build, layout and paint, and on the
//! presenter's own surface acquire, execute and present. The W-gate reads
//! this log at M4 on eBay search, cnn and the Wikipedia portal: a stage
//! that holds the window 50 ms or more, or input-to-present over 32 ms at
//! p95, moves M9 (the render thread) ahead of M5.
//!
//! Two measurements, both on the window thread, both always on (two
//! `Instant` reads per stage and one per entry; nothing is logged under
//! the threshold):
//!
//! 1. **Holds.** Every entry the window thread makes into the engine that
//!    can run page code or draw ([`WindowHold`]) is timed; the outermost
//!    one reports if it lasted [`HOLD_THRESHOLD_MS`] or more, with the
//!    milliseconds each stage took inside it:
//!
//!    ```text
//!    Window thread hold entry=live_turn ms=612 build_ms=0.0 layout_ms=0.0 display_list_ms=0.0 snapshot_ms=0.0 render_ms=0.0 acquire_ms=0.0 execute_ms=0.0 present_ms=0.0 other_ms=612.0
//!    Content thread hold entry=live_turn ms=612 ...
//!
//!    The word is `Content` when the entry runs on the page thread
//!    (named `page`). Flag-off entries stay `Window thread hold`.
//!    ```
//!
//!    `other_ms` is what no stage claims: script, timers, network
//!    settling, the hit test. The presenter's three stages run inside
//!    `render` and are its breakdown, so `other` is the hold less the
//!    five relayout stages. A nested entry (a relayout inside a click)
//!    is counted by the outer one and reports nothing itself.
//!
//! 2. **Input to present.** The app notes each input event's arrival
//!    time as it drains its queue ([`InputMark`]); the first unpresented
//!    one rides the next [`super::presenter::FrameCommit`] and the
//!    presenter logs the time from arrival to the surface present:
//!
//!    ```text
//!    Input to present ms=41 seq=1207
//!    Input to present p50_ms=12 p95_ms=38 max_ms=612 n=100   (every 100 inputs)
//!    ```
//!
//!    An input that changed no frame (a move over nothing) is dropped at
//!    the next `render_changed_views`, not charged to the next frame.
//!
//! Limits: the hold log sees entries into the engine, not the app's own
//! work between them (chrome messages, the UI WebView); `window_stall.rs`
//! in the app measures those from outside. A hold inside a single
//! `Instant` read's resolution is 0.

use std::cell::Cell;
use std::fmt;
use std::time::Instant;

/// The shortest hold that is reported, in milliseconds (the W-gate's
/// figure).
pub const HOLD_THRESHOLD_MS: f64 = 50.0;

/// How many input-to-present samples make one summary line.
pub const LATENCY_SUMMARY_EVERY: usize = 100;

/// What the window thread was doing, for the breakdown of a hold. The
/// first five are the relayout stages (`relayout_cause::Stage`); the
/// last three are the presenter's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldStage {
    Build,
    Layout,
    DisplayList,
    Snapshot,
    Render,
    /// Waiting for the surface's next texture (`get_surface_texture`):
    /// vsync and the swap chain.
    Acquire,
    /// Image upload and `Renderer::execute`.
    Execute,
    /// `Compositor::present`.
    Present,
}

const STAGES: usize = 8;
/// The first five of [`HoldStage`]: the ones that add up to a hold.
const RELAYOUT_STAGES: usize = 5;

/// Milliseconds a hold spent in each stage.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HoldMs {
    ms: [f64; STAGES],
}

impl HoldMs {
    fn add(&mut self, stage: HoldStage, ms: f64) {
        self.ms[stage as usize] += ms;
    }

    /// What the stages claim: the five relayout stages only. The
    /// presenter's three run inside `render` (`Stage::Render` times all
    /// of `Engine::render`), so they are its breakdown, not more time.
    fn staged(&self) -> f64 {
        self.ms[..RELAYOUT_STAGES].iter().sum()
    }

    /// What no stage claims, of a hold that took `total_ms`.
    pub fn other_ms(&self, total_ms: f64) -> f64 {
        (total_ms - self.staged()).max(0.0)
    }

    #[cfg(test)]
    fn of(&self, stage: HoldStage) -> f64 {
        self.ms[stage as usize]
    }
}

impl fmt::Display for HoldMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "build_ms={:.1} layout_ms={:.1} display_list_ms={:.1} snapshot_ms={:.1} render_ms={:.1} acquire_ms={:.1} execute_ms={:.1} present_ms={:.1}",
            self.ms[0], self.ms[1], self.ms[2], self.ms[3], self.ms[4], self.ms[5], self.ms[6], self.ms[7]
        )
    }
}

thread_local! {
    /// The stage milliseconds of the hold in progress on this thread.
    static HOLD_MS: Cell<HoldMs> = const { Cell::new(HoldMs { ms: [0.0; STAGES] }) };
    /// How many [`WindowHold`] guards are open; only the outermost
    /// reports.
    static HOLD_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// The last hold the outermost guard reported, for tests.
    #[cfg(test)]
    static LAST_HOLD: Cell<Option<(&'static str, f64)>> = const { Cell::new(None) };
    /// The last outermost entry that closed, reported or not, for tests.
    #[cfg(test)]
    static LAST_ENTRY: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// The last hold the outermost guard reported on this thread (entry,
/// milliseconds), for tests.
#[cfg(test)]
pub(crate) fn last_hold() -> Option<(&'static str, f64)> {
    LAST_HOLD.with(|c| c.get())
}

/// The last outermost entry that closed on this thread, whatever its
/// length, for tests.
#[cfg(test)]
pub(crate) fn last_entry() -> Option<&'static str> {
    LAST_ENTRY.with(|c| c.get())
}

/// Forgets both, for tests.
#[cfg(test)]
pub(crate) fn clear_last() {
    LAST_HOLD.with(|c| c.set(None));
    LAST_ENTRY.with(|c| c.set(None));
}

/// Adds `ms` of `stage` to the hold in progress (called by the relayout
/// stage timers and the presenter's).
pub(crate) fn add(stage: HoldStage, ms: f64) {
    HOLD_MS.with(|c| {
        let mut hold = c.get();
        hold.add(stage, ms);
        c.set(hold);
    });
}

/// Times one presenter stage until dropped.
pub(crate) struct StageTimer {
    stage: HoldStage,
    started: Instant,
}

pub(crate) fn time_stage(stage: HoldStage) -> StageTimer {
    StageTimer {
        stage,
        started: Instant::now(),
    }
}

impl Drop for StageTimer {
    fn drop(&mut self) {
        add(self.stage, self.started.elapsed().as_secs_f64() * 1000.0);
    }
}

/// One entry of the window thread into the engine. Dropping it ends the
/// hold; the outermost guard reports a hold of [`HOLD_THRESHOLD_MS`] or
/// more.
pub(crate) struct WindowHold {
    entry: &'static str,
    started: Instant,
    outermost: bool,
}

/// Starts timing an entry named `entry` (`live_turn`, `relayout`,
/// `scroll`, `click`, ...).
pub(crate) fn hold(entry: &'static str) -> WindowHold {
    let outermost = HOLD_DEPTH.with(|d| {
        let depth = d.get();
        d.set(depth + 1);
        depth == 0
    });
    if outermost {
        HOLD_MS.with(|c| c.set(HoldMs::default()));
    }
    WindowHold {
        entry,
        started: Instant::now(),
        outermost,
    }
}

impl Drop for WindowHold {
    fn drop(&mut self) {
        HOLD_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        if !self.outermost {
            return;
        }
        let total_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        let stages = HOLD_MS.with(|c| c.replace(HoldMs::default()));
        #[cfg(test)]
        LAST_ENTRY.with(|c| c.set(Some(self.entry)));
        if let Some(line) = hold_line(hold_thread_name(), self.entry, total_ms, &stages) {
            #[cfg(test)]
            LAST_HOLD.with(|c| c.set(Some((self.entry, total_ms))));
            tracing::warn!("{line}");
        }
    }
}

/// `Content` on the page thread, `Window` everywhere else. The content
/// thread is spawned with the name `page`.
pub(crate) fn hold_thread_name() -> &'static str {
    match std::thread::current().name() {
        Some("page") => "Content",
        _ => "Window",
    }
}

/// The log line of a hold, or `None` under the threshold. Pure, so the
/// decision is testable without a clock. `thread` is [`hold_thread_name`].
pub(crate) fn hold_line(thread: &str, entry: &str, total_ms: f64, stages: &HoldMs) -> Option<String> {
    if total_ms < HOLD_THRESHOLD_MS {
        return None;
    }
    Some(format!(
        "{thread} thread hold entry={entry} ms={:.0} {stages} other_ms={:.1}",
        total_ms,
        stages.other_ms(total_ms)
    ))
}

/// When an input event arrived at the app's queue, and its sequence
/// number there (`rustkit_viewhost::InputEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputMark {
    pub at: Instant,
    pub seq: u64,
}

/// Input-to-present samples, summarised every [`LATENCY_SUMMARY_EVERY`].
#[derive(Debug, Default)]
pub(crate) struct InputLatency {
    window: Vec<u32>,
    /// Samples taken since the presenter was built (tests read it).
    pub(crate) samples: u64,
}

impl InputLatency {
    /// Records one sample; returns the summary line when the window is
    /// full.
    pub(crate) fn record(&mut self, ms: u32) -> Option<String> {
        self.samples += 1;
        self.window.push(ms);
        if self.window.len() < LATENCY_SUMMARY_EVERY {
            return None;
        }
        let line = summary_line(&self.window);
        self.window.clear();
        Some(line)
    }
}

/// `p50_ms p95_ms max_ms n` of `samples` (nearest-rank percentiles).
pub(crate) fn summary_line(samples: &[u32]) -> String {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = |p: f64| -> u32 {
        if sorted.is_empty() {
            return 0;
        }
        let k = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
        sorted[k - 1]
    };
    format!(
        "Input to present p50_ms={} p95_ms={} max_ms={} n={}",
        rank(0.5),
        rank(0.95),
        sorted.last().copied().unwrap_or(0),
        sorted.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_under_the_threshold_is_not_reported() {
        assert_eq!(hold_line("Window", "scroll", 49.9, &HoldMs::default()), None);
        let line = hold_line("Window", "scroll", 50.0, &HoldMs::default()).expect("reported");
        assert!(line.starts_with("Window thread hold entry=scroll ms=50 "), "{line}");
        assert!(line.ends_with(" other_ms=50.0"), "{line}");
    }

    #[test]
    fn a_hold_on_the_content_thread_is_not_labelled_as_the_window() {
        assert_eq!(hold_thread_name(), "Window");
        let label = std::thread::Builder::new()
            .name("page".into())
            .spawn(hold_thread_name)
            .expect("spawn")
            .join()
            .expect("join");
        assert_eq!(label, "Content");
        let line = hold_line("Content", "live_turn", 80.0, &HoldMs::default()).expect("reported");
        assert!(
            line.starts_with("Content thread hold entry=live_turn ms=80 "),
            "{line}"
        );
    }

    #[test]
    fn a_hold_names_what_each_stage_took_and_what_nothing_claims() {
        let mut stages = HoldMs::default();
        stages.add(HoldStage::Layout, 300.0);
        stages.add(HoldStage::Render, 40.0);
        stages.add(HoldStage::Acquire, 16.0);
        stages.add(HoldStage::Acquire, 16.0);
        let line = hold_line("Window", "live_turn", 612.0, &stages).expect("reported");
        // The presenter's three stages are a breakdown of `render`, so
        // they are not counted again: `other` is 612 - 300 - 40.
        assert_eq!(
            line,
            "Window thread hold entry=live_turn ms=612 build_ms=0.0 layout_ms=300.0 display_list_ms=0.0 snapshot_ms=0.0 render_ms=40.0 acquire_ms=32.0 execute_ms=0.0 present_ms=0.0 other_ms=272.0"
        );
        // Without the layout: a 100 ms hold with a 40 ms render whose
        // acquire took 32 has 60 ms of `other`, not 28.
        let mut draw = HoldMs::default();
        draw.add(HoldStage::Render, 40.0);
        draw.add(HoldStage::Acquire, 32.0);
        assert_eq!(draw.other_ms(100.0), 60.0, "40 ms of render, 60 of script");
        // A 100 ms turn with 40 ms of script and a 60 ms render of which
        // 55 was the execute (the review's case): the script is `other`.
        let mut turn = HoldMs::default();
        turn.add(HoldStage::Render, 60.0);
        turn.add(HoldStage::Execute, 55.0);
        assert_eq!(turn.other_ms(100.0), 40.0);
        // Nothing can push `other` below zero.
        assert_eq!(turn.other_ms(50.0), 0.0);
    }

    #[test]
    fn every_entry_records_its_name_and_only_a_long_one_is_reported() {
        clear_last();
        {
            let _h = hold("render_view");
            add(HoldStage::Render, 1.0);
        }
        assert_eq!(last_entry(), Some("render_view"), "recorded whatever its length");
        assert_eq!(last_hold(), None, "under the threshold: not reported");
    }

    #[test]
    fn only_the_outermost_entry_reports_and_it_counts_the_nested_stages() {
        {
            let _outer = hold("click");
            {
                let _inner = hold("relayout");
                add(HoldStage::Layout, 70.0);
                let _timed = time_stage(HoldStage::Present);
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            // The inner guard reported nothing: its stages are still the
            // outer hold's.
            assert_eq!(LAST_HOLD.with(|c| c.get()), None);
            let stages = HOLD_MS.with(|c| c.get());
            assert_eq!(stages.of(HoldStage::Layout), 70.0);
            assert!(stages.of(HoldStage::Present) >= 60.0, "{stages}");
        }
        let (entry, ms) = LAST_HOLD.with(|c| c.take()).expect("the outer hold reported");
        assert_eq!(entry, "click");
        assert!(ms >= 60.0, "{ms}");
        assert_eq!(HOLD_DEPTH.with(|d| d.get()), 0);
        assert_eq!(HOLD_MS.with(|c| c.get()), HoldMs::default(), "cleared for the next entry");
    }

    #[test]
    fn a_short_entry_reports_nothing() {
        LAST_HOLD.with(|c| c.set(None));
        {
            let _h = hold("move");
            add(HoldStage::Build, 1.0);
        }
        assert_eq!(LAST_HOLD.with(|c| c.get()), None);
    }

    #[test]
    fn latency_summarises_every_hundred_samples_with_nearest_rank_percentiles() {
        let mut latency = InputLatency::default();
        for i in 1..LATENCY_SUMMARY_EVERY as u32 {
            assert_eq!(latency.record(i), None, "sample {i}");
        }
        let line = latency.record(612).expect("the hundredth sample summarises");
        assert_eq!(line, "Input to present p50_ms=50 p95_ms=95 max_ms=612 n=100");
        assert_eq!(latency.samples, 100);
        assert_eq!(latency.record(3), None, "a new window");
        assert_eq!(summary_line(&[]), "Input to present p50_ms=0 p95_ms=0 max_ms=0 n=0");
        assert_eq!(summary_line(&[7]), "Input to present p50_ms=7 p95_ms=7 max_ms=7 n=1");
    }
}
