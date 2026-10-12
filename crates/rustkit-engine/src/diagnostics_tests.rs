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
         #sized{{box-sizing:border-box;width:120px;height:30px;padding:10px;min-height:5vh;max-width:20em}}\
         </style></head><body>\
         <div id=block class=\"a b\">text</div>\
         <div id=flex><div id=f1></div><div id=f2></div></div>\
         <div id=grid><div id=g1></div><div id=g2></div></div>\
         <div id=rel><div id=abs></div></div>\
         <div id=gone><span id=inside>x</span></div>\
         <div id=clip><div id=wide></div></div>\
         <div id=fade><img id=pic alt=\"\" src=\"{GIF}\" width=40 height=30></div>\
         <div id=sized></div>\
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
        ("max-width", "none"), ("min-width", "0px"), ("grid-template-columns", "none"),
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
    // A flex item keeps min-width: auto; a block's computes to 0px.
    assert_eq!(f1["style"]["min-width"], "auto");
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
    // An abspos box's auto sides are its used offsets (200x100 block).
    assert_eq!(r["style"]["inset"], "30px 130px 10px 20px");
}

#[test]
fn a_display_none_element_has_no_box_and_says_why() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#gone");
    assert_eq!(r["box"]["has_box"], false);
    assert_eq!(r["box"]["no_box_reason"], "display:none, or dropped by the box tree builder");
    assert_eq!(r["box"]["border_box"], serde_json::Value::Null);
    // Not "none": the engine cannot tell display:none from a dropped box.
    assert_eq!(r["style"]["display"], serde_json::Value::Null);
    assert!(r["unavailable"]["style.display"].is_string());
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
    // Scrolled to the bottom (the offset clamps to the document's end).
    engine.set_scroll_offset(view, 0.0, 1.0e6).unwrap();
    let scroll = engine.get_scroll_offset(view).unwrap().1;
    let after = diag(&engine, view, "#far");
    assert_eq!(after["paint"]["outside_viewport"], false);
    assert_eq!(after["page"]["scroll"], json(&(0.0f32, scroll)));
    let moved = after["box"]["border_box"]["y"].as_f64().unwrap();
    assert!((y - moved - scroll as f64).abs() < 0.02, "{y} -> {moved}");
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
    // html, head, style, body and the 17 elements in body.
    assert_eq!(a.total_elements, 21);
    assert_eq!(a.elements.len(), 21);
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
    assert_eq!((capped.elements.len(), capped.total_elements, capped.truncated), (3, 21, true));
}

#[test]
fn page_urls_lose_query_and_fragment_unless_asked() {
    let u = Url::parse("https://example.test/a/b?token=1#frag").unwrap();
    assert_eq!(diagnostics::report_url(&u, false), "https://example.test/a/b");
    assert_eq!(diagnostics::report_url(&u, true), "https://example.test/a/b?token=1#frag");
    assert_eq!(diagnostics::scrub_url("https://cdn.test/i.png?sig=x"), "https://cdn.test/i.png");
    assert_eq!(diagnostics::scrub_url("data:image/png;base64,AAAA"), "data:…");
}

#[test]
fn sizes_follow_box_sizing_and_relative_lengths_resolve_to_px() {
    let (engine, view) = engine_with_page();
    let r = diag(&engine, view, "#sized");
    // getComputedStyle reports the border-box size under box-sizing:border-box.
    assert_eq!(r["style"]["width"], "120px");
    assert_eq!(r["style"]["height"], "30px");
    assert_eq!(r["box"]["content_box"]["w"], 100.0);
    assert_eq!(r["style"]["min-height"], "30px"); // 5vh of 600
    assert_eq!(r["style"]["max-width"], "320px"); // 20em at 16px
}

#[test]
fn a_body_background_moved_to_the_canvas_is_unknown_not_none() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    engine
        .load_html(view, "<html><body style=\"background:linear-gradient(red, blue)\"><p id=p>x</p></body></html>")
        .expect("load_html");
    let body = diag(&engine, view, "body");
    assert_eq!(body["style"]["background-image"], serde_json::Value::Null);
    assert_eq!(body["style"]["background-color"], serde_json::Value::Null);
    assert!(body["unavailable"]["style.background-image"].as_str().unwrap().contains("canvas"));
    assert_eq!(diag(&engine, view, "#p")["style"]["background-image"], "none");
}

// ---------------------------------------------------------------------------
// R1 review inputs: fields that were confidently wrong.
// ---------------------------------------------------------------------------

fn engine_with(html: &str, w: u32, h: u32) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: w, height: h }).expect("view");
    engine.load_html(view, html).expect("load_html");
    (engine, view)
}

