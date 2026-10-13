//! `box-sizing: inherit`. The reset most sites start with is
//! `html { box-sizing: border-box } *, :before, :after { box-sizing: inherit }`,
//! and `inherit` on `box-sizing` did nothing here: every box on such a page
//! stayed `content-box`, so a `width: 100%` wrapper with side padding was
//! wider than the window. Seen on cnn 2026-10-12 (`section.layout__wrapper`
//! 1344 wide in a 1280 window; 1280 in Chromium on the same bytes).

use super::*;
use crate::script_net_tests::serve_routes;

/// What `script` answers on a page with the inheriting reset, `css` after
/// it and `body`, in an 800x600 view.
fn measured(css: &str, body: &str, script: &str) -> String {
    let page = format!(
        "<html><head><style>html{{box-sizing:border-box}} *,:before,:after{{box-sizing:inherit}} body{{margin:0}} {css}</style></head>\
         <body>{body}</body></html>"
    );
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    engine.execute_script(view, script).expect("script")
}

const WIDTHS: &str = "[].map.call(document.querySelectorAll('[id]'), function (e) { \
     return e.id + ':' + e.getBoundingClientRect().width; }).join(' ')";

/// cnn's wrapper: `width: 100%` and side padding fit the window, and what
/// is inside it is the window less the padding.
#[test]
fn a_full_width_wrapper_with_padding_fits_the_window_under_the_inheriting_reset() {
    assert_eq!(
        measured(
            ".wrap{width:100%;max-width:1440px;margin:0 auto;padding:0 32px}",
            "<section id=wrap class=wrap><div id=inner>x</div></section>",
            WIDTHS,
        ),
        "String(\"wrap:800 inner:736\")"
    );
}

/// The value comes down from `html` through every level, and a box that
/// says `content-box` hands that to its own children.
#[test]
fn box_sizing_inherit_takes_the_parents_value_at_every_level() {
    assert_eq!(
        measured(
            ".b{width:200px;padding:10px;border:5px solid}  .c{box-sizing:content-box}",
            "<div><div><div id=deep class=b>x</div></div></div>\
             <div id=own class='b c'><div id=child class=b>x</div></div>",
            WIDTHS,
        ),
        "String(\"deep:200 own:230 child:230\")"
    );
}

/// `getComputedStyle` reports what was inherited, on an element and on a
/// box that set nothing itself.
#[test]
fn the_inherited_box_sizing_is_what_computed_style_reports() {
    assert_eq!(
        measured(
            ".c{box-sizing:content-box}",
            "<p id=a>x</p><div class=c><p id=b>x</p></div>",
            "getComputedStyle(document.getElementById('a')).boxSizing + ' ' + \
             getComputedStyle(document.getElementById('b')).boxSizing",
        ),
        "String(\"border-box content-box\")"
    );
}

/// A page without the reset is as before: `box-sizing` is not inherited
/// unless a rule says `inherit`.
#[test]
fn box_sizing_is_not_inherited_without_the_keyword() {
    let page = "<html><head><style>body{margin:0} .p{box-sizing:border-box;width:300px;padding:10px} \
                .k{width:100px;padding:10px}</style></head>\
                <body><div id=p class=p><div id=k class=k>x</div></div></body></html>";
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    assert_eq!(engine.execute_script(view, WIDTHS).expect("script"), "String(\"p:300 k:120\")");
}
