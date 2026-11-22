//! SVG-SHAPE-TRANSFORM (#1415): portable pin for the affine AABB helper.
//!
//! `svg_shape_visual_rect_tests` is headless-only (create_headless_view + layout
//! export). Default Linux `--lib` never saw `transformed_bounds` itself — the
//! same math that writes `visual_border_box` for an svg shape under rotate(90°).

use super::transformed_bounds;

#[test]
fn a_ninety_degree_rotate_swaps_a_rects_sides() {
    // rotate(90°) about the origin: (x,y) → (−y, x). A 2.5×5 rect at the origin
    // becomes a 5×2.5 AABB — the shape of the inspector path under rotate(90deg).
    let m = [0.0, 1.0, -1.0, 0.0, 0.0, 0.0];
    let (x, y, w, h) = transformed_bounds(m, 0.0, 0.0, 2.5, 5.0);
    assert!((w - 5.0).abs() < 1e-5 && (h - 2.5).abs() < 1e-5, "size got ({w},{h})");
    assert!((x - (-5.0)).abs() < 1e-5 && (y - 0.0).abs() < 1e-5, "origin got ({x},{y})");
}

#[test]
fn an_identity_keeps_the_rect() {
    let m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let (x, y, w, h) = transformed_bounds(m, 10.0, 20.0, 30.0, 40.0);
    assert_eq!((x, y, w, h), (10.0, 20.0, 30.0, 40.0));
}

#[test]
fn a_translate_moves_without_resizing() {
    let m = [1.0, 0.0, 0.0, 1.0, 5.0, 7.0];
    let (x, y, w, h) = transformed_bounds(m, 10.0, 20.0, 2.5, 5.0);
    assert_eq!((x, y, w, h), (15.0, 27.0, 2.5, 5.0));
}
