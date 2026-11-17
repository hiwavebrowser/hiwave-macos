//! TREE-REUSE (a): a view keeps each element's style from one build to the next across script
//! writes, and a build replays the style of every element the writes did not reach
//! (`docs/design/INCREMENTAL_RESTYLE.md` §3.3 item 1, PR 2).
//!
//! Step 0 (`hidden_tiles_build_tests`): on MotionMark Multiply's shape a full build costs ~11 us
//! per tile, hidden or shown, because every tile's own cascade runs (its inline style is unique,
//! so no two tiles share a style). The stage shows and hides tiles by writing `display`, which is
//! not a paint-only write, so each such frame builds the whole stage.
//!
//! A replayed style must be the style a full cascade gives: every test here runs the same writes
//! with retained styles on and off and compares the display lists and the computed styles of every
//! element, hidden ones included, after every frame.

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

fn frame(engine: &mut Engine, view: EngineViewId) {
    rt().block_on(engine.pump_live(view, 17));
}

/// `body` with an animation-frame loop that runs `write` (with `n` the frame number) each frame.
fn page(style: &str, body: &str, write: &str) -> String {
    format!(
        "<!doctype html><style>body{{margin:0;font:14px sans-serif}}{style}</style>{body}\
         <script>var n = 0; requestAnimationFrame(function f() {{ n++; {write} requestAnimationFrame(f); }});</script>"
    )
}

/// Multiply's shape: `tiles` tiles, each with its own inline position and colour, the first
/// `shown` visible and the rest `display: none`.
fn tiles(tiles: usize, shown: usize, write: &str) -> String {
    let mut body = String::from("<div id=stage>");
    for i in 0..tiles {
        body.push_str(&format!(
            "<div class=tile id=t{i} style=\"left:{}px;top:{}px;background-color:hsl({},80%,50%){}\"></div>",
            (i % 40) * 20,
            (i / 40 % 30) * 20,
            i % 360,
            if i < shown { "" } else { ";display:none" }
        ));
    }
    body.push_str("</div>");
    page(
        "#stage{position:relative;width:800px;height:600px;overflow:hidden}\
         .tile{position:absolute;width:18px;height:18px;border-radius:4px}",
        &body,
        write,
    )
}

/// The stage's tune: each frame shows one more tile and hides the oldest shown one, and moves a
/// shown tile (a geometry write, so no frame takes the paint-only path).
const TUNE: &str = "var a = document.getElementById('t' + (10 + n)), b = document.getElementById('t' + n), \
                    c = document.getElementById('t' + (n + 5)); \
                    a.style.display = ''; b.style.display = 'none'; c.style.left = (n * 3 % 700) + 'px';";

/// Every element's computed style (a few properties of each kind), as script reads it.
const DUMP: &str = "(function () { var out = []; var all = document.querySelectorAll('*'); \
    for (var i = 0; i < all.length; i++) { var s = getComputedStyle(all[i]); \
    out.push([all[i].tagName, all[i].id, s.display, s.color, s.fontSize, s.left, s.marginTop, \
    s.backgroundColor, s.width].join('|')); } return out.join('\\n'); })()";

/// Display lists and computed styles after each of `frames` frames, with retained styles `mode`.
fn run(page: &str, frames: usize, mode: RetainedStyles) -> (Vec<String>, Vec<String>) {
    set_retained_styles_for_test(Some(mode));
    let (mut engine, view, _server) = load(page);
    let (mut lists, mut styles) = (Vec::new(), Vec::new());
    for _ in 0..frames {
        frame(&mut engine, view);
        lists.push(format!(
            "{:?}",
            engine.views[&view]
                .display_list
                .as_ref()
                .expect("display list")
                .commands
        ));
        styles.push(format!(
            "{:?}",
            engine.execute_script(view, DUMP).expect("dump")
        ));
    }
    set_retained_styles_for_test(None);
    (lists, styles)
}

fn assert_same_as_a_full_cascade(name: &str, page: &str) {
    let (on_lists, on_styles) = run(page, 6, RetainedStyles::On);
    let (off_lists, off_styles) = run(page, 6, RetainedStyles::Off);
    for i in 0..on_lists.len() {
        assert!(
            on_styles[i] == off_styles[i],
            "{name}, frame {i}: computed styles differ from a full cascade's\n  retained: {}\n  full:     {}",
            on_styles[i],
            off_styles[i]
        );
        assert!(
            on_lists[i] == off_lists[i],
            "{name}, frame {i}: the display list differs from a full cascade's"
        );
    }
}

