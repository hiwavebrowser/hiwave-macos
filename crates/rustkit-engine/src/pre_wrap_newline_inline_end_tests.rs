//! PRE-WRAP-NEWLINE-AT-INLINE-END: a preserved newline at the end of an inline box ends the line,
//! so the text after the inline starts the next one. RustKit kept the line open: under pre-wrap,
//! `1 2 3<span>\n4 5\n</span>6 7 8` drew `123 / 45678` where Chrome draws `123 / 45 / 678`.
//!
//! Every row is Chrome for Testing 148.0.7778.216's, from
//! `docs/diagnostics/2026-11-27/pre_wrap_newline_inline_end_cft148.json`
//! (`tools/parity_oracle/pre_wrap_newline_inline_end_log.mjs`): each paragraph's lines, as the
//! characters on each (white space dropped), and its height, in Ahem 20px/1 at 300px and 100px.

use super::*;

const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");
const RECEIPT: &str = include_str!("../../../docs/diagnostics/2026-11-27/pre_wrap_newline_inline_end_cft148.json");

/// The lines of each paragraph `#p0`, `#p1`, ... of `body` and the paragraph's height: the painted
/// text runs, white space dropped, grouped by their middle and banded by each paragraph's box.
fn rows_and_heights(style: &str, body: &str, paragraphs: usize) -> Vec<(Vec<String>, f64)> {
    use base64::Engine as _;
    let face = format!(
        "<style>@font-face{{font-family:Ahem;src:url(data:font/ttf;base64,{})}} {style}</style>",
        base64::engine::general_purpose::STANDARD.encode(AHEM)
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, &format!("<!doctype html>{face}{body}")).expect("load_html");
    let file = std::env::temp_dir().join(format!("pre_wrap_newline_{}_{}.json", std::process::id(), body.len()));
    engine.export_layout_json(view, file.to_str().unwrap()).expect("export");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let _ = std::fs::remove_file(&file);
    fn find<'a>(v: &'a serde_json::Value, sel: &str) -> Option<&'a serde_json::Value> {
        if v.get("selector").and_then(|t| t.as_str()) == Some(sel) {
            return Some(v);
        }
        v.get("children")?.as_array()?.iter().find_map(|c| find(c, sel))
    }
    let bands: Vec<(f64, f64)> = (0..paragraphs)
        .map(|i| {
            let b = &find(&json["root"], &format!("#p{i}")).expect("paragraph")["border_box"];
            let y = b["y"].as_f64().unwrap();
            (y, y + b["height"].as_f64().unwrap())
        })
        .collect();
    let list = engine.views[&view].display_list.as_ref().expect("display list");
    let mut runs: Vec<(f64, f32, String)> = list
        .commands
        .iter()
        .filter_map(|c| match c {
            rustkit_layout::DisplayCommand::Text { text, x, y, font_size, .. } => {
                let chars: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                (!chars.is_empty()).then(|| (((*y + *font_size / 2.0) as f64).round(), *x, chars))
            }
            _ => None,
        })
        .collect();
    runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    bands
        .iter()
        .map(|&(top, bottom)| {
            let mut lines: Vec<(f64, String)> = Vec::new();
            for (y, _, chars) in runs.iter().filter(|r| r.0 >= top && r.0 < bottom) {
                match lines.last_mut() {
                    Some((ly, s)) if ly == y => s.push_str(chars),
                    _ => lines.push((*y, chars.clone())),
                }
            }
            (lines.into_iter().map(|(_, s)| s).collect(), bottom - top)
        })
        .collect()
}

/// Every Ahem case of the receipt at `width`: each paragraph's lines and height must be Chrome's.
fn ahem_cases_match_chrome_at(width: u32) {
    let receipt: serde_json::Value = serde_json::from_str(RECEIPT).expect("receipt");
    assert_eq!(receipt["chrome"], "148.0.7778.216");
    let cases: Vec<&serde_json::Value> = receipt["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["face"] == "Ahem" && c["width"] == width)
        .collect();
    assert!(cases.len() >= 9, "{} Ahem cases at {width}px", cases.len());
    let style = format!("body{{margin:0}} p{{margin:0;font:20px/1 Ahem;width:{width}px}}");
    let body: String = cases
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!("<p id=p{i} style=\"white-space:{}\">{}</p>", c["white_space"].as_str().unwrap(), c["markup"].as_str().unwrap())
        })
        .collect();
    let got = rows_and_heights(&style, &body, cases.len());
    let wrong: Vec<String> = cases
        .iter()
        .zip(&got)
        .filter_map(|(c, (rows, height))| {
            let chrome_rows: Vec<String> =
                c["rows"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect();
            let chrome_height = c["height"].as_f64().unwrap();
            (rows != &chrome_rows || (height - chrome_height).abs() >= 0.1).then(|| {
                format!("{}: Chrome {chrome_rows:?} {chrome_height}, got {rows:?} {height}", c["label"].as_str().unwrap())
            })
        })
        .collect();
    assert!(wrong.is_empty(), "Ahem 20px/1 at {width}px:\n{}", wrong.join("\n"));
}

#[test]
fn a_preserved_newline_at_an_inline_end_ends_the_line_at_300px() {
    ahem_cases_match_chrome_at(300);
}

#[test]
fn a_preserved_newline_at_an_inline_end_ends_the_line_at_100px() {
    ahem_cases_match_chrome_at(100);
}
