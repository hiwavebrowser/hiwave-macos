//! Element diagnostics records on a fixture page (`diagnostics.rs`).

use super::*;

const GIF: &str = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

fn page() -> String {
    format!(
        "<!DOCTYPE html><html><head><style>\
         body{{margin:0;font:16px Arial}}\
         #block{{width:300px;height:40px;margin:10px;padding:5px;border:2px solid black}}\
         #flex{{display:flex;width:400px;height:50px}}\
         #flex > div{{width:100px;flex-grow:1}}\
         #grid{{display:grid;grid-template-columns:100px 200px;width:300px}}\
         #grid > div{{height:30px}}\
         #rel{{position:relative;width:200px;height:100px}}\
         #abs{{position:absolute;left:20px;top:30px;width:50px;height:60px}}\
         #gone{{display:none}}\
         #clip{{overflow:hidden;width:100px;height:20px}}\
         #wide{{width:300px;height:10px}}\
         #far{{height:20px;margin-top:2000px}}\
         #fade{{opacity:0.5}}\
         </style></head><body>\
         <div id=block class=\"a b\">text</div>\
         <div id=flex><div id=f1></div><div id=f2></div></div>\
         <div id=grid><div id=g1></div><div id=g2></div></div>\
         <div id=rel><div id=abs></div></div>\
         <div id=gone><span id=inside>x</span></div>\
         <div id=clip><div id=wide></div></div>\
         <div id=fade><img id=pic alt=\"\" src=\"{GIF}\" width=40 height=30></div>\
         <p id=far>far</p>\
         </body></html>"
    )
}

fn engine_with_page() -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, &page()).expect("load_html");
    (engine, view)
}

fn opts() -> DiagnosticsOptions {
    DiagnosticsOptions::default()
}

/// The record as its JSON text parses back: f32 fields read as written.
fn json<T: serde::Serialize>(r: &T) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(r).unwrap()).unwrap()
}

fn target(engine: &Engine, view: EngineViewId, t: DiagnosticTarget) -> serde_json::Value {
    let r = engine
        .element_diagnostics(view, &t, &opts())
        .expect("diagnostics")
        .unwrap_or_else(|| panic!("no element for {t:?}"));
    json(&r)
}

fn diag(engine: &Engine, view: EngineViewId, sel: &str) -> serde_json::Value {
    target(engine, view, DiagnosticTarget::Selector(sel.into()))
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> serde_json::Value {
    serde_json::json!({ "x": x, "y": y, "w": w, "h": h })
}

const BODY: &str = "html > body:nth-child(2)";

#[test]
fn a_block_records_identity_tree_box_style_and_paint() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#block");
    assert_eq!(r["schema"], "hiwave-element-diagnostics/1");
    let id = &r["identity"];
    assert_eq!(id["tag"], "div");
    assert_eq!(id["id"], "block");
    assert_eq!(id["classes"], serde_json::json!(["a", "b"]));
    assert_eq!(id["selector"], format!("{BODY} > div:nth-child(1)"));
    assert_eq!(id["oracle_key"], "#block");
    assert_eq!(id["ancestry"], serde_json::json!(["body", "html"]));
    assert_eq!(id["child_index"], 0);
    assert_eq!(
        r["tree"],
        serde_json::json!({
            "in_dom": true, "connected": true, "in_shadow_tree": false, "shadow_host": null,
            "assigned_slot": null, "child_count": 0, "text_length": 4
        })
    );
    let b = &r["box"];
    assert_eq!(b["has_box"], true);
    assert_eq!(b["no_box_reason"], serde_json::Value::Null);
    assert_eq!(b["border_box"], rect(10.0, 10.0, 314.0, 54.0));
    assert_eq!(b["padding_box"], rect(12.0, 12.0, 310.0, 50.0));
    assert_eq!(b["content_box"], rect(17.0, 17.0, 300.0, 40.0));
    assert_eq!(b["containing_block"]["selector"], BODY);
    assert_eq!(b["formatting_context"], "block");
    let st = &r["style"];
    for (p, v) in [
        ("display", "block"), ("position", "static"), ("width", "300px"), ("height", "40px"),
        ("margin-top", "10px"), ("padding-left", "5px"), ("border-top-width", "2px"),
        ("box-sizing", "content-box"), ("font-size", "16px"), ("color", "rgb(0, 0, 0)"),
        ("background-image", "none"), ("transform", "none"), ("opacity", "1"), ("inset", "auto"),
        ("max-width", "none"), ("min-width", "auto"), ("grid-template-columns", "none"),
    ] {
        assert_eq!(st[p], v, "style {p}");
    }
    assert_eq!(st.as_object().unwrap().len(), STYLE_PROPERTIES.len());
    assert_eq!(st["contain"], serde_json::Value::Null);
    assert_eq!(r["unavailable"]["style.contain"], "not modelled by the engine");
    assert_eq!(
        r["paint"],
        serde_json::json!({
            "display_commands": null, "command_kinds": null, "clipped": false, "clipped_by": null,
            "outside_viewport": false, "effective_opacity": 1.0, "in_opacity_layer": false,
            "in_transform_layer": false
        })
    );
    assert_eq!(r["unavailable"]["paint.display_commands"], "display-list commands carry no node id");
    let page = &r["page"];
    assert_eq!(page["url"], "about:blank");
    assert_eq!(page["viewport"], serde_json::json!([800, 600]));
    assert_eq!(page["device_scale"], 1.0);
    assert_eq!(page["scroll"], serde_json::json!([0.0, 0.0]));
}

