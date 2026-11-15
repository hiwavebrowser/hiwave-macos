//! The SVG transform DOM, vs Chrome for Testing 148.0.7778.216
//! (`docs/diagnostics/2026-11-14/svg_transform_dom_cases.mjs`, output `svg_transform_dom_cft148.json`):
//! `SVGSVGElement.createSVGTransform` / `createSVGTransformFromMatrix`, `SVGTransform` and its setters, and the
//! `SVGAnimatedTransformList` / `SVGTransformList` behind `transform` and `gradientTransform`. MotionMark
//! 1.3.2's Suits test calls `createSVGTransform`, `setRotate` and `gradientTransform.baseVal.initialize` on
//! every particle reset, and stopped there with "not a callable function".

use super::*;
use std::collections::BTreeMap;

const CASES: &str = include_str!("../../../docs/diagnostics/2026-11-14/svg_transform_dom_cases.json");
const CHROME: &str = include_str!("../../../docs/diagnostics/2026-11-14/svg_transform_dom_cft148.json");

/// The probe's helper: an SVGTransform's matrix as six numbers rounded to three places.
const M: &str = "function m(t) { var x = t.matrix; return [x.a, x.b, x.c, x.d, x.e, x.f].map(function (v) { return Math.round(v * 1000) / 1000; }).join(','); }";

/// Runs one case script. The scripts are this repository's own fixed strings (the Chrome probe's), evaluated
/// in a throwaway test runtime: `eval` is how the probe runs them too.
fn run(body: &str) -> String {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html(
            "<!doctype html><svg id=s width=100 height=100><defs><linearGradient id=g></linearGradient></defs><rect id=r width=10 height=10></rect></svg>",
        )
        .unwrap(),
    ))
    .unwrap();
    let script = format!(
        "(function () {{ var svg = document.getElementById('s'), grad = document.getElementById('g'), rect = document.getElementById('r'); {M}; \
         try {{ return String(eval({body:?})); }} catch (e) {{ return 'threw:' + e.name + ':' + e.message; }} }})()"
    );
    match b.evaluate(&script) {
        Ok(JsValue::String(s)) => s,
        Ok(other) => format!("{other:?}"),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn the_svg_transform_dom_answers_as_in_chrome() {
    let cases: BTreeMap<String, String> = serde_json::from_str(CASES).unwrap();
    let chrome: BTreeMap<String, String> = serde_json::from_str(CHROME).unwrap();
    let mut wrong = Vec::new();
    for (name, body) in &cases {
        let got = run(body);
        if got != chrome[name] {
            wrong.push(format!("{name}: chrome {:?}, here {got:?}", chrome[name]));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), cases.len(), wrong.join("\n"));
}
