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
/// cannot go stale. Empty since style scoping (slice 2): the acceptance
/// page's shadow `<style>` applies.
const GAPS: &[(&str, &[&str])] = &[];

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

/// Cases whose hit tests do not match Chromium yet, as `GAPS`. A case
/// listed here that matches fails the test.
const HIT_GAPS: &[&str] = &[];

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

// ---- Slice 2: style scoping. Each case below is one page of
// shadow_tree_cases.json; the full-file test above checks them all, these
// name the behaviour each one pins.

/// One case's boxes against Chromium's, every id.
fn assert_case_matches_chromium(name: &str) {
    let (data, viewport) = cases();
    let c = case(&data, name);
    let chrome = split_boxes(c["chrome_boxes"].as_str().expect("chrome_boxes"));
    let engine = render(&data, name, viewport).boxes();
    assert_eq!(engine, chrome, "{name}: engine boxes (left) against Chromium's (right)");
}

/// A shadow `<style>` styles its own tree only: the document's `<p>`, and an
/// id the sheet names that exists only in the document, are untouched.
#[test]
fn a_shadow_style_does_not_leak_out() {
    assert_case_matches_chromium("scoped-style-does-not-leak-out");
}

/// Document rules (`.x{display:none}`, `slot{display:block}`, `p{margin:0}`,
/// `div{height}`) do not match inside a shadow tree.
#[test]
fn document_styles_do_not_leak_in() {
    assert_case_matches_chromium("document-style-does-not-leak-in");
}

/// `#ghost` in the document names an id that exists only in a shadow tree:
/// it matches nothing.
#[test]
fn a_document_rule_naming_a_shadow_id_does_not_match() {
    assert_case_matches_chromium("document-rule-naming-a-shadow-id");
}

/// Inherited properties cross the boundary: host -> shadow tree, and the
/// slot's flat-tree parent -> the slotted node (font-size and colour).
#[test]
fn inherited_properties_cross_into_the_shadow_tree() {
    assert_case_matches_chromium("inheritance-into-the-shadow-tree");
}

/// `:host` and `:host(.wide)` style the host from inside; an outer rule on
/// the host beats either whatever its specificity; `:host(.wide) .lab`,
/// `:host .lab` and `:host > .lab` match inside; `div .lab` does not reach
/// past the shadow root.
#[test]
fn host_rules_with_and_without_an_argument() {
    assert_case_matches_chromium("host-rules");
}

/// `::slotted(p)` styles assigned nodes only (not a `<p>` inside one), and a
/// light-DOM rule beats it whatever their specificities.
#[test]
fn slotted_rules_lose_to_light_rules() {
    assert_case_matches_chromium("slotted-rules");
}

/// Two nested shadow trees, each with its own `<style>`: each sheet styles
/// its own tree, and the inner `:host` styles the inner host.
#[test]
fn nested_shadow_trees_each_use_their_own_styles() {
    assert_case_matches_chromium("nested-shadow-styles");
}

/// `:defined` / `:not(:defined)`: a custom element is defined once
/// `customElements.define` ran for its name; one never defined is not;
/// every other element is.
#[test]
fn defined_follows_the_custom_element_registry() {
    assert_case_matches_chromium("defined-pseudo-class");
}

