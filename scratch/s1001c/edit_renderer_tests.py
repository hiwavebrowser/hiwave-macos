"""Queue item 3: renderer tests for elliptical corners. One-shot edit; asserts anchors match once."""
p = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-renderer/src/lib.rs'
s = open(p, newline='').read()


def rep(old, new, count=1):
    global s
    assert s.count(old) == count, (s.count(old), old[:80])
    s = s.replace(old, new)


rep('''        assert_eq!(entry.rounded[0].1.top_left, 20.0);
    }
''', '''        assert_eq!(entry.rounded[0].1.top_left, rustkit_layout::CornerRadius::circular(20.0));
    }

    #[test]
    fn a_clip_scaled_unevenly_scales_each_axis_of_its_radius() {
        // A circular corner under scale(2, 3) is an ellipse on screen. One
        // scalar per corner could only take the geometric mean for both.
        let entry = clip_entry_under(None, [2.0, 0.0, 0.0, 3.0, 0.0, 0.0], Rect::new(0.0, 0.0, 50.0, 50.0), radius(10.0));
        assert_eq!((entry.rect.width, entry.rect.height), (100.0, 150.0));
        assert_eq!(entry.rounded[0].1.top_left, rustkit_layout::CornerRadius { h: 20.0, v: 30.0 });
    }
''')

anchor = '''    #[test]
    fn the_corner_edge_is_antialiased_rather_than_a_staircase() {'''