#[test]
fn flex_and_grid_items_name_their_container() {
    let (engine, view) = engine_with_page();
    let f1 = diag(&engine, view, "#f1");
    let flex = format!("{BODY} > div:nth-child(2)");
    assert_eq!(f1["box"]["formatting_context"], format!("flex item of {flex}"));
    assert_eq!(f1["box"]["border_box"], rect(0.0, 74.0, 200.0, 50.0));
    assert_eq!(f1["box"]["containing_block"]["selector"], flex.as_str());
    assert_eq!(f1["style"]["flex-grow"], "1");
    assert_eq!(f1["box"]["grid_lines"], serde_json::Value::Null);
    assert!(f1["unavailable"].get("box.grid_lines").is_none());

    let g2 = diag(&engine, view, "#g2");
    let grid = format!("{BODY} > div:nth-child(3)");
    assert_eq!(g2["box"]["formatting_context"], format!("grid item of {grid}"));
    assert_eq!(g2["box"]["border_box"], rect(100.0, 124.0, 200.0, 30.0));
    assert_eq!(g2["unavailable"]["box.grid_lines"], "grid placement is not kept on the box");
    let container = diag(&engine, view, "#grid");
    assert_eq!(container["style"]["display"], "grid");
    assert_eq!(container["style"]["grid-template-columns"], "100px 200px");
}

#[test]
fn an_abspos_box_names_its_positioned_containing_block() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#abs");
    let rel = format!("{BODY} > div:nth-child(4)");
    assert_eq!(r["box"]["formatting_context"], format!("absolute relative to {rel}"));
    assert_eq!(r["box"]["containing_block"]["selector"], rel.as_str());
    assert_eq!(r["box"]["containing_block"]["content_box"], rect(0.0, 154.0, 200.0, 100.0));
    assert_eq!(r["box"]["border_box"], rect(20.0, 184.0, 50.0, 60.0));
    assert_eq!(r["style"]["position"], "absolute");
    assert_eq!(r["style"]["inset"], "30px auto auto 20px");
}

#[test]
fn a_display_none_element_has_no_box_and_says_why() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#gone");
    assert_eq!(r["box"]["has_box"], false);
    assert_eq!(r["box"]["no_box_reason"], "display:none, or dropped by the box tree builder");
    assert_eq!(r["box"]["border_box"], serde_json::Value::Null);
    assert_eq!(r["style"]["display"], "none");
    assert_eq!(r["style"]["width"], serde_json::Value::Null);
    assert_eq!(r["unavailable"]["style"], "no layout box: the engine keeps computed style only on boxes");
    assert_eq!(r["paint"]["clipped"], serde_json::Value::Null);
    assert_eq!(r["tree"]["child_count"], 1);

    // display is not inherited: the descendant's own display is unknown.
    let inside = diag(&engine, view, "#inside");
    assert_eq!(
        inside["box"]["no_box_reason"],
        format!("an ancestor has no box (nearest boxed ancestor: {BODY})")
    );
    assert_eq!(inside["style"]["display"], serde_json::Value::Null);

    let head = diag(&engine, view, "head");
    assert_eq!(head["box"]["no_box_reason"], "not rendered: head is display:none in the UA sheet");
    assert_eq!(head["identity"]["oracle_key"], "html > head");
}

#[test]
fn html_takes_the_layout_roots_geometry_but_no_style() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "html");
    assert_eq!(r["box"]["has_box"], true);
    assert_eq!(r["box"]["border_box"]["x"], 0.0);
    assert_eq!(r["box"]["border_box"]["w"], 800.0);
    assert_eq!(r["style"]["display"], serde_json::Value::Null);
    assert!(r["unavailable"]["style"].as_str().unwrap().contains("anonymous root"));
    assert_eq!(r["identity"]["ancestry"], serde_json::json!([]));
    assert_eq!(r["identity"]["child_index"], 0);
}

#[test]
fn a_clipped_element_names_its_clipping_ancestor() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#wide");
    let clip = format!("{BODY} > div:nth-child(6)");
    assert_eq!(r["paint"]["clipped"], true);
    assert_eq!(r["paint"]["clipped_by"], clip.as_str());
    assert_eq!(r["box"]["border_box"], rect(0.0, 254.0, 300.0, 10.0));
    // The clipping element itself is not clipped.
    assert_eq!(diag(&engine, view, "#clip")["paint"]["clipped"], false);
}

