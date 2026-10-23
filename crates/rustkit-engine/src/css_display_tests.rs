//! CSS Display 3 gaps that still diverged from Chrome on the Reddit feed
//! after adopted stylesheets painted: `display: contents`, blockification,
//! the inline initial value for unknown elements, and the popover box.

use super::*;

fn loaded(html: &str) -> (Engine, EngineViewId) {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    engine.load_html(view, html).expect("load");
    (engine, view)
}

fn eval(engine: &mut Engine, view: EngineViewId, script: &str) -> String {
    let out = engine
        .execute_script(view, &format!("String(eval({script:?}))"))
        .unwrap();
    out.strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
        .unwrap_or_else(|| panic!("{script} evaluated to {out}"))
        .to_string()
}

fn painted(engine: &Engine, view: EngineViewId) -> String {
    let list = engine.views[&view]
        .display_list
        .as_ref()
        .expect("display list");
    list.commands
        .iter()
        .filter_map(|cmd| match cmd {
            rustkit_layout::DisplayCommand::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn display_contents_computes_to_contents_inside_a_shadow_tree_with_slots() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"#host{display:flex;width:400px;height:40px;margin:0;padding:0}"#,
        r#"#slotted{display:block;width:60px;height:40px;margin:0;padding:0;border:0}</style>"#,
        r#"<body><div id="host"><template shadowrootmode="open"><style>"#,
        r#".contents{display:contents}"#,
        r#"#kid{display:block;width:50px;height:40px;margin:0;padding:0;border:0}"#,
        r#"</style><div id="bar" class="contents"><div id="kid"></div>"#,
        r#"<slot id="slot" name="s"></slot></div></template>"#,
        r#"<span id="slotted" slot="s"></span></div></body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    assert_eq!(
        js("getComputedStyle(document.getElementById('host').shadowRoot.getElementById('bar')).display"),
        "contents"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('host').shadowRoot.getElementById('slot')).display"),
        "contents"
    );
    assert_eq!(
        js("var r = document.getElementById('host').shadowRoot.getElementById('bar').getBoundingClientRect(); [r.width, r.height].join()"),
        "0,0"
    );
    // The host is a flex row. The contents element and the slot generate
    // no boxes, so the shadow child and the assigned span sit side by side.
    let placed = js(concat!(
        "var h = document.getElementById('host').shadowRoot;",
        "var kid = h.getElementById('kid').getBoundingClientRect();",
        "var slot = document.getElementById('slotted').getBoundingClientRect();",
        "[kid.width, slot.width, slot.x - kid.x].join()",
    ));
    let nums = numbers(&placed);
    assert!(
        (nums[0] - 50.0).abs() < 0.5
            && (nums[1] - 60.0).abs() < 0.5
            && (nums[2] - 50.0).abs() < 0.5,
        "children were not hoisted into the flex host: {placed}"
    );
}

/// The census case (68 of the top sites): a light-tree `display: contents`
/// wrapper in a flex row. Its children are flex items, side by side. With
/// the wrapper boxed they stacked inside one 100px block item.
/// Page: `docs/diagnostics/2026-10-20/css_display_contents.html`.
#[test]
fn a_contents_wrapper_in_a_flex_row_makes_its_children_flex_items() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"#f{display:flex;position:absolute;left:10px;top:10px;width:200px;height:100px}"#,
        r#"#w{display:contents}.k{width:100px;height:100px;flex:none}</style>"#,
        r#"<body><div id="f"><div id="w"><div class="k" id="k1"></div>"#,
        r#"<div class="k" id="k2"></div></div></div></body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    assert_eq!(js("getComputedStyle(document.getElementById('w')).display"), "contents");
    let placed = js(concat!(
        "var a = document.getElementById('k1').getBoundingClientRect();",
        "var b = document.getElementById('k2').getBoundingClientRect();",
        "[a.x, a.y, b.x, b.y].join()",
    ));
    let nums = numbers(&placed);
    assert!(
        (nums[0] - 10.0).abs() < 0.5
            && (nums[1] - 10.0).abs() < 0.5
            && (nums[2] - 110.0).abs() < 0.5
            && (nums[3] - 10.0).abs() < 0.5,
        "the contents wrapper's children are not flex items: {placed}"
    );
}

