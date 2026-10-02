//! The script-network bridge through a real page load: `Engine::load_url`
//! runs a page whose script uses the bridge directly (the XHR/fetch surfaces
//! come later and are built on exactly this).

use super::*;
use crate::script_net_tests::serve_routes;

fn load(config: EngineConfig, routes: Vec<(&'static str, &'static str, String)>) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(routes);
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    (engine, view, server)
}

/// Asked while scripts run, from a load handler, and from a timer; the
/// page asks relative to its own origin through `location`.
const ORIGIN_PAGE: &str = r#"<html><head><script>
var log = [];
function ask(path, tag) {
    window.__rustkit_net.request({ url: location.origin + path }, function (r) { log.push(tag + ':' + (r.ok ? r.status : r.error)); });
}
ask('/script', 'script');
window.addEventListener('load', function () { ask('/load', 'load'); });
setTimeout(function () { ask('/timer', 'timer'); }, 100);
</script></head><body>hi</body></html>"#;

#[test]
fn requests_from_scripts_handlers_and_timers_are_all_answered_during_the_load() {
    let (mut engine, view, server) = load(
        EngineConfig::default(),
        vec![("/", "text/html", ORIGIN_PAGE.to_string())],
    );
    let log = engine.execute_script(view, "log.join(',')").unwrap();
    assert_eq!(log, r#"String("script:200,load:200,timer:200")"#);
    // The page itself, plus the three requests (distinct URLs: the loader
    // caches a repeated GET).
    assert_eq!(server.hits.load(std::sync::atomic::Ordering::SeqCst), 4);
}

/// Off: the bridge does not exist on the page at all.
#[test]
fn with_script_network_off_the_page_has_no_bridge() {
    let (mut engine, view, server) = load(
        EngineConfig { script_network_enabled: false, ..EngineConfig::default() },
        vec![("/", "text/html", "<html><body>hi</body></html>".to_string())],
    );
    assert_eq!(
        engine.execute_script(view, "typeof window.__rustkit_net").unwrap(),
        r#"String("undefined")"#
    );
    assert_eq!(server.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// `load_html` has no origin and no policy: no bridge either.
#[test]
fn a_page_loaded_from_a_string_has_no_bridge() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    engine.load_html(view, "<html><body>hi</body></html>").unwrap();
    assert_eq!(
        engine.execute_script(view, "typeof window.__rustkit_net").unwrap(),
        r#"String("undefined")"#
    );
}
