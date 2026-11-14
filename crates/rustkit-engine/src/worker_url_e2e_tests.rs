//! A dedicated worker started from a URL, and `importScripts`, through the real engine: `Engine::load_url`,
//! the script-network bridge (`script_net`) and the fetch policy, against a local server. The R1 HOLD on
//! #1105 found both requests refused by the engine (every bindings test answered them itself). Expected
//! values: the pinned Chrome (CfT 148.0.7778.216) on the same routes,
//! `docs/diagnostics/2026-11-14/worker_url_e2e_chrome.mjs`, output `worker_url_e2e_cft148.json`.

use super::*;
use crate::script_net_tests::serve_routes;
use std::collections::BTreeMap;

const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-14/worker_url_e2e_cft148.json");
const JS: &str = "text/javascript";

fn routes() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("/", "text/html", "<!doctype html><title>w</title>".to_string()),
        ("/w.js", JS, "importScripts('/lib.js'); postMessage('ready:' + libValue); onmessage = function (e) { postMessage('echo:' + e.data); };".to_string()),
        ("/lib.js", JS, "var libValue = 'lib';".to_string()),
        ("/plain.js", "text/plain", "postMessage('ran');".to_string()),
        ("/imports_plain.js", JS, "try { importScripts('/lib_plain.js'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name); }".to_string()),
        ("/lib_plain.js", "text/plain", "var libValue = 'plain';".to_string()),
        ("/image.js", "image/png", "postMessage('ran');".to_string()),
        ("/imports_redirect.js", JS, "importScripts('/redir_lib.js'); postMessage('ready:' + libValue);".to_string()),
        ("/imports_missing.js", JS, "try { importScripts('/missing.js'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name); }".to_string()),
    ]
}

const CASES: &[(&str, &str)] = &[
    ("url_worker_with_import_scripts", "/w.js"),
    ("worker_script_served_as_text_plain", "/plain.js"),
    ("import_scripts_served_as_text_plain", "/imports_plain.js"),
    ("import_scripts_through_a_same_origin_redirect", "/imports_redirect.js"),
    ("import_scripts_of_a_missing_script", "/imports_missing.js"),
    ("worker_script_through_a_same_origin_redirect", "/redir_w.js"),
    // Chrome refuses a worker script of an image, audio, video or text/csv type (Fetch, "should response
    // to request be blocked due to its MIME type"). The Chrome file's `import_scripts_with_two_content_types`
    // (Chrome runs it) needs two Content-Type headers, which this test's server does not send.
    ("worker_script_served_as_image_png", "/image.js"),
];