/// `<picture>` has no UA display, so it is an inline (it was a block while
/// unknown elements started as block). With a `display: block` image in it,
/// as on lyft.com's hero, the inline only wraps a block-level box: Chrome
/// gives it no line box, so the picture is the image's height and the next
/// block starts right under it. The image is a replaced box, not a `Block`
/// box, and must still count as the block that culls the inline.
#[test]
fn a_picture_around_a_block_image_adds_no_line_box() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/1.4 sans-serif}"#,
        r#"img{display:block;width:200px;height:100px}#after{height:10px}</style>"#,
        r#"<body><div id="col"><picture id="p"><source srcset="a.webp" type="image/webp">"#,
        r#"<img id="i" src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"></picture></div>"#,
        r#"<div id="after"></div></body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    assert_eq!(js("getComputedStyle(document.getElementById('p')).display"), "inline");
    let placed = js(concat!(
        "var c = document.getElementById('col').getBoundingClientRect();",
        "var a = document.getElementById('after').getBoundingClientRect();",
        "[c.height, a.y].join()",
    ));
    let nums = numbers(&placed);
    assert!(
        (nums[0] - 100.0).abs() < 0.5 && (nums[1] - 100.0).abs() < 0.5,
        "the picture added a line box under its block image: {placed}"
    );
}

fn numbers(text: &str) -> Vec<f64> {
    text.split(',')
        .map(|part| part.trim().parse().unwrap_or(f64::NAN))
        .collect()
}

#[test]
fn a_contents_credit_bar_does_not_stretch_the_slotted_span_or_the_post() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"#post{display:block;width:964px;margin:0}"#,
        r#"#credit{display:block;width:545px;height:32px;margin:0;padding:0;border:0}</style>"#,
        r#"<body><shreddit-post id="post"><template shadowrootmode="open"><style>"#,
        r#":host{display:block;width:964px;margin:0}.contents{display:contents}"#,
        r#"</style><div id="bar" class="contents"><slot name="credit-bar"></slot></div>"#,
        r#"</template><span id="credit" slot="credit-bar"></span></shreddit-post></body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    assert_eq!(
        js("getComputedStyle(document.getElementById('post').shadowRoot.getElementById('bar')).display"),
        "contents"
    );
    assert_eq!(
        js("var r = document.getElementById('credit').getBoundingClientRect(); [r.width, r.height].join()"),
        "545,32"
    );
    assert_eq!(
        js("document.getElementById('post').getBoundingClientRect().height"),
        "32"
    );
}

#[test]
fn absolute_float_and_the_root_are_blockified_and_contents_is_not() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>html{display:inline}"#,
        r#"a#a{position:absolute;display:inline;float:left;top:0;left:0}"#,
        r#"span#f{float:left;display:inline}"#,
        r#"#c{display:contents;position:absolute}"#,
        r#"#in{display:inline}</style>"#,
        r#"<body style="margin:0"><a id="a" href="https://example.com">x</a>"#,
        r#"<span id="f">y</span><div id="c"><i id="k">k</i></div><span id="in">z</span></body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    assert_eq!(
        js("getComputedStyle(document.documentElement).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('a')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('a')).float"),
        "none"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('f')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('c')).display"),
        "contents"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('in')).display"),
        "inline"
    );
}

