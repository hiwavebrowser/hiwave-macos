//! FLEX-REMEASURE (STRESS-SPEED): a nested flex container's content is laid out once per pass,
//! not once per measuring pass at each flex level. Speedometer's Editor-TipTap page (body flex >
//! column flex > `flex: 1` editor with padding > ProseMirror's paragraphs) laid its 270 KB document
//! out three times per layout: by the block pre-pass and twice by the flex pass, with the same
//! width and height each time (docs/diagnostics/2026-11-24/STRESS-SPEED-editor-tiptap.md). LAT21's
//! memo (`RUSTKIT_LAYOUT_MEMO`) restored only the third: the item's `formatting_root` flips between
//! the pre-pass and the flex pass (it only matters when the subtree has floats), and the content
//! blocks share an empty float context, which the memo refused outright.
//!
//! Geometry: Chrome for Testing 148.0.7778.216 (win64),
//! docs/diagnostics/2026-11-24/flex_remeasure_cft148.json
//! (tools/parity_oracle/flex_remeasure_log.mjs, these pages): every container's border box, with
//! the memo off and on.

use super::*;
use rustkit_layout::layout_memo::{set_memo_min_subtree, set_memo_mode, take_memo_counters, MemoMode};

fn para(n: usize) -> String {
    (0..n)
        .map(|i| format!("<p id=p{i}>Paragraph {i}: {}</p>", "word ".repeat(20 + (i * 7) % 40)))
        .collect()
}

/// (label, css, body, Chrome: (id, x, y, w, h) of each container)
fn pages() -> Vec<(&'static str, String, String, Vec<(&'static str, [f64; 4])>)> {
    vec![
        (
            "editor",
            "html,body{margin:0;height:100%} body{display:flex;padding:5px;box-sizing:border-box;font:13px/1.5 Arial} \
             #app{flex:1;display:flex;flex-direction:column} #controls{display:flex;padding-bottom:5px} \
             #editor{flex:1;overflow:auto;padding:2px 5px} p{margin:0 0 8px}"
                .into(),
            format!(
                "<div id=app><div id=controls><button id=b>Create</button></div><div id=editor><div id=pm>{}</div></div></div>",
                para(30)
            ),
            vec![
                ("app", [5.0, 5.0, 1270.0, 790.0]),
                ("controls", [5.0, 5.0, 1270.0, 26.0]),
                ("editor", [5.0, 31.0, 1270.0, 764.0]),
                ("pm", [10.0, 33.0, 1260.0, 1129.0]),
            ],
        ),
        (
            "row stretch, percent child",
            "body{margin:0;font:13px/1.5 Arial} #row{display:flex;height:200px} #a{flex:1;padding:4px} \
             #b{flex:2;display:flex;flex-direction:column} #b1{height:50%} #b2{flex:1;padding:3px}"
                .into(),
            format!("<div id=row><div id=a>{}</div><div id=b><div id=b1>half</div><div id=b2>{}</div></div></div>", para(3), para(2)),
            vec![
                ("row", [0.0, 0.0, 1280.0, 200.0]),
                ("a", [0.0, 0.0, 432.0, 200.0]),
                ("b", [432.0, 0.0, 848.0, 200.0]),
                ("b1", [432.0, 0.0, 848.0, 96.5]),
                ("b2", [432.0, 96.5, 848.0, 103.5]),
            ],
        ),
        (
            "column flex, basis 0, nested row",
            "body{margin:0;font:13px/1.5 Arial} #col{display:flex;flex-direction:column;height:400px} \
             #top{flex:0 0 auto;padding:6px} #mid{flex:1 1 0;display:flex;min-height:0} #m1{flex:1;overflow:auto;padding:2px} #m2{width:120px}"
                .into(),
            format!("<div id=col><div id=top>Top bar</div><div id=mid><div id=m1>{}</div><div id=m2>side</div></div></div>", para(12)),
            vec![
                ("col", [0.0, 0.0, 1280.0, 400.0]),
                ("top", [0.0, 0.0, 1280.0, 31.5]),
                ("mid", [0.0, 31.5, 1280.0, 368.5]),
                ("m1", [0.0, 31.5, 1160.0, 368.5]),
                ("m2", [1160.0, 31.5, 120.0, 368.5]),
            ],
        ),
        (
            "nested flex with a margin chain",
            "body{margin:0;font:13px/1.5 Arial} #o{display:flex} #i{flex:1;display:flex;flex-direction:column} \
             #k{margin:10px 0} #k p{margin:12px 0}"
                .into(),
            format!("<div id=o><div id=i><div id=k>{}</div></div></div>", para(4)),
            vec![("o", [0.0, 0.0, 1280.0, 177.5]), ("i", [0.0, 0.0, 1280.0, 177.5]), ("k", [0.0, 10.0, 1280.0, 157.5])],
        ),
    ]
}

