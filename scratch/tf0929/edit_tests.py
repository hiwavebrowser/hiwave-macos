p = '/Users/petecopeland/Repos/.worktrees/rs-grid-fixed-tracks/crates/rustkit-engine/src/lib.rs'
t = open(p).read()
old_helper = '''        b.children.iter().find_map(|c| by_id(c, id))
    }

    #[test]
    fn translate_rotate_and_scale_parse'''
new_helper = '''        b.children.iter().find_map(|c| by_id(c, id))
    }

    /// `#t`'s page-space transform after a full style + layout pass.
    fn transform_of_t(html: &str) -> Option<[f32; 6]> {
        let e = Engine::new(EngineConfig::default()).expect("engine");
        let d = Document::parse_html(html).expect("parse");
        let mut root = e.build_layout_from_document(&d, &[]);
        root.set_viewport(1280.0, 800.0);
        root.layout(&rustkit_layout::Dimensions {
            content: rustkit_layout::Rect::new(0.0, 0.0, 1280.0, 0.0),
            ..Default::default()
        });
        own_transform_affine(by_id(&root, "t").expect("#t"))
    }

    #[test]
    fn translate_rotate_and_scale_parse'''
old1 = '''        let e = Engine::new(EngineConfig::default()).expect("engine");
        let view = e.build_layout_for_test_html(concat!('''
new1 = '''        let m = transform_of_t(concat!('''
old2 = '''            "</body></html>",
        ));
        let m = own_transform_affine(by_id(&view, "t").expect("#t")).expect("a transform");'''
new2 = '''            "</body></html>",
        ))
        .expect("a transform");'''
old3 = '''        let e = Engine::new(EngineConfig::default()).expect("engine");
        let d = Document::parse_html(concat!(
            r#"<body style="margin:0"><div id="t" style="width:50px;height:20px;"#,
            r#"translate:0 -200%"></div></body>"#,
        ))
        .expect("parse");
        let mut root = e.build_layout_from_document(&d, &[]);
        root.set_viewport(1280.0, 800.0);
        root.layout(&rustkit_layout::Dimensions {
            content: rustkit_layout::Rect::new(0.0, 0.0, 1280.0, 0.0),
            ..Default::default()
        });
        let m = own_transform_affine(by_id(&root, "t").expect("#t")).expect("a transform");'''
new3 = '''        let m = transform_of_t(concat!(
            r#"<body style="margin:0"><div id="t" style="width:50px;height:20px;"#,
            r#"translate:0 -200%"></div></body>"#,
        ))
        .expect("a transform");'''
for o, n in [(old_helper, new_helper), (old1, new1), (old2, new2), (old3, new3)]:
    assert t.count(o) == 1, o[:60]
    t = t.replace(o, n)
open(p, 'w').write(t)
print("ok")
