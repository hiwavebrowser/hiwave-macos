//! LETTER-SPACING-BREAK follow-ups (#1491 R1 nits 2 and 3).
//!
//! - A wrapped, letter- or word-spaced paragraph is aligned by each line's SPACED width: centered
//!   `1 2 3 4 5 6` at `letter-spacing:0.5em` in a 200px Ahem box starts both lines at 25 (150px
//!   lines), where aligning the unspaced width (100px) puts them at 50.
//!
//! The receipt also holds Chrome's rect for a span whose text is only preserved newlines (R1 nit
//! 3: every fragment empty). It is not asserted here: RustKit drops that span and its newline from
//! the layout tree altogether (`1 2 3<span>\n</span>6 7 8` lays out as `1 2 36 7 8`), a separate
//! gap proposed as its own row.
//!
//! Every value is Chrome for Testing 148.0.7778.216's, from
//! `docs/diagnostics/2026-11-28/spaced_align_empty_union_cft148.json`
//! (`tools/parity_oracle/spaced_align_empty_union_log.mjs`, the same markup). Ahem 20px/1.

use super::*;

const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");
const RECEIPT: &str = include_str!("../../../docs/diagnostics/2026-11-28/spaced_align_empty_union_cft148.json");

fn receipt() -> serde_json::Value {
    let r: serde_json::Value = serde_json::from_str(RECEIPT).expect("receipt");
    assert_eq!(r["chrome"], "148.0.7778.216");
    r
}

/// Lays out `body` under the receipt's style and returns the engine and its view.
fn load(style: &str, body: &str) -> (Engine, EngineViewId) {
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
    (engine, view)
}

#[test]
fn a_spaced_line_is_aligned_by_its_spaced_width() {
    let r = receipt();
    let style = r["style"].as_str().unwrap();
    let mut wrong = Vec::new();
    for case in r["align"].as_array().unwrap() {
        let body = format!("<p id=p style=\"{}\">{}</p>", case["style"].as_str().unwrap(), case["text"].as_str().unwrap());
        let (engine, view) = load(style, &body);
        let list = engine.views[&view].display_list.as_ref().expect("display list");
        // Each painted line: its characters (white space dropped) and its leftmost x.
        let mut lines: Vec<(f64, String, f64)> = Vec::new();
        let mut runs: Vec<(f64, f64, String)> = list
            .commands
            .iter()
            .filter_map(|c| match c {
                rustkit_layout::DisplayCommand::Text { text, x, y, font_size, .. } => {
                    let chars: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                    (!chars.is_empty()).then(|| (((*y + *font_size / 2.0) as f64).round(), *x as f64, chars))
                }
                _ => None,
            })
            .collect();
        runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for (y, x, chars) in runs {
            match lines.last_mut() {
                Some((ly, s, left)) if *ly == y => {
                    s.push_str(&chars);
                    *left = left.min(x);
                }
                _ => lines.push((y, chars, x)),
            }
        }
        let got: Vec<(String, f64)> = lines.into_iter().map(|(_, s, x)| (s, x)).collect();
        let chrome: Vec<(String, f64)> = case["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (l["chars"].as_str().unwrap().to_string(), l["left"].as_f64().unwrap()))
            .collect();
        let same = got.len() == chrome.len()
            && got.iter().zip(&chrome).all(|(g, c)| g.0 == c.0 && (g.1 - c.1).abs() < 0.5);
        if !same {
            wrong.push(format!("{}: Chrome (line, left) {chrome:?}, got {got:?}", case["label"].as_str().unwrap()));
        }
    }
    assert!(wrong.is_empty(), "Ahem 20px/1 in a 200px box:\n{}", wrong.join("\n"));
}