#[test]
fn an_element_scrolled_out_of_view_is_outside_the_viewport_until_scrolled_to() {
    let (mut engine, view) = engine_with_page();
    let before = diag(&engine, view, "#far");
    assert_eq!(before["paint"]["outside_viewport"], true);
    let y = before["box"]["border_box"]["y"].as_f64().unwrap();
    assert!(y > 2000.0, "{y}");
    engine.set_scroll_offset(view, 0.0, 1720.0).unwrap();
    let after = diag(&engine, view, "#far");
    assert_eq!(after["paint"]["outside_viewport"], false);
    assert_eq!(after["page"]["scroll"], serde_json::json!([0.0, 1720.0]));
    let moved = after["box"]["border_box"]["y"].as_f64().unwrap();
    assert!((y - moved - 1720.0).abs() < 0.02, "{y} -> {moved}");
    assert_eq!(diag(&engine, view, "#block")["paint"]["outside_viewport"], true);
}

#[test]
fn an_image_in_a_faded_parent_records_its_box_and_opacity_chain() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#pic");
    assert_eq!(r["identity"]["tag"], "img");
    assert_eq!(r["box"]["border_box"], rect(0.0, 274.0, 40.0, 30.0));
    assert_eq!(r["box"]["formatting_context"], "inline");
    assert_eq!(r["style"]["opacity"], "1");
    assert_eq!(r["paint"]["effective_opacity"], 0.5);
    assert_eq!(r["paint"]["in_opacity_layer"], true);
    assert_eq!(r["paint"]["in_transform_layer"], false);
}

#[test]
fn a_point_and_a_node_id_find_the_same_element_as_a_selector() {
    let (engine, view) = engine_with_page();
    let by_sel = diag(&engine, view, "#abs");
    let by_point = target(&engine, view, DiagnosticTarget::Point { x: 30.0, y: 200.0 });
    assert_eq!(by_point, by_sel);
    let raw = by_sel["identity"]["node_id"].as_u64().unwrap() as usize;
    assert_eq!(target(&engine, view, DiagnosticTarget::Node(raw)), by_sel);
    // Text is hit on behalf of its element.
    let text = target(&engine, view, DiagnosticTarget::Point { x: 20.0, y: 25.0 });
    assert_eq!(text["identity"]["id"], "block");
}

#[test]
fn no_match_is_none_and_an_invalid_selector_is_an_error() {
    let (engine, view) = engine_with_page();
    let none = engine.element_diagnostics(view, &DiagnosticTarget::Selector("#nope".into()), &opts());
    assert!(none.unwrap().is_none());
    let bad = engine.element_diagnostics(view, &DiagnosticTarget::Selector("p:frobnicate".into()), &opts());
    assert!(bad.is_err());
    let off = engine.element_diagnostics(view, &DiagnosticTarget::Point { x: 5000.0, y: 5.0 }, &opts());
    assert!(off.unwrap().is_none());
}

#[test]
fn the_dump_covers_every_element_is_deterministic_compact_and_capped() {
    let (engine, view) = engine_with_page();
    let a = engine.all_element_diagnostics(view, &opts()).unwrap();
    let b = engine.all_element_diagnostics(view, &opts()).unwrap();
    let text = serde_json::to_string(&a).unwrap();
    assert_eq!(text, serde_json::to_string(&b).unwrap());
    // html, head, style, body and the 16 elements in body.
    assert_eq!(a.total_elements, 20);
    assert_eq!(a.elements.len(), 20);
    assert!(!a.truncated);
    let tags: Vec<&str> = a.elements.iter().take(4).map(|e| e.identity.tag.as_str()).collect();
    assert_eq!(tags, ["html", "head", "style", "body"]);
    for e in &a.elements {
        assert!(e.page.is_none(), "page is hoisted to the dump");
        let len = serde_json::to_string(e).unwrap().len();
        assert!(len < 4096, "{} is {len} bytes", e.identity.selector);
    }
    let single = engine
        .element_diagnostics(view, &DiagnosticTarget::Selector("#abs".into()), &opts())
        .unwrap()
        .unwrap();
    assert!(serde_json::to_string(&single).unwrap().len() < 4096);

    let capped = engine
        .all_element_diagnostics(view, &DiagnosticsOptions { max_elements: 3, ..opts() })
        .unwrap();
    assert_eq!((capped.elements.len(), capped.total_elements, capped.truncated), (3, 20, true));
}

#[test]
fn page_urls_lose_query_and_fragment_unless_asked() {
    let u = Url::parse("https://example.test/a/b?token=1#frag").unwrap();
    assert_eq!(diagnostics::report_url(&u, false), "https://example.test/a/b");
    assert_eq!(diagnostics::report_url(&u, true), "https://example.test/a/b?token=1#frag");
    assert_eq!(diagnostics::scrub_url("https://cdn.test/i.png?sig=x"), "https://cdn.test/i.png");
    assert_eq!(diagnostics::scrub_url("data:image/png;base64,AAAA"), "data:…");
}
