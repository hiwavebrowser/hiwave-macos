//! CB3c: a `<button>`'s width is its label plus the horizontal padding plus the horizontal border,
//! each side the author's when declared and the UA's otherwise (6 px padding, 2 px border per side),
//! as the height is (CB3b). The expected numbers are Chrome for Testing 148.0.7778.216, measured
//! 2026-11-11 with `docs/diagnostics/2026-11-11/cb3c_button_widths_chrome` (puppeteer-core) on the
//! page `cb3c_button_widths.html` that this test loads; they agree with Reduce's Chrome 143 numbers
//! on the cells both measured (`#lh1` 24, `#p0` 12, `#b0` 20, `#all` 8 for the label x).

use super::*;

const PAGE: &str = include_str!("../../../docs/diagnostics/2026-11-11/cb3c_button_widths.html");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-11/cb3c_button_widths_cft148.json");

fn measured() -> std::collections::BTreeMap<String, (f64, f64)> {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, PAGE).expect("load_html");
    let out = engine
        .execute_script(view, "String(document.getElementById('out').textContent)")
        .unwrap();
    let text = out.strip_prefix("String(\"").and_then(|v| v.strip_suffix("\")")).unwrap_or(&out).to_string();
    text.split(";;")
        .filter_map(|part| {
            let (id, size) = part.split_once('=')?;
            let (w, h) = size.split_once('x')?;
            Some((id.to_string(), (w.parse().ok()?, h.parse().ok()?)))
        })
        .collect()
}

fn chrome() -> std::collections::BTreeMap<String, (f64, f64)> {
    let value: serde_json::Value = serde_json::from_str(CHROME).expect("json");
    value
        .as_object()
        .expect("object")
        .iter()
        .map(|(id, wh)| (id.clone(), (wh[0].as_f64().unwrap(), wh[1].as_f64().unwrap())))
        .collect()
}

/// Cells Chrome wraps or that the engine does not lay out the same way for a reason outside this
/// row: `w120|Medium Button` wraps its label onto a second line (42 tall), which the engine does
/// not do for a button (separate row), and its width is the declared 120 either way.
const HEIGHT_NOT_PINNED: &[&str] = &["w120|Medium Button"];

#[test]
fn every_button_in_the_matrix_is_the_size_chrome_148_measured() {
    let (got, want) = (measured(), chrome());
    assert_eq!(got.len(), want.len(), "the page measured every cell: {} of {}", got.len(), want.len());
    let mut wrong = Vec::new();
    for (id, (cw, ch)) in &want {
        let (w, h) = got[id];
        let width_ok = (w - cw).abs() < 0.75;
        let height_ok = HEIGHT_NOT_PINNED.contains(&id.as_str()) || (h - ch).abs() < 0.75;
        if !(width_ok && height_ok) {
            wrong.push(format!("{id}: got {w:.2}x{h:.2}, Chrome 148 {cw:.2}x{ch:.2}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} cells differ from Chrome 148:\n{}", wrong.len(), want.len(), wrong.join("\n"));
}
