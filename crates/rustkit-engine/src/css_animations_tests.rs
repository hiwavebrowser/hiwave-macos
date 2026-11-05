//! `@keyframes` animations run (S13b). Until now the engine parsed
//! `animation` and painted the one frame it settles on; nothing moved and
//! no `animationstart`/`animationend` fired. The fade-in idiom
//! (`opacity: 0; animation: fade 200ms forwards`) must paint once the
//! timers pass, a spinner must keep turning, and `animation-play-state:
//! paused` must hold.
//!
//! The clock is the page's timer clock (`pump`), and the oracle is Chrome
//! 143 through `tools/parity_oracle/css_animations_log.mjs`.

use super::*;
use crate::script_net_tests::serve_routes;
use rustkit_layout::DisplayCommand;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn load(page: &str) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine
        .execute_script(view, &format!("String({expr})"))
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

fn run(engine: &mut Engine, view: EngineViewId, script: &str) {
    engine.execute_script(view, script).unwrap();
}

/// Live turns of `step` ms until `total` ms have passed (the window loop).
fn pump(engine: &mut Engine, view: EngineViewId, total: u64, step: u64) {
    let rt = rt();
    let mut t = 0;
    while t < total {
        rt.block_on(engine.pump_live(view, step));
        t += step;
    }
}

fn computed(engine: &mut Engine, view: EngineViewId, prop: &str) -> String {
    read(
        engine,
        view,
        &format!("getComputedStyle(document.getElementById('h')).getPropertyValue('{prop}')"),
    )
}

fn opacity(engine: &mut Engine, view: EngineViewId) -> f64 {
    let s = computed(engine, view, "opacity");
    s.parse()
        .unwrap_or_else(|_| panic!("computed opacity {s:?}"))
}

fn log(engine: &mut Engine, view: EngineViewId) -> String {
    read(engine, view, "log.join()")
}

/// Whether the hero's text is in the display list outside any opacity
/// group below 1 (the page has no other translucent box).
fn hero_painted(engine: &Engine, view: EngineViewId) -> bool {
    let list = engine.views[&view]
        .display_list
        .as_ref()
        .expect("a display list");
    let faded = list
        .commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::PushOpacity(o) if *o < 1.0));
    let text = list
        .commands
        .iter()
        .any(|c| matches!(c, DisplayCommand::Text { text, .. } if text.contains("Placeholder")));
    text && !faded
}

fn page(css: &str) -> String {
    format!(
        "<!doctype html><style>body{{margin:0;background:#fff}}\
         #h{{width:200px;height:100px;background:#123;color:#fff;font:40px sans-serif}}{css}</style>\
         <div id=h>Placeholder</div>\
         <script>var h=document.getElementById('h');window.log=[];\
         ['animationstart','animationiteration','animationend','animationcancel'].forEach(function(t){{\
         h.addEventListener(t,function(e){{log.push(e.type.slice(9)+':'+e.animationName+'@'+e.elapsedTime)}})}});</script>"
    )
}

const FADE: &str = "@keyframes fade{from{opacity:0}to{opacity:1}}";

/// `opacity: 0` + `animation: fade 200ms forwards` paints once the timers
/// pass, holding the end state; `animationstart` and `animationend` fire
/// once each.
#[test]
fn a_forwards_fade_in_paints_after_the_timers_pass() {
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{opacity:0;animation:fade 200ms linear forwards}}{FADE}"
    )));
    pump(&mut engine, view, 100, 10);
    let mid = opacity(&mut engine, view);
    assert!(mid > 0.2 && mid < 0.8, "midway opacity {mid}");
    pump(&mut engine, view, 200, 16);
    assert_eq!(opacity(&mut engine, view), 1.0);
    assert!(hero_painted(&engine, view), "painted at opacity 1");
    assert_eq!(log(&mut engine, view), "start:fade@0,end:fade@0.2");
    pump(&mut engine, view, 500, 50);
    assert_eq!(opacity(&mut engine, view), 1.0, "forwards holds");
    assert_eq!(
        log(&mut engine, view),
        "start:fade@0,end:fade@0.2",
        "end fires once"
    );
}

