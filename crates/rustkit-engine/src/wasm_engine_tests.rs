//! WASM W1 in the engine: the `webassembly` flag (HIWAVE_WEBASSEMBLY=1, off by
//! default) puts `WebAssembly` in every realm, frames included, and a page that
//! used it is still released when navigated away from (the store holding its
//! JS imports goes with the realm; docs/design/WASM-W1.md, #1440's lesson).

use super::*;
use std::rc::Weak;

/// (import "env" "f" (func (param i32) (result i32))) (export "g" ... calls f)
const IMP: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,6,1,96,1,127,1,127,2,9,1,3,101,110,118,1,102,0,0,3,2,1,0,7,5,1,1,103,0,1,10,8,1,6,0,32,0,16,0,11])";

fn engine(webassembly: bool, nested_contexts: bool) -> (Engine, EngineViewId) {
    let config = EngineConfig { webassembly, nested_contexts, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    (engine, view)
}

/// Off (the default): no `WebAssembly`, as on develop; pages keep their
/// non-wasm path.
#[test]
fn off_by_default_there_is_no_webassembly() {
    let (mut engine, view) = engine(false, false);
    engine.load_html(view, "<html><body>x</body></html>").expect("load");
    assert_eq!(engine.execute_script(view, "typeof WebAssembly").expect("js"), r#"String("undefined")"#);
}

/// On: the page has it, and a frame's realm has its own.
#[test]
fn on_every_realm_has_it_frames_included() {
    let (mut engine, view) = engine(true, true);
    engine.load_html(view, "<html><body><iframe id=f></iframe></body></html>").expect("load");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    for _ in 0..4 {
        rt.block_on(engine.pump_live(view, 20));
    }
    assert_eq!(
        engine
            .execute_script(
                view,
                &format!("var w = document.getElementById('f').contentWindow; \
                 [typeof WebAssembly, w && typeof w.WebAssembly, w && w.WebAssembly !== WebAssembly, \
                  new WebAssembly.Instance(new WebAssembly.Module({IMP}), {{ env: {{ f: x => x + 1 }} }}).exports.g(41)].join('|')")
            )
            .expect("js"),
        r#"String("object|object|true|42")"#
    );
}

/// A page that instantiated a module whose JS import closes over the
/// document is released after navigation (its realm's store, holding that
/// import, goes with the realm).
#[test]
fn a_page_that_used_webassembly_is_released_after_navigation() {
    let (mut engine, view) = engine(true, false);
    engine.load_html(view, "<html><body><p id=p>one</p></body></html>").expect("first load");
    assert_eq!(
        engine
            .execute_script(
                view,
                &format!("var p = document.getElementById('p'); \
                 window.ex = new WebAssembly.Instance(new WebAssembly.Module({IMP}), {{ env: {{ f: x => p.textContent.length + x }} }}).exports; \
                 String(ex.g(1))")
            )
            .expect("js"),
        r#"String("4")"#
    );
    let first: Weak<Document> =
        Rc::downgrade(engine.views.get(&view).and_then(|v| v.document.as_ref()).expect("a document"));
    for _ in 0..3 {
        engine.load_html(view, "<html><body><p>two</p></body></html>").expect("next load");
    }
    boa_gc::force_collect();
    assert_eq!(first.strong_count(), 0, "the replaced document is still held after a collection");
}

/// On, with spares (#1440): a frame a script inserts takes a pre-built spare
/// and has its window inside the insertion; that realm has `WebAssembly` too,
/// as a frame the drain attaches does (every frame has it in Chrome).
#[test]
fn on_a_script_inserted_frame_from_a_spare_has_it() {
    let config = EngineConfig { webassembly: true, nested_contexts: true, post_load_requests_live: true, ..EngineConfig::default() };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    engine.load_html(view, "<html><body><iframe id=magic></iframe></body></html>").expect("load");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    for _ in 0..6 {
        rt.block_on(engine.pump_live(view, 20));
    }
    assert_eq!(
        engine
            .execute_script(
                view,
                "var f = document.createElement('iframe'); document.body.appendChild(f); \
                 [f.contentWindow !== null, f.contentWindow && typeof f.contentWindow.WebAssembly].join()"
            )
            .expect("js"),
        r#"String("true,object")"#
    );
}
