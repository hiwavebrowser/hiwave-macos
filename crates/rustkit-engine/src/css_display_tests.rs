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

fn rect(engine: &mut Engine, view: EngineViewId, id: &str) -> (f64, f64, f64, f64) {
    let raw = eval(
        engine,
        view,
        &format!(
            "var r = document.getElementById('{id}').getBoundingClientRect(); \
             [r.x, r.y, r.width, r.height].join()"
        ),
    );
    let mut n = raw
        .split(',')
        .map(|p| p.trim().parse::<f64>().unwrap_or(f64::NAN));
    (
        n.next().unwrap_or(f64::NAN),
        n.next().unwrap_or(f64::NAN),
        n.next().unwrap_or(f64::NAN),
        n.next().unwrap_or(f64::NAN),
    )
}

/// The credit span stays the inline line. A block inside it is broken out
/// (CSS 2.1 §9.2.1.1) instead of the span becoming a 964×800 block.
#[test]
fn an_inline_credit_span_splits_around_its_block_and_stays_a_line() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}</style>"#,
        r#"<shreddit-post id="post"><template shadowrootmode="open"><style>"#,
        r#":host{display:block;width:964px}.contents{display:contents}</style>"#,
        r#"<div class="contents"><slot name="credit-bar"></slot></div>"#,
        r#"</template><span id="credit" slot="credit-bar">"#,
        r#"username and the credit line"#,
        r#"<span id="extra" style="display:block;height:800px;width:200px"></span>"#,
        r#"</span></shreddit-post>"#,
    ));
    let credit = rect(&mut engine, view, "credit");
    let extra = rect(&mut engine, view, "extra");
    let post = rect(&mut engine, view, "post");
    assert!(
        credit.2 < 500.0 && credit.3 < 40.0,
        "credit span became a block {credit:?}"
    );
    assert!(
        (extra.3 - 800.0).abs() < 1.0 && extra.1 + 1.0 >= credit.1 + credit.3,
        "block was not broken out below the line: extra {extra:?} credit {credit:?}"
    );
    assert_eq!(
        eval(
            &mut engine,
            view,
            "getComputedStyle(document.getElementById('credit')).display"
        ),
        "inline"
    );
    assert!(post.2 > 900.0, "post column {post:?}");
}

/// A blank line between an inline custom element and its block child is not
/// a 20px strut (the wikipedia header gap).
#[test]
fn a_block_inside_an_inline_does_not_keep_a_whitespace_line() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}</style>"#,
        r#"<header id="h"><x-bar id="bar">"#,
        "\n  ",
        r#"<div id="inner" style="height:40px"></div>"#,
        "\n",
        r#"</x-bar></header><div id="below" style="height:10px"></div>"#,
    ));
    let inner = rect(&mut engine, view, "inner");
    let below = rect(&mut engine, view, "below");
    let bar = rect(&mut engine, view, "bar");
    assert!((inner.3 - 40.0).abs() < 1.0, "{inner:?}");
    assert!(
        below.1 < 45.0,
        "header gained a line box: below at {} (inner {inner:?})",
        below.1
    );
    assert!(bar.3 < 45.0, "custom element grew a strut: {bar:?}");
}

/// Unknown elements in a flex row are flex items. They sit on one line
/// inside the row, not on an overflowing max-content line (microsoft header).
#[test]
fn custom_elements_in_a_flex_row_stay_on_one_line_inside_the_row() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#row{display:flex;width:320px}</style>"#,
        r#"<header id="row"><ms-logo id="logo">Logo</ms-logo>"#,
        r#"<ms-nav id="nav">Home Office Windows Xbox</ms-nav></header>"#,
        r#"<div id="next" style="height:8px"></div>"#,
    ));
    let row = rect(&mut engine, view, "row");
    let logo = rect(&mut engine, view, "logo");
    let nav = rect(&mut engine, view, "nav");
    let next = rect(&mut engine, view, "next");
    assert!((row.2 - 320.0).abs() < 1.0, "row {row:?}");
    assert!(
        nav.0 > logo.0 + 1.0 && (nav.1 - logo.1).abs() < 2.0,
        "logo and nav stacked or overflowed: logo {logo:?} nav {nav:?}"
    );
    assert!(
        nav.0 + nav.2 <= 321.0,
        "nav ran past the row: nav {nav:?} row {row:?}"
    );
    assert!(next.1 < 40.0, "row grew a second line: next at {}", next.1);
}

