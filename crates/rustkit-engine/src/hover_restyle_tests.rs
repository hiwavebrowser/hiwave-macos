//! A pointer restyle (`:hover`, `:active`) that changes only what is
//! painted keeps the laid-out tree: the elements that gained or lost the
//! pseudo-class are built again with what is under them, and nothing else
//! of the page; the paint properties that changed are copied onto the boxes
//! that are already laid out, and the page is painted from them. A restyle
//! that changes anything layout reads is a full relayout, as before.
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

/// Fails with the text around the first place two painted states differ.
fn assert_same(got: &(String, String), want: &(String, String), what: &str) {
    for (name, got, want) in [("display list", &got.0, &want.0), ("tree", &got.1, &want.1)] {
        if got == want {
            continue;
        }
        let at = got.bytes().zip(want.bytes()).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
        let window = |s: &str| {
            let from = (0..=at.saturating_sub(300)).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0);
            let to = (at + 200).min(s.len());
            let to = (to..=s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(s.len());
            s[from..to].to_string()
        };
        panic!("{what}: the {name} differs at byte {at}\n   got  ...{}\n   want ...{}", window(got), window(want));
    }
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
    assert_eq!(LAST_RESTYLE_MISS.with(|why| why.borrow().clone()), "style: height");
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

/// Forty cards in a 300px column, each 40px tall, whose rules read where a
/// card and its children sit among their siblings. `NOWHERE` is beside them.
fn cards(extra_css: &str) -> (Engine, EngineViewId) {
    let mut html = String::from(
        "<style>body{margin:0;font:16px/20px sans-serif}\
         #list{width:300px}\
         .card{height:38px;border:1px solid gray;color:black}\
         .card:hover{background:#eee;color:maroon}\
         .card:nth-child(3):hover{border-color:red}\
         .card:hover span:nth-of-type(2){background:yellow}\
         .card:hover b:last-child{color:green}\
         .card a{color:blue} .card a:hover{text-decoration:underline}\
         .card:active{background:#ccd}",
    );
    html.push_str(extra_css);
    html.push_str("</style><div id=list>");
    for i in 0..40 {
        html.push_str(&format!(
            "<div class=card><span>card {i}</span> <span>second</span> <a href='/x'>link</a> <b>last</b></div>"
        ));
    }
    html.push_str("</div><p id=after>after the cards</p>");
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, &html).expect("load_html");
    (engine, view)
}

/// A point over the text of card `n` (0-based), left of its link.
fn card(n: usize) -> (f32, f32) {
    (20.0, n as f32 * 40.0 + 20.0)
}

fn walked() -> usize {
    LAST_RESTYLE_WALKED.with(|walked| walked.get())
}

#[test]
fn a_hover_builds_the_hovered_card_and_not_the_page() {
    let (mut engine, view) = cards("");
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, card(2));
    assert_eq!(skipped(&engine, view), 1);
    // Two builds of: the list, the card, its four children and their text.
    assert!(walked() < 40, "the two builds went into {} nodes of about 500", walked());
    let short = painted(&engine, view);
    assert_ne!(short.0, at_rest.0);
    assert_eq!(short, painted_by_a_full_relayout(&mut engine, view), "the third card, by position");
}

#[test]
fn a_move_from_one_card_to_another_restyles_both() {
    let (mut engine, view) = cards("");
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, card(2));
    hover(&mut engine, view, card(7));
    assert_eq!(skipped(&engine, view), 2);
    assert!(walked() < 80, "{} nodes", walked());
    let short = painted(&engine, view);
    assert_same(&short, &painted_by_a_full_relayout(&mut engine, view), "after the move");
    hover(&mut engine, view, NOWHERE);
    assert_eq!(skipped(&engine, view), 3);
    assert_eq!(painted(&engine, view), at_rest);
}

