//! Script elements load the way a browser loads them, however the page
//! gets them into the document.
//!
//! `data:` scripts are decoded where they are, never sent to the network or
//! the shield, and run as classic or module scripts by their `type`: from
//! the parser, inserted by script, or `import()`ed. Until 2026-10-16 each
//! `data:` request went through the request interceptor first, where the
//! shield logged "Failed to create request" for every one.
//!
//! Scripts inserted the ways a challenge page or a loader inserts them are
//! fetched and run: whatever made the element, however its `src` was set,
//! wherever and however it went into the document.

use super::error_page_script_tests::{serve, text, Server};
use super::*;
use std::sync::Mutex;

/// Records every URL the loader asks the interceptor about, and blocks
/// every `data:` one: a `data:` script that runs never reached it.
struct Recorder(Arc<Mutex<Vec<String>>>);

impl rustkit_net::InterceptHandler for Recorder {
    fn intercept(&self, request: &rustkit_net::Request) -> rustkit_net::InterceptAction {
        self.0.lock().unwrap().push(request.url.to_string());
        if request.url.scheme() == "data" {
            rustkit_net::InterceptAction::Block
        } else {
            rustkit_net::InterceptAction::Allow
        }
    }
}

fn recording_engine() -> (Engine, EngineViewId, tokio::runtime::Runtime, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut interceptor = rustkit_net::RequestInterceptor::new();
    interceptor.add_handler(Arc::new(Recorder(seen.clone())));
    let mut engine = Engine::with_interceptor(EngineConfig::default(), Some(interceptor)).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 300, height: 200 })
        .expect("view");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    (engine, view, rt, seen)
}

fn page(body: &'static str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
    (200, "text/html", vec![], body.into())
}

fn paths(server: &Server) -> Vec<String> {
    server.requests().into_iter().map(|(p, _)| p).collect()
}

/// Every way a page hands the engine a `data:` script: the parser's classic
/// and module scripts (percent-encoded, plain and base64), the same inserted
/// by script, and `import()`. Each runs, by its type, and none is a request.
#[test]
fn data_url_scripts_run_without_the_network_or_the_shield() {
    fn site(path: &str, _: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
        match path {
            "/" => page(
                r#"<html><head>
<script>window.log = [];</script>
<script src="data:text/javascript,window.STICKY_CANARY=%22on%22;log.push('canary')"></script>
<script src="data:text/javascript;base64,bG9nLnB1c2goJ2Jhc2U2NCcpOw=="></script>
<script src="data:application/javascript;charset=utf-8,log.push(typeof document.currentScript.src === 'string' ? 'classic' : 'no')"></script>
<script type="module" src="data:text/javascript,let navPromise=null;async function getNavigation(){return navPromise||(navPromise=Promise.resolve('nav'))}window.getNavigation=getNavigation;log.push('module '+(typeof window.getNavigation))"></script>
<script type="module" src="data:text/javascript;base64,ZXhwb3J0IGNvbnN0IHg9MTsgbG9nLnB1c2goJ21vZHVsZTY0Jyk7"></script>
<script src="data:text/plain,log.push('plain classic')"></script>
</head><body><script>
var a = document.createElement('script');
a.src = 'data:text/javascript,log.push(%22inserted%20classic%20%22%2B(document.currentScript%3D%3D%3Da))';
a.onload = function () { log.push('load classic'); };
document.head.appendChild(a);
var m = document.createElement('script');
m.type = 'module';
m.src = 'data:text/javascript,log.push("inserted module " + (document.currentScript === null))';
m.onload = function () { log.push('load module'); };
document.head.appendChild(m);
var b = document.createElement('script');
b.setAttribute('src', 'data:text/javascript;base64,' + btoa("log.push('inserted base64')"));
document.body.appendChild(b);
import('data:text/javascript,export default 42').then(function (ns) { log.push('import ' + ns.default); },
  function (e) { log.push('import failed ' + e); });
import('data:text/plain,export default 1').then(function () { log.push('plain module ran'); },
  function () { log.push('plain module refused'); });
</script></body></html>"#,
            ),
            _ => (404, "text/plain", vec![], "".into()),
        }
    }
    let server = serve(site);
    let (mut engine, view, rt, intercepted) = recording_engine();
    rt.block_on(engine.load_url(view, server.url("/"))).expect("load");
    rt.block_on(engine.pump_live(view, 100));
    let log = text(&mut engine, view, "log.join(' | ')");
    for want in [
        "canary",
        "base64",
        "classic",
        "plain classic",
        "module function",
        "module64",
        "inserted classic true",
        "load classic",
        "inserted module true",
        "load module",
        "inserted base64",
        "import 42",
        // A module must be served as JavaScript, from data: as from http.
        "plain module refused",
    ] {
        assert!(log.contains(&format!("{want} |")) || log.ends_with(&format!("{want}\")")), "{want:?} missing from {log}");
    }
    assert_eq!(text(&mut engine, view, "String(window.STICKY_CANARY)"), r#"String("on")"#);
    let data: Vec<String> = intercepted.lock().unwrap().iter().filter(|u| u.starts_with("data:")).cloned().collect();
    assert!(data.is_empty(), "data: scripts reached the interceptor: {data:?}");
    assert_eq!(paths(&server), vec!["/"], "no data: script is a request");
}

