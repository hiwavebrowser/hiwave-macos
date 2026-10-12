//! Shadow DOM content renders: style, layout and paint walk the flat tree.
//!
//! `tools/parity_oracle/shadow_tree_cases.json` holds each page and what the
//! pinned Chromium reports for it (`shadow_tree_log.mjs --write`):
//! `chrome_boxes`, `id:x:y:width:height` of every element with an id, the
//! document's first and then each shadow tree's (an element that renders no
//! box reads 0:0:0:0); `chrome_hits`, the innermost element under each
//! probe point. The pages size every box in px, so no box depends on font
//! metrics and the test runs on any platform.

use super::*;

/// The expression `shadow_tree_log.mjs` evaluates in Chromium (`BOXES`).
const BOXES: &str = "(function () {\
    var out = [];\
    function walk(root) {\
      Array.prototype.forEach.call(root.querySelectorAll('[id]'), function (e) {\
        var r = e.getBoundingClientRect();\
        out.push(e.id + ':' + Math.round(r.left) + ':' + Math.round(r.top) + ':' + Math.round(r.width) + ':' + Math.round(r.height));\
      });\
      Array.prototype.forEach.call(root.querySelectorAll('*'), function (e) { if (e.shadowRoot) walk(e.shadowRoot); });\
    }\
    walk(document);\
    return out.join(' ');\
  })()";

/// Boxes the engine does not place where Chromium does yet, by case. An id
/// listed here that starts to match Chromium fails the test, so the list
/// cannot go stale.
///
/// All of them are the acceptance page's shadow `<style>` (`:host` padding
/// and border, `::slotted(.item)` margin): shadow-tree styles are not
/// applied yet (style scoping is the next slice).
const GAPS: &[(&str, &[&str])] = &[
    (
        "acceptance",
        &[
            "card", "a", "b", "d", "bottom", "hdr", "titlebox", "main", "fb2",
        ],
    ),
    (
        "acceptance-mutated",
        &[
            "card", "a", "b", "d", "bottom", "hdr", "titlebox", "main", "added", "fb2",
        ],
    ),
];

struct Rendered {
    engine: Engine,
    view: EngineViewId,
}

impl Rendered {
    fn load(html: &str, script: Option<&str>, viewport: (u32, u32)) -> Self {
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine
            .create_headless_view(Bounds::new(0, 0, viewport.0, viewport.1))
            .expect("headless view");
        engine.load_html(view, html).expect("load_html");
        // `load_html` runs no page script: run the inline ones now, in
        // order. Every case keeps its scripts at the end of the body, so
        // this is when Chromium's parser runs them too.
        let mut rest = html;
        while let Some(start) = rest.find("<script>") {
            let body = &rest[start + "<script>".len()..];
            let end = body.find("</script>").expect("</script>");
            engine
                .execute_script(view, &body[..end])
                .expect("page script");
            rest = &body[end..];
        }
        if let Some(script) = script {
            engine.execute_script(view, script).expect("case script");
        }
        Rendered { engine, view }
    }

    fn js(&mut self, script: &str) -> String {
        let value = self
            .engine
            .execute_script(self.view, script)
            .expect("script");
        value
            .strip_prefix("String(\"")
            .and_then(|v| v.strip_suffix("\")"))
            .unwrap_or(&value)
            .replace("\\\"", "\"")
    }

    /// `id:x:y:width:height` by id.
    fn boxes(&mut self) -> Vec<(String, String)> {
        split_boxes(&self.js(BOXES))
    }

    /// The text runs the display list paints, in order.
    fn painted_text(&self) -> Vec<String> {
        let list = self.engine.views[&self.view]
            .display_list
            .as_ref()
            .expect("display list");
        list.commands
            .iter()
            .filter_map(|cmd| match cmd {
                rustkit_layout::DisplayCommand::Text { text, .. } => Some(text.trim().to_string()),
                _ => None,
            })
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// The innermost element the layout hit test finds at (x, y): its id,
    /// else its tag name.
    fn hit(&self, x: f32, y: f32) -> String {
        let view = &self.engine.views[&self.view];
        let layout = view.layout.as_ref().expect("layout");
        let Some(node) = layout.hit_test(x, y).and_then(|h| h.node_id) else {
            return "none".into();
        };
        let document = view.document.as_ref().expect("document");
        let node = document
            .get_node(rustkit_dom::NodeId::new(node))
            .expect("hit node");
        node.get_attribute("id")
            .map(str::to_string)
            .or_else(|| node.tag_name().map(str::to_string))
            .unwrap_or_else(|| "none".into())
    }
}

fn split_boxes(s: &str) -> Vec<(String, String)> {
    s.split_whitespace()
        .map(|b| {
            let (id, rest) = b.split_once(':').expect("id:x:y:w:h");
            (id.to_string(), rest.to_string())
        })
        .collect()
}

fn cases() -> (serde_json::Value, (u32, u32)) {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/parity_oracle/shadow_tree_cases.json"
    ))
    .expect("case file");
    let viewport = (
        data["viewport"][0].as_u64().unwrap() as u32,
        data["viewport"][1].as_u64().unwrap() as u32,
    );
    (data, viewport)
}

