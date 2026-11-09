//! LAT9: an image's download budget is its own.
//!
//! Seen in a real-window session (2026-11-09) on a search-results page: the
//! page's script swapped each card's thumbnail `src` for a larger image
//! after the load. The pass that fetched the swapped images had one
//! deadline for all of them, set when the pass was planned, and ran its one
//! SVG before any raster. Live turns of 1.5 to 4.7 s left the pass 2 ms of
//! polling per turn; the SVG used the whole budget, and 78 rasters then
//! failed on their first poll, "over the subresource budget", with no
//! request made. Each card had already dropped its thumbnail, so the grid
//! painted blank.
//!
//! Here each image's budget starts when its own request starts, the SVG and
//! raster images share the slots side by side, the fetch and decode run on
//! the network runtime while the page thread is busy, and an `<img>` whose
//! `src` changed keeps painting what it showed until the new image is in.

use super::*;
use crate::script_net_tests::serve_byte_routes;

/// A 1 x 1 PNG.
const PNG: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 252, 207, 192, 80, 15, 0, 4, 133, 1, 128, 132, 169, 140, 33, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;

fn png_route(path: &str) -> (&'static str, &'static str, Vec<u8>) {
    (Box::leak(path.to_string().into_boxed_str()), "image/png", PNG.to_vec())
}

fn runtime() -> &'static tokio::runtime::Runtime {
    thread_local! {
        static RT: &'static tokio::runtime::Runtime = Box::leak(Box::new(
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap(),
        ));
    }
    RT.with(|rt| *rt)
}

