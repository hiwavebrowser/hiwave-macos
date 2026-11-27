//! Portable SVG-PREPASS-CACHE pins (#1448 / batch 126).
//!
//! The headless oracle (`two_documents_laid_out_in_turn_each_keep_their_svg_paint_styles`)
//! needs `feature = "headless"`, so default Linux `--lib` never saw the LRU keep that
//! returns the same `Rc` for the same sheets. `element_paint` compares those `Rc`s by
//! pointer; a miss recomputes every SVG paint style on each document switch.

use super::*;
use std::rc::Rc;

fn rules_for(engine: &Engine, external: &Rc<Vec<Stylesheet>>) -> Rc<SvgPaintRules> {
    let order = vec![(SheetRef::External(0), String::new())];
    engine
        .lctx
        .svg_paint_rules(Vec::new(), external, &order, Some((800.0, 600.0)))
}

#[test]
fn two_sheet_sets_reuse_their_kept_rules() {
    let engine = Engine::new(EngineConfig::default()).expect("engine");
    let red = Rc::new(vec![Stylesheet::parse("svg { fill: red }").expect("css")]);
    let blue = Rc::new(vec![Stylesheet::parse("svg { fill: blue }").expect("css")]);

    let first_red = rules_for(&engine, &red);
    let first_blue = rules_for(&engine, &blue);
    assert!(
        !Rc::ptr_eq(&first_red, &first_blue),
        "distinct sheet sets must not share one paint-rules Rc"
    );

    let again_red = rules_for(&engine, &red);
    assert!(
        Rc::ptr_eq(&first_red, &again_red),
        "the kept set must return the same Rc so element_paint keeps styles"
    );
    let again_blue = rules_for(&engine, &blue);
    assert!(
        Rc::ptr_eq(&first_blue, &again_blue),
        "alternating documents must each find their own kept set"
    );
}

#[test]
fn a_fifth_sheet_set_evicts_the_oldest_kept_rules() {
    // SVG_PAINT_RULES_KEPT = 4: a fifth distinct set must drop the oldest so
    // memory stays bounded across a page and its frames laid out in turn.
    let engine = Engine::new(EngineConfig::default()).expect("engine");
    let sheets: Vec<Rc<Vec<Stylesheet>>> = (0..5)
        .map(|i| {
            Rc::new(vec![
                Stylesheet::parse(&format!("svg {{ fill: rgb({i},0,0) }}")).expect("css"),
            ])
        })
        .collect();

    let first: Vec<Rc<SvgPaintRules>> = sheets.iter().map(|s| rules_for(&engine, s)).collect();
    assert_eq!(
        engine.lctx.svg_prepass.borrow().paint_rules_kept.len(),
        4,
        "the keep list must cap at SVG_PAINT_RULES_KEPT"
    );

    // Ask the survivors first: a miss on the evicted set would push a new
    // entry and drop another of the four (LRU).
    for i in 1..5 {
        let again = rules_for(&engine, &sheets[i]);
        assert!(
            Rc::ptr_eq(&first[i], &again),
            "set {i} of the last four must still hit the keep list"
        );
    }

    let again_oldest = rules_for(&engine, &sheets[0]);
    assert!(
        !Rc::ptr_eq(&first[0], &again_oldest),
        "the oldest of five must be recomputed after eviction"
    );
}
