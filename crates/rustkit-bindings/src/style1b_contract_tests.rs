//! STYLE1b (a): the inline style contract. The pinned Chrome (CfT 148.0.7778.216) answers every case
//! of `docs/diagnostics/2026-11-15/style1b_contract_page.js` (`style1b_contract_cft148.json`). This
//! is a ledger, not a fix: it names the cases where RustKit differs today, so a change that closes
//! one (or opens one) shows here.

use super::*;

const PAGE_JS: &str = include_str!("../../../docs/diagnostics/2026-11-15/style1b_contract_page.js");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-15/style1b_contract_cft148.json");

/// Cases where RustKit's answer differs from Chrome's at the time of the (a) report (develop
/// be8eb272), in the order the cases are compared (by name). Inline style stores declarations as
/// strings with no value parsing: no validation (invalid values and unknown names are kept),
/// no normalisation (colours, quotes, keyword case), no shorthand expansion (length, item(),
/// longhand reads); also custom property names are lowercased, and the attribute's absence after
/// removeAttribute is null here and "" in Chrome (cause not traced).
const KNOWN_GAPS: &[&str] = &[
    "attr-invalid-dropped",
    "attr-verbatim",
    "color-normalised",
    "csstext-messy",
    "custom-property",
    "remove-attribute-then-style",
    "set-invalid-ignored",
    "set-unitless-ignored",
    "shorthand-background",
    "shorthand-in-attr",
    "shorthand-margin",
    "shorthand-margin-then-longhand",
    "unknown-property-ignored",
    "url-and-quotes",
];

fn run() -> serde_json::Value {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html("<html><head></head><body></body></html>").unwrap()))
        .unwrap();
    b.evaluate(&format!("(0, eval)({})", serde_json::to_string(PAGE_JS).unwrap())).unwrap();
    match b.evaluate("JSON.stringify(STYLE1B_RUN())").unwrap() {
        JsValue::String(s) => serde_json::from_str(&s).unwrap(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_inline_style_contract_gaps_are_the_known_ones() {
    let want: serde_json::Value = serde_json::from_str(CHROME).unwrap();
    let got = run();
    let mut gaps = Vec::new();
    for (name, chrome) in want["cases"].as_object().unwrap() {
        let here = got.get(name).cloned().unwrap_or(serde_json::Value::Null);
        if &here != chrome {
            eprintln!("STYLE1B-GAP {name}\n  chrome {chrome}\n  here   {here}");
            gaps.push(name.as_str());
        }
    }
    eprintln!("STYLE1B-CONTRACT {} cases, {} gaps", want["cases"].as_object().unwrap().len(), gaps.len());
    assert_eq!(gaps, KNOWN_GAPS, "the inline style gaps changed");
}