fn load(
    page: String,
    mut routes: Vec<(&'static str, &'static str, Vec<u8>)>,
    budget_ms: u64,
) -> (Engine, EngineViewId, crate::script_net_tests::Server, std::time::Duration) {
    routes.insert(0, ("/", "text/html", page.into_bytes()));
    let server = serve_byte_routes(routes);
    let config = EngineConfig { subresource_budget_ms: budget_ms, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let started = std::time::Instant::now();
    runtime().block_on(engine.load_url(view, url)).expect("load_url");
    let took = started.elapsed();
    (engine, view, server, took)
}

fn at(server: &crate::script_net_tests::Server, path: &str) -> Url {
    Url::parse(&format!("http://127.0.0.1:{}{path}", server.port)).unwrap()
}

/// Live turns, `pause` of page-thread work between two, until nothing is
/// in flight or `limit` has passed.
fn turns(engine: &mut Engine, view: EngineViewId, pause: std::time::Duration, limit: std::time::Duration) {
    let rt = runtime();
    let end = std::time::Instant::now() + limit;
    rt.block_on(engine.pump_live(view, 16));
    while std::time::Instant::now() < end {
        std::thread::sleep(pause);
        if rt.block_on(engine.pump_live(view, 16)).in_flight == 0 {
            break;
        }
    }
    rt.block_on(engine.pump_live(view, 16));
}

/// The `<img>` URLs the view's display list paints.
fn painted(engine: &Engine, view: EngineViewId) -> Vec<String> {
    engine.views[&view]
        .display_list
        .as_ref()
        .expect("display list")
        .commands
        .iter()
        .filter_map(|c| match c {
            rustkit_layout::DisplayCommand::Image { url, .. } => Some(url.clone()),
            _ => None,
        })
        .collect()
}

/// The incident's shape: after the load, script adds one SVG that takes
/// longer than the budget and 50 rasters that are quick. The rasters must
/// not wait behind the SVG, nor inherit the clock it spent.
#[test]
fn a_slow_svg_does_not_spend_the_budget_of_fifty_rasters() {
    let script = "var h='<img src=\"/slow.svg?delay=1500\">';\
        for(var i=0;i<50;i++){h+='<img src=\"/r'+i+'.png\">';}\
        document.getElementById('grid').innerHTML=h;";
    let page = "<html><body><div id=grid></div></body></html>".to_string();
    let mut routes: Vec<_> = (0..50).map(|i| png_route(&format!("/r{i}.png"))).collect();
    routes.push(("/slow.svg?delay=1500", "image/svg+xml", SVG.as_bytes().to_vec()));
    let (mut engine, view, server, _) = load(page, routes, 1_000);
    engine.execute_script(view, script).unwrap();
    turns(&mut engine, view, std::time::Duration::from_millis(20), std::time::Duration::from_secs(8));
    let missing: Vec<usize> = (0..50)
        .filter(|i| !engine.image_manager().is_cached(&at(&server, &format!("/r{i}.png"))))
        .collect();
    assert!(missing.is_empty(), "rasters dropped behind the slow SVG: {missing:?}");
    let requested = (0..50).filter(|i| !server.served(&format!("/r{i}.png")).is_empty()).count();
    assert_eq!(requested, 50, "every raster was requested");
}

/// Live turns that hold the page thread longer than the whole budget (the
/// session's were 1.5 to 4.7 s against 8 s): the images are fetched and
/// decoded meanwhile, and the next turn puts them in.
#[test]
fn images_load_while_turns_hold_the_page_longer_than_the_budget() {
    let script = "var h='';\
        for(var i=0;i<20;i++){h+='<img src=\"/t'+i+'.png?delay=150\">';}\
        document.getElementById('grid').innerHTML=h;";
    let page = "<html><body><div id=grid></div></body></html>".to_string();
    let routes: Vec<_> = (0..20).map(|i| png_route(&format!("/t{i}.png?delay=150"))).collect();
    let (mut engine, view, server, _) = load(page, routes, 600);
    engine.execute_script(view, script).unwrap();
    turns(&mut engine, view, std::time::Duration::from_millis(900), std::time::Duration::from_secs(8));
    let missing: Vec<usize> = (0..20)
        .filter(|i| !engine.image_manager().is_cached(&at(&server, &format!("/t{i}.png?delay=150"))))
        .collect();
    assert!(missing.is_empty(), "images lost to turns longer than the budget: {missing:?}");
}

/// Eight images that never answer hold every slot until their own budgets
/// end; the twelve queued behind them then get budgets of their own.
#[test]
fn an_image_timeout_fires_for_that_image_only() {
    let mut body = String::new();
    let mut routes = Vec::new();
    for i in 0..8 {
        body += &format!("<img src=\"/hang{i}.png?delay=3000\">");
        routes.push(png_route(&format!("/hang{i}.png?delay=3000")));
    }
    for i in 0..12 {
        body += &format!("<img src=\"/f{i}.png\">");
        routes.push(png_route(&format!("/f{i}.png")));
    }
    let page = format!("<html><body>{body}</body></html>");
    let (mut engine, view, server, took) = load(page, routes, 500);
    assert!(took < std::time::Duration::from_millis(2_500), "the load waited for the hung images: {took:?}");
    turns(&mut engine, view, std::time::Duration::from_millis(20), std::time::Duration::from_secs(6));
    let missing: Vec<usize> = (0..12)
        .filter(|i| !engine.image_manager().is_cached(&at(&server, &format!("/f{i}.png"))))
        .collect();
    assert!(missing.is_empty(), "quick images failed on the hung ones' clock: {missing:?}");
    let hung = (0..8)
        .filter(|i| engine.image_manager().is_cached(&at(&server, &format!("/hang{i}.png?delay=3000"))))
        .count();
    assert_eq!(hung, 0, "a hung image is dropped at its own budget");
}

/// Chrome keeps an `<img>`'s current image up while the new `src` loads:
/// the box paints the old image until the new one has decoded.
#[test]
fn a_src_swap_keeps_the_old_image_until_the_new_one_decodes() {
    let page = "<html><body><img id=card src=/old.png width=40 height=40></body></html>".to_string();
    let routes = vec![png_route("/old.png"), png_route("/new.png?delay=700")];
    let (mut engine, view, server, _) = load(page, routes, 5_000);
    let old = at(&server, "/old.png").to_string();
    let new = at(&server, "/new.png?delay=700").to_string();
    assert_eq!(painted(&engine, view), [old.clone()], "precondition: the thumbnail painted");
    engine
        .execute_script(view, "document.getElementById('card').src='/new.png?delay=700'")
        .unwrap();
    runtime().block_on(engine.pump_live(view, 16));
    assert_eq!(painted(&engine, view), [old.clone()], "the old image stays up while the new one loads");
    turns(&mut engine, view, std::time::Duration::from_millis(20), std::time::Duration::from_secs(5));
    assert_eq!(painted(&engine, view), [new], "the new image replaced it once decoded");
}

/// A swap whose new image never arrives keeps the old one painted.
#[test]
fn a_failed_src_swap_keeps_the_old_image() {
    let page = "<html><body><img id=card src=/old.png width=40 height=40></body></html>".to_string();
    let (mut engine, view, server, _) = load(page, vec![png_route("/old.png")], 5_000);
    let old = at(&server, "/old.png").to_string();
    engine
        .execute_script(view, "document.getElementById('card').src='/missing.png'")
        .unwrap();
    turns(&mut engine, view, std::time::Duration::from_millis(20), std::time::Duration::from_secs(5));
    assert!(!server.served("/missing.png").is_empty(), "precondition: the new image was requested");
    assert_eq!(painted(&engine, view), [old], "the old image stays when the new one fails");
}