#[test]
fn a_hover_inside_a_hovered_card_restyles_the_inner_element() {
    let (mut engine, view) = cards("");
    hover(&mut engine, view, card(5));
    // The link is the third child: past "card 5 second ".
    let link = engine
        .execute_script(
            view,
            "(function(){var r=document.querySelectorAll('.card')[5].querySelector('a').getBoundingClientRect();\
             return Math.round(r.left+r.width/2)+','+Math.round(r.top+r.height/2)})()",
        )
        .expect("script");
    let link = link.trim_start_matches("String(\"").trim_end_matches("\")");
    let (x, y) = link.split_once(',').expect("x,y");
    hover(&mut engine, view, (x.parse().unwrap(), y.parse().unwrap()));
    assert_eq!(skipped(&engine, view), 2, "the card stays hovered; the link gains it");
    assert!(walked() < 20, "{} nodes: the link alone", walked());
    assert_eq!(painted(&engine, view), painted_by_a_full_relayout(&mut engine, view));
}

#[test]
fn a_press_restyles_the_pressed_card() {
    let (mut engine, view) = cards("");
    hover(&mut engine, view, card(4));
    let hovered = painted(&engine, view);
    let (x, y) = card(4);
    engine.mouse_down_at_point(view, x, y);
    assert_eq!(skipped(&engine, view), 2, "a background is paint");
    let pressed = painted(&engine, view);
    assert_ne!(pressed.0, hovered.0);
    assert_same(&pressed, &painted_by_a_full_relayout(&mut engine, view), "pressed");
}

#[test]
fn a_hover_rule_with_a_sibling_combinator_takes_the_full_path() {
    let (mut engine, view) = cards(".card:hover + .card{background:pink}");
    let at_rest = painted(&engine, view);
    hover(&mut engine, view, card(2));
    assert_eq!(skipped(&engine, view), 0, "the next card is not under the hovered one");
    let hovered = painted(&engine, view);
    assert_ne!(hovered.0, at_rest.0);
    assert_eq!(hovered, painted_by_a_full_relayout(&mut engine, view));
}

/// A field in a 40px row and, under it, a box whose hover rule changes
/// paint only; `extra_css` adds each test's `:active` or `:focus` rules.
fn field_page(extra_css: &str) -> (Engine, EngineViewId) {
    let html = format!(
        "<style>body{{margin:0;font:16px/20px sans-serif}} #x{{height:30px;background:tan}} #x:hover{{background:red}} {extra_css}</style>\
         <div style='height:40px'><input id=i></div><div id=x>x</div>"
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, &html).expect("load_html");
    (engine, view)
}

/// Points over the field and over the box under it.
const FIELD: (f32, f32) = (20.0, 10.0);
const BOX: (f32, f32) = (20.0, 55.0);

/// Whether the laid-out tree carries a focused control with its caret.
fn caret_drawn(painted: &(String, String)) -> bool {
    painted.1.contains("focused_caret: Some(")
}

fn last_miss() -> String {
    LAST_RESTYLE_MISS.with(|why| why.borrow().clone())
}

#[test]
fn a_press_that_focuses_a_field_paints_its_caret() {
    let (mut engine, view) = field_page("input:active{background:#eee}");
    assert!(!caret_drawn(&painted(&engine, view)));
    engine.mouse_down_at_point(view, FIELD.0, FIELD.1);
    assert!(engine.focused_node(view).is_some(), "the press focused the field");
    assert_eq!(restyles(&engine, view), 1, "the sheet reads :active on the field");
    assert_eq!(skipped(&engine, view), 0, "the focus moved since the last layout: the full path");
    assert_eq!(last_miss(), "focus moved");
    let pressed = painted(&engine, view);
    assert!(caret_drawn(&pressed), "the field is painted focused, with its caret");
    assert_same(&pressed, &painted_by_a_full_relayout(&mut engine, view), "pressed");
}

