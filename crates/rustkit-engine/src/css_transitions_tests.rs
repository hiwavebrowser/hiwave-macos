//! CSS transitions run (S13a). Until now the engine parsed `transition` and
//! never ticked it: a `transition: opacity 100ms` class toggle jumped to the
//! end value and no `transitionend` ever fired, so a page that waits for one
//! before its next step (a menu that removes itself on `transitionend`, a
//! hero that reveals the next block) stalled.
//!
//! The clock is the page's timer clock (`pump` moves it as it moves
//! `requestAnimationFrame`), and the oracle is Chrome 143 through
//! `tools/parity_oracle/css_transitions_log.mjs`.

use super::*;
use crate::script_net_tests::serve_routes;

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

/// The box fades in over 100 ms when `.on` lands; every transition event
/// is logged as `type:property`.
const PAGE: &str = "<!doctype html><style>body{margin:0}\
#h{width:100px;height:100px;background:rgb(0,0,255);opacity:0;transition:opacity 100ms linear}\
#h.on{opacity:1}</style><div id=h></div>\
<script>var h=document.getElementById('h');window.log=[];\
['transitionrun','transitionstart','transitionend','transitioncancel'].forEach(function(t){\
h.addEventListener(t,function(e){log.push(e.type.slice(10)+':'+e.propertyName)})});</script>";

/// A `transition: opacity 100ms` class toggle reaches 1 within 200 ms of
/// timers and fires `transitionend` exactly once (after `transitionrun`
/// and `transitionstart`, as Chrome orders them).
#[test]
fn a_class_toggle_transitions_opacity_and_fires_transitionend_once() {
    let (mut engine, view, _server) = load(PAGE);
    assert_eq!(opacity(&mut engine, view), 0.0);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 48, 16);
    let mid = opacity(&mut engine, view);
    assert!(mid > 0.05 && mid < 0.95, "midway opacity {mid}");
    pump(&mut engine, view, 160, 16);
    assert_eq!(opacity(&mut engine, view), 1.0);
    assert_eq!(
        log(&mut engine, view),
        "run:opacity,start:opacity,end:opacity"
    );
    pump(&mut engine, view, 200, 16);
    assert_eq!(
        log(&mut engine, view),
        "run:opacity,start:opacity,end:opacity",
        "end fires once"
    );
}

/// `getComputedStyle` reads the interpolated value midway: linear timing,
/// 50 ms into 100 ms is 0.5.
#[test]
fn get_computed_style_reads_the_interpolated_value_midway() {
    let (mut engine, view, _server) = load(PAGE);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 50, 10);
    let mid = opacity(&mut engine, view);
    assert!(
        (mid - 0.5).abs() <= 0.12,
        "opacity at 50 ms of a linear 100 ms fade: {mid}"
    );
}

/// Taking the transition away midway (`transition: none`) cancels it:
/// `transitioncancel`, never `transitionend`, and the value is the end
/// value at once.
#[test]
fn a_cancelled_transition_fires_transitioncancel_not_end() {
    let (mut engine, view, _server) = load(PAGE);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 48, 16);
    assert!(opacity(&mut engine, view) < 0.95);
    run(&mut engine, view, "h.style.transition='none'");
    pump(&mut engine, view, 200, 16);
    assert_eq!(
        log(&mut engine, view),
        "run:opacity,start:opacity,cancel:opacity"
    );
    assert_eq!(opacity(&mut engine, view), 1.0);
}

/// An element taken out of the document midway: its transition is
/// cancelled, and the cancel reaches the element's own listeners.
#[test]
fn removing_the_element_cancels_its_transition() {
    let (mut engine, view, _server) = load(PAGE);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 48, 16);
    run(&mut engine, view, "h.remove()");
    pump(&mut engine, view, 200, 16);
    assert_eq!(
        log(&mut engine, view),
        "run:opacity,start:opacity,cancel:opacity"
    );
}

/// Toggling back midway reverses from the value reached: the first
/// transition is cancelled, a second runs back to 0 and ends once.
#[test]
fn retargeting_midway_reverses_from_the_current_value() {
    let (mut engine, view, _server) = load(PAGE);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 48, 16);
    let reached = opacity(&mut engine, view);
    run(&mut engine, view, "h.classList.remove('on')");
    pump(&mut engine, view, 16, 16);
    let back = opacity(&mut engine, view);
    assert!(
        back <= reached && back > 0.0,
        "reversing from {reached}: {back}"
    );
    pump(&mut engine, view, 200, 16);
    assert_eq!(opacity(&mut engine, view), 0.0);
    assert_eq!(
        log(&mut engine, view),
        "run:opacity,start:opacity,cancel:opacity,run:opacity,start:opacity,end:opacity"
    );
}

