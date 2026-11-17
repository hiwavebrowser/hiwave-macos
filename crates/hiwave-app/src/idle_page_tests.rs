//! IDLE-CPU: the browser's own pages come to rest.
//!
//! The start page (`ui/about.html`) had a CSS animation that never ended. A page with a running
//! animation is given a frame every 16 ms for as long as it runs, so the app, sitting on its start
//! page with nothing changing, drew about 53 frames a second for good (measured headless: 265
//! frames in 5 s, 44% of a core on the page's thread).
//!
//! Each page here is loaded on a headless view as its panel runs it and given live turns of 17 ms
//! of the page's clock. After `SETTLE_TURNS` the page must be at rest: over the next `REST_TURNS`
//! no animation frame is painted, nothing is laid out and no timer callback runs. The waits are
//! counts of turns on the page's clock, not time.

use rustkit_engine::{Engine, EngineConfig, EngineViewId};
use rustkit_viewhost::Bounds;

/// Turns a page is given to finish what it starts on load: 600 turns of 17 ms are 10.2 s of the
/// page's clock (the start page's glow runs twice, 6 s; its fade-in 0.6 s).
const SETTLE_TURNS: usize = 600;
/// Turns it must then stay at rest: 2 s of the page's clock.
const REST_TURNS: usize = 120;

/// What a page did over some turns.
#[derive(Debug, Default, PartialEq)]
struct Work {
    animation_frames: u32,
    relayouts: u32,
    timer_callbacks: u32,
}

fn page(html: &str, app_owned: bool) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 800))
        .expect("view");
    engine.set_app_owned(view, app_owned);
    engine.load_html(view, html).expect("page");
    (engine, view)
}

fn turns(engine: &mut Engine, view: EngineViewId, n: usize) -> Work {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let counts = |engine: &Engine| {
        let stats = engine.relayout_stats(view).expect("stats");
        (stats.animation_frames_painted, stats.relayouts)
    };
    let before = counts(engine);
    let mut timer_callbacks = 0;
    for _ in 0..n {
        timer_callbacks += rt.block_on(engine.pump_live(view, 17)).timers_ran;
    }
    let after = counts(engine);
    Work {
        animation_frames: after.0 - before.0,
        relayouts: after.1 - before.1,
        timer_callbacks,
    }
}

fn assert_comes_to_rest(name: &str, html: &str, app_owned: bool) {
    let (mut engine, view) = page(html, app_owned);
    let settling = turns(&mut engine, view, SETTLE_TURNS);
    let resting = turns(&mut engine, view, REST_TURNS);
    assert_eq!(
        resting,
        Work::default(),
        "{name}: still working {SETTLE_TURNS} turns after its load (the first {SETTLE_TURNS} turns did {settling:?})"
    );
}

#[test]
fn the_start_page_comes_to_rest() {
    let (mut engine, view) = page(crate::ABOUT_HTML, false);
    // It does animate on load (the fade-in and the glow): the count below is not of a page whose
    // animations never ran.
    let settling = turns(&mut engine, view, SETTLE_TURNS);
    assert!(settling.animation_frames > 0, "the start page's animations did not run: {settling:?}");
    let resting = turns(&mut engine, view, REST_TURNS);
    assert_eq!(resting, Work::default(), "the start page is still working after {SETTLE_TURNS} turns ({settling:?} before)");
}

#[test]
fn the_shelf_page_comes_to_rest() {
    assert_comes_to_rest("shelf", crate::SHELF_HTML, true);
}

#[test]
fn the_settings_page_comes_to_rest() {
    assert_comes_to_rest("settings", crate::SETTINGS_HTML, true);
}

#[test]
fn the_inspector_page_comes_to_rest() {
    assert_comes_to_rest("inspector", crate::INSPECTOR_HTML, true);
}

#[test]
fn the_report_page_comes_to_rest() {
    assert_comes_to_rest("report", crate::REPORT_HTML, false);
}

#[test]
fn the_browser_interface_page_comes_to_rest() {
    assert_comes_to_rest("chrome", &crate::chrome_html_with_bridge(), true);
}

/// The browser-interface page's script ran in this harness (#1236 R1): a page whose script stopped
/// early would come to rest too. Its state hook (`__hiwaveApplyState`, which the app's state
/// updates call) is set, and none of its scripts threw.
#[test]
fn the_browser_interface_pages_script_runs_in_the_idle_harness() {
    let (mut engine, view) = page(&crate::chrome_html_with_bridge(), true);
    turns(&mut engine, view, 10);
    let hook = engine
        .execute_script(view, "String(typeof window.__hiwaveApplyState)")
        .expect("script");
    assert_eq!(hook, r#"String("function")"#, "the page's script did not reach its state hook");
    let log = engine.script_log(view).expect("script log");
    assert!(!log.is_empty(), "no script of the page's ran");
    let threw: Vec<_> = log
        .iter()
        .filter(|record| matches!(record.outcome, rustkit_engine::ScriptOutcome::Threw(_)))
        .collect();
    assert!(threw.is_empty(), "the page's scripts threw: {threw:#?}");
}