fn case<'a>(data: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no case {name}"))
}

fn render(data: &serde_json::Value, name: &str, viewport: (u32, u32)) -> Rendered {
    let c = case(data, name);
    Rendered::load(c["html"].as_str().unwrap(), c["script"].as_str(), viewport)
}

/// Every case's boxes against Chromium's, gaps aside.
#[test]
fn shadow_content_is_laid_out_where_chromium_puts_it() {
    let (data, viewport) = cases();
    let mut wrong = Vec::new();
    let mut gaps_that_pass = Vec::new();
    for c in data["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let chrome = split_boxes(
            c["chrome_boxes"]
                .as_str()
                .expect("run shadow_tree_log.mjs --write"),
        );
        let engine =
            Rendered::load(c["html"].as_str().unwrap(), c["script"].as_str(), viewport).boxes();
        let gaps = GAPS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, g)| *g)
            .unwrap_or(&[]);
        if engine.len() != chrome.len() {
            wrong.push(format!(
                "{name}: engine lists {engine:?}\n   chrome lists {chrome:?}"
            ));
            continue;
        }
        for ((id, want), (engine_id, got)) in chrome.iter().zip(&engine) {
            assert_eq!(id, engine_id, "{name}: the element lists differ in order");
            match (want == got, gaps.contains(&id.as_str())) {
                (false, false) => wrong.push(format!("{name} #{id}: engine {got}, chrome {want}")),
                (true, true) => gaps_that_pass.push(format!("{name} #{id}")),
                _ => {}
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "not where Chromium puts them:\n{}",
        wrong.join("\n")
    );
    assert!(
        gaps_that_pass.is_empty(),
        "listed in GAPS but now match Chromium (remove them): {gaps_that_pass:?}"
    );
}

/// Cases whose hit tests do not match Chromium yet, for the same reason as
/// their `GAPS`: without the shadow `<style>`, the boxes the probes aim at
/// sit 6px higher. A case listed here that matches fails the test.
const HIT_GAPS: &[&str] = &["acceptance", "acceptance-mutated"];

/// The layout hit test finds the innermost flat-tree element Chromium finds:
/// shadow content, and slotted light nodes (not the slot, which has no box).
#[test]
fn the_hit_test_reaches_into_the_flat_tree() {
    let (data, viewport) = cases();
    let mut wrong = Vec::new();
    for c in data["cases"].as_array().unwrap() {
        let Some(probes) = c["probes"].as_array() else {
            continue;
        };
        let name = c["name"].as_str().unwrap();
        let page = render(&data, name, viewport);
        let chrome: Vec<&str> = c["chrome_hits"].as_str().unwrap().split(' ').collect();
        let engine: Vec<String> = probes
            .iter()
            .map(|p| page.hit(p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32))
            .collect();
        match (engine == chrome, HIT_GAPS.contains(&name)) {
            (false, false) => wrong.push(format!("{name}: engine {engine:?}, chrome {chrome:?}")),
            (true, true) => wrong.push(format!("{name} matches Chromium: remove it from HIT_GAPS")),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The display list paints the shadow tree and the assigned nodes, and not
/// the light child no slot takes or the fallback of a slot that has nodes.
#[test]
fn the_display_list_paints_the_flat_tree() {
    let (data, viewport) = cases();
    let page = render(&data, "acceptance", viewport);
    assert_eq!(page.painted_text(), ["H", "A", "B", "D", "none"]);
    let page = render(&data, "acceptance-mutated", viewport);
    assert_eq!(page.painted_text(), ["H", "A", "B", "D", "none"]);
    let page = render(&data, "unassigned-child-not-rendered", viewport);
    assert_eq!(page.painted_text(), ["in"]);
    let page = render(&data, "fallback-content", viewport);
    assert_eq!(page.painted_text(), ["fallback", "given"]);
}

/// A page with a shadow host renders the light DOM unchanged around it, and
/// document queries still do not see into the shadow tree.
#[test]
fn document_queries_still_stop_at_the_shadow_boundary() {
    let (data, viewport) = cases();
    let mut page = render(&data, "acceptance", viewport);
    assert_eq!(
        page.js("[String(document.getElementById('hdr')), document.querySelectorAll('slot, header').length, document.getElementById('card').children.length, document.getElementById('card').innerHTML.indexOf('header')].join()"),
        "null,0,4,-1"
    );
    let mut page = render(&data, "declarative-shadow-root", viewport);
    assert_eq!(
        page.js("var h = document.getElementById('host'); [h.shadowRoot.mode, h.shadowRoot.firstChild.id, h.children.length, h.innerHTML, String(document.getElementById('dsd-top')), document.querySelectorAll('template').length].join()"),
        "open,dsd-top,1,<div id=\"dl\" style=\"height:21px\">light</div>,null,1"
    );
}