/// The red test: on Multiply's shape (1,000 tiles, 10 shown), a frame that shows a tile, hides
/// one and moves one cascades those tiles, not the stage (it cascaded all 1,000).
#[test]
fn a_frame_that_shows_and_hides_tiles_cascades_those_tiles_alone() {
    set_retained_styles_for_test(Some(RetainedStyles::On));
    let (mut engine, view, _server) = load(&tiles(1_000, 10, TUNE));
    for _ in 0..3 {
        frame(&mut engine, view);
    }
    let mut per_frame = Vec::new();
    for _ in 0..5 {
        let before = cascades_for_test();
        frame(&mut engine, view);
        per_frame.push(cascades_for_test() - before);
    }
    set_retained_styles_for_test(None);
    assert!(
        per_frame.iter().all(|&c| c > 0 && c <= 20),
        "element cascades per frame (3 tiles written of 1,000): {per_frame:?}"
    );
}

#[test]
fn tiles_shown_hidden_and_moved_keep_a_full_cascades_styles() {
    assert_same_as_a_full_cascade("tiles", &tiles(200, 10, TUNE));
}

#[test]
fn an_inherited_change_reaches_the_descendants() {
    assert_same_as_a_full_cascade(
        "inherited",
        &page(
            ".a{color:rgb(200,0,0);font-size:12px} .b{color:rgb(0,0,200);font-size:20px} em{margin-top:1em;display:block}",
            "<div id=p class=a><p id=q>x<span id=s>y<em id=e>z</em></span></p></div><p id=r>w</p>",
            "document.getElementById('p').className = n % 2 ? 'b' : 'a';",
        ),
    );
}

#[test]
fn a_sibling_selector_follows_its_sibling() {
    assert_same_as_a_full_cascade(
        "sibling",
        &page(
            ".on + p{color:rgb(0,128,0);margin-top:9px} .on ~ div{width:40px}",
            "<p id=a>a</p><p id=b>b</p><div id=c>c</div><div id=d>d</div>",
            "document.getElementById('a').className = n % 2 ? 'on' : '';",
        ),
    );
}

#[test]
fn a_structural_pseudo_class_follows_an_insert_and_a_removal() {
    assert_same_as_a_full_cascade(
        "nth-child",
        &page(
            "li:nth-child(2n){color:rgb(255,0,0)} li:first-child{margin-top:7px} li:last-child{width:50px;display:block}",
            "<ul id=u><li id=l1>1</li><li id=l2>2</li><li id=l3>3</li><li id=l4>4</li></ul>",
            "var u = document.getElementById('u'); if (n % 2) { var li = document.createElement('li'); \
             li.textContent = 'new'; u.insertBefore(li, u.firstChild); } else { u.removeChild(u.firstChild); }",
        ),
    );
}

#[test]
fn an_attribute_on_an_ancestor_restyles_its_descendants() {
    assert_same_as_a_full_cascade(
        "attribute",
        &page(
            "[data-k='1'] span{margin-top:5px;display:inline-block;color:rgb(9,9,9)}",
            "<div id=p><p><span id=s>x</span></p></div><span id=t>y</span>",
            "document.getElementById('p').setAttribute('data-k', String(n % 2));",
        ),
    );
}

#[test]
fn a_has_rule_follows_its_descendant() {
    assert_same_as_a_full_cascade(
        "has",
        &page(
            ".box{width:100px} .box:has(.on){background-color:rgb(255,0,0);width:60px}",
            "<div class=box id=x><p><i id=i>i</i></p></div><div class=box id=y></div>",
            "document.getElementById('i').className = n % 2 ? 'on' : '';",
        ),
    );
}

#[test]
fn a_custom_property_write_reaches_its_readers() {
    assert_same_as_a_full_cascade(
        "custom property",
        &page(
            "#r{--c:rgb(1,2,3);--w:10px} p{color:var(--c);width:var(--w)}",
            "<div id=r><p id=a>a</p><div><p id=b>b</p></div></div>",
            "var r = document.getElementById('r'); r.style.setProperty('--c', n % 2 ? 'rgb(9,8,7)' : 'rgb(1,2,3)'); \
             r.style.setProperty('--w', (10 + n) + 'px');",
        ),
    );
}

#[test]
fn a_hidden_elements_style_answers_after_its_write() {
    assert_same_as_a_full_cascade(
        "hidden",
        &page(
            ".h{display:none;color:rgb(0,0,0)} .h span{font-size:30px}",
            "<div class=h id=h><span id=s>x</span></div><div id=v>v</div>",
            "var h = document.getElementById('h'); h.style.color = 'rgb(' + n + ',0,0)'; h.style.left = n + 'px'; \
             if (n == 4) h.style.display = 'block';",
        ),
    );
}
