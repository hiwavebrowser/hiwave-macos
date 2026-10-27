//! The page is laid out again between the lifecycle steps (see
//! `Engine::refresh_layout_for_script`), so a handler that runs after a
//! script's writes reads the geometry those writes made.

use super::*;
use crate::script_net_tests::serve_routes;

fn read(engine: &mut Engine, view: EngineViewId, name: &str) -> String {
    let value = engine.execute_script(view, &format!("String(window.{name})")).unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

#[test]
fn lifecycle_handlers_read_geometry_after_the_previous_steps_writes() {
    let page = "<html><head><style>body{margin:0}</style></head><body>\
        <div id=a style='width:100px;height:20px'></div>\
        <script>\
        window.__during = document.getElementById('a').offsetHeight;\
        document.getElementById('a').style.height = '80px';\
        document.addEventListener('DOMContentLoaded', function () {\
          window.__dcl = document.getElementById('a').offsetHeight;\
          document.getElementById('a').style.height = '120px';\
        });\
        window.addEventListener('load', function () {\
          window.__load = document.getElementById('a').offsetHeight;\
          document.getElementById('a').style.height = '160px';\
        });\
        setTimeout(function () {\
          window.__timer = document.getElementById('a').getBoundingClientRect().height;\
        }, 0);\
        </script></body></html>";
    let server = serve_routes(vec![("/", "text/html", page.to_string())]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 200 })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    // Same task as the write: still the old layout (a documented limit).
    assert_eq!(read(&mut engine, view, "__during"), "20");
    assert_eq!(read(&mut engine, view, "__dcl"), "80", "DOMContentLoaded sees the script's write");
    assert_eq!(read(&mut engine, view, "__load"), "120", "load sees the DOMContentLoaded handler's write");
    assert_eq!(read(&mut engine, view, "__timer"), "160", "a timer sees the load handler's write");
}

#[test]
fn a_geometry_read_in_the_same_task_as_a_write_sees_the_write() {
    // cnn's header (#786, Atlas 2026-10-23): hide nav items one at a time
    // and measure the row again after each, until it fits. Browsers lay out
    // on the read (forced layout); here every read gave the first layout's
    // width, so the loop hid every item it could reach. 16 boxes of 60px are
    // 960px; Chrome stops after 8, at 480px.
    let mut items = String::new();
    for i in 0..16 {
        items.push_str(&format!("<span id=i{i} style='display:inline-block;width:60px;height:10px'></span>"));
    }
    let page = format!(
        "<html><head><style>body{{margin:0}}</style></head><body>\
         <div id=row style='display:inline-block;white-space:nowrap'>{items}</div></body></html>"
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 1200, height: 200 })
        .expect("view");
    engine.load_html(view, &page).expect("load_html");
    engine
        .execute_script(
            view,
            "var row = document.getElementById('row'); var n = 0;\
             window.__first = row.offsetWidth;\
             while (row.offsetWidth > 500 && n < 16) { document.getElementById('i' + n).style.display = 'none'; n++; }\
             window.__hidden = n; window.__last = row.getBoundingClientRect().width;",
        )
        .unwrap();
    assert_eq!(read(&mut engine, view, "__first"), "960");
    assert_eq!(read(&mut engine, view, "__hidden"), "8", "hidden until the row fits, as in Chrome");
    assert_eq!(read(&mut engine, view, "__last"), "480");
    // One forced layout per write that a read followed: the 8 hides.
    assert_eq!(engine.relayout_stats(view).unwrap().forced_layouts, 8);
}

#[test]
fn a_forced_layout_reads_inline_svg_and_a_focused_control_without_a_borrow_panic() {
    // Prometheus (#808 review): the build borrows svg_cache, rule_reads,
    // the pointer chains and edit_states; a forced layout from script must
    // find none of them held. An inline <svg> fills svg_cache, a focused
    // control with a script-set value has an edit state.
    let page = "<html><head><style>body{margin:0} div:hover{color:red}</style></head><body>\
        <svg width=40 height=40><rect width=40 height=40 fill=red /></svg>\
        <input id=f value=abc style='width:100px'>\
        <div id=b style='width:10px;height:10px'></div><div id=c></div></body></html>";
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 200 })
        .expect("view");
    engine.load_html(view, page).expect("load_html");
    engine.execute_script(view, "document.getElementById('f').focus()").unwrap();
    engine
        .execute_script(
            view,
            "var f = document.getElementById('f'); f.value = 'set by script';\
             var b = document.getElementById('b'); b.style.width = '70px';\
             window.__w = b.offsetWidth; window.__cs = getComputedStyle(b).width;\
             document.getElementById('c').innerHTML = '<svg id=t width=25 height=30><circle r=5 cx=5 cy=5 /></svg>';\
             window.__t = document.getElementById('t').getBoundingClientRect().height;\
             window.__f = document.activeElement === f;",
        )
        .unwrap();
    assert_eq!(read(&mut engine, view, "__w"), "70");
    assert_eq!(read(&mut engine, view, "__cs"), "70px");
    assert_eq!(read(&mut engine, view, "__t"), "30", "an svg the script inserted is laid out");
    assert_eq!(read(&mut engine, view, "__f"), "true");
    assert_eq!(engine.relayout_stats(view).unwrap().forced_layouts, 2);
    // The value the forced layout applied is still the control's after the flush.
    let value = engine.execute_script(view, "document.getElementById('f').value").unwrap();
    assert!(value.contains("set by script"), "{value}");
}

#[test]
fn a_forced_layout_after_a_resize_lays_out_in_the_new_viewport() {
    let page = "<html><head><style>body{margin:0}</style></head><body>\
        <div id=a style='width:50%;height:10px'></div></body></html>";
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 200 })
        .expect("view");
    engine.load_html(view, page).expect("load_html");
    engine.resize_view(view, Bounds { x: 0, y: 0, width: 800, height: 200 }).unwrap();
    engine
        .execute_script(
            view,
            "var a = document.getElementById('a'); a.style.height = '20px';\
             window.__w = a.offsetWidth; window.__h = a.offsetHeight;",
        )
        .unwrap();
    assert_eq!(read(&mut engine, view, "__h"), "20");
    assert_eq!(read(&mut engine, view, "__w"), "400", "half of the 800px viewport");
}
