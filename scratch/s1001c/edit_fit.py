"""Queue item 3: make the overlap reduction exact for equal radii (side * (r / sum), so a pill is
exactly half its side) and correct two tests whose expected values ignored the vertical sides."""
L = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-layout/src/lib.rs'
E = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-engine/src/lib.rs'


def edit(path, pairs):
    s = open(path, newline='').read()
    for old, new in pairs:
        assert s.count(old) == 1, (s.count(old), old[:80])
        s = s.replace(old, new)
    open(path, 'w', newline='').write(s)


edit(L, [
    ('''    /// One factor for all eight radii keeps every corner's shape. For four
    /// equal circular radii it is `min(r, width / 2, height / 2)`.
''', '''    /// One factor for all eight radii keeps every corner's shape. It is
    /// applied as `side * (r / sum)` for the side that overlaps most, so two
    /// equal radii on that side come out at exactly half of it: four equal
    /// circular radii give `min(r, width / 2, height / 2)` to the bit.
'''),
    ('''        let mut f = 1.0_f32;
        for (side, sum) in [
            (width, tl.h + tr.h),
            (width, bl.h + br.h),
            (height, tl.v + bl.v),
            (height, tr.v + br.v),
        ] {
            if sum > side.max(0.0) {
                f = f.min(side.max(0.0) / sum);
            }
        }
        if f >= 1.0 {
            return Self {
                top_left: tl,
                top_right: tr,
                bottom_right: br,
                bottom_left: bl,
            };
        }
        let scale = |c: CornerRadius| CornerRadius {
            h: c.h * f,
            v: c.v * f,
        };
''', '''        // The side whose radii overlap most: (side length, sum of its two).
        let mut tightest: Option<(f32, f32)> = None;
        for (side, sum) in [
            (width.max(0.0), tl.h + tr.h),
            (width.max(0.0), bl.h + br.h),
            (height.max(0.0), tl.v + bl.v),
            (height.max(0.0), tr.v + br.v),
        ] {
            let tighter = match tightest {
                Some((s, total)) => side / sum < s / total,
                None => true,
            };
            if sum > side && tighter {
                tightest = Some((side, sum));
            }
        }
        let Some((side, sum)) = tightest else {
            return Self {
                top_left: tl,
                top_right: tr,
                bottom_right: br,
                bottom_left: bl,
            };
        };
        let scale = |c: CornerRadius| CornerRadius {
            h: side * (c.h / sum),
            v: side * (c.v / sum),
        };
'''),
    ('''        // §5.5. 200x100: the top side holds 150 + 150, so f = 200/300.
''', '''        // §5.5. 200x400: the top side holds 150 + 150, so f = 200/300.
'''),
    ('''        .fitted(200.0, 100.0);
        assert_eq!(fitted.top_left, CornerRadius::circular(100.0));
''', '''        .fitted(200.0, 400.0);
        assert_eq!(fitted.top_left, CornerRadius::circular(100.0));
'''),
    ('''        // Four equal circular radii: half the shorter side, as before.
        assert_eq!(
            BorderRadius::uniform(9999.0).fitted(200.0, 100.0),
            BorderRadius::uniform(50.0)
        );
''', '''        // The vertical sides count too. 200x100: the right side holds
        // 150 + 15 and is the tightest, f = 100/165.
        let fitted = BorderRadius {
            top_left: CornerRadius::circular(150.0),
            top_right: CornerRadius::circular(150.0),
            bottom_right: CornerRadius { h: 30.0, v: 15.0 },
            bottom_left: CornerRadius::default(),
        }
        .fitted(200.0, 100.0);
        assert!((fitted.top_right.v - 150.0 * 100.0 / 165.0).abs() < 1e-3);
        assert!((fitted.top_right.v + fitted.bottom_right.v - 100.0).abs() < 1e-3);

        // Four equal circular radii: exactly half the shorter side, as
        // before, whatever the radius and the side are.
        assert_eq!(
            BorderRadius::uniform(9999.0).fitted(200.0, 100.0),
            BorderRadius::uniform(50.0)
        );
        for (r, w, h) in [(9999.0_f32, 311.59375_f32, 37.59375_f32), (1e6, 73.3, 1280.7), (24.0, 40.1, 31.9)] {
            assert_eq!(
                BorderRadius::uniform(r).fitted(w, h),
                BorderRadius::uniform(w.min(h) / 2.0),
                "uniform {r} on {w}x{h}"
            );
        }
'''),
])

edit(E, [
    ('''    /// §5.5), so corners keep their proportions. Two 150px top corners on a
    /// 200px-wide box scale by 200/300; they are not each cut to half the
    /// shorter side.
''', '''    /// §5.5), so corners keep their proportions. Two 150px-wide top corners
    /// on a 200px-wide box scale by 200/300, both axes; they are not each
    /// cut to half the shorter side.
'''),
    ('''        let radius = painted_radius(&e, "border-radius:150px 150px 30px 30px");
        assert!(radius.contains("top_left: CornerRadius { h: 100.0, v: 100.0 }"), "{radius}");
        assert!(radius.contains("bottom_left: CornerRadius { h: 20.0, v: 20.0 }"), "{radius}");
''', '''        let radius = painted_radius(&e, "border-radius:150px 150px 30px 30px / 60px 60px 15px 15px");
        assert!(radius.contains("top_left: CornerRadius { h: 100.0, v: 40.0 }"), "{radius}");
        assert!(radius.contains("bottom_left: CornerRadius { h: 20.0, v: 10.0 }"), "{radius}");
'''),
])
print('ok')