/// The text colour and background of every element with a box, in the
/// cases that set `colors`, against Chromium's `getComputedStyle`.
#[test]
fn scoped_colours_match_chromium() {
    let (data, viewport) = cases();
    let mut wrong = Vec::new();
    for c in data["cases"].as_array().unwrap() {
        let Some(chrome) = c["chrome_colors"].as_str() else {
            continue;
        };
        let name = c["name"].as_str().unwrap();
        let page = render(&data, name, viewport);
        for entry in chrome.split(' ') {
            let (id, want) = entry.split_once(':').unwrap();
            if want == "-" {
                continue;
            }
            let got = page.colors_of(id);
            if got != want {
                wrong.push(format!("{name} #{id}: engine {got}, chrome {want}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A `<style>` added to a shadow root after the first layout restyles that
/// tree (and only it); removing it again restores the earlier style.
#[test]
fn adding_and_removing_a_shadow_style_after_layout_restyles() {
    let html = "<!doctype html><html><head><style>body{margin:0;font:16px/20px monospace}</style></head><body><div id=h></div><div id=light>l</div><script>document.getElementById('h').attachShadow({mode:'open'}).innerHTML = '<div id=t>x</div>';</script></body></html>";
    let mut page = Rendered::load(html, None, (800, 600));
    let boxes = |page: &mut Rendered| {
        page.boxes()
            .into_iter()
            .map(|(id, b)| format!("{id}:{b}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(boxes(&mut page), "h:0:0:800:20 light:0:20:800:20 t:0:0:800:20");
    page.js("var s = document.createElement('style'); s.id = 'sheet'; s.textContent = 'div{height:12px}'; document.getElementById('h').shadowRoot.appendChild(s); 'ok'");
    assert_eq!(boxes(&mut page), "h:0:0:800:12 light:0:12:800:20 t:0:0:800:12 sheet:0:0:0:0");
    page.js("document.getElementById('h').shadowRoot.getElementById('sheet').textContent = 'div{height:14px}'; 'ok'");
    assert_eq!(boxes(&mut page), "h:0:0:800:14 light:0:14:800:20 t:0:0:800:14 sheet:0:0:0:0");
    page.js("document.getElementById('h').shadowRoot.getElementById('sheet').remove(); 'ok'");
    assert_eq!(boxes(&mut page), "h:0:0:800:20 light:0:20:800:20 t:0:0:800:20");
}

impl Rendered {
    /// `color/background` of the element with this id (shadow trees
    /// included) as Chromium serializes them, from the box the engine laid
    /// out for it; `-` when it has none.
    fn colors_of(&self, id: &str) -> String {
        let view = &self.engine.views[&self.view];
        let document = view.document.as_ref().expect("document");
        let mut node = None;
        document.traverse_shadow_including(|n| {
            if node.is_none() && n.get_attribute("id") == Some(id) {
                node = Some(n.id.raw());
            }
        });
        let Some(node) = node else {
            return "no element".into();
        };
        fn find(b: &rustkit_layout::LayoutBox, node: usize) -> Option<&rustkit_layout::LayoutBox> {
            if b.node_id == Some(node) {
                return Some(b);
            }
            b.children.iter().find_map(|c| find(c, node))
        }
        let Some(b) = find(view.layout.as_ref().expect("layout"), node) else {
            return "-".into();
        };
        fn css(c: &rustkit_css::Color) -> String {
            if c.a >= 1.0 {
                format!("rgb({},{},{})", c.r, c.g, c.b)
            } else {
                format!("rgba({},{},{},{})", c.r, c.g, c.b, c.a)
            }
        }
        format!("{}/{}", css(&b.style.color), css(&b.style.background_color))
    }
}

/// Whether each element with an id is painted, against Chromium's
/// `chrome_painted`, in the cases that set `painted` (the component page:
/// custom elements defined with and without shadow roots, never defined,
/// declarative roots with and without slots, `:host` opacity and
/// visibility, `::slotted` visibility, `:not(:defined)` rules). An element
/// is painted when the display list fills its background colour; each
/// element of such a case has its own.
#[test]
fn the_display_list_paints_what_chromium_paints() {
    let (data, viewport) = cases();
    let mut wrong = Vec::new();
    for c in data["cases"].as_array().unwrap() {
        let Some(chrome) = c["chrome_painted"].as_str() else {
            continue;
        };
        let name = c["name"].as_str().unwrap();
        let page = render(&data, name, viewport);
        let painted = page.painted_colours();
        for entry in chrome.split(' ') {
            let (id, want) = entry.split_once(':').unwrap();
            let background = page.colors_of(id);
            let got = match background.split_once('/') {
                Some((_, bg)) => {
                    let rgb = bg
                        .trim_start_matches("rgba(")
                        .trim_start_matches("rgb(")
                        .trim_end_matches(')');
                    let rgb: Vec<&str> = rgb.split(',').take(3).collect();
                    match painted.contains(&rgb.join(",")) {
                        true => "P",
                        false => "-",
                    }
                }
                None => "-",
            };
            if got != want {
                wrong.push(format!("{name} #{id}: engine {got}, chrome {want} ({background})"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---- A definition that never arrives (macOS receipt, microsoft.com).

/// A header custom element with light children, as on microsoft.com: a
/// skip link, a banner element and a `header` with a logo. Its sheet hides
/// it until it is defined; the page's script defines it. Chromium runs the
/// script (after a first one that runs for 400 ms) and lays it out at
/// `wrap:0:0:800:55 skip:0:0:1:1 hd:0:1:800:54 logo:79:17:108:23
/// main:0:55:800:100`, all painted (Chromium 141).
const UNDEFINED_HEADER: &str = "<!doctype html><html><head><style>body{margin:0;font:16px/20px monospace} \
    x-hdr:not(:defined){display:block;height:54px;visibility:hidden} x-hdr{display:block}</style></head><body>\
    <div id=wrap style=\"background:rgb(1,2,3)\"><x-hdr id=hdr>\
    <a id=skip href=#main style=\"display:block;height:1px;width:1px;background:rgb(10,20,30)\"></a>\
    <x-banner id=ban></x-banner>\
    <header id=hd style=\"height:54px;padding-top:16px;box-sizing:border-box;background:rgb(40,50,60)\">\
    <div id=logo style=\"width:108px;height:23px;margin-left:79px;background:rgb(70,80,90)\"></div></header>\
    </x-hdr></div><main id=main style=\"height:100px;background:rgb(100,110,120)\"></main>\
    <script>var t = Date.now(); while (Date.now() - t < 400) {}</script>\
    <script>customElements.define('x-hdr', class extends HTMLElement {});</script></body></html>";

/// Loads `html` through the navigation path (page scripts, load event),
/// with `budget_ms` for the page's scripts.
fn navigate(html: &str, budget_ms: u64) -> Rendered {
    let server = crate::script_net_tests::serve_routes(vec![("/", "text/html", html.to_string())]);
    let config = EngineConfig {
        script_budget_ms: budget_ms,
        ..EngineConfig::default()
    };
    let mut engine = Engine::new(config).expect("engine");
    let view = engine
        .create_headless_view(Bounds::new(0, 0, 800, 600))
        .expect("headless view");
    let url = url::Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(engine.load_url(view, url)).expect("load_url");
    Rendered { engine, view }
}

impl Rendered {
    /// The solid colours the display list fills, as `r,g,b`.
    fn painted_colours(&self) -> Vec<String> {
        let list = self.engine.views[&self.view]
            .display_list
            .as_ref()
            .expect("display list");
        list.commands
            .iter()
            .filter_map(|cmd| match cmd {
                rustkit_layout::DisplayCommand::SolidColor(c, _) => {
                    Some(format!("{},{},{}", c.r, c.g, c.b))
                }
                _ => None,
            })
            .collect()
    }
}

/// The page's defining script did not run in the engine (here: the first
/// script spent the page's 200 ms script budget; on a live page also a
/// script that fails or is skipped), so the header's
/// definition never arrives. Once the page's scripts are over, its
/// `:not(:defined)` rule must not keep the header's children unpainted:
/// they are laid out where Chromium puts them, and painted.
#[test]
fn light_children_of_an_element_whose_definition_never_arrives_are_painted() {
    let mut page = navigate(UNDEFINED_HEADER, 200);
    assert_eq!(
        page.js("String(customElements.get('x-hdr'))"),
        "undefined",
        "the page's script was meant not to run"
    );
    let boxes: Vec<String> = page
        .boxes()
        .into_iter()
        .filter(|(id, _)| id != "hdr" && id != "ban")
        .map(|(id, b)| format!("{id}:{b}"))
        .collect();
    let painted = page.painted_colours();
    let unpainted: Vec<&str> = [
        ("wrap", "1,2,3"),
        ("skip", "10,20,30"),
        ("hd", "40,50,60"),
        ("logo", "70,80,90"),
        ("main", "100,110,120"),
    ]
    .into_iter()
    .filter(|(_, colour)| !painted.iter().any(|c| c == colour))
    .map(|(id, _)| id)
    .collect();
    assert!(
        unpainted.is_empty() && boxes.join(" ") == "wrap:0:0:800:55 skip:0:0:1:1 hd:0:1:800:54 logo:79:17:108:23 main:0:55:800:100",
        "laid out at {boxes:?}, against Chromium's wrap:0:0:800:55 skip:0:0:1:1 hd:0:1:800:54 logo:79:17:108:23 main:0:55:800:100;\n\
         not painted: {unpainted:?}"
    );
}