/// Turn the live loop until the page's `log` holds `want` entries (or 5 s pass), then a few turns more so an
/// extra entry would show. Event-driven: a loaded machine only takes longer, it does not cut a case short.
fn pump_until_logged(engine: &mut Engine, view: EngineViewId, rt: &tokio::runtime::Runtime, want: usize) {
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(5) {
        rt.block_on(engine.pump_live(view, 16));
        if read_log(engine, view).len() >= want {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    for _ in 0..5 {
        rt.block_on(engine.pump_live(view, 16));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn read_log(engine: &mut Engine, view: EngineViewId) -> Vec<String> {
    let out = engine.execute_script(view, "JSON.stringify(log)").expect("log");
    let json = out.strip_prefix("String(").and_then(|s| s.strip_suffix(')')).unwrap_or(&out);
    let text: String = serde_json::from_str(json).unwrap_or_else(|_| json.to_string());
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("log: {out}"))
}

/// What the page logs for a worker started from `path`, once it has heard as many messages as Chrome did.
fn heard(path: &str, want: usize) -> Vec<String> {
    let server = serve_routes(routes());
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    engine
        .execute_script(
            view,
            &format!(
                "window.log = []; var w = new Worker({path:?}); \
                 w.onmessage = function (e) {{ log.push('msg:' + e.data); if (String(e.data).indexOf('ready') === 0) w.postMessage('hi'); }}; \
                 w.onerror = function (e) {{ log.push('error:' + e.constructor.name); }};"
            ),
        )
        .expect("start the worker");
    pump_until_logged(&mut engine, view, &rt, want);
    read_log(&mut engine, view)
}

#[test]
fn url_workers_and_import_scripts_load_through_the_engine_as_in_chrome() {
    let chrome: BTreeMap<String, Vec<String>> = serde_json::from_str(CHROME).unwrap();
    let mut wrong = Vec::new();
    for (name, path) in CASES {
        let got = heard(path, chrome[*name].len());
        if got != chrome[*name] {
            wrong.push(format!("{name}: chrome {:?}, here {got:?}", chrome[*name]));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), CASES.len(), wrong.join("\n"));
}

/// The #1105 follow-ups (R1 at eee9d5c4), vs the pinned Chrome (CfT 148.0.7778.216) on the same routes:
/// `docs/diagnostics/2026-11-14/worker_csp_probe.mjs`, output `worker_csp_cft148.json` (its CSP cases wait for
/// page CSP, which the engine does not apply yet). Two local servers: the page's and another origin's (the
/// same address on another port). Extra response headers ride in the content type, which the test server writes as is.
const FOLLOW_UPS_CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-14/worker_csp_cft148.json");

const FOLLOW_UPS: &[(&str, &str, &str)] = &[
    ("url_worker_no_csp", "url", "/w.js"),
    // Not here: `import_cross_origin_without_cors` / `_with_cors`. The policy's private-network rule lets a
    // loopback page reach only its own origin, so a second local server is out of reach; the no-cors script
    // request is pinned at the policy (`a_no_cors_script_request_hands_the_engine_its_body`).
    ("worker_script_text_plain_nosniff", "url", "/plain_nosniff.js"),
    ("worker_location_after_redirect", "url", "/redir_w.js"),
    ("import_scripts_partial_failure", "url", "/imports_partial.js"),
    ("import_scripts_of_a_blob_url", "blob_import", "/imports_blob.js"),
];

fn follow_up_log(kind: &str, what: &str, want: usize) -> Vec<String> {
    let other = serve_routes(vec![
        ("/lib.js", JS, "self.libValue = 'b';".to_string()),
        ("/lib_cors.js", "text/javascript
Access-Control-Allow-Origin: *", "self.libValue = 'b';".to_string()),
    ]);
    // Another origin: the same address on another port (Chrome's file used localhost).
    let origin_b = format!("http://127.0.0.1:{}", other.port);
    let server = serve_routes(vec![
        ("/", "text/html", "<!doctype html><title>w</title>".to_string()),
        ("/w.js", JS, "postMessage('ran:' + self.location.pathname);".to_string()),
        ("/plain_nosniff.js", "text/plain
X-Content-Type-Options: nosniff", "postMessage('ran');".to_string()),
        ("/imports_cross.js", JS, format!("try {{ importScripts({:?} + '/lib.js'); postMessage('imported:' + self.libValue); }} catch (e) {{ postMessage('import:' + e.name); }}", origin_b)),
        ("/imports_cross_cors.js", JS, format!("try {{ importScripts({:?} + '/lib_cors.js'); postMessage('imported:' + self.libValue); }} catch (e) {{ postMessage('import:' + e.name); }}", origin_b)),
        ("/imports_partial.js", JS, "self.order = []; try { importScripts('/a.js', '/missing.js'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name + ':' + self.order.join()); }".to_string()),
        ("/a.js", JS, "self.order.push('a');".to_string()),
        ("/imports_blob.js", JS, "onmessage = function (e) { try { importScripts(e.data); postMessage('imported:' + self.libValue); } catch (err) { postMessage('import:' + err.name); } };".to_string()),
    ]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    // The probe's page script.
    engine
        .execute_script(
            view,
            &format!(
                "window.log = []; var w = new Worker({what:?});                  if ({kind:?} === 'blob_import') w.postMessage(URL.createObjectURL(new Blob([\"self.libValue = 'blob';\"], {{ type: 'text/javascript' }})));                  w.onmessage = function (e) {{ log.push('msg:' + e.data); }};                  w.onerror = function (e) {{ log.push('error'); e.preventDefault(); }};"
            ),
        )
        .expect("start the worker");
    pump_until_logged(&mut engine, view, &rt, want);
    drop(other);
    read_log(&mut engine, view)
}

#[test]
fn worker_follow_ups_load_and_fail_as_in_chrome() {
    let chrome: BTreeMap<String, Vec<String>> = serde_json::from_str(FOLLOW_UPS_CHROME).unwrap();
    let mut wrong = Vec::new();
    for (name, kind, what) in FOLLOW_UPS {
        let got = follow_up_log(kind, what, chrome[*name].len());
        if got != chrome[*name] {
            wrong.push(format!("{name}: chrome {:?}, here {got:?}", chrome[*name]));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ:
{}", wrong.len(), FOLLOW_UPS.len(), wrong.join("
"));
}
