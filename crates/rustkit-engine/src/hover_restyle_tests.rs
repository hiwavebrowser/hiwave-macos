//! A pointer restyle (`:hover`, `:active`) that changes only what is
//! painted keeps the laid-out tree: the styles are built again, the paint
//! properties that changed are copied onto the boxes that are already laid
//! out, and the page is painted from them. A restyle that changes anything
//! layout reads is a full relayout, as before.
//!
//! Every test that takes the short path compares its result (display list
//! and laid-out tree) with what a full relayout gives for the same pointer
//! state.

use super::*;
use crate::script_net_tests::serve_routes;

/// A link, a box that grows under the pointer, and a card whose hover
/// rules change paint only, holding the layout modes a page mixes: wrapped
/// text, flex, grid, a list, a table, an inline-block.
const PAGE: &str = "<style>\
    body{margin:0;font:16px/20px sans-serif}\
    a{color:blue;text-decoration:none} a:hover{color:red;text-decoration:underline}\
    #grow{height:30px;background:tan} #grow:hover{height:50px}\
    .card{width:300px;padding:10px;border:2px solid gray;background:white}\
    .card:hover{background:#eee;border-color:black;box-shadow:0 2px 4px rgba(0,0,0,.5)}\
    .card:hover .title{color:green}\
    .row{display:flex;gap:10px} .row>div{flex:1;height:20px;background:silver}\
    .grid{display:grid;grid-template-columns:1fr 2fr;gap:4px}\
    </style>\
    <div style='height:30px'><a id=l href='/x'>a link with some words</a></div>\
    <div id=grow></div>\
    <div class=card id=c><div class=title>Title of the card that wraps onto a second line when it is this long</div>\
    <div class=row><div>a</div><div>b</div></div>\
    <div class=grid><span>one</span><span>two</span></div>\
    <ul><li>item <b>bold</b></li></ul>\
    <table><tr><td>cell</td><td>cell two</td></tr></table>\
    <span style='display:inline-block;width:40px;height:10px;background:navy'></span></div>";

/// Viewport points over the link, the growing box and the card's title.
const LINK: (f32, f32) = (10.0, 10.0);
const GROW: (f32, f32) = (10.0, 45.0);
const CARD: (f32, f32) = (150.0, 80.0);
/// Below everything: the body's own background.
const NOWHERE: (f32, f32) = (380.0, 580.0);

fn page() -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, PAGE).expect("load_html");
    (engine, view)
}

fn hover(engine: &mut Engine, view: EngineViewId, at: (f32, f32)) {
    engine.mouse_move_at_point(view, at.0, at.1);
}

fn skipped(engine: &Engine, view: EngineViewId) -> u32 {
    engine.relayout_stats(view).expect("view").layouts_skipped
}

fn restyles(engine: &Engine, view: EngineViewId) -> u32 {
    engine
        .relayout_stats(view)
        .expect("view")
        .by_first_cause
        .get("hover_or_focus")
}

/// The display list and the laid-out tree, styles included.
fn painted(engine: &Engine, view: EngineViewId) -> (String, String) {
    let state = engine.views.get(&view).expect("view");
    (
        format!("{:?}", state.display_list.as_ref().expect("list").commands),
        format!("{:?}", state.layout.as_ref().expect("layout")),
    )
}

/// What a full relayout gives for the pointer state the view is in.
fn painted_by_a_full_relayout(engine: &mut Engine, view: EngineViewId) -> (String, String) {
    let before = skipped(engine, view);
    engine.relayout(view).expect("relayout");
    assert_eq!(skipped(engine, view), before, "a relayout nobody named is a full one");
    painted(engine, view)
}

#[test]
fn a_hover_that_changes_a_colour_skips_layout() {
    let (mut engine, view) = page();
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, LINK);
    assert_eq!(restyles(&engine, view), 1, "the sheet reads :hover on the link");
    assert_eq!(skipped(&engine, view), 1, "colour and underline are paint");
    let short = painted(&engine, view);
    assert_ne!(short.0, at_rest.0, "the link is painted in its hover colour");
    assert_eq!(short, painted_by_a_full_relayout(&mut engine, view));
}

#[test]
fn a_hover_over_a_card_restyles_its_descendants_without_layout() {
    let (mut engine, view) = page();
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, CARD);
    assert_eq!(restyles(&engine, view), 1);
    assert_eq!(
        skipped(&engine, view),
        1,
        "background, border colour, shadow and a descendant's colour are paint"
    );
    let short = painted(&engine, view);
    assert_ne!(short.0, at_rest.0);
    assert_eq!(short, painted_by_a_full_relayout(&mut engine, view));
}

#[test]
fn leaving_puts_the_resting_paint_back() {
    let (mut engine, view) = page();
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, CARD);
    hover(&mut engine, view, LINK);
    hover(&mut engine, view, NOWHERE);
    assert_eq!(skipped(&engine, view), 3);
    assert_eq!(painted(&engine, view), at_rest);
}

#[test]
fn a_hover_that_changes_a_size_is_laid_out() {
    let (mut engine, view) = page();
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, GROW);
    assert_eq!(restyles(&engine, view), 1);
    assert_eq!(skipped(&engine, view), 0, "height is layout");
    let grown = painted(&engine, view);
    assert_ne!(grown.1, at_rest.1, "the card moved down");
    assert_eq!(grown, painted_by_a_full_relayout(&mut engine, view));
    // And the short path works again from the new layout.
    hover(&mut engine, view, LINK);
    assert_eq!(skipped(&engine, view), 0, "leaving the box shrinks it: layout");
    hover(&mut engine, view, NOWHERE);
    assert_eq!(skipped(&engine, view), 1);
    assert_eq!(painted(&engine, view), at_rest);
}

#[test]
fn a_hover_whose_listener_writes_to_the_page_is_laid_out() {
    let server = serve_routes(vec![(
        "/",
        "text/html",
        "<html><head><style>body{margin:0} #b{height:50px;background:tan} #b:hover{background:red}</style></head>\
         <body><div id=b></div><p id=out></p><script>\
         document.getElementById('b').addEventListener('mouseover', function () { \
           document.getElementById('out').textContent = 'over'; });</script></body></html>"
            .to_string(),
    )]);
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
    hover(&mut engine, view, (10.0, 10.0));
    assert_eq!(skipped(&engine, view), 0, "the listener added text: layout");
    assert_eq!(painted(&engine, view), painted_by_a_full_relayout(&mut engine, view));
}
