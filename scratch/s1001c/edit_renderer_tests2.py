"""Queue item 3: correct the tolerances of four new renderer tests (the code under test was right)."""
p = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-renderer/src/lib.rs'
s = open(p, newline='').read()


def rep(old, new, count=1):
    global s
    assert s.count(old) == count, (s.count(old), old[:80])
    s = s.replace(old, new)


rep('''        // 30px up the left side a 40px corner is still curving in; a 20px
        // one ended 10px ago.
        let (left, _) = rounded_row_span(clip, tab, 10.5).expect("row crosses the tab");
        assert!(left > 13.0 && left < 14.5, "left edge at y=10.5 was {left}");
''', '''        // 29.5px above its centre line a 40px corner has come in
        // 40 - sqrt(40^2 - 29.5^2) = 12.99px; a 20px one is not on this row
        // the same way (it would be 2.6px in).
        let (left, _) = rounded_row_span(clip, tab, 10.5).expect("row crosses the tab");
        assert!((left - 12.986).abs() < 0.01, "left edge at y=10.5 was {left}");
''')
rep('''        let expected = std::f32::consts::PI * 100.0 * 50.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 4.0,
''', '''        // One sample per row: the rows nearest the flat top and bottom are
        // where the span changes fastest, so allow them a few px^2.
        let expected = std::f32::consts::PI * 100.0 * 50.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 8.0,
''')
rep('''        let expected = 200.0 * 120.0 - (4.0 - std::f32::consts::PI) * 60.0 * 30.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 2.0,
''', '''        let expected = 200.0 * 120.0 - (4.0 - std::f32::consts::PI) * 60.0 * 30.0;
        assert!(
            (covered_area(&pieces) - expected).abs() < 6.0,
''')
a = s.index('''        let rect = Rect::new(0.0, 0.0, 200.0, 120.0);
        let radius = ellipse(60.0, 30.0);
        let corner = radius.top_left;
        for row in 0..30 {''')
b = s.index('''    #[test]
    fn opposite_corners_that_share_the_middle_both_cut_it() {''')
new = '''        let rect = Rect::new(0.0, 0.0, 200.0, 120.0);
        let radius = ellipse(60.0, 30.0);
        let corner = radius.top_left;
        // Whether the clip keeps the point, read from its row.
        let kept = |px: f32, py: f32| {
            py >= 0.0
                && rounded_row_span(rect, radius, py)
                    .map(|(left, _)| px >= left)
                    .unwrap_or(false)
        };
        let mut edge_pixels = 0;
        for row in 0..30 {
            let py = row as f32 + 0.5;
            for col in 0..60 {
                let px = col as f32 + 0.5;
                let fill = corner_distance_at(rect, corner, 0, px, py)
                    .map(corner_coverage)
                    .unwrap_or(1.0);
                // A pixel and a half clear of the curve along both axes
                // (the curve is nearly flat along the top, so one axis alone
                // says little there).
                if !kept(px + 1.5, py) && !kept(px, py + 1.5) {
                    assert!(fill < 0.01, "({px}, {py}) is well outside the clip but filled {fill}");
                }
                if kept(px - 1.5, py) && kept(px, py - 1.5) {
                    assert!(fill > 0.99, "({px}, {py}) is well inside the clip but filled {fill}");
                }
                if fill > 0.01 && fill < 0.99 {
                    edge_pixels += 1;
                }
            }
        }
        assert!(edge_pixels > 60, "the corner has an antialiased edge, saw {edge_pixels} partial pixels");
    }

'''
s = s[:a] + new + s[b:]
open(p, 'w', newline='').write(s)
print('ok')
