//! `document.elementFromPoint` uses the layout hit test once a page is laid out.

use super::*;

fn read(engine: &mut Engine, view: EngineViewId, expr: &str) -> String {
    let value = engine.execute_script(view, &format!("String({expr})")).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

#[test]
fn element_from_point_hits_the_topmost_laid_out_box() {
    let page = "<html><head><style>body{margin:0}#a{position:absolute;left:10px;top:20px;width:100px;height:40px}#b{position:absolute;left:20px;top:30px;width:30px;height:10px}</style></head>\
        <body><div id=a></div><div id=b></div></body></html>";
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 })
        .expect("view");
    engine.load_html(view, page).expect("load");
    assert_eq!(read(&mut engine, view, "document.elementFromPoint(25, 35) && document.elementFromPoint(25, 35).id"), "b");
    assert_eq!(read(&mut engine, view, "document.elementFromPoint(15, 25) && document.elementFromPoint(15, 25).id"), "a");
    assert_eq!(read(&mut engine, view, "document.elementFromPoint(-5, 10)"), "null");
    let chain = read(&mut engine, view, "document.elementsFromPoint(25, 35).map(function (e) { return e.id || e.localName; }).join()");
    assert!(chain.starts_with("b,"), "{chain}");
}