#[test]
fn a_hover_after_a_click_into_a_field_paints_the_focus() {
    let (mut engine, view) = field_page("");
    engine.click_at_point(view, FIELD.0, FIELD.1);
    assert!(engine.focused_node(view).is_some(), "the click focused the field");
    // No rule read the click, so nothing was laid out for it: the focus is
    // not painted until the next relayout (as on develop, where that
    // relayout is always a full one).
    assert_eq!(restyles(&engine, view), 0);
    assert!(!caret_drawn(&painted(&engine, view)));
    hover(&mut engine, view, BOX);
    assert_eq!(restyles(&engine, view), 1);
    assert_eq!(skipped(&engine, view), 0, "the focus moved since the last layout: the full path");
    assert_eq!(last_miss(), "focus moved");
    let hovered = painted(&engine, view);
    assert!(caret_drawn(&hovered), "the hover's relayout paints the focus and the caret");
    assert_same(&hovered, &painted_by_a_full_relayout(&mut engine, view), "hovered after the click");
    // That layout recorded the focus: the next paint-only move takes the
    // short path and keeps the caret.
    hover(&mut engine, view, NOWHERE);
    assert_eq!(skipped(&engine, view), 1);
    let left = painted(&engine, view);
    assert!(caret_drawn(&left));
    assert_same(&left, &painted_by_a_full_relayout(&mut engine, view), "left the box");
}

#[test]
fn a_press_elsewhere_blurs_the_field_in_paint() {
    let (mut engine, view) = field_page("#x:active{background:red}");
    engine.mouse_down_at_point(view, FIELD.0, FIELD.1);
    engine.relayout(view).expect("relayout");
    assert!(caret_drawn(&painted(&engine, view)), "the focused field is laid out and painted");
    engine.mouse_down_at_point(view, BOX.0, BOX.1);
    assert_eq!(engine.focused_node(view), None, "a press on the box takes the focus away");
    assert_eq!(skipped(&engine, view), 0, "the focus moved since the last layout: the full path");
    assert_eq!(last_miss(), "focus moved");
    let blurred = painted(&engine, view);
    assert!(!caret_drawn(&blurred), "no caret on a field that lost the focus");
    assert_same(&blurred, &painted_by_a_full_relayout(&mut engine, view), "blurred");
}

#[test]
fn focus_rules_after_a_click_paint_as_a_full_relayout_does() {
    // `:focus`, `:focus-visible` and `:focus-within` are not matched by
    // the engine yet (the same on develop): what is checked is that the
    // short path never shows something a full relayout does not, with the
    // caret, when a page has those rules.
    let (mut engine, view) = field_page(
        "#i:focus{background:yellow} #i:focus-visible{outline:2px solid blue} div:focus-within{background:pink}",
    );
    engine.click_at_point(view, FIELD.0, FIELD.1);
    hover(&mut engine, view, BOX);
    assert_eq!(skipped(&engine, view), 0, "the focus moved: the full path");
    let hovered = painted(&engine, view);
    assert!(caret_drawn(&hovered));
    assert_same(&hovered, &painted_by_a_full_relayout(&mut engine, view), "focus rules, hovered");
    hover(&mut engine, view, NOWHERE);
    assert_eq!(skipped(&engine, view), 1);
    assert_same(&painted(&engine, view), &painted_by_a_full_relayout(&mut engine, view), "focus rules, left");
}

#[test]
fn a_label_click_that_focuses_a_textarea_paints_its_caret() {
    let html = "<style>body{margin:0;font:16px/20px sans-serif} label{display:block;height:20px} label:active{color:red}\
                #x{height:30px;background:tan} #x:hover{background:red}</style>\
                <label for=t>name</label><div style='height:60px'><textarea id=t rows=2></textarea></div><div id=x>x</div>";
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 400,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, html).expect("load_html");
    // The press restyles the label; the release gives the textarea the focus.
    engine.click_at_point(view, 10.0, 10.0);
    assert!(engine.focused_node(view).is_some(), "the label's click focused its textarea");
    hover(&mut engine, view, (20.0, 95.0));
    let hovered = painted(&engine, view);
    assert!(caret_drawn(&hovered), "the textarea is painted focused, with its caret");
    assert_same(&hovered, &painted_by_a_full_relayout(&mut engine, view), "hovered after the label click");
    hover(&mut engine, view, NOWHERE);
    let left = painted(&engine, view);
    assert!(caret_drawn(&left));
    assert_same(&left, &painted_by_a_full_relayout(&mut engine, view), "left the box");
}

