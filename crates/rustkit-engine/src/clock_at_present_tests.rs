//! CLOCK-AT-PRESENT: with one animation round per turn (RAF-ONE-ROUND, `set_one_animation_round`),
//! a turn that runs late presents the frame of the turn's time: its callbacks are stamped with
//! that frame's time, and between turns the document timeline and the CSS animation clock hold at
//! it. Pinned on Chrome for Testing 148.0.7778.216, headless,
//! `docs/diagnostics/2026-11-17/clock_at_present_probe_cft148.json` (5 runs, frames whose callback
//! works 25 ms, top document and a frame document inserted after load): in every late frame
//! `timeline.currentTime - t` = 0 and the CSS animation's `startTime + currentTime - t` = 0 and
//! `currentTime - opacity * 10000` = 0; in a task after it, `timeline.currentTime - t` = 0,
//! `startTime + currentTime - timeline` = 0, `currentTime - opacity * 10000` = 0 (200 of 200 rows).
//!
//! Before: a late round's callback asked for the frame of the turn's grid point, due already, which
//! ran on the next turn stamped with that older grid point (a turn behind the frame presented
//! with it), and the timeline a task read after the turn was the turn's grid point, ahead of the
//! last stamp; a frame document's CSS clock read the timers' clock while that frame waited.
//! Deterministic page clock: stamps are grid times, no wall-clock reading is asserted.

use super::*;
use crate::script_net_tests::serve_routes;

struct Doc {
    engine: Engine,
    view: EngineViewId,
    rt: tokio::runtime::Runtime,
    _server: crate::script_net_tests::Server,
}

const FADE: &str = "<style>@keyframes fade{from{opacity:0}to{opacity:1}}#a{width:10px;height:10px;animation:fade 10s linear}</style>";

/// A CSS animation and a rAF loop recording its stamps, in the document whose window `w()` is.
fn looped(body: &str) -> String {
    format!("<!doctype html>{FADE}<div id=a></div><script>window.stamps = [];
function tick(t) {{ stamps.push(t); requestAnimationFrame(tick); }}
{body}</script>")
}

impl Doc {
    fn load(page: String, child: String) -> Self {
        let server = serve_routes(vec![("/", "text/html", page), ("/child.html", "text/html", child)]);
        let config = EngineConfig {
            nested_contexts: true,
            post_load_requests_live: true,
            deterministic_page_clock: true,
            ..EngineConfig::default()
        };
        crate::set_one_animation_round(true);
        let mut engine = Engine::new(config).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
            .expect("view");
        let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(engine.load_url(view, url)).expect("load_url");
        let mut doc = Doc { engine, view, rt, _server: server };
        for _ in 0..100 {
            if doc.read("window.ready") == "true" {
                break;
            }
            doc.turn(20);
        }
        assert_eq!(doc.read("window.ready"), "true", "the page is ready");
        doc
    }

    fn turn(&mut self, ms: u64) {
        self.rt.block_on(self.engine.pump_live(self.view, ms));
    }

    fn read(&mut self, expr: &str) -> String {
        let v = self.engine.execute_script(self.view, &format!("String({expr})")).unwrap();
        v.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&v).to_string()
    }

    fn nums(&mut self, expr: &str) -> Vec<f64> {
        let v = self.read(expr);
        v.split(',').map(|x| x.parse::<f64>().unwrap_or_else(|_| panic!("{expr}: {v}"))).collect()
    }

    /// After each of `turns` turns of `ms`: [timeline - the last stamp, the CSS animation's
    /// startTime + currentTime - timeline, its currentTime - the painted opacity's time].
    fn rows(&mut self, turns: usize, ms: u64) -> Vec<Vec<f64>> {
        let expr = "(function () { var W = w(), c = W.document.getElementById('a').getAnimations()[0], tl = W.document.timeline.currentTime;\
            return [tl - W.stamps[W.stamps.length - 1], c.startTime + c.currentTime - tl,\
              c.currentTime - parseFloat(W.getComputedStyle(W.document.getElementById('a')).opacity) * 10000].join(','); })()";
        (0..turns)
            .map(|_| {
                self.turn(ms);
                self.nums(expr)
            })
            .collect()
    }
}

fn single() -> Doc {
    Doc::load(looped("window.ready = true; function w() { return window; } requestAnimationFrame(tick);"), String::new())
}

fn framed() -> Doc {
    let page = "<!doctype html><body><iframe id=f src=/child.html></iframe><script>window.ready = false;
document.getElementById('f').addEventListener('load', function () { ready = true; });
function w() { return document.getElementById('f').contentWindow; }</script></body>"
        .to_string();
    Doc::load(page, looped("requestAnimationFrame(tick);"))
}

fn assert_rows(rows: &[Vec<f64>], what: &str) {
    for (i, r) in rows.iter().enumerate() {
        assert!(r[0].abs() < 1.0, "{what} turn {i}: timeline - last rAF stamp {rows:?} (Chrome: 0)");
        assert!(r[1].abs() < 1.0, "{what} turn {i}: startTime + currentTime - timeline {rows:?} (Chrome: 0)");
        assert!(r[2].abs() < 1.0, "{what} turn {i}: currentTime - painted time {rows:?} (Chrome: 0)");
    }
}

/// Late turns (50 ms, three grid steps): the frame presented is stamped with the turn's frame
/// time, and a task after it reads that stamp. Was: the stamp was a turn behind.
#[test]
fn a_late_turns_frame_is_stamped_with_its_own_time() {
    let mut doc = single();
    doc.rows(3, 50);
    let rows = doc.rows(8, 50);
    assert_rows(&rows, "single");
    let stamps = doc.nums("stamps.slice(-8).join(',')");
    for pair in stamps.windows(2) {
        assert!((pair[1] - pair[0] - 50.0).abs() < 1.0, "a stamp a turn: {stamps:?}");
    }
}

/// The same in a frame document of a frame tree.
#[test]
fn a_late_turns_frame_is_stamped_with_its_own_time_in_a_frame_document() {
    let mut doc = framed();
    doc.rows(3, 50);
    let rows = doc.rows(8, 50);
    assert_rows(&rows, "frame");
}

/// Turns a little longer than a frame (20 ms: now and then two grid points pass): the painted
/// time is the time the animation reports, in the top document.
#[test]
fn a_css_animation_paints_the_presented_frames_time() {
    let mut doc = single();
    doc.rows(3, 20);
    let rows = doc.rows(12, 20);
    assert_rows(&rows, "single");
}

/// ... and in a frame document (the css_timeline row the switch broke).
#[test]
fn a_css_animation_paints_the_presented_frames_time_in_a_frame_document() {
    let mut doc = framed();
    doc.rows(3, 20);
    let rows = doc.rows(12, 20);
    assert_rows(&rows, "frame");
}
