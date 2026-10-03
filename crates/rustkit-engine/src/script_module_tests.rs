//! Module scripts through a real page load (Z package C0, slice 2):
//! `<script type=module>` runs through the module host, its graph is
//! fetched under the page's `FetchPolicy::fetch_module`, and the script
//! element hears `load` or `error`.

use super::*;
use crate::script_net_tests::serve_routes;

type Routes = Vec<(&'static str, &'static str, String)>;

fn load(routes: Routes) -> (Engine, EngineViewId, crate::script_net_tests::Server) {
    let server = serve_routes(routes);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
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

fn log(engine: &mut Engine, view: EngineViewId) -> String {
    let value = engine.execute_script(view, "log.join(',')").unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

const JS: &str = "text/javascript";

fn page(body: &str) -> String {
    format!("<html><head><script>var log = [];</script>{body}</head><body>hi</body></html>")
}

/// The reduced case: a page whose only code is a module. Today the script
/// log says "type=module unsupported" and nothing runs.
#[test]
fn an_external_module_and_its_imports_run() {
    let (mut engine, view, _server) = load(vec![
        ("/", "text/html", page(r#"<script type="module" src="/main.js"></script>"#)),
        ("/main.js", JS, "import { b } from './lib/b.js'; log.push('main:' + b);".into()),
        ("/lib/b.js", JS, "import './c.js'; export const b = 'B'; log.push('b');".into()),
        ("/lib/c.js", JS, "log.push('c');".into()),
    ]);
    assert_eq!(log(&mut engine, view), "c,b,main:B");
    let records = engine.script_log(view).unwrap();
    assert!(
        records.iter().all(|r| !matches!(r.outcome, ScriptOutcome::Skipped(_))),
        "{records:#?}"
    );
}