/// The review's cost page: 300 inline-block cards, about 5,700 boxes, whose
/// hover rules change paint only; with `miss`, one of them changes a height.
fn large_page(miss: bool) -> String {
    let mut html = String::from(
        "<style>body{margin:0;font:14px/18px sans-serif}\
         .card{width:360px;padding:6px;border:1px solid #999;margin:4px;display:inline-block;vertical-align:top}\
         .card .t{font-weight:bold} .row{display:flex;gap:4px} .row>span{flex:1;background:#eee}\
         #nohover{height:30px;background:#ccc}\
         .card:hover{background:#eef;border-color:#00f} .card:hover .t{color:#c00} .card a:hover{text-decoration:underline}",
    );
    if miss {
        html.push_str(".card:hover .row{height:30px}");
    }
    html.push_str("</style><div id=nohover>top bar no rule</div>");
    for i in 0..300 {
        html.push_str(&format!(
            "<div class=card><div class=t>Card {i} title with some words that wrap</div>\
             <p>Body text for card {i}, a sentence or two of text that wraps across lines in the card. <a href='#'>link {i}</a></p>\
             <div class=row><span>a</span><span>b</span><span>c</span></div><ul><li>one</li><li>two</li></ul></div>"
        ));
    }
    html
}

/// Timing, not a check: `--release -- --ignored --nocapture
/// zz_timing_of_a_hover_on_a_large_page`, once with the default and once
/// with `RUSTKIT_RESTYLE_REPAINT=0` (every pointer restyle laid out, as
/// before the short path). Prints the median milliseconds of a pointer move
/// onto a card and off it, of the same on the page whose hover changes a
/// height, and of a relayout nobody named.
#[test]
#[ignore = "timing"]
fn zz_timing_of_a_hover_on_a_large_page() {
    fn boxes(b: &LayoutBox) -> usize {
        1 + b.children.iter().map(boxes).sum::<usize>()
    }
    fn median(mut ms: Vec<f64>) -> f64 {
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ms[ms.len() / 2]
    }
    for miss in [false, true] {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 1280, height: 800 })
            .expect("view");
        engine.load_html(view, &large_page(miss)).expect("load_html");
        let count = boxes(engine.views.get(&view).unwrap().layout.as_ref().unwrap());
        let mut moves = Vec::new();
        let mut split: [Vec<f64>; 4] = Default::default();
        for rep in 0..30 {
            for at in [([100.0, 470.0, 840.0][rep % 3], 60.0), (100.0, 15.0)] {
                let before = skipped(&engine, view);
                let started = std::time::Instant::now();
                hover(&mut engine, view, at);
                moves.push(started.elapsed().as_secs_f64() * 1000.0);
                if skipped(&engine, view) > before {
                    let last = LAST_RESTYLE_SPLIT.with(|last| last.get());
                    for (part, ms) in split.iter_mut().zip(last) {
                        part.push(ms);
                    }
                }
            }
        }
        if !split[0].is_empty() {
            let [builds, list, snapshot, render] = split.map(median);
            println!(
                "HOVERTIMING short path split: builds+compare+patch {builds:.2} display_list {list:.1} snapshot {snapshot:.1} render {render:.1} ms"
            );
        }
        let taken = skipped(&engine, view);
        let mut relayouts = Vec::new();
        for _ in 0..15 {
            let started = std::time::Instant::now();
            engine.relayout(view).expect("relayout");
            relayouts.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        println!(
            "HOVERTIMING miss_page={miss} boxes={count} moves={} short_path={taken} move_median_ms={:.1} relayout_median_ms={:.1}",
            moves.len(),
            median(moves),
            median(relayouts)
        );
    }
}
