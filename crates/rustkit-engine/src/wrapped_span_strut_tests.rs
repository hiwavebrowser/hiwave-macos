//! WRAPPED-SPAN-STRUT: a span whose text wraps over several lines in a smaller font than its block
//! sits on the strut's baseline on every line. Each line box unites the block's strut with the
//! span's own box on the baseline (SET2 gives the lines that pitch), so a line's baseline is the
//! larger ascent below its top: with a 16px Segoe UI block and a 12.8px span the strut's 17, not
//! the span's 14 + 1 half-leading. RustKit stepped the lines by the right pitch but left the span
//! at its own half-leading, 2px high on every line, and reported its rect 2px high and 1.3px
//! short (the settings page's privacy notice, section 10). Chrome for Testing 148.0.7778.216
//! (win64), docs/diagnostics/2026-11-21/wrapped_span_strut_cft148.json
//! (tools/parity_oracle/wrapped_span_strut_log.mjs, the same markup). Under Arial the two ascents
//! are equal (14), which is why the Arial rows were already right.

use super::*;

const TEXT: &str = "HiWave analytics are 100% local and private. We track only domain names, never full URLs, \
    page titles, or search queries. All data stays on your device and is never sent to any server.";
const SPAN: &str = "font-size:12.8px;line-height:1.5";

/// (block height, span y, span height, neighbour y) for one block, from the layout export the
/// boards read: an inline whose text wrapped reports `fragment_union_border_box`, the union of its
/// line fragments, which is what Chrome's `getBoundingClientRect()` is. (The script-side
/// `getBoundingClientRect()` still answers one line for such an inline; a separate row.)
fn measure(family: &str, inner: &str) -> (f64, f64, f64, f64) {
    let page = format!(
        "<!doctype html><style>body{{margin:0}}</style><div id=d style=\"font:16px {family};width:500px\">{inner}</div>"
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 400 })
        .expect("view");
    engine.load_html(view, &page).expect("load_html");
    let file = std::env::temp_dir().join(format!(
        "wrapped_span_strut_{}_{}.json",
        std::process::id(),
        inner.len() * 7 + family.len()
    ));
    engine.export_layout_json(view, file.to_str().unwrap()).expect("export");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let _ = std::fs::remove_file(&file);
    fn find<'a>(v: &'a serde_json::Value, sel: &str) -> Option<&'a serde_json::Value> {
        if v.get("selector").and_then(|t| t.as_str()) == Some(sel) {
            return Some(v);
        }
        v.get("children")?.as_array()?.iter().find_map(|c| find(c, sel))
    }
    let root = &json["root"];
    let rect = |sel: &str| -> Option<(f64, f64)> {
        let b = find(root, sel)?;
        let r = b.get("fragment_union_border_box").unwrap_or(&b["border_box"]);
        Some((r["y"].as_f64()?, r["height"].as_f64()?))
    };
    let block = rect("#d").expect("the block");
    let span = rect("#s").expect("the span");
    let neighbour = rect("#n").map_or(-1.0, |r| r.0);
    (block.1, span.0, span.1, neighbour)
}

fn check(label: &str, got: (f64, f64, f64, f64), chrome: (f64, f64, f64, f64)) -> Option<String> {
    let close = |a: f64, b: f64| (a - b).abs() < 0.1;
    (!(close(got.0, chrome.0) && close(got.1, chrome.1) && close(got.2, chrome.2) && close(got.3, chrome.3)))
        .then(|| format!("{label}: Chrome (block, span y, span h, neighbour y) {chrome:?}, got {got:?}"))
}

#[cfg(windows)]
#[test]
fn a_wrapped_smaller_span_sits_on_the_struts_baseline_on_every_line() {
    let fam = "'Segoe UI'";
    let alone = format!("<strong style=\"display:block\">Notice</strong><span id=s style=\"{SPAN}\">{TEXT}</span>");
    let normal = format!("<span id=s style=\"font-size:12.8px\">{TEXT}</span>");
    let after = format!("<span id=s style=\"{SPAN}\">{TEXT}</span> <b id=n>tail</b>");
    let wrong: Vec<String> = [
        ("span alone after a block", &alone, (84.5625, 24.0, 59.375, -1.0)),
        ("span alone, normal line-height", &normal, (63.0, 3.0, 59.0, -1.0)),
        ("text after the span", &after, (63.5625, 3.0, 59.375, 42.375)),
    ]
    .iter()
    .filter_map(|(label, inner, chrome)| check(label, measure(fam, inner), *chrome))
    .collect();
    assert!(wrong.is_empty(), "Segoe UI block, 12.8px span:\n{}", wrong.join("\n"));
}

/// The control: under Arial the strut's ascent and the span's are equal, and develop was right.
#[test]
fn a_wrapped_smaller_span_with_the_struts_ascent_is_unchanged() {
    let alone = format!("<strong style=\"display:block\">Notice</strong><span id=s style=\"{SPAN}\">{TEXT}</span>");
    let after = format!("<span id=s style=\"{SPAN}\">{TEXT}</span> <b id=n>tail</b>");
    let wrong: Vec<String> = [
        ("span alone after a block", &alone, (75.5625, 20.0, 53.375, -1.0)),
        ("text after the span", &after, (57.5625, 2.0, 53.375, 38.375)),
    ]
    .iter()
    .filter_map(|(label, inner, chrome)| check(label, measure("Arial", inner), *chrome))
    .collect();
    assert!(wrong.is_empty(), "Arial block, 12.8px span:\n{}", wrong.join("\n"));
}