#[test]
fn an_unknown_element_computes_to_inline_and_a_div_stays_block() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><body style="margin:0">"#,
        r#"<shreddit-post-flair id="f">flair</shreddit-post-flair>"#,
        r#"<div id="d">d</div><hgroup id="hg">h</hgroup><search id="se">s</search>"#,
        r#"</body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    assert_eq!(
        js("getComputedStyle(document.getElementById('f')).display"),
        "inline"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('d')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('hg')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('se')).display"),
        "block"
    );
    let width = js("document.getElementById('f').getBoundingClientRect().width");
    let width: f64 = width.parse().expect(&width);
    assert!(width < 200.0, "custom element stretched to {width}");
}

#[test]
fn a_shown_popover_is_fixed_and_fit_content_and_a_closed_one_is_hidden() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#".show{display:block;font:16px/20px Arial}"#,
        r#"#forced{display:block}"#,
        r#"#forced[popover]:not(:popover-open){display:none !important}"#,
        r#"#h{display:block;width:10px;height:10px}</style><body>"#,
        r#"<header><div id="forced" popover="auto">Search</div>"#,
        r#"<div id="shown" class="show" popover="manual">Search</div>"#,
        r#"<div id="closed" popover="auto">secret</div></header>"#,
        r#"<div id="until" hidden="until-found" style="width:40px;height:20px">until</div>"#,
        r#"<dialog id="shut">closed dialog</dialog>"#,
        r#"<dialog id="open" open>Hi</dialog>"#,
        r#"<div id="h" hidden><template shadowrootmode="open">"#,
        r#"<style>:host([hidden]){color:rgb(1, 2, 3)}</style>"#,
        r#"</template></div>"#,
        r#"<div id="emhost"><template shadowrootmode="open">"#,
        r#"<style>:host{font-size:20px}#em{font-size:2em}</style><span id="em">Search</span>"#,
        r#"</template></div><p id="ok">ok</p></body>"#,
    ));
    let mut js = |s: &str| eval(&mut engine, view, s);
    // Author `!important` using `:not(:popover-open)` hides a popover even
    // when an inline `display` tries to show it. The pseudo matches nothing
    // until something calls showPopover.
    assert_eq!(
        js("getComputedStyle(document.getElementById('forced')).display"),
        "none"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('closed')).display"),
        "none"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('shown')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('shown')).position"),
        "fixed"
    );
    let size = js(
        "var r = document.getElementById('shown').getBoundingClientRect(); [r.width, r.height].join()",
    );
    let mut parts = size.split(',');
    let width: f64 = parts.next().unwrap().parse().expect(&size);
    let height: f64 = parts.next().unwrap().parse().expect(&size);
    assert!(
        width > 10.0 && width < 400.0,
        "popover width {width} (page is 800)"
    );
    assert!(
        height > 8.0 && height < 80.0,
        "popover height {height} (page is 600)"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('until')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('shut')).display"),
        "none"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('open')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('open')).position"),
        "absolute"
    );
    let dialog = js("document.getElementById('open').getBoundingClientRect().width");
    let dialog: f64 = dialog.parse().expect(&dialog);
    assert!(dialog < 400.0, "open dialog stretched to {dialog}");
    // Author `display` shows the hidden host; `:host([hidden])` still matches
    // and sets the colour. A miss would leave the inherited black.
    assert_eq!(
        js("getComputedStyle(document.getElementById('h')).display"),
        "block"
    );
    assert_eq!(
        js("getComputedStyle(document.getElementById('h')).color"),
        "rgb(1, 2, 3)"
    );
    // `2em` inside the shadow resolves against the host's 20px, not 16px
    // and not some inherited 200px face.
    assert_eq!(
        js("getComputedStyle(document.getElementById('emhost').shadowRoot.getElementById('em')).fontSize"),
        "40px"
    );
    let text = painted(&engine, view);
    for word in ["Search", "Hi", "until", "ok"] {
        assert!(text.contains(word), "missing {word} in {text:?}");
    }
    assert!(!text.contains("secret"), "{text:?}");
    assert!(!text.contains("closed dialog"), "{text:?}");
}
