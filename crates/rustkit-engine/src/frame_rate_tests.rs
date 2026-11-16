//! PACE2: animation frames and 10 ms timer links per wall second on the content thread's loop.
//!
//! The page (`docs/diagnostics/2026-11-15/frame_rate_probe.html`) runs a requestAnimationFrame
//! loop and a `setTimeout(link, 10)` chain for 3000 ms of `performance.now`. Chrome for Testing
//! 148.0.7778.216, five runs (`frame_rate_probe_cft148.json`): 180-181 frames and 281-283 links.
//!
//! The grid probe (`frame_grid_probe.html`) runs a requestAnimationFrame loop whose every 4th
//! callback runs 4 ms and every 25th 24 ms, and records the gaps between the callbacks'
//! timestamps (`frame_grid_probe_cft148.json`: 180-181 frames, every gap 16.5-16.8 ms).

use std::time::{Duration, Instant};

use crate::load_turns_tests::{html, serve, Load};

pub(crate) const PROBE_PAGE: &str =
    include_str!("../../../docs/diagnostics/2026-11-15/frame_rate_probe.html");
pub(crate) const GRID_PAGE: &str =
    include_str!("../../../docs/diagnostics/2026-11-15/frame_grid_probe.html");

/// Loads `page` with `#manual`, starts it in a live turn after the load, and returns its record.
pub(crate) fn run_probe_page(page: &'static str) -> serde_json::Value {
    run_probe_page_with(page, "")
}

/// [`run_probe_page`], with `setup` run before `go()`.
pub(crate) fn run_probe_page_with(page: &'static str, setup: &str) -> serde_json::Value {
    let fixture = serve(vec![("/", html(page))]);
    let mut load = Load::start(&fixture, "/#manual");
    load.wait_complete();
    let _ = load.eval(&format!("{setup} go(); 'started'"), Duration::from_secs(10));
    // No commands while the probe runs: an eval is a turn the loop did not pace.
    std::thread::sleep(Duration::from_millis(3300));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (answer, _) = load.eval(
            "window.rec && rec.done ? JSON.stringify(rec) : ''",
            Duration::from_secs(10),
        );
        let answer = answer.unwrap_or_default();
        let text = answer
            .strip_prefix("String(\"")
            .and_then(|s| s.strip_suffix("\")"))
            .unwrap_or_default()
            .replace("\\\"", "\"");
        if !text.is_empty() {
            return serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
        }
        assert!(Instant::now() < deadline, "the probe did not finish");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The frame rate probe's record.
pub(crate) fn run_probe() -> serde_json::Value {
    run_probe_page(PROBE_PAGE)
}

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(period: u32) -> u32;
}

/// Windows sleeps in 15.6 ms ticks unless a process asks for 1 ms; `HIWAVE_PACE2_1MS` asks, so a
/// sleep lasts about what it does on macOS.
fn sleeps_as_on_macos_if_asked() {
    #[cfg(windows)]
    if std::env::var_os("HIWAVE_PACE2_1MS").is_some() {
        unsafe { timeBeginPeriod(1) };
    }
}

#[test]
#[ignore]
fn measure_the_content_threads_frame_rate() {
    sleeps_as_on_macos_if_asked();
    for _ in 0..3 {
        eprintln!("PACE2-RESULT {}", run_probe());
    }
}

#[test]
#[ignore]
fn measure_the_content_threads_frame_grid() {
    sleeps_as_on_macos_if_asked();
    for _ in 0..3 {
        eprintln!("PACE2-GRID {}", run_probe_page(GRID_PAGE));
    }
}

/// LINKS-SLOW: the lateness of each link of the 10 ms chain on the content thread's loop
/// (`links_lateness_probe.html`; Chrome, five runs, `links_lateness_probe_cft148.json`: 278-281
/// links, lateness mean 0.64-0.75 ms, median 0.6-0.7, p90 1.1-1.5, max 2.0-2.7).
pub(crate) const LINKS_PAGE: &str =
    include_str!("../../../docs/diagnostics/2026-11-16/links_lateness_probe.html");

#[test]
#[ignore]
fn measure_the_content_threads_link_lateness() {
    sleeps_as_on_macos_if_asked();
    let setup = if std::env::var_os("LINKS_NO_FRAMES").is_some() { "window.NO_FRAMES = true;" } else { "" };
    for _ in 0..3 {
        let rec = run_probe_page_with(LINKS_PAGE, setup);
        let min = rec["late"]
            .as_array()
            .map(|l| l.iter().filter_map(|x| x.as_f64()).fold(f64::INFINITY, f64::min))
            .unwrap_or(f64::NAN);
        eprintln!(
            "LINKS-RESULT frames {} links {} summary {} min {min}",
            rec["frames"], rec["timers"], rec["summary"]
        );
    }
}