/// `display: contents` children join the flex row and are blockified, so
/// they still fit the row instead of summing a max-content line.
#[test]
fn contents_children_join_a_flex_row_without_overflowing_it() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#row{display:flex;width:300px}#wrap{display:contents}</style>"#,
        r#"<div id="row"><div id="wrap"><a id="logo">MS</a>"#,
        r#"<nav id="nav">Products</nav></div></div>"#,
    ));
    let logo = rect(&mut engine, view, "logo");
    let nav = rect(&mut engine, view, "nav");
    assert!(
        nav.0 > logo.0 && (nav.1 - logo.1).abs() < 2.0,
        "contents children did not join the row: logo {logo:?} nav {nav:?}"
    );
    assert!(
        nav.0 + nav.2 <= 301.0,
        "nav overflowed the flex row: {nav:?}"
    );
    assert_eq!(
        eval(
            &mut engine,
            view,
            "getComputedStyle(document.getElementById('logo')).display"
        ),
        "block",
        "a flex item that arrived through display:contents was not blockified"
    );
}

/// A shown popover is fixed and fit-content. It does not push `main` and
/// it does not stretch a credit bar that contains it.
#[test]
fn a_popover_menu_does_not_stretch_the_credit_bar_or_push_main() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#credit{display:inline-flex}"#,
        r#"#main{height:40px}</style>"#,
        r#"<div id="page" style="display:flex;width:800px">"#,
        r#"<span id="credit">username and the credit line"#,
        r#"<div id="menu" popover="manual">Search</div></span>"#,
        r#"<main id="main">feed</main></div>"#,
    ));
    let credit = rect(&mut engine, view, "credit");
    let menu = rect(&mut engine, view, "menu");
    let main = rect(&mut engine, view, "main");
    assert_eq!(
        eval(
            &mut engine,
            view,
            "getComputedStyle(document.getElementById('menu')).display"
        ),
        "none",
        "a closed popover was shown"
    );
    assert!(
        menu.2 == 0.0 && menu.3 == 0.0,
        "closed popover has a box {menu:?}"
    );
    assert!(
        credit.2 < 500.0 && credit.3 < 80.0,
        "credit bar stretched around the popover {credit:?}"
    );
    assert!(
        main.0 < 500.0 && main.0 + 1.0 >= credit.0 + credit.2,
        "main was pushed by the search box: main {main:?} credit {credit:?}"
    );

    // Author `display` shows the panel. It is out of flow, so main stays
    // put and the panel stays fit-content rather than the page width.
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#menu{display:block}#main{height:40px}</style>"#,
        r#"<div id="page" style="display:flex;width:800px">"#,
        r#"<div id="menu" popover="manual">Search</div>"#,
        r#"<main id="main">feed</main></div>"#,
    ));
    let menu = rect(&mut engine, view, "menu");
    let main = rect(&mut engine, view, "main");
    assert_eq!(
        eval(
            &mut engine,
            view,
            "getComputedStyle(document.getElementById('menu')).position"
        ),
        "fixed"
    );
    assert!(
        menu.2 > 10.0 && menu.2 < 400.0,
        "shown popover is not fit-content: {menu:?}"
    );
    assert!(
        (main.0).abs() < 1.0,
        "fixed popover pushed main to x={}: {main:?}",
        main.0
    );
}

/// A header of custom elements is still there after a resize back to the
/// original viewport (yahoo came back blank apart from the logo).
#[test]
fn a_header_of_custom_elements_survives_a_resize() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#bar{display:flex;width:100%} y-logo{display:block}"#,
        r#"#main{height:80px}</style>"#,
        r#"<header id="bar"><y-logo id="logo">Yahoo</y-logo>"#,
        r#"<y-nav id="nav">News</y-nav></header><main id="main">stories</main>"#,
    ));
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 400,
                height: 300,
            },
        )
        .expect("shrink");
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
        )
        .expect("grow");
    let logo = rect(&mut engine, view, "logo");
    let main = rect(&mut engine, view, "main");
    let text = painted(&engine, view);
    assert!(
        logo.2 > 8.0 && logo.3 > 8.0,
        "logo vanished after resize {logo:?}"
    );
    assert!(main.3 > 40.0, "main vanished after resize {main:?}");
    assert!(
        text.contains("Yahoo") && text.contains("stories"),
        "{text:?}"
    );
}