#[test]
fn percentage_insets_resolve_against_the_containing_block() {
    let (engine, view) = engine_with(
        "<html><body style=\"margin:0\">\
         <div style=\"position:relative;width:400px;height:120px\">\
         <div id=a style=\"position:absolute;top:50%;right:10%;width:40px;height:20px\"></div>\
         <div id=rp style=\"position:relative;top:10%;left:5%;height:10px\"></div></div>\
         <div id=s style=\"top:50%;height:10px\"></div>\
         </body></html>",
        800,
        600,
    );
    let r = diag(&engine, view, "#a");
    assert_eq!(r["box"]["border_box"], rect(320.0, 60.0, 40.0, 20.0));
    assert_eq!(r["style"]["top"], "60px");
    assert_eq!(r["style"]["right"], "40px");
    // auto insets of an abspos box are its used offsets, as getComputedStyle.
    assert_eq!(r["style"]["bottom"], "40px");
    assert_eq!(r["style"]["left"], "320px");
    assert_eq!(r["style"]["inset"], "60px 40px 40px 320px");
    // Relative: % of the containing block; an auto side is the negated opposite.
    let rp = diag(&engine, view, "#rp");
    assert_eq!(rp["style"]["inset"], "12px -20px -12px 20px");
    // A static box's insets do not apply: the computed value, as written.
    let s = diag(&engine, view, "#s");
    assert_eq!(s["style"]["top"], "50%");
    assert_eq!(s["style"]["left"], "auto");
}

#[test]
fn display_is_none_only_when_the_cascade_says_so() {
    let (engine, view) = engine_with(
        "<html><body><span id=e></span><select><option id=o>A</option></select></body></html>",
        800,
        600,
    );
    for sel in ["#e", "#o"] {
        let r = diag(&engine, view, sel);
        if r["box"]["has_box"] == false {
            assert_eq!(r["style"]["display"], serde_json::Value::Null, "{sel}");
            assert!(r["unavailable"]["style.display"].is_string(), "{sel}");
        } else {
            assert_ne!(r["style"]["display"], "none", "{sel}");
        }
    }
}

#[test]
fn z_index_zero_and_auto_are_unknown_not_auto() {
    let (engine, view) = engine_with(
        "<html><body><div id=z style=\"position:absolute;z-index:0\">x</div></body></html>",
        800,
        600,
    );
    let r = diag(&engine, view, "#z");
    assert_eq!(r["style"]["z-index"], serde_json::Value::Null);
    assert!(r["unavailable"]["style.z-index"].as_str().unwrap().contains("auto"));
}

#[test]
fn a_transformed_box_reports_its_bounding_client_rect() {
    let (engine, view) = engine_with(
        "<html><body style=\"margin:8px\">\
         <div id=r style=\"transform:rotate(45deg);width:40px;height:40px\"><div id=c style=\"width:10px;height:10px\"></div></div>\
         <div id=t style=\"transform:translate(5px,6px) scale(2);width:10px;height:10px\"></div>\
         </body></html>",
        800,
        600,
    );
    let r = diag(&engine, view, "#r");
    let bb = &r["box"]["border_box"];
    let near = |v: &serde_json::Value, want: f64| (v.as_f64().unwrap() - want).abs() < 0.02;
    assert!(near(&bb["w"], 56.57) && near(&bb["h"], 56.57), "{bb}");
    assert!(near(&bb["x"], 28.0 - 28.28), "{bb}");
    assert!(near(&bb["y"], 8.0 + 20.0 - 28.28), "{bb}");
    // A rotated padding box is not a rect: unknown, not the untransformed one.
    assert_eq!(r["box"]["padding_box"], serde_json::Value::Null);
    assert!(r["unavailable"]["box.padding_box"].is_string());
    // A descendant is mapped through the ancestor's transform too.
    let c = diag(&engine, view, "#c");
    assert!(near(&c["box"]["border_box"]["w"], 14.14), "{}", c["box"]["border_box"]);
    // Translate and scale keep rects axis-aligned: all three are mapped.
    let t = diag(&engine, view, "#t");
    assert_eq!(t["box"]["border_box"], rect(8.0, 8.0 + 40.0 - 5.0 + 6.0, 20.0, 20.0));
    assert_eq!(t["box"]["content_box"], rect(8.0, 49.0, 20.0, 20.0));
}