/// Without a forward fill the box returns to its own value at the end.
#[test]
fn without_a_fill_the_box_returns_to_its_own_value() {
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{opacity:.5;animation:fade 100ms linear}}{FADE}"
    )));
    pump(&mut engine, view, 50, 10);
    let mid = opacity(&mut engine, view);
    assert!(mid > 0.3 && mid < 0.7, "midway opacity {mid}");
    pump(&mut engine, view, 200, 16);
    assert_eq!(opacity(&mut engine, view), 0.5);
}

/// An infinite spinner keeps changing its transform, and
/// `animationiteration` fires at each turn.
#[test]
fn an_infinite_spinner_keeps_turning() {
    let (mut engine, view, _server) = load(&page(
        "#h{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}",
    ));
    pump(&mut engine, view, 250, 50);
    let quarter = computed(&mut engine, view, "transform");
    pump(&mut engine, view, 250, 50);
    let half = computed(&mut engine, view, "transform");
    assert_ne!(quarter, half, "the transform moves");
    assert!(quarter.starts_with("matrix("), "a matrix: {quarter}");
    // A quarter turn: matrix(0, 1, -1, 0, 0, 0) within rounding.
    let q: Vec<f64> = quarter
        .trim_start_matches("matrix(")
        .trim_end_matches(')')
        .split(',')
        .map(|c| c.trim().parse().unwrap())
        .collect();
    assert!(
        q[0].abs() < 0.05 && (q[1] - 1.0).abs() < 0.05,
        "quarter turn: {quarter}"
    );
    pump(&mut engine, view, 2000, 100);
    assert_eq!(
        log(&mut engine, view),
        "start:spin@0,iteration:spin@1,iteration:spin@2"
    );
    let later = computed(&mut engine, view, "transform");
    pump(&mut engine, view, 300, 100);
    assert_ne!(
        later,
        computed(&mut engine, view, "transform"),
        "still turning"
    );
}

/// `animation-play-state: paused` holds the current value; `running`
/// resumes from it.
#[test]
fn play_state_paused_holds_and_running_resumes() {
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{opacity:0;animation:fade 200ms linear forwards}}{FADE}"
    )));
    pump(&mut engine, view, 100, 10);
    run(&mut engine, view, "h.style.animationPlayState='paused'");
    pump(&mut engine, view, 16, 16);
    let held = opacity(&mut engine, view);
    assert!(held > 0.3 && held < 0.7, "held midway: {held}");
    pump(&mut engine, view, 300, 50);
    assert_eq!(opacity(&mut engine, view), held, "paused holds");
    assert_eq!(log(&mut engine, view), "start:fade@0");
    run(&mut engine, view, "h.style.animationPlayState='running'");
    pump(&mut engine, view, 300, 16);
    assert_eq!(opacity(&mut engine, view), 1.0);
    assert_eq!(log(&mut engine, view), "start:fade@0,end:fade@0.2");
}

/// Two alternating iterations: up, then back down, ending on the box's
/// own value; `animationiteration` once.
#[test]
fn iteration_count_and_alternate_direction() {
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{opacity:0;animation:fade 100ms linear 2 alternate}}{FADE}"
    )));
    pump(&mut engine, view, 50, 10);
    let up = opacity(&mut engine, view);
    assert!((up - 0.5).abs() <= 0.15, "going up at 50 ms: {up}");
    pump(&mut engine, view, 100, 10);
    let down = opacity(&mut engine, view);
    assert!((down - 0.5).abs() <= 0.15, "coming down at 150 ms: {down}");
    pump(&mut engine, view, 20, 10);
    assert!(opacity(&mut engine, view) < down, "still coming down");
    pump(&mut engine, view, 200, 16);
    assert_eq!(opacity(&mut engine, view), 0.0);
    assert_eq!(
        log(&mut engine, view),
        "start:fade@0,iteration:fade@0.1,end:fade@0.2"
    );
}

