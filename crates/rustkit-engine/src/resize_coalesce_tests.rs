//! A live window resize is dozens of `Resized` events a second. Until
//! 2026-10-06 the app answered each with `Engine::resize_view`: a full layout
//! and a `resize` event at `window`, before the next size was read, so the
//! content lagged the drag by as many layouts as events had queued
//! (hand-test item H9: "resize works but is slow to adapt"). Now the app
//! gives the view its bounds as they come (`set_view_bounds`) and the live
//! turn (`pump_live`) lays out once, at the last size, and the page hears
//! one `resize`, as a browser fires at most one per frame.

use super::*;
use crate::script_net_tests::serve_routes;

fn load(
    page: &str,
) -> (
    Engine,
    EngineViewId,
    crate::script_net_tests::Server,
    tokio::runtime::Runtime,
) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 200,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server, rt)
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

/// A page that counts its `resize` events and has a box half the viewport wide.
const PAGE: &str =
    "<html><head><style>body{margin:0} #half{width:50%;height:20px}</style></head><body>\
    <div id=half></div><script>\
    window.resizes = 0; window.last = '';\
    window.addEventListener('resize', function () {\
      window.resizes += 1; window.last = innerWidth + 'x' + innerHeight;\
    });\
    </script></body></html>";

/// Ten sizes between two live turns: one layout, one `resize`, at the last.
#[test]
fn many_bounds_in_one_turn_are_one_layout_and_one_resize_event() {
    let (mut engine, view, _server, rt) = load(PAGE);
    assert_eq!(read(&mut engine, view, "resizes"), "0");
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('half').offsetWidth"
        ),
        "200"
    );

    for width in (410..=500).step_by(10) {
        engine
            .set_view_bounds(
                view,
                Bounds {
                    x: 0,
                    y: 0,
                    width,
                    height: 200,
                },
            )
            .expect("set_view_bounds");
    }
    // The bounds the view reports already follow the last size (the
    // surface half of a resize is immediate); the page has not been laid
    // out or told yet.
    assert_eq!(engine.views[&view].headless_bounds.unwrap().width, 500);
    assert_eq!(
        read(&mut engine, view, "resizes"),
        "0",
        "no resize event before the live turn"
    );

    let turn = rt.block_on(engine.pump_live(view, 0));
    assert!(turn.relaid_out, "the live turn laid the page out");

    assert_eq!(
        read(&mut engine, view, "resizes"),
        "1",
        "ten sizes in one turn are one resize event"
    );
    assert_eq!(
        read(&mut engine, view, "last"),
        "500x200",
        "the event carries the last size"
    );
    assert_eq!(read(&mut engine, view, "innerWidth"), "500");
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('half').offsetWidth"
        ),
        "250",
        "the layout is at the last size"
    );

    // Nothing left to do on the next turn.
    let turn = rt.block_on(engine.pump_live(view, 0));
    assert!(
        !turn.relaid_out,
        "a turn with no new bounds does not lay out again"
    );
    assert_eq!(read(&mut engine, view, "resizes"), "1");
}

/// `flush_pending_resize` on its own: false with nothing pending, true once
/// after bounds were set, false again after that.
#[test]
fn flush_reports_whether_there_was_anything_to_do() {
    let (mut engine, view, _server, _rt) = load(PAGE);
    assert!(
        !engine.flush_pending_resize(view).unwrap(),
        "nothing pending after the load"
    );
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 300,
                height: 200,
            },
        )
        .expect("set_view_bounds");
    assert!(
        engine.flush_pending_resize(view).unwrap(),
        "the new bounds are laid out"
    );
    assert!(!engine.flush_pending_resize(view).unwrap(), "and only once");
    assert_eq!(read(&mut engine, view, "resizes"), "1");
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('half').offsetWidth"
        ),
        "150"
    );
}

/// `resize_view` keeps its meaning: everything now, and it clears what
/// `set_view_bounds` left pending (the live turn must not lay out a size the
/// view has already left).
#[test]
fn resize_view_is_immediate_and_settles_pending_bounds() {
    let (mut engine, view, _server, rt) = load(PAGE);
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 300,
                height: 200,
            },
        )
        .expect("set_view_bounds");
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 600,
                height: 200,
            },
        )
        .expect("resize_view");
    assert_eq!(
        read(&mut engine, view, "resizes"),
        "1",
        "resize_view fires now"
    );
    assert_eq!(read(&mut engine, view, "last"), "600x200");
    let turn = rt.block_on(engine.pump_live(view, 0));
    assert!(!turn.relaid_out, "nothing pending after resize_view");
    assert_eq!(read(&mut engine, view, "resizes"), "1");
}

