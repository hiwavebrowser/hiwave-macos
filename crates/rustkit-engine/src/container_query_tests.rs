//! `@container` size queries (css-contain-3 §6). Their rules used to be
//! dropped by the parser, so a page's container-specific styles never
//! applied. Heights below are Chromium 148's on this page at 400 x 200.

use super::*;
use crate::script_net_tests::serve_routes;

const PAGE: &str = r#"<!DOCTYPE html><html><head><style>
body { margin: 0 }
.c { container-type: inline-size; width: 300px }
#c1 { container-name: card }
#auto { container: wide / inline-size; padding: 0 50px }
#inner { container-type: inline-size; width: 100px }
@container card (width >= 200px) { #k1 { height: 11px } }
@container (min-width: 250px) { #k2 { height: 12px } }
@container (min-width: 400px) { #k3 { height: 13px } }
@container wide (width = 300px) { #k4 { height: 14px } }
@container (min-width: 1px) { #k5 { height: 15px } }
@container (max-width: 150px) { #k6 { height: 16px } }
@container nope (min-width: 1px) { #k1 { height: 99px } }
@container card (max-width: 150px) { #k6 { height: 99px } }
@container (height > 0px) { #k2 { height: 99px } }
</style></head><body>
<div class="c" id="c1"><div id="k1"></div><div id="inner"><div id="k6"></div></div></div>
<div class="c"><div id="k2"></div></div>
<div class="c"><div id="k3"></div></div>
<div id="auto"><div id="k4"></div></div>
<div id="k5"></div>
</body></html>"#;

#[test]
fn container_rules_apply_where_their_query_container_matches() {
    let server = serve_routes(vec![("/", "text/html", PAGE.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 200 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    let heights: Vec<String> = (1..=6)
        .map(|k| {
            let expr = format!("String(document.getElementById('k{k}').offsetHeight)");
            let value = engine.execute_script(view, &expr).unwrap();
            value
                .strip_prefix("String(\"")
                .and_then(|v| v.strip_suffix("\")"))
                .unwrap_or(&value)
                .to_string()
        })
        .collect();
    // k1: named, 300 >= 200. k2: the nearest container; an inline-size one
    // has no height to ask. k3: 300 < 400. k4: an auto width less padding.
    // k5: no container. k6: the nearest container is the 100px one.
    assert_eq!(heights, ["11", "12", "0", "14", "0", "16"]);
}