/// A delay with `both`: the first keyframe shows during the delay
/// (backwards fill), `animationstart` fires when the delay is over, and
/// the end state holds.
#[test]
fn delay_with_both_fills_backwards_then_holds() {
    let (mut engine, view, _server) = load(&page(
        "#h{opacity:0;animation:up 100ms linear 100ms both}@keyframes up{from{opacity:.5}to{opacity:1}}",
    ));
    pump(&mut engine, view, 50, 10);
    assert_eq!(
        opacity(&mut engine, view),
        0.5,
        "the first keyframe during the delay"
    );
    assert_eq!(log(&mut engine, view), "");
    pump(&mut engine, view, 100, 10);
    let mid = opacity(&mut engine, view);
    assert!((mid - 0.75).abs() <= 0.1, "midway at 150 ms: {mid}");
    assert_eq!(log(&mut engine, view), "start:up@0");
    pump(&mut engine, view, 200, 16);
    assert_eq!(opacity(&mut engine, view), 1.0);
    assert_eq!(log(&mut engine, view), "start:up@0,end:up@0.1");
}

/// Taking the animation away midway (`animation: none`) cancels it: the
/// value is the box's own again and `animationcancel` fires.
#[test]
fn removing_the_animation_cancels_it() {
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{opacity:0;animation:fade 200ms linear forwards}}{FADE}"
    )));
    pump(&mut engine, view, 100, 10);
    assert!(opacity(&mut engine, view) > 0.2);
    run(&mut engine, view, "h.style.animation='none'");
    pump(&mut engine, view, 32, 16);
    assert_eq!(opacity(&mut engine, view), 0.0);
    let logged = log(&mut engine, view);
    assert!(
        logged.starts_with("start:fade@0,cancel:fade@"),
        "cancel, never end: {logged}"
    );
    pump(&mut engine, view, 300, 50);
    assert_eq!(log(&mut engine, view), logged);
}

/// Two animations on one element run side by side; the later one wins a
/// property both set.
#[test]
fn several_animations_run_together() {
    let (mut engine, view, _server) = load(&page(
        "#h{opacity:0;animation:fade 100ms linear forwards, slide 100ms linear forwards}\
         @keyframes fade{to{opacity:1}}@keyframes slide{to{transform:translateX(100px)}}",
    ));
    pump(&mut engine, view, 200, 16);
    assert_eq!(opacity(&mut engine, view), 1.0);
    assert_eq!(
        computed(&mut engine, view, "transform"),
        "matrix(1, 0, 0, 1, 100, 0)"
    );
    let mut ended: Vec<String> = log(&mut engine, view)
        .split(',')
        .filter(|e| e.starts_with("end:"))
        .map(String::from)
        .collect();
    ended.sort();
    assert_eq!(ended, ["end:fade@0.1", "end:slide@0.1"]);
}

/// Parity captures run no animations: the settled frame paints at once.
#[test]
fn with_animations_disabled_the_settled_frame_paints() {
    let html = format!(
        "<!doctype html>{}",
        page(&format!("#h{{opacity:0;animation:fade 1s forwards}}{FADE}"))
    );
    let config = EngineConfig {
        disable_animations: true,
        ..EngineConfig::default()
    };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, &html).expect("load");
    assert!(
        hero_painted(&engine, view),
        "settled on the end frame at load"
    );
}

/// A zero-duration animation: `animationend` reports an `elapsedTime` of
/// 0 (Chrome), and one with `infinite` iterations removed during its
/// delay cancels without a NaN anywhere.
#[test]
fn a_zero_duration_animation_ends_at_zero_and_cancels_cleanly() {
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{opacity:0;animation:fade 0s linear forwards}}{FADE}"
    )));
    pump(&mut engine, view, 32, 16);
    assert_eq!(log(&mut engine, view), "start:fade@0,end:fade@0");
    assert_eq!(opacity(&mut engine, view), 1.0);
    let (mut engine, view, _server) = load(&page(&format!(
        "#h{{animation:fade 0s linear 1s infinite}}{FADE}"
    )));
    pump(&mut engine, view, 32, 16);
    run(&mut engine, view, "h.remove()");
    pump(&mut engine, view, 32, 16);
    // Still in its delay: Chrome fires no animationcancel for it.
    assert_eq!(log(&mut engine, view), "");
    assert!(!engine.views[&view].lay.animations.borrow().active());
}

