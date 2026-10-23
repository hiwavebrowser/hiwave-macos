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

/// Loads `page` in a headless view of `width` x `height` and returns the
/// `offsetHeight` of `#k1`..`#k{count}`.
fn offset_heights(page: &str, width: u32, height: u32, count: usize) -> Vec<String> {
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width,
            height,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    (1..=count)
        .map(|k| {
            let expr = format!("String(document.getElementById('k{k}').offsetHeight)");
            let value = engine.execute_script(view, &expr).unwrap();
            value
                .strip_prefix("String(\"")
                .and_then(|v| v.strip_suffix("\")"))
                .unwrap_or(&value)
                .to_string()
        })
        .collect()
}

#[test]
fn container_rules_apply_where_their_query_container_matches() {
    let heights = offset_heights(PAGE, 400, 200, 6);
    // k1: named, 300 >= 200. k2: the nearest container; an inline-size one
    // has no height to ask. k3: 300 < 400. k4: an auto width less padding.
    // k5: no container. k6: the nearest container is the 100px one.
    assert_eq!(heights, ["11", "12", "0", "14", "0", "16"]);
}

/// #782 HOLD (Atlas #837): a container whose laid-out width is not its own
/// `width` answered with the style width, so wide-layout rules fired on a
/// narrow box. Same page as docs/diagnostics/2026-10-23/
/// css_container_query_flex.html; Chromium 148 lays `.s` at 300px, `.main`
/// at 400px, `.a` and `.gi` at 200px, `.n` at 300px, and applies only #k5's
/// rule.
const LAYOUT_WIDTH_PAGE: &str = r#"<!DOCTYPE html><html><head><style>
body { margin: 0 }
.f { display: flex; width: 300px } .s { width: 500px; min-width: 0; container-type: inline-size }
.f2 { display: flex; width: 600px } .main { width: 100%; container-type: inline-size } .side { width: 200px; flex-shrink: 0 }
.p { position: relative; width: 200px } .w { width: 600px }
.a { position: absolute; top: 0; left: 0; width: 100%; container-type: inline-size }
.g { display: grid; grid-template-columns: 200px; width: 600px } .gi { width: 100%; container-type: inline-size }
.n { flex-shrink: 0; width: 300px; container-type: inline-size }
@container (min-width: 400px) { #k1, #k3, #k4 { height: 11px } }
@container (min-width: 500px) { #k2 { height: 12px } }
@container (min-width: 250px) { #k5 { height: 15px } }
</style></head><body>
<div class="f"><div class="s"><div id="k1"></div></div></div>
<div class="f2"><div class="main"><div id="k2"></div></div><div class="side"></div></div>
<div class="p"><div class="w"><div class="a"><div id="k3"></div></div></div></div>
<div class="g"><div class="gi"><div id="k4"></div></div></div>
<div class="f"><div class="n"><div id="k5"></div></div></div>
</body></html>"#;

#[test]
fn container_query_answers_with_the_laid_out_width_not_the_style_width() {
    let heights = offset_heights(LAYOUT_WIDTH_PAGE, 800, 600, 5);
    // k1: flex shrinks 500 -> 300. k2: 100% of 600 shrinks to 400 beside a
    // 200px sidebar. k3: 100% of the positioned 200px box, not of the 600px
    // parent. k4: 100% of the 200px grid column. k5: flex-shrink:0 keeps 300 (`flex: none` is not parsed yet).
    assert_eq!(heights, ["0", "0", "0", "0", "15"]);
}