#[test]
fn grid_templates_that_are_not_all_px_are_unknown() {
    let (engine, view) = engine_with(
        "<html><body style=\"margin:0\">\
         <div id=g style=\"display:grid;width:400px;grid-template-columns:100px 1fr 2fr\"><div>a</div></div>\
         <div id=p style=\"display:grid;grid-template-columns:100px 200px;grid-template-rows:30px\"><div>a</div></div>\
         <div id=b style=\"height:10px;grid-template-columns:1fr 2fr\"></div>\
         </body></html>",
        800,
        600,
    );
    let g = diag(&engine, view, "#g");
    assert_eq!(g["style"]["grid-template-columns"], serde_json::Value::Null);
    assert!(g["unavailable"]["style.grid-template-columns"].is_string());
    // No row template: Chromium reports the implicit tracks, which the engine does not keep.
    assert_eq!(g["style"]["grid-template-rows"], serde_json::Value::Null);
    let p = diag(&engine, view, "#p");
    assert_eq!(p["style"]["grid-template-columns"], "100px 200px");
    assert_eq!(p["style"]["grid-template-rows"], "30px");
    // Not a grid container: getComputedStyle reports the computed (specified) list.
    let b = diag(&engine, view, "#b");
    assert_eq!(b["style"]["grid-template-columns"], "1fr 2fr", "{}", b["unavailable"]);
}

#[test]
fn a_point_outside_the_viewport_hits_nothing() {
    let (engine, view) = engine_with(
        "<html><body style=\"margin:0\"><div style=\"height:2000px\">tall</div></body></html>",
        800,
        800,
    );
    for (x, y) in [(10.0, 900.0), (-5.0, -5.0), (5000.0, 10.0), (10.0, 800.0)] {
        let r = engine.element_diagnostics(view, &DiagnosticTarget::Point { x, y }, &opts()).unwrap();
        assert!(r.is_none(), "({x},{y}) hit {:?}", r.map(|r| r.identity.selector));
    }
    assert!(engine.element_diagnostics(view, &DiagnosticTarget::Point { x: 10.0, y: 799.0 }, &opts()).unwrap().is_some());
}

#[test]
fn urls_lose_user_name_and_password() {
    let u = Url::parse("https://user:secret@example.test/a?token=1#f").unwrap();
    assert_eq!(diagnostics::report_url(&u, false), "https://example.test/a");
    assert_eq!(diagnostics::report_url(&u, true), "https://example.test/a?token=1#f");
    assert_eq!(diagnostics::scrub_url("https://u:p@cdn.test/i.png?sig=T#f"), "https://cdn.test/i.png");
    assert_eq!(diagnostics::scrub_url("//u:p@cdn.test/i.png"), "//cdn.test/i.png");
    assert_eq!(diagnostics::scrub_url("img/a@2x.png"), "img/a@2x.png");
}

#[test]
fn every_invalid_selector_is_an_error() {
    let (engine, view) = engine_with_page();
    for sel in ["", "p[", "[", "div:has(", "a[href=\"x]", "p:frobnicate"] {
        let r = engine.element_diagnostics(view, &DiagnosticTarget::Selector(sel.into()), &opts());
        assert!(r.is_err(), "{sel:?} gave {:?}", r.map(|o| o.map(|r| r.identity.selector)));
    }
    for sel in ["[data-x=\"a]b\"]", "div:not(#gone)", "p"] {
        assert!(engine.element_diagnostics(view, &DiagnosticTarget::Selector(sel.into()), &opts()).is_ok(), "{sel}");
    }
}

#[test]
fn a_removed_element_is_not_in_the_dom() {
    let (engine, view) = engine_with_page();
    let raw = diag(&engine, view, "#wide")["identity"]["node_id"].as_u64().unwrap() as usize;
    let doc = engine.views.get(&view).unwrap().document.clone().unwrap();
    doc.get_node(rustkit_dom::NodeId::new(raw)).unwrap().remove_from_parent();
    let r = target(&engine, view, DiagnosticTarget::Node(raw));
    assert_eq!(r["tree"]["connected"], false);
    assert_eq!(r["tree"]["in_dom"], false);
}

#[test]
fn a_box_inside_a_scroll_container_says_element_scroll_is_not_applied() {
    let (engine, view) = engine_with(
        "<html><body><div id=s style=\"overflow:auto;height:50px\"><div id=in style=\"height:200px\">x</div></div>\
         <div id=out style=\"height:10px\"></div></body></html>",
        800,
        600,
    );
    let notes = |sel: &str| diag(&engine, view, sel)["notes"].to_string();
    assert!(notes("#in").contains("scroll"), "{}", notes("#in"));
    assert!(!notes("#out").contains("scroll"), "{}", notes("#out"));
    assert!(!notes("#s").contains("scroll"), "{}", notes("#s"));
}