/// `transition: all`: a length, a colour and a transform interpolate too
/// (Chrome: width 150px, rgb(128, 0, 128) and matrix(1, 0, 0, 1, 50, 0)
/// at 50 ms of a linear 100 ms).
#[test]
fn transform_colour_and_length_interpolate() {
    let page = "<!doctype html><style>body{margin:0}\
#h{width:100px;height:20px;background:rgb(0,0,255);transform:translateX(0px);transition:all 100ms linear}\
#h.on{width:200px;background:rgb(255,0,0);transform:translateX(100px)}</style><div id=h></div>\
<script>var h=document.getElementById('h');window.log=[];\
['transitionend'].forEach(function(t){h.addEventListener(t,function(e){log.push(e.type.slice(10)+':'+e.propertyName)})});</script>";
    let (mut engine, view, _server) = load(page);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 50, 10);
    let width: f64 = computed(&mut engine, view, "width")
        .trim_end_matches("px")
        .parse()
        .expect("width");
    assert!((width - 150.0).abs() <= 12.0, "width at 50 ms: {width}");
    let colour = computed(&mut engine, view, "background-color");
    let channels: Vec<f64> = colour
        .trim_start_matches("rgb(")
        .trim_end_matches(')')
        .split(',')
        .map(|c| {
            c.trim()
                .parse()
                .unwrap_or_else(|_| panic!("colour {colour:?}"))
        })
        .collect();
    assert!(
        (channels[0] - 128.0).abs() <= 30.0
            && channels[1] == 0.0
            && (channels[2] - 128.0).abs() <= 30.0,
        "background-color at 50 ms: {colour}"
    );
    let transform = computed(&mut engine, view, "transform");
    let tx: f64 = transform
        .trim_start_matches("matrix(")
        .trim_end_matches(')')
        .split(',')
        .nth(4)
        .unwrap_or_else(|| panic!("transform {transform:?}"))
        .trim()
        .parse()
        .unwrap();
    assert!(
        (tx - 50.0).abs() <= 12.0,
        "translateX at 50 ms: {transform}"
    );
    pump(&mut engine, view, 200, 16);
    assert_eq!(computed(&mut engine, view, "width"), "200px");
    assert_eq!(
        computed(&mut engine, view, "background-color"),
        "rgb(255, 0, 0)"
    );
    assert_eq!(
        computed(&mut engine, view, "transform"),
        "matrix(1, 0, 0, 1, 100, 0)"
    );
    let mut ended: Vec<&str> = Vec::new();
    let logged = log(&mut engine, view);
    ended.extend(logged.split(',').filter(|s| !s.is_empty()));
    ended.sort();
    assert_eq!(
        ended,
        ["end:background-color", "end:transform", "end:width"]
    );
}

/// A delay holds the start value, then the transition runs: `transitionrun`
/// at once, `transitionstart` when the delay is over.
#[test]
fn delay_holds_the_start_value_then_runs() {
    let page = PAGE.replace(
        "transition:opacity 100ms linear",
        "transition:opacity 100ms linear 100ms",
    );
    let (mut engine, view, _server) = load(&page);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 50, 10);
    assert_eq!(opacity(&mut engine, view), 0.0, "still delayed at 50 ms");
    assert_eq!(log(&mut engine, view), "run:opacity");
    pump(&mut engine, view, 100, 10);
    let mid = opacity(&mut engine, view);
    assert!((mid - 0.5).abs() <= 0.15, "opacity at 150 ms: {mid}");
    assert_eq!(log(&mut engine, view), "run:opacity,start:opacity");
    pump(&mut engine, view, 150, 10);
    assert_eq!(opacity(&mut engine, view), 1.0);
    assert_eq!(
        log(&mut engine, view),
        "run:opacity,start:opacity,end:opacity"
    );
}

/// `transition-property: width` does not run opacity (no event), and a
/// property with a zero duration changes at once.
#[test]
fn only_the_named_property_transitions() {
    let page = PAGE.replace(
        "transition:opacity 100ms linear",
        "transition:width 100ms linear",
    );
    let (mut engine, view, _server) = load(&page);
    run(&mut engine, view, "h.classList.add('on')");
    pump(&mut engine, view, 16, 16);
    assert_eq!(opacity(&mut engine, view), 1.0);
    pump(&mut engine, view, 200, 16);
    assert_eq!(log(&mut engine, view), "");
}