/// An ancestor going `display: none` cancels the animations under it;
/// shown again, the animation starts over from its first frame.
#[test]
fn a_hidden_ancestor_cancels_and_showing_restarts() {
    let page = "<!doctype html><style>body{margin:0;background:#fff}#p.hide{display:none}\
#h{width:200px;height:100px;background:#123;opacity:0;animation:fade 1s linear forwards}\
@keyframes fade{from{opacity:0}to{opacity:1}}</style>\
<div id=p><div id=h>Placeholder</div></div>\
<script>var p=document.getElementById('p'),h=document.getElementById('h');window.log=[];\
['animationstart','animationend','animationcancel'].forEach(function(t){\
h.addEventListener(t,function(e){log.push(e.type.slice(9)+':'+e.animationName+'@'+e.elapsedTime)})});</script>";
    let (mut engine, view, _server) = load(page);
    pump(&mut engine, view, 300, 50);
    assert!(opacity(&mut engine, view) > 0.2);
    run(&mut engine, view, "p.classList.add('hide')");
    pump(&mut engine, view, 32, 16);
    let logged = log(&mut engine, view);
    assert!(
        logged.starts_with("start:fade@0,cancel:fade@"),
        "cancelled: {logged}"
    );
    assert!(
        !engine.views[&view].lay.animations.borrow().active(),
        "nothing runs under a hidden ancestor"
    );
    run(&mut engine, view, "p.classList.remove('hide')");
    pump(&mut engine, view, 48, 16);
    let restarted = opacity(&mut engine, view);
    assert!(
        restarted < 0.15,
        "starts over from the first frame: {restarted}"
    );
    assert_eq!(
        read(&mut engine, view, "log.length+':'+log[2]"),
        "3:start:fade@0"
    );
}

/// An element that is `display: none` itself is cancelled, and starts
/// over when shown.
#[test]
fn an_element_hidden_itself_cancels_and_restarts() {
    let page = "<!doctype html><style>body{margin:0;background:#fff}#h.hide{display:none}\
#h{width:200px;height:100px;background:#123;opacity:0;animation:fade 1s linear forwards}\
@keyframes fade{from{opacity:0}to{opacity:1}}</style>\
<div id=h>Placeholder</div>\
<script>var h=document.getElementById('h');window.log=[];\
['animationstart','animationend','animationcancel'].forEach(function(t){\
h.addEventListener(t,function(e){log.push(e.type.slice(9)+':'+e.animationName)})});</script>";
    let (mut engine, view, _server) = load(page);
    pump(&mut engine, view, 300, 50);
    run(&mut engine, view, "h.classList.add('hide')");
    pump(&mut engine, view, 32, 16);
    assert_eq!(log(&mut engine, view), "start:fade,cancel:fade");
    assert!(!engine.views[&view].lay.animations.borrow().active());
    run(&mut engine, view, "h.classList.remove('hide')");
    pump(&mut engine, view, 48, 16);
    assert!(opacity(&mut engine, view) < 0.15, "starts over");
    assert_eq!(log(&mut engine, view), "start:fade,cancel:fade,start:fade");
}

/// An `animation-name` with no `@keyframes` of that name runs nothing: no
/// events, nothing kept moving.
#[test]
fn an_unknown_keyframes_name_runs_nothing() {
    let (mut engine, view, _server) =
        load(&page("#h{opacity:.5;animation:nosuch 1s linear infinite}"));
    pump(&mut engine, view, 100, 20);
    assert_eq!(log(&mut engine, view), "");
    assert!(!engine.views[&view].lay.animations.borrow().active());
    assert_eq!(opacity(&mut engine, view), 0.5);
}