const WIDTH_PAGE: &str = "<html><body><script>\
    window.resizes = 0; window.seen = ''; window.first = String(innerWidth);\
    window.addEventListener('resize', function () {\
      window.resizes += 1; window.seen += innerWidth + ',';\
    });\
    </script></body></html>";

fn load_sized(
    page: &str,
    width: u32,
    height: u32,
) -> (
    Engine,
    EngineViewId,
    crate::script_net_tests::Server,
    tokio::runtime::Runtime,
) {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width,
            height,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let yielded = rt
        .block_on(engine.load_url_yielding(view, url))
        .expect("load");
    assert_eq!(
        yielded,
        LoadYield::Complete,
        "a page with no inserted script does not yield"
    );
    (engine, view, server, rt)
}

/// A queued size the viewport has already left is not dispatched. The page
/// keeps the width it read, and does not hear 1280 after it is already 1060.
#[test]
fn a_stale_queued_resize_is_not_dispatched() {
    let (mut engine, view, _server, rt) = load_sized(WIDTH_PAGE, 1060, 700);
    assert_eq!(read(&mut engine, view, "window.first"), "1060");
    assert_eq!(read(&mut engine, view, "resizes"), "0");
    {
        let state = engine.views.get_mut(&view).unwrap();
        state.pending_resize = Some(Bounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        });
    }
    let turn = rt.block_on(engine.pump_live(view, 0));
    assert!(!turn.relaid_out, "the stale size was not laid out");
    assert_eq!(
        read(&mut engine, view, "resizes"),
        "0",
        "no resize at the stale width"
    );
    assert_eq!(read(&mut engine, view, "innerWidth"), "1060");
    assert_eq!(read(&mut engine, view, "window.seen"), "");
}

/// The page reads the viewport it was loaded at. A later repeat of that
/// size is not a resize. The next real size is one event, at that size.
#[test]
fn a_page_reads_its_width_before_the_sidebar_resize() {
    let (mut engine, view, _server, rt) = load_sized(WIDTH_PAGE, 1280, 800);
    assert_eq!(read(&mut engine, view, "window.first"), "1280");
    assert_eq!(read(&mut engine, view, "resizes"), "0");

    // Chrome's initial sync sends the closed width. The view is already
    // that wide, so the page does not hear a resize at 1280.
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 800,
            },
        )
        .expect("same bounds");
    let turn = rt.block_on(engine.pump_live(view, 0));
    assert!(!turn.relaid_out);
    assert_eq!(read(&mut engine, view, "resizes"), "0");

    // The open sidebar, and a stale full-window size queued behind it in
    // the same turn: one event, at the viewport the view ends on.
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 800,
            },
        )
        .expect("stale");
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 220,
                y: 0,
                width: 1060,
                height: 800,
            },
        )
        .expect("sidebar");
    let turn = rt.block_on(engine.pump_live(view, 0));
    assert!(turn.relaid_out);
    assert_eq!(read(&mut engine, view, "resizes"), "1");
    assert_eq!(read(&mut engine, view, "window.seen"), "1060,");
    assert_eq!(read(&mut engine, view, "innerWidth"), "1060");
}

/// Reddit's `m:` utilities are `@media (min-width: 960px)`. A 1280 viewport
/// and a 1060 one (1280 minus the 220px sidebar) are both at or above 960,
/// so `m:block` stays and the hamburger stays hidden. `@media (min-width:
/// 1200px)` is the query that flips. A classic scrollbar is not subtracted
/// from the layout width (a 4000px page does not become 1265).
const BREAKPOINT_PAGE: &str = r#"<html><head><style>
body{margin:0}
header{display:block;height:56px}
.hidden{display:none}
.burger{display:block;width:44px;height:40px}
#wide{width:10px;height:10px}
@media (min-width:960px){
  .m\:block{display:block}
  .m\:hidden{display:none}
}
@media (min-width:1200px){
  #wide{width:100px}
}
#desk{width:2019px;height:32px}
</style></head><body>
<header id=h></header>
<span id=desk class="hidden m:block"></span>
<span id=burger class="burger m:hidden"></span>
<div id=wide></div>
<div style="height:4000px"></div>
<script>window.resizes=0;window.addEventListener('resize',function(){window.resizes++;});</script>
</body></html>"#;

fn widths(engine: &mut Engine, view: EngineViewId) -> String {
    read(
        engine,
        view,
        "[document.getElementById('h').offsetWidth, document.getElementById('desk').offsetWidth, document.getElementById('burger').offsetWidth, document.getElementById('wide').offsetWidth, innerWidth, resizes].join('|')",
    )
}

fn resize_count(engine: &Engine, view: EngineViewId) -> u32 {
    engine
        .relayout_stats(view)
        .map(|stats| stats.by_first_cause.get("resize"))
        .unwrap_or(0)
}

