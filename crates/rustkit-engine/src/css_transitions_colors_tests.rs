//! Colour transitions beyond `color`, `background-color` and the border
//! colours (owner's hand test 2026-11-17, group A): `outline-color` and
//! `text-decoration-color` jumped to their end value. `currentcolor` is a
//! value of its own: it interpolates as the element's colour, and a
//! `currentcolor` that stays `currentcolor` while `color` jumps starts no
//! transition.
//!
//! Oracle: Chrome for Testing 148.0.7778.216 through
//! `tools/parity_oracle/css_transitions_colors_log.mjs`, read at
//! currentTime 50 of a 100 ms linear transition:
//!
//! ```text
//! outline-color rgb: rgb(128, 0, 128) | anims [outline-color]
//! outline-color currentcolor->red: rgb(128, 0, 128) | anims [outline-color]
//! outline-color red->currentcolor: rgb(128, 0, 128) | anims [outline-color]
//! outline-color currentcolor, color jumps: rgb(255, 0, 0) | anims []
//! outline-color transparent->red: rgba(255, 0, 0, 0.5) | anims [outline-color]
//! outline shorthand named: rgb(128, 0, 128) | anims [outline-color]
//! text-decoration-color rgb: rgb(128, 0, 128) | anims [text-decoration-color]
//! text-decoration-color transparent->red: rgba(255, 0, 0, 0.5) | anims [text-decoration-color]
//! text-decoration shorthand named: rgb(128, 0, 128) | anims [text-decoration-color]
//! ```

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

/// Live turns of `step` ms until `total` ms have passed.
fn pump(engine: &mut Engine, view: EngineViewId, total: u64, step: u64) {
    let rt = rt();
    let mut t = 0;
    while t < total {
        rt.block_on(engine.pump_live(view, step));
        t += step;
    }
}

/// `#h` styled by `css`, with `transition: <transition>`; `.on` lands,
/// and the page is read 50 ms into the 100 ms linear transition:
/// `prop`'s computed value and the transitions running on `#h`.
fn midway(css: &str, transition: &str, prop: &str) -> (String, String, Engine, EngineViewId) {
    let page = format!(
        "<!doctype html><style>body{{margin:0}}#h{{font:16px sans-serif;transition:{transition}}}{css}</style>\
<div id=h>text <span id=c>child</span></div>\
<script>var h=document.getElementById('h');</script>"
    );
    let (mut engine, view, server) = load(&page);
    engine
        .execute_script(view, "h.classList.add('on')")
        .unwrap();
    pump(&mut engine, view, 50, 10);
    let value = read(
        &mut engine,
        view,
        &format!("getComputedStyle(h).getPropertyValue('{prop}')"),
    );
    let names = read(
        &mut engine,
        view,
        "h.getAnimations().map(function(a){return a.transitionProperty}).join()",
    );
    drop(server);
    (value, names, engine, view)
}

/// The channels of `rgb(...)`/`rgba(...)` (alpha 1 when absent).
fn channels(colour: &str) -> [f64; 4] {
    let inner = colour
        .trim_start_matches("rgba(")
        .trim_start_matches("rgb(")
        .trim_end_matches(')');
    let parts: Vec<f64> = inner
        .split(',')
        .map(|c| {
            c.trim()
                .parse()
                .unwrap_or_else(|_| panic!("colour {colour:?}"))
        })
        .collect();
    [
        parts[0],
        parts[1],
        parts[2],
        parts.get(3).copied().unwrap_or(1.0),
    ]
}

/// Midway between blue and red is rgb(128, 0, 128) in Chrome; the pump's
/// turns land within a frame of 50 ms.
fn assert_purple(colour: &str, what: &str) {
    let [r, g, b, a] = channels(colour);
    assert!(
        (r - 128.0).abs() <= 30.0 && g == 0.0 && (b - 128.0).abs() <= 30.0 && a == 1.0,
        "{what} at 50 ms (Chrome: rgb(128, 0, 128)): {colour}"
    );
}

const LINEAR: &str = "all 100ms linear";

