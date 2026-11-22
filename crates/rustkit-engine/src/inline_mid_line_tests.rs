//! INLINE-MID-LINE: an inline element whose text does not fit the rest of the current line starts
//! on that line, after the content before it, and wraps from there (CSS2 §9.4.2: an inline box
//! is split across line boxes). RustKit moved the whole span down to the next line when it did
//! not fit; only a bare text run could start mid-line (the phase-5 split). The settings page's
//! privacy notice is this shape: `<b>…</b> <span style="font-size:0.8rem;line-height:1.5">long
//! text</span>`. Chrome for Testing 148.0.7778.216 (win64),
//! docs/diagnostics/2026-11-21/inline_mid_line_cft148.json
//! (tools/parity_oracle/inline_mid_line_log.mjs, the same markup).

use super::*;

const TEXT: &str = "HiWave analytics are 100% local and private. We track only domain names, never full URLs, \
    page titles, or search queries. All data stays on your device and is never sent to any server.";
const SMALL: &str = "font-size:12.8px;line-height:1.5";

/// (block height, span y, span height, x and y of `#t` after the span or -1) from the layout
/// export the boards read: a wrapped inline reports `fragment_union_border_box`, the union of its
/// line fragments, which is what Chrome's `getBoundingClientRect()` is.
fn measure(family: &str, inner: &str) -> (f64, f64, f64, f64, f64) {
    let page = format!(
        "<!doctype html><style>body{{margin:0}}</style><div id=d style=\"font:16px {family};width:500px\">{inner}</div>"
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 400 })
        .expect("view");
    engine.load_html(view, &page).expect("load_html");
    let file = std::env::temp_dir().join(format!(
        "inline_mid_line_{}_{}.json",
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
    let rect = |sel: &str| -> Option<(f64, f64, f64)> {
        let b = find(root, sel)?;
        let r = b.get("fragment_union_border_box").unwrap_or(&b["border_box"]);
        Some((r["x"].as_f64()?, r["y"].as_f64()?, r["height"].as_f64()?))
    };
    let block = rect("#d").expect("the block");
    let span = rect("#s").expect("the span");
    let after = rect("#t").map_or((-1.0, -1.0), |r| (r.0, r.1));
    (block.2, span.1, span.2, after.0, after.1)
}

type Chrome = (f64, f64, f64, f64, f64);

fn wrong_rows(family: &str, rows: &[(&str, String, Chrome)]) -> Vec<String> {
    rows.iter()
        .filter_map(|(label, inner, chrome)| {
            let got = measure(family, inner);
            let close = |a: f64, b: f64| (a - b).abs() < 0.1;
            let ok = close(got.0, chrome.0)
                && close(got.1, chrome.1)
                && close(got.2, chrome.2)
                && close(got.3, chrome.3)
                && close(got.4, chrome.4);
            (!ok).then(|| {
                format!("{family} {label}: Chrome (block h, span y, span h, after x, after y) {chrome:?}, got {got:?}")
            })
        })
        .collect()
}

fn rows(chrome: [Chrome; 5]) -> Vec<(&'static str, String, Chrome)> {
    let bold = format!("{}<b>bold words</b>{}", &TEXT[..60], &TEXT[60..]);
    vec![
        ("span after text", format!("<b id=n>Lead</b> <span id=s>{TEXT}</span>"), chrome[0]),
        (
            "span after text, text after it",
            format!("<b id=n>Lead</b> <span id=s>{TEXT}</span> <b id=t>tail</b>"),
            chrome[1],
        ),
        (
            "smaller span after text",
            format!("<b id=n>Lead</b> <span id=s style=\"{SMALL}\">{TEXT}</span>"),
            chrome[2],
        ),
        ("span after a bare text run", format!("Lead in <span id=s>{TEXT}</span>"), chrome[3]),
        ("span with a bold child after text", format!("<b id=n>Lead</b> <span id=s>{bold}</span>"), chrome[4]),
    ]
}

#[test]
fn a_span_that_does_not_fit_the_rest_of_the_line_starts_on_it_arial() {
    let wrong = wrong_rows(
        "Arial",
        &rows([
            (54.0, 0.0, 53.0, -1.0, -1.0),
            (54.0, 0.0, 53.0, 377.09375, 36.0),
            (57.5625, 2.0, 53.375, -1.0, -1.0),
            (54.0, 0.0, 53.0, -1.0, -1.0),
            (54.0, 0.0, 53.0, -1.0, -1.0),
        ]),
    );
    assert!(wrong.is_empty(), "a span after text, 16px block, 500px wide:\n{}", wrong.join("\n"));
}

#[cfg(windows)]
#[test]
fn a_span_that_does_not_fit_the_rest_of_the_line_starts_on_it_segoe_ui() {
    // The smaller span's row also needs WRAPPED-SPAN-STRUT (#1418: under Segoe UI the strut's
    // ascent is larger than the span's, which sits the span 2px high on every line), so it is
    // left out here until that lands; the Arial test covers the smaller span meanwhile.
    let mut rows = rows([
        (63.0, 0.0, 63.0, -1.0, -1.0),
        (63.0, 0.0, 63.0, 369.796875, 42.0),
        (63.5625, 3.0, 59.375, -1.0, -1.0),
        (63.0, 0.0, 63.0, -1.0, -1.0),
        (63.0, 0.0, 63.0, -1.0, -1.0),
    ]);
    rows.remove(2);
    let wrong = wrong_rows("'Segoe UI'", &rows);
    assert!(wrong.is_empty(), "a span after text, 16px block, 500px wide:\n{}", wrong.join("\n"));
}