/// A host frame that reads 1060 while the viewport is still 1280×696 is not
/// a resize. ResizeObserver callbacks are `observer_callback`, not `resize`.
/// `matchMedia` does not schedule layout. `devicePixelRatio` stays 1.
/// Nothing reads `visualViewport`. The layout width stays 1280, so `m:`
/// does not flip and a scrollbar gutter is not subtracted.
#[test]
fn a_stale_host_frame_does_not_resize_an_unchanged_viewport() {
    let (mut engine, view, _server, _rt) = load_sized(BREAKPOINT_PAGE, 1280, 696);
    assert_eq!(widths(&mut engine, view), "1280|2019|0|100|1280|0");
    assert_eq!(resize_count(&engine, view), 0);

    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 696,
            },
        )
        .expect("same bounds");
    // The NSView reports the sidebar width, but place_host_frame did not
    // move it after this confirmation. That read is not the viewport.
    engine.set_test_host_frame(view, false, Some((1060, 696)));
    assert!(!engine.flush_pending_resize(view).unwrap());
    assert_eq!(
        widths(&mut engine, view),
        "1280|2019|0|100|1280|0",
        "unchanged viewport kept its layout width"
    );
    assert_eq!(resize_count(&engine, view), 0, "no resize relayout");
}

/// `place_host_frame` while the engine is borrowed is a real viewport. The
/// layout width becomes 1060. `m:` (960) still matches, so the desktop span
/// keeps its box and the hamburger stays hidden. `min-width: 1200px` drops.
#[test]
fn a_host_frame_ahead_of_bounds_is_the_layout_width() {
    let (mut engine, view, _server, _rt) = load_sized(BREAKPOINT_PAGE, 1280, 696);
    assert_eq!(widths(&mut engine, view), "1280|2019|0|100|1280|0");
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 696,
            },
        )
        .expect("queue");
    engine.set_test_host_frame(view, true, Some((1060, 696)));
    assert!(engine.flush_pending_resize(view).unwrap());
    assert_eq!(
        widths(&mut engine, view),
        "1060|2019|0|10|1060|1",
        "1060 is still an m: viewport; the 1200px rule is what flips"
    );
    assert_eq!(resize_count(&engine, view), 1);
}

/// Yahoo's story survives a same-viewport flush even when a host frame
/// reads a different size. The blank-after-resize path is a real size
/// change (`the_yahoo_grid_keeps_the_story_after_a_resize`), not this one.
#[test]
fn a_stale_host_frame_does_not_blank_the_yahoo_grid() {
    let page = r#"<html><head><style>
body{margin:0;font:16px/20px sans-serif}
.page{display:grid;min-height:100vh;grid-template-columns:repeat(1,minmax(0,1fr));grid-template-rows:auto 1fr auto}
header{display:inline-flex;width:100%;flex-direction:column}
.row{display:inline-flex;width:100%}.contents{display:contents}
#logo{display:block;width:80px;height:24px}#story{height:40px}
</style></head><body>
<div class="page" id="page"><header id="uh"><div class="row">
<div class="contents"><a id="logo">Yahoo</a></div>
<div class="contents"><div id="search" style="width:200px;height:20px">find</div></div>
</div></header>
<div id="mid"><div id="story">Headlines</div></div>
<footer id="foot" style="height:20px">foot</footer></div>
</body></html>"#;
    let (mut engine, view, _server, _rt) = load_sized(page, 1280, 696);
    let before = read(
        &mut engine,
        view,
        "[document.getElementById('uh').offsetWidth, document.getElementById('story').offsetHeight, document.getElementById('logo').offsetWidth].join('|')",
    );
    assert_eq!(before, "1280|40|80");
    engine
        .set_view_bounds(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 696,
            },
        )
        .expect("same");
    engine.set_test_host_frame(view, false, Some((1060, 696)));
    assert!(!engine.flush_pending_resize(view).unwrap());
    let after = read(
        &mut engine,
        view,
        "[document.getElementById('uh').offsetWidth, document.getElementById('story').offsetHeight, document.getElementById('logo').offsetWidth].join('|')",
    );
    assert_eq!(
        after, before,
        "yahoo grid unchanged when the viewport is unchanged"
    );
    assert_eq!(resize_count(&engine, view), 0);
}

#[test]
fn resize_dispatch_follows_the_current_viewport() {
    let stored = Bounds {
        x: 10,
        y: 20,
        width: 1060,
        height: 700,
    };
    let pending = Bounds {
        x: 0,
        y: 0,
        width: 1280,
        height: 800,
    };
    let got = super::resize_dispatch_bounds(pending, stored, None);
    assert_eq!((got.width, got.height), (1060, 700));
    let got = super::resize_dispatch_bounds(pending, pending, Some((1060, 700)));
    assert_eq!((got.x, got.y, got.width, got.height), (0, 0, 1060, 700));
}