/// The border boxes of `ids`, and the layout visits of the load, with the memo in `mode`.
fn lay_out(css: &str, body: &str, ids: &[&str], mode: MemoMode) -> (Vec<[f64; 4]>, u64) {
    set_memo_mode(Some(mode));
    set_memo_min_subtree(Some(1));
    take_memo_counters();
    let page = format!("<!doctype html><style>{css}</style>{body}");
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 1280, height: 800 }).expect("view");
    rustkit_layout::take_layout_visits();
    engine.load_html(view, &page).expect("load_html");
    let visits = rustkit_layout::take_layout_visits();
    let file = std::env::temp_dir().join(format!("flex_remeasure_{}_{}.json", std::process::id(), body.len() + css.len()));
    engine.export_layout_json(view, file.to_str().unwrap()).expect("export");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let _ = std::fs::remove_file(&file);
    set_memo_mode(None);
    set_memo_min_subtree(None);
    fn find<'a>(v: &'a serde_json::Value, sel: &str) -> Option<&'a serde_json::Value> {
        if v.get("selector").and_then(|t| t.as_str()) == Some(sel) {
            return Some(v);
        }
        v.get("children")?.as_array()?.iter().find_map(|c| find(c, sel))
    }
    let rects = ids
        .iter()
        .map(|id| {
            let b = &find(&json["root"], &format!("#{id}")).unwrap_or_else(|| panic!("#{id}"))["border_box"];
            [b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap(), b["width"].as_f64().unwrap(), b["height"].as_f64().unwrap()]
        })
        .collect();
    (rects, visits)
}

/// With the memo on, the editor page's content is laid out about once: no more than 1.25 times the
/// layout visits of the same content with no flex box around it.
#[test]
fn with_the_memo_on_a_nested_flex_editor_lays_its_content_out_about_once() {
    let (label, css, body, chrome) = pages().into_iter().next().unwrap();
    let ids: Vec<&str> = chrome.iter().map(|(id, _)| *id).collect();
    let (_, flexed) = lay_out(&css, &body, &ids, MemoMode::On);
    let plain_css = format!("{css} body,#app,#controls{{display:block}}");
    let (_, plain) = lay_out(&plain_css, &body, &ids, MemoMode::On);
    eprintln!("{label}: {flexed} visits in its flex boxes, {plain} with no flex box");
    assert!(
        flexed * 4 <= plain * 5,
        "{label}: {flexed} layout visits under two flex levels, {plain} with none (want at most 1.25x)"
    );
}

/// The memo changes no box: every container is where it is with the memo off, and where Chrome 148
/// puts it.
#[test]
fn nested_flex_pages_lay_out_identically_with_the_memo_off_and_on_and_as_chrome() {
    let mut wrong = Vec::new();
    for (label, css, body, chrome) in pages() {
        let ids: Vec<&str> = chrome.iter().map(|(id, _)| *id).collect();
        let (off, _) = lay_out(&css, &body, &ids, MemoMode::Off);
        let (on, _) = lay_out(&css, &body, &ids, MemoMode::On);
        for (i, (id, want)) in chrome.iter().enumerate() {
            let close = |a: &[f64; 4], b: &[f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.5);
            if !close(&off[i], &on[i]) {
                wrong.push(format!("{label} #{id}: memo off {:?}, on {:?}", off[i], on[i]));
            }
            if !close(&off[i], want) {
                wrong.push(format!("{label} #{id}: Chrome {want:?}, got {:?}", off[i]));
            }
        }
    }
    assert!(wrong.is_empty(), "nested flex pages:\n{}", wrong.join("\n"));
}

/// FLEX-MEMO: NewsSite's shape: a column flex page whose navbar and article list are rows of small
/// flex containers (an icon and a label each), nested in flex at every level. With the memo on, each
/// flex container is laid out about once per pass: NewsSite-Nuxt made ~3,800 flex-container layouts
/// per forced layout for at most ~330 containers, and the memo, which wrapped only block children,
/// saw almost none of them (each container's subtree is small, and the flex algorithm's own
/// recursion bypassed it). Bound: at most 2.25 layouts per container (3.9 before).
#[test]
fn with_the_memo_on_nested_small_flex_rows_are_laid_out_about_once_each() {
    let rows: String = (0..24)
        .map(|i| format!("<li class=r><span class=i>*</span><span class=l>Item {i}</span></li>"))
        .collect();
    let css = "body{margin:0;font:13px/1.5 Arial} #page{display:flex;flex-direction:column} \
               #nav{display:flex;flex-wrap:wrap;gap:4px} ul{display:flex;flex-direction:column;margin:0;padding:0} \
               .r{display:flex;align-items:center;gap:6px} .i{display:flex;width:16px} .l{display:flex;flex:1}";
    let body = format!(
        "<div id=page><div id=nav><ul id=u1>{rows}</ul><ul id=u2>{rows}</ul></div><main id=m><ul id=u3>{rows}</ul></main></div>"
    );
    let page = format!("<!doctype html><style>{css}</style>{body}");
    let containers = 3 + 3 + 3 * 24 * 3;
    for (mode, want) in [(MemoMode::On, true), (MemoMode::Off, false)] {
        set_memo_mode(Some(mode));
        set_memo_min_subtree(None);
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 1280, height: 800 }).expect("view");
        rustkit_layout::take_flex_layouts();
        engine.load_html(view, &page).expect("load_html");
        let layouts = rustkit_layout::take_flex_layouts();
        set_memo_mode(None);
        eprintln!("memo {mode:?}: {layouts} flex-container layouts for {containers} flex containers");
        if want {
            // 3.9 each before FLEX-MEMO (874 for 222). What is left are calls with other inputs: the
            // block pre-pass hands a container another width, or its flow cursor as the height,
            // which the flex algorithm reads (main size, percent heights), so they are new layouts.
            assert!(
                layouts * 4 <= containers * 9,
                "memo on: {layouts} flex-container layouts for {containers} flex containers (want at most 2.25 each)"
            );
        }
    }
}
