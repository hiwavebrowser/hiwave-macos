"""Queue item 3: layout tests for per-axis corner radii. One-shot edit; asserts anchors match once."""
p = '/Users/petecopeland/Repos/.worktrees/rs-control-semantics/crates/rustkit-layout/src/lib.rs'
s = open(p, newline='').read()


def rep(old, new, count=1):
    global s
    assert s.count(old) == count, (s.count(old), old[:80])
    s = s.replace(old, new)


rep('''            (16.0, 16.0, 16.0, 16.0),
            "the clip must carry the box's own radius on all four corners"''',
    '''            (
                CornerRadius::circular(16.0),
                CornerRadius::circular(16.0),
                CornerRadius::circular(16.0),
                CornerRadius::circular(16.0)
            ),
            "the clip must carry the box's own radius on all four corners"''')
rep('        assert_eq!(radius.bottom_left, 12.0);\n', '        assert_eq!(radius.bottom_left, CornerRadius::circular(12.0));\n')
rep('        assert_eq!(radius.top_left, 12.0);\n', '        assert_eq!(radius.top_left, CornerRadius::circular(12.0));\n')
rep('''        assert_eq!(
            radius.top_left, 8.0,
            "12px radius inside a 4px border is 8px"
        );
    }
''', '''        assert_eq!(
            radius.top_left,
            CornerRadius::circular(8.0),
            "12px radius inside a 4px border is 8px"
        );
    }

    #[test]
    fn unequal_borders_give_the_overflow_clip_an_elliptical_radius() {
        // CSS Backgrounds 3 §5.2: the padding edge's horizontal radius is the
        // outer one less the vertical border beside it, and its vertical
        // radius the outer one less the horizontal border. One scalar per
        // corner could only take the thicker border off both.
        let mut parent = rounded_overflow_parent(12.0, true);
        parent.dimensions.border = EdgeSizes {
            top: 10.0,
            right: 2.0,
            bottom: 14.0,
            left: 4.0,
        };

        let list = DisplayList::build(&under_root(parent));
        let (_, radius) = rounded_clip(&list).expect("must still clip");
        assert_eq!(radius.top_left, CornerRadius { h: 8.0, v: 2.0 });
        assert_eq!(radius.top_right, CornerRadius { h: 10.0, v: 2.0 });
        assert_eq!(
            radius.bottom_right,
            CornerRadius::default(),
            "a 14px border swallows the 12px vertical radius: that corner is square"
        );
        assert_eq!(radius.bottom_left, CornerRadius::default());
    }

    #[test]
    fn fitting_scales_every_radius_by_one_factor() {
        // §5.5. 200x100: the top side holds 150 + 150, so f = 200/300.
        let fitted = BorderRadius {
            top_left: CornerRadius::circular(150.0),
            top_right: CornerRadius::circular(150.0),
            bottom_right: CornerRadius { h: 30.0, v: 15.0 },
            bottom_left: CornerRadius::default(),
        }
        .fitted(200.0, 100.0);
        assert_eq!(fitted.top_left, CornerRadius::circular(100.0));
        assert_eq!(fitted.bottom_right, CornerRadius { h: 20.0, v: 10.0 });
        assert_eq!(fitted.bottom_left, CornerRadius::default());

        // Nothing overlaps: unchanged, even with a radius past half the box.
        let lone = BorderRadius {
            top_left: CornerRadius { h: 180.0, v: 90.0 },
            ..BorderRadius::default()
        };
        assert_eq!(lone.fitted(200.0, 100.0), lone);

        // Four equal circular radii: half the shorter side, as before.
        assert_eq!(
            BorderRadius::uniform(9999.0).fitted(200.0, 100.0),
            BorderRadius::uniform(50.0)
        );

        // One zero axis makes the corner square, so it takes no room.
        let half_zero = BorderRadius {
            top_left: CornerRadius { h: 500.0, v: 0.0 },
            top_right: CornerRadius::circular(40.0),
            ..BorderRadius::default()
        };
        let fitted = half_zero.fitted(200.0, 100.0);
        assert_eq!(fitted.top_left, CornerRadius::default());
        assert_eq!(fitted.top_right, CornerRadius::circular(40.0));
        assert!(BorderRadius {
            top_left: CornerRadius { h: 5.0, v: 0.0 },
            ..BorderRadius::default()
        }
        .is_zero());
    }
''')
open(p, 'w', newline='').write(s)
print('ok')
