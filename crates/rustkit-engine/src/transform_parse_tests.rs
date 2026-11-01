//! Plan row C10 (b2): the 3D transform functions were not parsed, and a value
//! the parser did not know was skipped, so `translate3d(-50%,0,0)` did nothing
//! and `translateX(5px) nonsense(1)` kept its first half. With no perspective a
//! 3D transform flattens onto the page. Expected rects are Chromium 143.0.7499.4
//! for a 100x50 block at (20,10) (`trench/z/tools/c10/c10b2_probe.mjs` on the hub).

use super::*;

#[track_caller]
fn rect(transform: &str, want: &str) {
    let page = format!(
        "<!doctype html><style>html{{font-size:10px}}body{{margin:0}}\
         #b{{position:absolute;left:20px;top:10px;width:100px;height:50px;font-size:10px;\
         transform:{transform}}}</style><div id=b></div>"
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, &page).expect("load");
    let got = engine
        .execute_script(
            view,
            "var r = document.getElementById('b').getBoundingClientRect(); \
             [r.x, r.y, r.width, r.height].map(function (v) { return +v.toFixed(3); }).join(' ')",
        )
        .expect("script");
    assert_eq!(got, format!("String(\"{want}\")"), "transform: {transform}");
}

/// What `transform` parsed to, as a matrix at this box's size.
#[track_caller]
fn matrix(value: &str) -> [f32; 6] {
    parse_transform(value)
        .unwrap_or_else(|| panic!("refused: {value}"))
        .to_matrix(100.0, 50.0)
}

#[track_caller]
fn same(value: &str, want: [f32; 6]) {
    let got = matrix(value);
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-3, "{value}: {got:?}, want {want:?}");
    }
}

const ONE: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[test]
fn translate3d_flattens() {
    same("translate3d(5px,6px,0)", [1.0, 0.0, 0.0, 1.0, 5.0, 6.0]);
    same("translate3d(5px,6px,100px)", [1.0, 0.0, 0.0, 1.0, 5.0, 6.0]);
    same(
        "translate3d(-50%,-50%,0)",
        [1.0, 0.0, 0.0, 1.0, -50.0, -25.0],
    );
}

#[test]
fn translate_z_is_a_transform_that_moves_nothing() {
    same("translateZ(10px)", ONE);
    same("translateZ(0)", ONE);
}

#[test]
fn scale3d_drops_its_depth() {
    same("scale3d(2,3,1)", [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
    same("scale3d(2,3,5)", [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
    same("scaleZ(2)", ONE);
}

#[test]
fn a_rotation_about_z_is_a_rotation() {
    same("rotateZ(90deg)", [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
    same("rotate3d(0,0,1,90deg)", [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
    same("rotate3d(0,0,2,90deg)", [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_rotation_about_x_or_y_foreshortens() {
    same("rotateX(60deg)", [1.0, 0.0, 0.0, 0.5, 0.0, 0.0]);
    same("rotateY(60deg)", [0.5, 0.0, 0.0, 1.0, 0.0, 0.0]);
    same("rotate3d(1,0,0,60deg)", [1.0, 0.0, 0.0, 0.5, 0.0, 0.0]);
    same("rotate3d(0,0,0,90deg)", ONE);
}

#[test]
fn matrix3d_flattens() {
    same(
        "matrix3d(1,0,0,0, 0,1,0,0, 0,0,1,0, 7,9,0,1)",
        [1.0, 0.0, 0.0, 1.0, 7.0, 9.0],
    );
    same(
        "matrix3d(2,0,0,0, 0,3,0,0, 0,0,1,0, 7,9,5,1)",
        [2.0, 0.0, 0.0, 3.0, 7.0, 9.0],
    );
}

#[test]
fn perspective_alone_moves_nothing() {
    same("perspective(500px)", ONE);
    same("perspective(none)", ONE);
    same(
        "perspective(500px) translateX(10px)",
        [1.0, 0.0, 0.0, 1.0, 10.0, 0.0],
    );
}

#[test]
fn a_percentage_scales() {
    same("scale(50%)", [0.5, 0.0, 0.0, 0.5, 0.0, 0.0]);
    same("scale(50%,200%)", [0.5, 0.0, 0.0, 2.0, 0.0, 0.0]);
    same("scaleX(50%)", [0.5, 0.0, 0.0, 1.0, 0.0, 0.0]);
    same("scale3d(50%,50%,1)", [0.5, 0.0, 0.0, 0.5, 0.0, 0.0]);
}

#[test]
fn function_names_ignore_case() {
    same("TRANSLATE(5px,6px)", [1.0, 0.0, 0.0, 1.0, 5.0, 6.0]);
}

#[test]
fn a_bad_value_refuses_the_whole_declaration() {
    for value in [
        "translate3d(5px,6px)",
        "translateZ(10%)",
        "scale3d(2,3)",
        "matrix3d(1,0,0,1)",
        "translate3d(5px 6px 0)",
        "translate(5px 6px)",
        "scale(2 3)",
        "rotate(90)",
        "translate(5,6)",
        "translateX(5px) nonsense(1)",
        "nonsense(1) translateX(5px)",
        "translateX(5px) nonsense",
        "translateX(5px",
        "translate (5px)",
        "translate()",
    ] {
        assert!(
            parse_transform(value).is_none(),
            "{value} should be refused"
        );
    }
}

#[test]
fn zero_needs_no_unit() {
    same("rotate(0)", ONE);
    same("translate(0,0)", ONE);
}

#[test]
fn the_box_reads_the_flattened_rect() {
    rect("translate3d(-50%,-50%,0)", "-30 -15 100 50");
    rect("rotateY(60deg)", "45 10 50 50");
    rect("rotateX(60deg)", "20 22.5 100 25");
    rect("scale3d(2,3,5)", "-30 -40 200 150");
    rect(
        "matrix3d(2,0,0,0, 0,3,0,0, 0,0,1,0, 7,9,5,1)",
        "-23 -31 200 150",
    );
}

#[test]
fn a_refused_value_leaves_the_box_alone() {
    rect("translateX(5px) nonsense(1)", "20 10 100 50");
}

#[test]
fn calc_in_a_translate() {
    rect("translate(calc(10px + 5%),0)", "35 10 100 50");
    rect("translateX(calc(100% - 10px))", "110 10 100 50");
}

/// Known gap: `to_matrix` resolves against 16px and has no viewport. It reads
/// `36 26` and `20 10` here.
#[test]
#[ignore = "transform lengths in em, rem, vw and vh do not see the element or the viewport"]
fn font_and_viewport_units_in_a_translate() {
    rect("translate(1em,1rem)", "30 20 100 50");
    rect("translate(1vw,1vh)", "28 16 100 50");
}

/// Known gap: depth is not kept, so what perspective does to a z is lost.
#[test]
#[ignore = "perspective with depth is not an affine the 2D painter can hold"]
fn perspective_with_depth() {
    rect("perspective(500px) translateZ(100px)", "7.5 3.75 125 62.5");
    rect(
        "perspective(500px) rotateY(60deg)",
        "42.63 7.63 50.378 54.741",
    );
}
