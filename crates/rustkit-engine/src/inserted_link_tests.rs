//! Plan row CH2.E20: a `<link rel=stylesheet>` that a script inserts is
//! fetched and its rules apply (HTML, `link` type `stylesheet`: a link that
//! becomes connected obtains its resource; lazy CSS on real sites does this).
//! Until this row the engine looked for stylesheet links once, during the
//! load, so a link a script added was never requested.

use super::*;
use crate::script_net_tests::{serve_routes, Server};

const SHEET: &str = "#t { width: 77px }";

fn served_page(page: &str) -> (Engine, EngineViewId, Server) {
    let server = serve_routes(vec![
        ("/", "text/html", page.to_string()),
        ("/s.css", "text/css", SHEET.to_string()),
        ("/u.css", "text/css", "#u { width: 55px }".to_string()),
    ]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

fn read(engine: &mut Engine, view: EngineViewId, script: &str) -> String {
    let got = engine.execute_script(view, &format!("String({script})")).expect("script");
    got.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&got).to_string()
}

fn width(engine: &mut Engine, view: EngineViewId, id: &str) -> String {
    read(engine, view, &format!("document.getElementById('{id}').offsetWidth"))
}

/// Live turns until `#id` is `want` px wide or two seconds pass.
fn pump_until_width(engine: &mut Engine, view: EngineViewId, id: &str, want: &str) -> String {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let start = std::time::Instant::now();
    loop {
        rt.block_on(engine.pump_live(view, start.elapsed().as_millis() as u64));
        let got = width(engine, view, id);
        if got == want || start.elapsed() > std::time::Duration::from_secs(2) {
            return got;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

const INSERT: &str = "var l = document.createElement('link'); \
    l.setAttribute('rel', 'stylesheet'); l.setAttribute('href', '/s.css'); \
    document.head.appendChild(l);";

#[test]
fn a_link_a_parser_run_script_appends_is_fetched_and_applies() {
    let page = format!(
        "<!doctype html><html><head></head><body style=margin:0>\
         <div id=t style=height:10px></div><script>{INSERT}</script></body></html>"
    );
    let (mut engine, view, server) = served_page(&page);
    let got = pump_until_width(&mut engine, view, "t", "77");
    assert_eq!(server.served("/s.css").len(), 1, "the inserted sheet is requested once");
    assert_eq!(got, "77", "its rule applies");
}

#[test]
fn a_link_appended_after_the_load_is_fetched_on_a_live_turn() {
    let (mut engine, view, server) =
        served_page("<!doctype html><html><head></head><body style=margin:0><div id=t style=height:10px></div></body></html>");
    assert_eq!(width(&mut engine, view, "t"), "800");
    engine.execute_script(view, INSERT).expect("insert");
    let got = pump_until_width(&mut engine, view, "t", "77");
    assert_eq!(server.served("/s.css").len(), 1, "the inserted sheet is requested once");
    assert_eq!(got, "77", "its rule applies");
}

#[test]
fn a_link_already_in_the_markup_is_not_fetched_again_when_another_is_added() {
    let (mut engine, view, server) = served_page(
        "<!doctype html><html><head><link rel=stylesheet href=/s.css></head><body style=margin:0>\
         <div id=t style=height:10px></div><div id=u style=height:10px></div></body></html>",
    );
    assert_eq!(width(&mut engine, view, "t"), "77");
    engine
        .execute_script(
            view,
            "var l = document.createElement('link'); l.rel = 'stylesheet'; \
             l.setAttribute('href', '/u.css'); document.head.appendChild(l);",
        )
        .expect("insert");
    let got = pump_until_width(&mut engine, view, "u", "55");
    assert_eq!(got, "55", "the added sheet applies");
    assert_eq!(width(&mut engine, view, "t"), "77", "the markup sheet still applies");
    assert_eq!(server.served("/s.css").len(), 1, "the markup sheet is not requested again");
    assert_eq!(server.served("/u.css").len(), 1, "the added sheet is requested once");
}
