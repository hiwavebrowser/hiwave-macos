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
