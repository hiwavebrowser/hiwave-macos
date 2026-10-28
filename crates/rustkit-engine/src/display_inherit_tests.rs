//! `display: inherit`. `inherit_property` had no `display` arm, so the
//! keyword fell through to "keep what you have" and a `<div>` stayed
//! `block` whatever its parent was. Seen on apple's global nav 2026-10-28:
//! `.globalnav-flyout` and `.globalnav-menu-list` say `display: inherit`
//! under a `display: contents` item of a flex list, so in Chrome the ten
//! menu items are flex items of the list. Here the two wrappers made one
//! block flex item and the items stacked in it: only the first fitted in
//! the 44px bar.

use super::*;

fn loaded(html: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, html).expect("load");
    (engine, view)
}

const PLACES: &str = "[].map.call(document.querySelectorAll('[id]'), function (e) { \
     var r = e.getBoundingClientRect(); \
     return e.id + ':' + getComputedStyle(e).display + ',' + r.x + ',' + r.y; }).join(' ')";

/// A wrapper that inherits its flex parent's `display` is a flex container
/// itself, so its children sit in a row. Chrome 148: `w:flex,0,0 a:block,0,0
/// b:block,50,0`.
#[test]
fn display_inherit_takes_the_parents_flex() {
    let (mut engine, view) = loaded(concat!(
        "<!doctype html><style>body{margin:0} #p{display:flex;width:300px}",
        " #w{display:inherit} #a,#b{width:50px;height:20px}</style>",
        "<body><div id=p><div id=w><div id=a></div><div id=b></div></div></div></body>",
    ));
    assert_eq!(
        engine.execute_script(view, PLACES).expect("script"),
        "String(\"p:flex,0,0 w:flex,0,0 a:block,0,0 b:block,50,0\")"
    );
}

/// It goes down level by level, and a wrapper under a block parent stays
/// block. Chrome 148: `q:block,0,0 v:block,0,0 c:block,0,0 d:block,0,20`
/// beside the flex chain `r:flex,0,40 u:flex,0,40 t:flex,0,40 e:block,0,40
/// f:block,50,40`.
#[test]
fn display_inherit_follows_the_chain_and_keeps_block_under_block() {
    let (mut engine, view) = loaded(concat!(
        "<!doctype html><style>body{margin:0} #q{display:block} #r{display:flex}",
        " .i{display:inherit} #c,#d,#e,#f{width:50px;height:20px}</style>",
        "<body><div id=q><div id=v class=i><div id=c></div><div id=d></div></div></div>",
        "<div id=r><div id=u class=i><div id=t class=i><div id=e></div><div id=f></div></div></div></div>",
        "</body>",
    ));
    assert_eq!(
        engine.execute_script(view, PLACES).expect("script"),
        "String(\"q:block,0,0 v:block,0,0 c:block,0,0 d:block,0,20 \
         r:flex,0,40 u:flex,0,40 t:flex,0,40 e:block,0,40 f:block,50,40\")"
    );
}
