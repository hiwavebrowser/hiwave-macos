//! Plan row C2 (a): an `overflow: auto|scroll|hidden` box scrolls. The offsets
//! are stored and clamped, but nothing downstream reads them: a child's
//! geometry and the hit test stay at offset zero, and so does the paint.
//! Expected strings are Chromium 143.0.7499.4 on the same page
//! (`trench/z/tools/c2/c2_probe.mjs` on the hub).

use super::*;

const PAGE: &str = concat!(
    "<!doctype html><style>body{margin:0}",
    "#s{overflow:auto;width:200px;height:100px;position:absolute;left:10px;top:20px}",
    "#w{width:500px;height:400px}",
    "#a{height:50px;background:red}#b{height:50px;background:lime}",
    "#c{height:300px;background:blue}",
    "#h{overflow:hidden;width:100px;height:50px;position:absolute;left:300px;top:20px}",
    "#hc{height:200px;background:orange}",
    "#v{overflow:visible;width:100px;height:50px;position:absolute;left:450px;top:20px}",
    "#vc{height:200px;background:teal}",
    "</style><body><div id=s><div id=w><div id=a>a</div><div id=b>b</div><div id=c>c</div>",
    "</div></div><div id=h><div id=hc>h</div></div><div id=v><div id=vc>v</div></div></body>",
);

fn loaded() -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, PAGE).expect("load");
    (engine, view)
}

const AT: &str = "var $ = function (i) { return document.getElementById(i); }; \
     var at = function (i) { var r = $(i).getBoundingClientRect(); return r.x + ',' + r.y; }; ";

#[track_caller]
fn check(script: &str, want: &str) {
    let (mut engine, view) = loaded();
    let got = engine.execute_script(view, &format!("{AT}{script}")).expect("script");
    assert_eq!(got, format!("String(\"{want}\")"));
}

#[test]
fn the_extents_before_any_scroll() {
    check(
        "var s = $('s'); [s.scrollTop, s.scrollLeft, s.scrollHeight, s.scrollWidth, \
         s.clientHeight, s.clientWidth, at('b')].join(' ')",
        "0 0 400 500 100 200 10,70",
    );
}

#[test]
fn scroll_top_moves_the_children_and_not_the_box() {
    check(
        "var s = $('s'); s.scrollTop = 30; [s.scrollTop, at('s'), at('a'), at('b')].join(' ')",
        "30 10,20 10,-10 10,40",
    );
}

#[test]
fn scroll_left_moves_the_children() {
    check(
        "var s = $('s'); s.scrollTop = 30; s.scrollLeft = 40; [s.scrollLeft, at('b')].join(' ')",
        "40 -30,40",
    );
}

#[test]
fn the_offsets_clamp_at_both_ends() {
    check(
        "var s = $('s'); s.scrollTop = 9999; s.scrollLeft = 9999; \
         var far = [s.scrollTop, s.scrollLeft, at('c')].join(' '); \
         s.scrollTop = -5; s.scrollLeft = -5; far + ' ' + s.scrollTop + ' ' + s.scrollLeft",
        "300 300 -290,-180 0 0",
    );
}

#[test]
fn scroll_to_and_scroll_by_on_an_element() {
    check(
        "var s = $('s'); s.scrollTo(0, 60); var one = s.scrollTop + ' ' + at('b'); \
         s.scrollBy(0, 10); one + ' ' + s.scrollTop + ' ' + at('b')",
        "60 10,10 70 10,0",
    );
}

#[test]
fn the_hit_test_follows_the_scroll() {
    check(
        "var s = $('s'); s.scrollTop = 70; document.elementFromPoint(20, 30).id",
        "b",
    );
}

/// `overflow: hidden` scrolls from script too; `visible` does not scroll at all.
#[test]
fn hidden_scrolls_from_script_and_visible_does_not() {
    check(
        "var h = $('h'), v = $('v'); h.scrollTop = 25; v.scrollTop = 25; \
         [h.scrollTop, at('hc'), v.scrollTop, at('vc')].join(' ')",
        "25 300,-5 0 450,20",
    );
}
