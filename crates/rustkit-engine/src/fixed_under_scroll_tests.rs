//! A `position: fixed` box stays where it is in the window while the page
//! scrolls under it. The page's scroll is one translate around the whole
//! list (`scrolled_commands`, the window path of `render_view`), so until
//! this file a fixed header left the screen with the first screen of the
//! page. Seen in the real window on 2026-10-12 (`fixed_scroll`: the bar's
//! area after the scroll was 0).

use super::*;
use crate::script_net_tests::serve_routes;
use rustkit_layout::DisplayCommand;

/// The engine's list for `body`, in a 200x100 view.
fn page_list(body: &str) -> (Engine, EngineViewId, Vec<DisplayCommand>) {
    let page = format!(
        "<html><head><style>body{{margin:0}} div{{background:#000}} #tall{{height:1000px;background:none}}</style></head>\
         <body>{body}<div id=tall></div></body></html>"
    );
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let list = engine.views[&view].display_list.as_ref().expect("display list").commands.clone();
    (engine, view, list)
}

fn capture(engine: &mut Engine, view: EngineViewId, commands: &[DisplayCommand], tag: &str) -> Vec<u8> {
    let viewhost_id = engine.views[&view].viewhost_id;
    let path = std::env::temp_dir().join(format!("rustkit-fixed-{tag}-{}.ppm", std::process::id()));
    let renderer = engine.renderer.as_mut().expect("renderer");
    renderer.set_viewport_size(200, 100);
    engine
        .compositor
        .capture_frame_with_renderer(viewhost_id, path.to_str().unwrap(), renderer, commands)
        .expect("capture");
    let ppm = std::fs::read(&path).expect("frame");
    let _ = std::fs::remove_file(&path);
    ppm
}

/// Whether the pixel is dark, in a `P6` frame 200 wide and 100 tall.
fn dark(ppm: &[u8], x: usize, y: usize) -> bool {
    let pixels = 200 * 100 * 3;
    assert!(ppm.len() > pixels, "a 200x100 frame");
    ppm[ppm.len() - pixels + (y * 200 + x) * 3] < 128
}

/// A fixed bar at the top left and an absolutely positioned block 60 down,
/// in a page scrolled by 50: the block moves up, the bar does not.
#[test]
fn a_fixed_box_stays_put_while_the_page_scrolls() {
    let (mut engine, view, list) = page_list(
        "<div style='position:fixed;left:0;top:0;width:40px;height:20px'></div>\
         <div style='position:absolute;left:100px;top:60px;width:40px;height:40px'></div>",
    );
    let at_top = capture(&mut engine, view, &list, "top");
    assert!(dark(&at_top, 20, 10) && dark(&at_top, 120, 80), "the bar at the top, the block 60 down");

    let scrolled = capture(&mut engine, view, &scrolled_commands(&list, (0.0, 50.0)), "scrolled");
    assert!(dark(&scrolled, 120, 30), "the block is drawn 50 up");
    assert!(!dark(&scrolled, 120, 80), "and not where it was");
    assert!(dark(&scrolled, 20, 10), "the fixed bar is still at the top of the window");
}

/// What is inside a fixed box stays with it, a fixed box inside it too,
/// and so does a fixed box that is deep in the scrolling content.
#[test]
fn a_fixed_box_keeps_its_contents_and_can_sit_deep_in_the_page() {
    let (mut engine, view, list) = page_list(
        "<div style='position:fixed;left:0;top:0;width:100px;height:10px'>\
           <div style='position:absolute;left:0;top:20px;width:20px;height:20px'></div>\
           <div style='position:fixed;left:40px;top:20px;width:20px;height:20px'></div>\
         </div>\
         <section style='margin-top:300px'>\
           <div style='position:fixed;left:160px;top:60px;width:20px;height:20px'></div></section>",
    );
    let scrolled = capture(&mut engine, view, &scrolled_commands(&list, (0.0, 50.0)), "nested");
    assert!(dark(&scrolled, 50, 5), "the bar");
    assert!(dark(&scrolled, 10, 30), "its absolutely positioned child");
    assert!(dark(&scrolled, 50, 30), "the fixed box inside it is not moved twice");
    assert!(dark(&scrolled, 170, 70), "a fixed box 300 down the document");
}

/// Across as well as down.
#[test]
fn a_fixed_box_stays_put_when_the_page_scrolls_sideways() {
    let (mut engine, view, list) =
        page_list("<div style='position:fixed;left:100px;top:40px;width:40px;height:20px'></div>");
    let scrolled = capture(&mut engine, view, &scrolled_commands(&list, (30.0, 20.0)), "sideways");
    assert!(dark(&scrolled, 120, 50), "where it was");
    assert!(!dark(&scrolled, 85, 25), "not 30 left and 20 up");
}

/// With no scroll the window draws the page's list as it is.
#[test]
fn an_unscrolled_page_draws_its_fixed_box_where_layout_put_it() {
    let (mut engine, view, list) =
        page_list("<div style='position:fixed;left:100px;top:40px;width:40px;height:20px'></div>");
    let frame = capture(&mut engine, view, &list, "plain");
    assert!(dark(&frame, 120, 50) && !dark(&frame, 120, 20) && !dark(&frame, 50, 50));
}
