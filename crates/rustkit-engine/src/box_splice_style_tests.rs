//! BOX-REBUILD-ON-STYLE: a paint-only style write on one element of a long sibling list costs work
//! for that element, not for every sibling.
//!
//! LPF1's paint-only restyle (`repaint_script_restyled`) builds only the written elements, walking
//! down to them; a child no written element is under is skipped, but each skipped child was still
//! recorded as a later sibling's earlier one (tag, a copy of its attributes, its classes, its
//! state) for selectors that read earlier siblings. Only children that come after it and are built
//! read that record. MotionMark Multiply keeps ~11,176 tiles under one parent and writes the first
//! one's style a frame: every frame recorded ~11k siblings nothing read (legion5 ~8 ms, Mac ~4 ms
//! of build at complexity 1). The restyle build now stops after the last child it builds or walks.

use super::*;

/// MotionMark Multiply's shape: absolutely positioned tiles with px geometry and a percent border
/// radius, created hidden (display:none / visibility:hidden alternately).
fn multiply_page(tiles: usize) -> String {
    let mut html = String::from(
        "<!doctype html><style>#stage{position:relative;width:800px;height:600px;background:#000}\
         #stage div{position:absolute}.div-0{border-top-right-radius:100%}.div-1{border-top-left-radius:100%}\
         #stage div:nth-child(2){outline:1px solid red}</style><div id=stage>",
    );
    for i in 0..tiles {
        let (x, y) = ((i % 40) * 20, (i / 40) * 20);
        html.push_str(&format!(
            "<div id=t{i} class=div-{} style='left:{x}px;top:{y}px;width:20px;height:20px;background:rgb(0,0,1)'></div>",
            i % 2
        ));
    }
    html.push_str("</div>");
    html
}

#[test]
fn a_paint_write_on_the_first_of_3000_siblings_records_none_of_the_rest() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
    engine.load_html(view, &multiply_page(3000)).expect("load");
    // Prime the kept tree the paint-only path builds into (it is kept after a script write).
    engine.execute_script(view, "document.getElementById('t0').style.backgroundColor = 'rgb(0,0,2)'; document.body.offsetHeight; 0").expect("js");
    let before = skipped_siblings_for_test();
    for frame in 1..4 {
        engine
            .execute_script(
                view,
                &format!("document.getElementById('t0').style.backgroundColor = 'rgb({frame},0,0)'; document.body.offsetHeight; 0"),
            )
            .expect("js");
    }
    let skipped = skipped_siblings_for_test() - before;
    // The write landed, and an earlier-sibling selector still matches as in a full build.
    assert_eq!(
        engine
            .execute_script(view, "getComputedStyle(document.getElementById('t0')).backgroundColor + '|' + getComputedStyle(document.getElementById('t1')).outlineColor")
            .expect("js"),
        r#"String("rgb(3, 0, 0)|rgb(255, 0, 0)")"#
    );
    assert!(skipped <= 3, "3 writes on the first tile recorded {skipped} skipped siblings (3000 tiles)");
}

/// Atlas's check on the cut: a tile AFTER an earlier frame's cut, written in a later frame, still
/// gets selectors that read its earlier siblings (`+`, `~`, `:nth-child`) and its later ones
/// (`:nth-last-child`, `:last-child`) right, the same as a full build gives (LPF1's restyle off:
/// the other arm). The restyle arm took the restyle build for each write and recorded only the
/// siblings before the written tile: none for t0, 1500 for t1500, 2999 for t2999.
#[test]
fn sibling_selectors_on_a_tile_after_an_earlier_cut_match_a_full_build() {
    let full = sibling_selectors_after_the_cut("", 1500 + 2999);
    assert!(row(&full, "t2999").contains(",dashed,"), "{full}");
}

/// The same with a `:has` rule reading the next sibling: the paint-only restyle declines writes on
/// a page with one (no sibling skipped: every write builds in full), and the styles still match the
/// full build.
#[test]
fn a_has_rule_reading_a_sibling_keeps_the_full_build() {
    let full = sibling_selectors_after_the_cut("#stage div:has(+ #t2999){letter-spacing:5px}", 0);
    assert!(row(&full, "t2998").ends_with(",5px"), "{full}");
    assert!(row(&full, "t2999").ends_with(",normal"), "{full}");
}

fn row<'a>(rows: &'a str, id: &str) -> &'a str {
    rows.split('|').find(|r| r.starts_with(&format!("{id},"))).unwrap_or_default()
}

/// Writes tile 0 twice (the cut lands after it), then tiles 1500 and 2999, with the restyle on and
/// off; asserts both arms read the same styles and the restyle arm skipped `want_skipped` siblings.
/// Returns the full build's styles.
fn sibling_selectors_after_the_cut(extra: &str, want_skipped: usize) -> String {
    let rules = format!(
        "#t1499 + div{{outline:1px solid rgb(0,128,0)}}#t1497 ~ div{{border-left:2px solid rgb(0,0,128)}}\
         #stage div:nth-last-child(1){{outline:3px solid rgb(128,0,0)}}#stage div:nth-child(odd){{text-indent:7px}}\
         #stage div:last-child{{outline-style:dashed}}{extra}"
    );
    let read = "['t1500','t2998','t2999','t0'].map(function (id) { var s = getComputedStyle(document.getElementById(id)); \
                return [id, s.backgroundColor, s.outlineColor, s.outlineWidth, s.borderLeftColor, s.textIndent, s.outlineStyle, s.letterSpacing].join(','); }).join('|')";
    let run = |restyle: bool| {
        set_script_restyle_for_test(restyle);
        let mut engine = Engine::new(EngineConfig::default()).expect("engine");
        let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 }).expect("view");
        let page = multiply_page(3000).replace("</style>", &format!("{rules}</style>"));
        engine.load_html(view, &page).expect("load");
        let skipped_before = skipped_siblings_for_test();
        for (i, id) in ["t0", "t0", "t1500", "t2999"].iter().enumerate() {
            engine
                .execute_script(
                    view,
                    &format!("document.getElementById('{id}').style.backgroundColor = 'rgb({},1,1)'; document.body.offsetHeight; 0", i + 10),
                )
                .expect("js");
        }
        let out = engine.execute_script(view, read).expect("js");
        // `execute_script` answers `String("...")`.
        let out = out.trim_start_matches("String(\"").trim_end_matches("\")").to_string();
        set_script_restyle_for_test(true);
        (out, skipped_siblings_for_test() - skipped_before)
    };
    let ((restyled, skipped), (full, full_skipped)) = (run(true), run(false));
    assert_eq!(skipped, want_skipped, "restyle arm skipped {skipped}");
    assert_eq!(full_skipped, 0);
    assert_eq!(restyled, full, "the restyle path differs from a full build");
    // And the full build's answer is the selectors' (not two equal wrong answers).
    assert!(row(&full, "t1500").starts_with("t1500,rgb(12, 1, 1),rgb(0, 128, 0)"), "{full}");
    assert!(row(&full, "t2999").starts_with("t2999,rgb(13, 1, 1),rgb(128, 0, 0),3px"), "{full}");
    full
}