new = '''    fn ellipse(h: f32, v: f32) -> rustkit_layout::BorderRadius {
        let corner = rustkit_layout::CornerRadius { h, v };
        rustkit_layout::BorderRadius {
            top_left: corner,
            top_right: corner,
            bottom_right: corner,
            bottom_left: corner,
        }
    }

    #[test]
    fn top_only_radii_as_tall_as_the_box_are_not_cut_to_half_of_it() {
        // `40px 40px 0 0` on a 200x40 tab: nothing on the left or right side
        // overlaps, so the corners stay 40px (CSS Backgrounds 3 §5.5). Each
        // radius used to be cut to half the shorter side, 20px.
        let clip = Rect::new(0.0, 0.0, 200.0, 40.0);
        let tab = rustkit_layout::BorderRadius {
            top_left: rustkit_layout::CornerRadius::circular(40.0),
            top_right: rustkit_layout::CornerRadius::circular(40.0),
            ..Default::default()
        };
        let pieces = clip_quad_to_rounded(clip, &[(clip, tab)]);
        let expected = 200.0 * 40.0 - (2.0 - std::f32::consts::FRAC_PI_2) * 40.0 * 40.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 2.0,
            "two 40px quarter circles, covered {} expected {expected}",
            covered_area(&pieces)
        );
        // 30px up the left side a 40px corner is still curving in; a 20px
        // one ended 10px ago.
        let (left, _) = rounded_row_span(clip, tab, 10.5).expect("row crosses the tab");
        assert!(left > 13.0 && left < 14.5, "left edge at y=10.5 was {left}");
    }

    #[test]
    fn an_elliptical_clip_follows_the_ellipse() {
        // 200x100 with 100x50 corners is one whole ellipse.
        let clip = Rect::new(0.0, 0.0, 200.0, 100.0);
        let (left, right) = rounded_row_span(clip, ellipse(100.0, 50.0), 25.0).expect("row crosses");
        // dy = 25 of 50: dx = 100 * sqrt(1 - 0.25) = 86.60
        assert!((left - 13.397).abs() < 0.01, "left {left}");
        assert!((right - 186.603).abs() < 0.01, "right {right}");

        let pieces = clip_quad_to_rounded(clip, &[(clip, ellipse(100.0, 50.0))]);
        let expected = std::f32::consts::PI * 100.0 * 50.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 4.0,
            "ellipse area {expected}, covered {}",
            covered_area(&pieces)
        );
        // Inside the old circular corner (a 50px circle at (50, 50)), outside
        // the ellipse.
        assert!(!painted_at(&pieces, 30.0, 10.5, 0.01), "(30, 10.5) is outside the ellipse");
        assert!(painted_at(&pieces, 100.0, 2.5, 0.999), "the top of the ellipse is painted");
    }

    #[test]
    fn an_elliptical_corner_loses_its_own_area_to_the_clip() {
        // Each corner loses (1 - pi/4) * h * v.
        let clip = Rect::new(0.0, 0.0, 200.0, 120.0);
        let pieces = clip_quad_to_rounded(clip, &[(clip, ellipse(60.0, 30.0))]);
        let expected = 200.0 * 120.0 - (4.0 - std::f32::consts::PI) * 60.0 * 30.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 2.0,
            "covered {} expected {expected}",
            covered_area(&pieces)
        );
    }

    #[test]
    fn the_edge_distance_is_exact_for_a_circle_and_close_for_an_ellipse() {
        // Circle: the plain radius-minus-distance.
        assert_eq!(ellipse_edge_distance(3.0, 4.0, 10.0, 10.0), 5.0);
        assert_eq!(ellipse_edge_distance(6.0, 8.0, 10.0, 10.0), 0.0);

        // Ellipse (60, 30): walk the curve; half a pixel along the normal on
        // either side must read as half a pixel, to well under the width of
        // the antialiasing ramp.
        let (h, v) = (60.0_f32, 30.0_f32);
        for step in 0..=20 {
            let t = step as f32 / 20.0 * std::f32::consts::FRAC_PI_2;
            let (x, y) = (h * t.cos(), v * t.sin());
            assert!(ellipse_edge_distance(x, y, h, v).abs() < 1e-3, "on the curve at t={t}");
            let (nx, ny) = (x / (h * h), y / (v * v));
            let len = (nx * nx + ny * ny).sqrt();
            let (nx, ny) = (nx / len, ny / len);
            let outside = ellipse_edge_distance(x + 0.5 * nx, y + 0.5 * ny, h, v);
            let inside = ellipse_edge_distance(x - 0.5 * nx, y - 0.5 * ny, h, v);
            assert!((outside + 0.5).abs() < 0.05, "t={t}: half a pixel outside read {outside}");
            assert!((inside - 0.5).abs() < 0.05, "t={t}: half a pixel inside read {inside}");
        }
        // The centre is as far inside as the shorter axis.
        assert_eq!(ellipse_edge_distance(0.0, 0.0, h, v), 30.0);
    }

    #[test]
    fn the_fill_and_the_clip_agree_on_an_elliptical_corner() {
        // The fill paints corner pixels from `ellipse_edge_distance`; the clip
        // cuts rows with `rounded_row_span`. If they drew different curves the
        // clip would eat the fill's edge or leave a rim outside it.
        let rect = Rect::new(0.0, 0.0, 200.0, 120.0);
        let radius = ellipse(60.0, 30.0);
        let corner = radius.top_left;
        for row in 0..30 {
            let py = row as f32 + 0.5;
            let (left, _) = rounded_row_span(rect, radius, py).expect("row crosses");
            for col in 0..60 {
                let px = col as f32 + 0.5;
                let fill = corner_distance_at(rect, corner, 0, px, py)
                    .map(corner_coverage)
                    .unwrap_or(1.0);
                if px < left - 1.5 {
                    assert!(fill < 0.01, "({px}, {py}) is outside the clip (left {left}) but filled {fill}");
                }
                if px > left + 1.5 {
                    assert!(fill > 0.99, "({px}, {py}) is inside the clip (left {left}) but filled {fill}");
                }
            }
        }
    }

    #[test]
    fn opposite_corners_that_share_the_middle_both_cut_it() {
        // `80px 0` on a 100px square: a leaf. The top-left and bottom-right
        // boxes overlap in the middle and a point there must be inside both
        // curves. Adjacent corners never overlap once fitted.
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let big = rustkit_layout::CornerRadius::circular(80.0);
        let leaf = rustkit_layout::BorderRadius {
            top_left: big,
            bottom_right: big,
            ..Default::default()
        };
        assert_eq!(leaf.fitted(100.0, 100.0), leaf, "nothing adjacent overlaps");
        assert!(corner_boxes_overlap(rect, big, big));
        assert!(!corner_boxes_overlap(rect, rustkit_layout::CornerRadius::circular(50.0), rustkit_layout::CornerRadius::circular(50.0)));

        // (25, 75): inside the top-left box and curve region's box overlap?
        // It is in both boxes; the bottom-right curve (centre (20, 20),
        // r 80) is 55 from its centre on one axis and 5 on the other: inside.
        // (22, 22) is in both boxes too and outside the top-left curve
        // (centre (80, 80), distance 82).
        assert_eq!(Renderer::point_in_rounded_rect(50.0, 50.0, rect, leaf), 1.0);
        assert_eq!(Renderer::point_in_rounded_rect(22.0, 22.0, rect, leaf), 0.0);
        assert_eq!(Renderer::point_in_rounded_rect(78.0, 78.0, rect, leaf), 0.0);
        // The square corners are whole.
        assert_eq!(Renderer::point_in_rounded_rect(99.0, 1.0, rect, leaf), 1.0);

        // Row 10 is cut on the left by the top-left curve only.
        let (left, right) = rounded_row_span(rect, leaf, 10.0).expect("row crosses");
        assert!(left > 40.0 && left < 42.0, "left {left}");
        assert_eq!(right, 100.0);
    }

    #[test]
    fn a_gradient_cell_is_tested_against_the_ellipse() {
        let rect = Rect::new(0.0, 0.0, 200.0, 100.0);
        let whole = ellipse(100.0, 50.0);
        assert_eq!(Renderer::point_in_rounded_rect(30.0, 10.0, rect, whole), 0.0);
        assert_eq!(Renderer::point_in_rounded_rect(100.0, 3.0, rect, whole), 1.0);
        assert_eq!(Renderer::point_in_rounded_rect(15.0, 50.0, rect, whole), 1.0);
        // On the curve: half covered.
        let on_curve = Renderer::point_in_rounded_rect(100.0 - 86.603, 25.0, rect, whole);
        assert!((on_curve - 0.5).abs() < 0.02, "on the curve read {on_curve}");
    }

'''
rep(anchor, new + anchor)
open(p, 'w', newline='').write(s)
print('ok')
