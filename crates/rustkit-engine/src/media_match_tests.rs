//! `matchMedia` follows the viewport the cascade lays out with, and a
//! resolution query follows the view's device pixel ratio.

use super::*;
use crate::script_net_tests::serve_routes;

fn load_sized(
    page: &str,
    width: u32,
    height: u32,
) -> (
    Engine,
    EngineViewId,
    crate::script_net_tests::Server,
    tokio::runtime::Runtime,
) {
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
    let yielded = rt
        .block_on(engine.load_url_yielding(view, url))
        .expect("load");
    assert_eq!(yielded, LoadYield::Complete);
    (engine, view, server, rt)
}

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine
        .execute_script(view, &format!("String({expr})"))
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

const PAGE: &str = r#"<html><head><style>
body{margin:0}
.hidden{display:none}
#desk{width:10px;height:10px}
#hi{width:10px;height:10px}
@media (min-width: 960px) { #desk{display:block;width:2019px} }
@media (min-resolution: 2dppx) { #hi{width:40px} }
</style></head><body>
<span id=desk class=hidden></span>
<div id=hi></div>
<script>
window.changes = 0;
var q = matchMedia('(min-width: 960px)');
window.q = q;
q.addEventListener('change', function () { window.changes++; });
</script>
</body></html>"#;

#[test]
fn match_media_and_the_cascade_share_the_viewport() {
    let (mut engine, view, _server, _rt) = load_sized(PAGE, 1280, 696);
    assert_eq!(read(&mut engine, view, "window.q.matches"), "true");
    assert_eq!(read(&mut engine, view, "devicePixelRatio"), "1");
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('desk').offsetWidth"
        ),
        "2019"
    );
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('hi').offsetWidth"
        ),
        "10",
        "resolution 2dppx does not match at devicePixelRatio 1"
    );

    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 800,
                height: 696,
            },
        )
        .expect("resize");
    assert_eq!(read(&mut engine, view, "innerWidth"), "800");
    assert_eq!(read(&mut engine, view, "window.q.matches"), "false");
    assert_eq!(read(&mut engine, view, "window.changes"), "1");
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('desk').offsetWidth"
        ),
        "0",
        "the cascade dropped min-width 960px with the same viewport"
    );
    assert_eq!(
        read(
            &mut engine,
            view,
            "visualViewport.width + 'x' + visualViewport.scale"
        ),
        "800x1"
    );

    engine.set_device_pixel_ratio(view, 2.0).expect("dpr");
    assert_eq!(read(&mut engine, view, "devicePixelRatio"), "2");
    assert_eq!(
        read(
            &mut engine,
            view,
            "matchMedia('(min-resolution: 2dppx)').matches"
        ),
        "true"
    );
    assert_eq!(
        read(
            &mut engine,
            view,
            "document.getElementById('hi').offsetWidth"
        ),
        "40",
        "the cascade's resolution query uses the same scale"
    );
    assert_eq!(
        read(&mut engine, view, "window.q.matches"),
        "false",
        "a scale change does not flip a width query"
    );
    assert_eq!(read(&mut engine, view, "window.changes"), "1");
}