#[test]
fn outline_color_interpolates() {
    let (value, names, ..) = midway(
        "#h{outline:2px solid rgb(0,0,255)}#h.on{outline-color:rgb(255,0,0)}",
        LINEAR,
        "outline-color",
    );
    assert_purple(&value, "outline-color");
    assert_eq!(names, "outline-color");
}

#[test]
fn outline_color_from_currentcolor_interpolates_from_the_colour() {
    let (value, _, ..) = midway(
        "#h{color:rgb(0,0,255);outline:2px solid}#h.on{outline-color:rgb(255,0,0)}",
        LINEAR,
        "outline-color",
    );
    assert_purple(&value, "outline-color from currentcolor");
}

#[test]
fn outline_color_to_currentcolor_interpolates_to_the_colour() {
    let (value, _, ..) = midway(
        "#h{color:rgb(0,0,255);outline:2px solid rgb(255,0,0)}#h.on{outline-color:currentcolor}",
        LINEAR,
        "outline-color",
    );
    assert_purple(&value, "outline-color to currentcolor");
}

/// `currentcolor` before and after is no change, whatever `color` did:
/// the outline follows the colour at once and no transition runs.
#[test]
fn a_currentcolor_outline_starts_no_transition_when_the_colour_jumps() {
    let (value, names, ..) = midway(
        "#h{color:rgb(0,0,255);outline:2px solid}#h.on{color:rgb(255,0,0)}",
        "outline-color 100ms linear",
        "outline-color",
    );
    assert_eq!(value, "rgb(255, 0, 0)");
    assert_eq!(names, "");
}

#[test]
fn outline_color_from_transparent_interpolates_premultiplied() {
    let (value, _, ..) = midway(
        "#h{outline:2px solid transparent}#h.on{outline-color:rgb(255,0,0)}",
        LINEAR,
        "outline-color",
    );
    let [r, g, b, a] = channels(&value);
    assert!(
        r == 255.0 && g == 0.0 && b == 0.0 && (a - 0.5).abs() <= 0.15,
        "outline-color from transparent at 50 ms (Chrome: rgba(255, 0, 0, 0.5)): {value}"
    );
}

/// `transition: outline` names the shorthand, which covers its colour.
#[test]
fn the_outline_shorthand_covers_outline_color() {
    let (value, names, ..) = midway(
        "#h{outline:2px solid rgb(0,0,255)}#h.on{outline-color:rgb(255,0,0)}",
        "outline 100ms linear",
        "outline-color",
    );
    assert_purple(&value, "outline-color under `transition: outline`");
    assert_eq!(names, "outline-color");
}

#[test]
fn text_decoration_color_interpolates() {
    let (value, names, mut engine, view) = midway(
        "#h{text-decoration:underline rgb(0,0,255)}#h.on{text-decoration-color:rgb(255,0,0)}",
        LINEAR,
        "text-decoration-color",
    );
    assert_purple(&value, "text-decoration-color");
    assert_eq!(names, "text-decoration-color");
    pump(&mut engine, view, 100, 10);
    assert_eq!(
        read(
            &mut engine,
            view,
            "getComputedStyle(h).getPropertyValue('text-decoration-color')"
        ),
        "rgb(255, 0, 0)",
        "the end value"
    );
}

#[test]
fn text_decoration_color_from_transparent_interpolates_premultiplied() {
    let (value, _, ..) = midway(
        "#h{text-decoration:underline transparent}#h.on{text-decoration-color:rgb(255,0,0)}",
        LINEAR,
        "text-decoration-color",
    );
    let [r, g, b, a] = channels(&value);
    assert!(
        r == 255.0 && g == 0.0 && b == 0.0 && (a - 0.5).abs() <= 0.15,
        "text-decoration-color from transparent at 50 ms (Chrome: rgba(255, 0, 0, 0.5)): {value}"
    );
}

#[test]
fn the_text_decoration_shorthand_covers_its_colour() {
    let (value, names, ..) = midway(
        "#h{text-decoration:underline rgb(0,0,255)}#h.on{text-decoration-color:rgb(255,0,0)}",
        "text-decoration 100ms linear",
        "text-decoration-color",
    );
    assert_purple(&value, "text-decoration-color under `transition: text-decoration`");
    assert_eq!(names, "text-decoration-color");
}
