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

/// Wikipedia's empty `#siteNotice` carries `margin: 24px 0` inside a grid
/// item. The two margins adjoin, so the row is 24px and the content starts
/// there — not 48px, which is the +20px gap under the header.
#[test]
fn an_empty_sitenotice_collapses_its_own_margins_inside_the_grid_row() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#".page{display:grid;width:1200px;grid-template-columns:minmax(0,1fr);"#,
        r#"grid-template-areas:'siteNotice' 'pageContent'}"#,
        r#".vector-sitenotice-container{grid-area:siteNotice}"#,
        r#".mw-content-container{grid-area:pageContent}"#,
        r#"#siteNotice{font-size:0.8em;margin:24px 0;position:relative;text-align:center}"#,
        r#"#content{height:10px}</style>"#,
        r#"<div class="page"><div class="vector-sitenotice-container" id="wrap">"#,
        r#"<div id="siteNotice"></div></div>"#,
        r#"<div class="mw-content-container"><main id="content"></main></div></div>"#,
    ));
    let notice = rect(&mut engine, view, "siteNotice");
    let wrap = rect(&mut engine, view, "wrap");
    let content = rect(&mut engine, view, "content");
    assert!(notice.3 < 1.0, "empty notice has a border box {notice:?}");
    assert!(
        (wrap.3 - 24.0).abs() < 0.5,
        "sitenotice row is not the collapsed 24px margin: {wrap:?}"
    );
    assert!(
        (content.1 - 24.0).abs() < 0.5,
        "content did not start under the collapsed margin: {content:?}"
    );
}

/// Vector's menu checkbox is `position:absolute; width/height:100%` inside
/// `.vector-dropdown{position:relative}`. Blockifying it must not stretch it
/// over the header, or the Donate link never receives the click.
#[test]
fn a_dropdown_checkbox_stays_inside_its_relative_dropdown() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#".mw-header{display:flex;align-items:center;width:800px;height:66px}"#,
        r#".vector-header-start,.vector-header-end{display:flex;align-items:center}"#,
        r#".vector-header-end{flex-grow:1;justify-content:flex-end}"#,
        r#".vector-dropdown{position:relative}"#,
        r#".vector-dropdown .vector-dropdown-checkbox{position:absolute;top:0;left:0;"#,
        r#"z-index:1;opacity:0;width:100%;height:100%;margin:0;padding:0}"#,
        r#"label{display:inline-flex;width:40px;height:32px}</style>"#,
        r#"<header class="mw-header" id="hdr"><div class="vector-header-start"><nav>"#,
        r#"<div class="vector-dropdown" id="dd">"#,
        r#"<input type="checkbox" id="cb" class="vector-dropdown-checkbox">"#,
        r#"<label id="lab">Menu</label></div></nav></div>"#,
        r#"<div class="vector-header-end"><a id="donate" href="https://donate.wikimedia.org/">Donate</a>"#,
        r#"</div></header>"#,
    ));
    let cb = rect(&mut engine, view, "cb");
    let dd = rect(&mut engine, view, "dd");
    let donate = rect(&mut engine, view, "donate");
    assert!(
        (cb.2 - dd.2).abs() < 1.0 && (cb.3 - dd.3).abs() < 1.0 && dd.2 < 80.0,
        "checkbox escaped the dropdown: cb {cb:?} dd {dd:?}"
    );
    assert!(
        donate.0 > cb.0 + cb.2,
        "checkbox covers Donate: cb {cb:?} donate {donate:?}"
    );
}

/// Reddit's feed column. `minmax(0, calc(756px)) minmax(0, calc(316px))`
/// in a 917px grid with a 24px gap is 577 + 316, and
/// `minmax(0, calc(100% - 340px))` is the same 577. Dropping the calc
/// left one column the width of the container.
#[test]
fn a_calc_track_list_gives_the_feed_column_577() {
    for columns in [
        "minmax(0, calc(756px)) minmax(0, calc(316px))",
        "minmax(0, calc(100% - 340px)) minmax(0, 316px)",
    ] {
        let (mut engine, view) = loaded(&format!(
            concat!(
                r#"<!doctype html><style>body{{margin:0}}"#,
                r#"#m{{display:grid;width:917px;gap:24px;grid-template-columns:{columns}}}"#,
                r#"#a,#b{{height:60px}}</style>"#,
                r#"<div id="m"><div id="a"></div><div id="b"></div></div>"#,
            ),
            columns = columns
        ));
        let a = rect(&mut engine, view, "a");
        let b = rect(&mut engine, view, "b");
        assert!(
            (a.2 - 577.0).abs() < 1.0 && (b.2 - 316.0).abs() < 1.0 && (b.0 - 601.0).abs() < 1.0,
            "{columns}: a {a:?} b {b:?}"
        );
    }
}

/// The shell that puts the feed at x=339: `315px 965px` with the feed in
/// column 2 and the sidebar `position:fixed`.
#[test]
fn the_feed_column_starts_after_the_fixed_sidebar() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"#g{display:grid;width:1280px;grid-template-columns:315px 965px}"#,
        r#"#sub{grid-column-start:2;height:40px}"#,
        r#"#side{position:fixed;top:56px;width:315px;height:40px}</style>"#,
        r#"<div id="g"><div id="sub"></div><div id="side"></div></div>"#,
    ));
    let sub = rect(&mut engine, view, "sub");
    let side = rect(&mut engine, view, "side");
    assert!(
        (sub.0 - 315.0).abs() < 1.0 && (sub.2 - 965.0).abs() < 1.0,
        "feed column {sub:?}"
    );
    assert!(
        side.0.abs() < 1.0 && (side.1 - 56.0).abs() < 1.0,
        "fixed sidebar was a grid item {side:?}"
    );
}

/// An inline-flex search box is the width of its in-flow item. The 1623px
/// absolute and the fixed popover do not join that width.
#[test]
fn an_inline_flex_ignores_the_absolute_and_the_popover() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"#s{display:inline-flex}#b{width:100px;height:20px}"#,
        r#"#a{position:absolute;width:1623px;height:20px;left:0}"#,
        r#"#pop{width:3000px}</style>"#,
        r#"<div id="s"><div id="b"></div><div id="a"></div>"#,
        r#"<div id="pop" popover style="display:block"></div></div>"#,
        r#"<main id="main" style="height:20px">m</main>"#,
    ));
    let s = rect(&mut engine, view, "s");
    let main = rect(&mut engine, view, "main");
    assert!(
        (s.2 - 100.0).abs() < 1.0 && s.3 < 40.0,
        "search container took the absolute's width {s:?}"
    );
    assert!(main.0.abs() < 1.0, "search container pushed main {main:?}");
}

/// `#p:not(:popover-open){display:none}` beats `#p{display:flex}`, so a
/// closed search panel does not cover the page.
#[test]
fn a_closed_popover_loses_to_not_popover_open() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"#p{display:flex;width:200px;height:40px}"#,
        r#"#p:not(:popover-open){display:none}</style>"#,
        r#"<div id="p" popover>Search</div><main id="m">feed</main>"#,
    ));
    let p = rect(&mut engine, view, "p");
    assert_eq!(
        eval(
            &mut engine,
            view,
            "getComputedStyle(document.getElementById('p')).display"
        ),
        "none"
    );
    assert!(
        p.2 == 0.0 && p.3 == 0.0,
        "closed search panel has a box {p:?}"
    );
}

/// A flex-item credit span stays one line when its only tall child is a
/// closed popover, even where an author `display:flex` would have shown it.
#[test]
fn a_credit_span_stays_one_line_when_its_popover_is_closed() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#row{display:flex;width:545px}#credit{display:flex}"#,
        r#"#menu{display:flex;width:200px;height:800px}"#,
        r#"#menu:not(:popover-open){display:none}</style>"#,
        r#"<div id="row"><span id="credit">username"#,
        r#"<span id="menu" popover>Search panel text</span></span></div>"#,
    ));
    let credit = rect(&mut engine, view, "credit");
    let menu = rect(&mut engine, view, "menu");
    assert!(menu.2 == 0.0 && menu.3 == 0.0, "popover shown {menu:?}");
    assert!(
        credit.2 < 200.0 && credit.3 < 40.0,
        "credit span stretched around the closed popover {credit:?}"
    );
}

/// Microsoft's header row: `display:contents` hoists the nowrap links into
/// `nav`, `uhf-contextual-nav` is `flex: 1`, and `.hidden` is
/// `display:none !important`. Logo, links and actions stay on one row
/// inside the 1152px container.
#[test]
fn the_microsoft_header_row_keeps_the_logo_and_nav_on_one_line() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"#bar{display:flex;flex-direction:row;align-items:center;gap:10px;"#,
        r#"width:1152px;height:54px}"#,
        r#"uhf-brand{display:flex}.logo{display:flex;width:137px;height:54px}"#,
        r#"uhf-contextual-nav{flex:1}.desk{display:inline}"#,
        r#"nav.uhf{display:flex;align-items:center;gap:5px;min-width:0}"#,
        r#".contents{display:contents}"#,
        r#"a.link{display:flex;white-space:nowrap;height:54px;padding:0 8px}"#,
        r#".hidden{display:none!important}"#,
        r#"uhf-actions{display:flex;margin-left:auto}#act{width:80px;height:54px}</style>"#,
        r#"<div id="bar"><uhf-brand><div class="contents">"#,
        r#"<a class="logo" id="logo"></a></div></uhf-brand>"#,
        r#"<uhf-contextual-nav id="navhost"><uhf-contextual-nav-desktop class="desk">"#,
        r#"<nav class="uhf"><uhf-overflow-menu class="contents"><div class="contents">"#,
        r#"<div class="contents"><a class="link" id="l1">Microsoft 365</a>"#,
        r#"<a class="link" id="l2">Azure</a>"#,
        r#"<a class="link hidden" id="hid">Support</a></div></div></uhf-overflow-menu>"#,
        r#"</nav></uhf-contextual-nav-desktop></uhf-contextual-nav>"#,
        r#"<uhf-actions id="actions"><div id="act"></div></uhf-actions></div>"#,
    ));
    let logo = rect(&mut engine, view, "logo");
    let l1 = rect(&mut engine, view, "l1");
    let hid = rect(&mut engine, view, "hid");
    let actions = rect(&mut engine, view, "actions");
    assert!(
        (logo.2 - 137.0).abs() < 1.0 && (l1.1 - logo.1).abs() < 2.0 && l1.0 > logo.0,
        "logo and nav are not one row: logo {logo:?} l1 {l1:?}"
    );
    assert!(hid.2 == 0.0 && hid.3 == 0.0, "hidden link shown {hid:?}");
    assert!(
        actions.0 + actions.2 <= 1152.5,
        "header row overflowed: actions {actions:?} l1 {l1:?}"
    );
}

/// Yahoo's page grid is `repeat(1, minmax(0, 1fr))` / `auto 1fr auto`, and
/// the header is an inline-flex of `display:contents` wrappers. Resizing
/// away and back must leave the story painted, not only the logo.
#[test]
fn the_yahoo_grid_keeps_the_story_after_a_resize() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#".page{display:grid;min-height:100vh;grid-template-columns:repeat(1,minmax(0,1fr));"#,
        r#"grid-template-rows:auto 1fr auto}"#,
        r#"header{display:inline-flex;width:100%;flex-direction:column}"#,
        r#".row{display:inline-flex;width:100%}.contents{display:contents}"#,
        r#"#logo{display:block;width:80px;height:24px}#story{height:40px}</style>"#,
        r#"<div class="page" id="page"><header id="uh"><div class="row">"#,
        r#"<div class="contents"><a id="logo">Yahoo</a></div>"#,
        r#"<div class="contents"><div id="search" style="width:200px;height:20px">find</div></div>"#,
        r#"</div><div class="contents"></div></header>"#,
        r#"<div id="mid"><div id="story">Headlines</div></div>"#,
        r#"<footer id="foot" style="height:20px">foot</footer></div>"#,
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
    let story = rect(&mut engine, view, "story");
    let logo = rect(&mut engine, view, "logo");
    let text = painted(&engine, view);
    assert!(logo.2 > 8.0, "logo vanished {logo:?}");
    assert!(
        story.3 > 20.0 && story.1 > logo.1,
        "story vanished after resize {story:?} logo {logo:?}"
    );
    assert!(
        text.contains("Yahoo") && text.contains("Headlines"),
        "{text:?}"
    );
}

/// Microsoft's header row is `height: 54px` and the container is
/// `height: 100%; display: flex; align-items: center`. The block pre-pass
/// stacks the three 54px items to 162px. Centring them in that stack puts
/// the row on the header's bottom edge.
#[test]
fn a_percent_height_header_row_centres_inside_the_bar() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"header{height:54px}#c{display:flex;flex-direction:row;align-items:center;height:100%;gap:10px}"#,
        r#"#b{width:137px;height:54px}#n{flex:1;height:54px}#a{width:80px;height:54px;margin-left:auto}</style>"#,
        r#"<header id="h"><div id="c"><div id="b"></div><div id="n"></div><div id="a"></div></div></header>"#,
    ));
    let h = rect(&mut engine, view, "h");
    let b = rect(&mut engine, view, "b");
    let n = rect(&mut engine, view, "n");
    let a = rect(&mut engine, view, "a");
    assert!(
        (b.1 - h.1).abs() < 1.0 && (n.1 - b.1).abs() < 1.0 && (a.1 - b.1).abs() < 1.0,
        "header row left the bar: h {h:?} b {b:?} n {n:?} a {a:?}"
    );
    assert!(
        n.0 > b.0 && a.0 > n.0 && a.0 + a.2 <= h.2 + 1.0,
        "logo and nav are not one row inside the header: h {h:?} b {b:?} n {n:?} a {a:?}"
    );
}

/// Upgrading a search component attaches a shadow root and an adopted
/// sheet. The sheet's `:host` sizes the host; the suggestions popover and
/// the credit menu stay closed (`:not(:popover-open)` beats the sheet's
/// `display:flex`), so neither the 5384px panel nor the 800px menu joins
/// the host.
#[test]
fn an_upgraded_search_keeps_its_popover_closed() {
    let (mut engine, view) = loaded(
        r#"<!doctype html><body style="margin:0"><x-search id="h"></x-search><main id="m">feed</main></body>"#,
    );
    engine
        .execute_script(
            view,
            r#"
class XSearch extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    var sheet = new CSSStyleSheet();
    sheet.replaceSync(':host{display:inline-flex;height:32px} #box{width:200px;height:32px} #panel{display:flex;width:5384px;height:400px} #panel:not(:popover-open){display:none} #credit{display:flex} #menu{display:flex;width:200px;height:800px} #menu:not(:popover-open){display:none}');
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = '<div id="box"></div><div id="panel" popover="auto">suggestions</div><span id="credit">user<div id="menu" popover="manual">menu</div></span>';
  }
}
customElements.define('x-search', XSearch);
"#,
        )
        .expect("upgrade");
    let shape = eval(
        &mut engine,
        view,
        r#"
        var h = document.getElementById('h');
        var root = h.shadowRoot;
        var hr = h.getBoundingClientRect();
        var panel = root.getElementById('panel');
        var menu = root.getElementById('menu');
        var credit = root.getElementById('credit');
        var pr = panel.getBoundingClientRect();
        var mr = menu.getBoundingClientRect();
        var cr = credit.getBoundingClientRect();
        var main = document.getElementById('m').getBoundingClientRect();
        [getComputedStyle(h).display, Math.round(hr.width), Math.round(hr.height), getComputedStyle(panel).display, Math.round(pr.width), Math.round(pr.height), getComputedStyle(menu).display, Math.round(mr.height), Math.round(cr.width), Math.round(cr.height), Math.round(main.x)].join('|')
        "#,
    );
    let parts: Vec<&str> = shape.split('|').collect();
    assert_eq!(parts[0], "inline-flex", "{shape}");
    let host_w: f64 = parts[1].parse().unwrap();
    let host_h: f64 = parts[2].parse().unwrap();
    assert!(
        host_w < 500.0 && host_h < 80.0,
        "upgraded search took the popover's size: {shape}"
    );
    assert_eq!(parts[3], "none", "suggestions popover open: {shape}");
    assert_eq!(parts[6], "none", "credit menu open: {shape}");
    let credit_h: f64 = parts[9].parse().unwrap();
    let main_x: f64 = parts[10].parse().unwrap();
    assert!(credit_h < 40.0, "credit bar grew around the menu: {shape}");
    assert!(main_x.abs() < 1.0, "search pushed main: {shape}");
}

/// The suggestions panel and the credit menu are not in the shadow HTML as
/// popovers. The upgrade sets `popover` on the element (HTML's reflected
/// IDL). Until that attribute exists the UA does not hide them, and the
/// panel's 5384px row and the menu's 800px column join the host.
#[test]
fn an_upgraded_search_popover_property_stays_closed() {
    let (mut engine, view) = loaded(
        r#"<!doctype html><body style="margin:0"><x-search id="h"></x-search><main id="m">feed</main></body>"#,
    );
    engine
        .execute_script(
            view,
            r#"
class XSearch extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    var sheet = new CSSStyleSheet();
    sheet.replaceSync(':host{display:inline-flex} #box{width:200px;height:32px} #credit{display:inline-flex}');
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = '<div id="box"></div><div id="panel"><div style="width:5384px;height:400px"></div></div><span id="credit">user<div id="menu"><div style="width:20px;height:800px"></div></div></span>';
    var panel = root.getElementById('panel');
    var menu = root.getElementById('menu');
    panel.popover = 'auto';
    menu.popover = 'manual';
    window.__pop = [panel.popover, menu.popover, panel.getAttribute('popover'), menu.getAttribute('popover')].join('|');
  }
}
customElements.define('x-search', XSearch);
"#,
        )
        .expect("upgrade");
    assert_eq!(
        eval(&mut engine, view, "window.__pop"),
        "auto|manual|auto|manual"
    );
    let shape = eval(
        &mut engine,
        view,
        r#"
        var h = document.getElementById('h');
        var root = h.shadowRoot;
        var hr = h.getBoundingClientRect();
        var panel = root.getElementById('panel');
        var credit = root.getElementById('credit');
        var pr = panel.getBoundingClientRect();
        var cr = credit.getBoundingClientRect();
        var main = document.getElementById('m').getBoundingClientRect();
        [Math.round(hr.width), Math.round(hr.height), getComputedStyle(panel).display, getComputedStyle(panel).position, Math.round(pr.width), Math.round(pr.height), Math.round(cr.width), Math.round(cr.height), Math.round(main.x)].join('|')
        "#,
    );
    let parts: Vec<&str> = shape.split('|').collect();
    let host_w: f64 = parts[0].parse().unwrap();
    let host_h: f64 = parts[1].parse().unwrap();
    assert!(
        host_w < 500.0 && host_h < 80.0,
        "search container took the closed popover: {shape}"
    );
    assert_eq!(parts[2], "none", "popover display: {shape}");
    assert_eq!(parts[3], "fixed", "popover position: {shape}");
    let credit_h: f64 = parts[7].parse().unwrap();
    let main_x: f64 = parts[8].parse().unwrap();
    assert!(credit_h < 40.0, "credit span took the closed menu: {shape}");
    assert!(main_x.abs() < 1.0, "search pushed main: {shape}");
}

/// Reddit hides a hovercard's card with
/// `faceplate-hovercard:not(:defined) > [slot=content] { display: none }`
/// until the element upgrades. The upgrade's adopted sheet then keeps the
/// card in `[faceplate-popper-content] { position: fixed; visibility: hidden }`
/// inside a `:host { display: contents }` root. The slotted credit span stays
/// one line; the card does not join it.
#[test]
fn an_upgraded_hovercard_stays_out_of_the_credit_bar() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0;font:16px/20px sans-serif}"#,
        r#"faceplate-hovercard:not(:defined) > [slot="content"]{display:none}"#,
        r#"span.flex{display:flex;align-items:center}"#,
        r#"</style><span id="credit" class="flex" slot="credit-bar">"#,
        r#"<faceplate-hovercard id="card"><span id="name">u/name</span>"#,
        r#"<div id="cardbody" slot="content" style="width:400px;height:800px"></div>"#,
        r#"</faceplate-hovercard></span>"#,
    ));
    let before = rect(&mut engine, view, "credit");
    assert!(
        before.3 < 40.0,
        "undefined hovercard already stretched the credit bar {before:?}"
    );
    engine
        .execute_script(
            view,
            r#"
class FaceplateHovercard extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    var sheet = new CSSStyleSheet();
    sheet.replaceSync(':host{display:contents}#faceplate-tooltip{display:flex;flex-direction:column;width:fit-content}[faceplate-popper-content]{position:fixed;visibility:hidden;opacity:0}');
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = '<slot></slot><div id="faceplate-tooltip" faceplate-popper-content><slot name="content"></slot></div>';
  }
}
customElements.define('faceplate-hovercard', FaceplateHovercard);
"#,
        )
        .expect("upgrade");
    let shape = eval(
        &mut engine,
        view,
        r#"
        var card = document.getElementById('card');
        var tip = card.shadowRoot.getElementById('faceplate-tooltip');
        var credit = document.getElementById('credit').getBoundingClientRect();
        var body = document.getElementById('cardbody').getBoundingClientRect();
        [getComputedStyle(card).display, getComputedStyle(tip).position, getComputedStyle(tip).visibility, Math.round(credit.width), Math.round(credit.height), Math.round(body.height)].join('|')
        "#,
    );
    let parts: Vec<&str> = shape.split('|').collect();
    assert_eq!(parts[0], "contents", "host display: {shape}");
    assert_eq!(parts[1], "fixed", "closed card stayed in flow: {shape}");
    assert_eq!(parts[2], "hidden", "closed card visible: {shape}");
    let credit_h: f64 = parts[4].parse().unwrap();
    assert!(
        credit_h < 40.0,
        "credit span grew when the hovercard upgraded: {shape}"
    );
}

/// The header search row is `display:flex` inside a `flex: 1 1 0%` item.
/// `.hidden` is `display:none`; `.m\:block` is `display:block` only from
/// 960px. A closed suggestions box (`position:fixed`) must not stretch the
/// row to 5384, and at a desktop viewport the `m:block` span keeps a box.
#[test]
fn an_upgraded_search_row_stays_in_the_header() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#".hidden{display:none}@media (min-width:960px){.m\:block{display:block}}"#,
        r#"header{height:56px;overflow:hidden}nav{display:flex;height:56px;align-items:center}"#,
        r#".flex-1{flex:1 1 0%;height:40px}.flex{display:flex}.w-full{width:100%}"#,
        r#"[faceplate-popper-content]{position:fixed;visibility:hidden;opacity:0}"#,
        r#"</style><header id="h"><nav>"#,
        r#"<div id="search" class="flex-1"><div id="row" class="flex w-full">"#,
        r#"<span id="desk" class="hidden m:block" style="width:2019px;height:32px">Find</span>"#,
        r#"<div id="drop" faceplate-popper-content style="width:5384px;height:560px"></div>"#,
        r#"</div><div id="sib" style="width:40px;height:40px"></div></div></nav></header>"#,
    ));
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 800,
            },
        )
        .expect("desktop");
    let search = rect(&mut engine, view, "search");
    let desk = rect(&mut engine, view, "desk");
    let drop = rect(&mut engine, view, "drop");
    let sib = rect(&mut engine, view, "sib");
    // Chrome at 1280: `.m\:block` wins over `.hidden`, so the flex item's
    // min-content is the 2019px span. The fixed suggestions box is out of
    // flow and does not push that to 5384 or the row past the header.
    assert!(
        search.2 > 1500.0 && search.2 < 2500.0,
        "search row took the suggestions width {search:?} drop {drop:?}"
    );
    assert!(desk.2 > 100.0, "m:block span lost its box at 1280 {desk:?}");
    assert!(
        sib.0 < search.0 + search.2 + 80.0,
        "sibling left the header: search {search:?} sib {sib:?}"
    );
    assert!(
        search.3 < 80.0,
        "search row grew to the suggestions height {search:?} drop {drop:?}"
    );
}

/// `shreddit-feed` upgrades to `display: flex; flex-direction: column` inside
/// the `minmax(0, 756px)` track. A 964px header row overflows the post;
/// the post stays the track width (Chrome: 577 in a 917px shell with a
/// 316px rail and a 24px gap).
#[test]
fn an_upgraded_feed_column_keeps_the_post_at_the_track() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"@media (min-width:768px){"#,
        r#".xs\:\[\&\.fixed-sidebar\]\:grid-cols-\[minmax\(0\,756px\)_minmax\(0\,316px\)\].fixed-sidebar{"#,
        r#"grid-template-columns:minmax(0,756px) minmax(0,316px);color:rgb(1, 2, 3)}}"#,
        r#"#shell{display:grid;width:917px;column-gap:24px}"#,
        r#"#feed{display:flex;flex-direction:column}"#,
        r#"#header{display:flex}#wide{width:964px;height:32px;flex:none}"#,
        r#"#rail{width:316px}</style>"#,
        r#"<div id="shell" class="xs:[&.fixed-sidebar]:grid-cols-[minmax(0,756px)_minmax(0,316px)] fixed-sidebar">"#,
        r#"<div id="feed"><article id="post"><div id="header"><div id="wide"></div></div>"#,
        r#"</article></div><aside id="rail"></aside></div>"#,
    ));
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 800,
            },
        )
        .expect("desktop");
    let color = eval(
        &mut engine,
        view,
        "getComputedStyle(document.getElementById('shell')).color",
    );
    assert_eq!(color, "rgb(1, 2, 3)", "escaped grid class did not match");
    let feed = rect(&mut engine, view, "feed");
    let post = rect(&mut engine, view, "post");
    let header = rect(&mut engine, view, "header");
    let rail = rect(&mut engine, view, "rail");
    assert!(
        (feed.2 - 577.0).abs() < 2.0,
        "feed column is not the 577px track {feed:?}"
    );
    assert!(
        (post.2 - 577.0).abs() < 2.0,
        "post grew to its header's min-content {post:?} header {header:?}"
    );
    assert!(
        (header.2 - 577.0).abs() < 2.0,
        "post header filled the wide row {header:?}"
    );
    assert!(
        (rail.0 - 601.0).abs() < 2.0 && (rail.2 - 316.0).abs() < 2.0,
        "rail {rail:?}"
    );
}

/// The same shell after the custom elements upgrade. `shreddit-feed`'s
/// adopted sheet sets `:host { display: flex; flex-direction: column }`,
/// and `shreddit-post`'s sets a block host with a flex header in the
/// shadow. The 964px header child overflows the 577px track. The slotted
/// credit span stays one line.
#[test]
fn an_upgraded_post_keeps_the_track_and_the_credit_line() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#"@media (min-width:768px){"#,
        r#".xs\:\[\&\.fixed-sidebar\]\:grid-cols-\[minmax\(0\,756px\)_minmax\(0\,316px\)\].fixed-sidebar{"#,
        r#"grid-template-columns:minmax(0,756px) minmax(0,316px)}}"#,
        r#"#shell{display:grid;width:917px;column-gap:24px}"#,
        r#"span.flex{display:flex;align-items:center;height:32px}"#,
        r#"#rail{width:316px}</style>"#,
        r#"<div id="shell" class="xs:[&.fixed-sidebar]:grid-cols-[minmax(0,756px)_minmax(0,316px)] fixed-sidebar">"#,
        r#"<shreddit-feed id="feed"><article><shreddit-post id="post">"#,
        r#"<span id="credit" class="flex" slot="credit-bar">u/name</span>"#,
        r#"</shreddit-post></article></shreddit-feed><aside id="rail"></aside></div>"#,
    ));
    engine
        .execute_script(
            view,
            r#"
class ShredditFeed extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    var sheet = new CSSStyleSheet();
    sheet.replaceSync(':host{display:flex;flex-direction:column}');
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = '<slot></slot>';
  }
}
class ShredditPost extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    var sheet = new CSSStyleSheet();
    sheet.replaceSync(':host{display:block;position:relative}.header{display:flex}slot[name=credit-bar]{display:block}');
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = '<div class="header" id="header"><div id="wide" style="width:964px;height:32px"></div></div><slot name="credit-bar"></slot><slot></slot>';
  }
}
customElements.define('shreddit-feed', ShredditFeed);
customElements.define('shreddit-post', ShredditPost);
"#,
        )
        .expect("upgrade");
    let shape = eval(
        &mut engine,
        view,
        r#"
        var post = document.getElementById('post');
        var header = post.shadowRoot.getElementById('header').getBoundingClientRect();
        var wide = post.shadowRoot.getElementById('wide').getBoundingClientRect();
        var feed = document.getElementById('feed').getBoundingClientRect();
        var postR = post.getBoundingClientRect();
        var credit = document.getElementById('credit').getBoundingClientRect();
        var rail = document.getElementById('rail').getBoundingClientRect();
        [feed.width, postR.width, header.width, wide.width, credit.height, rail.x, rail.width, getComputedStyle(post).display].join('|')
        "#,
    );
    let parts: Vec<&str> = shape.split('|').collect();
    let nums: Vec<f64> = parts[..7].iter().map(|s| s.parse().unwrap()).collect();
    assert!(
        (nums[0] - 577.0).abs() < 2.0 && (nums[1] - 577.0).abs() < 2.0,
        "upgraded column left the track {shape}"
    );
    assert!(
        (nums[2] - 577.0).abs() < 2.0 && (nums[3] - 964.0).abs() < 2.0,
        "header grew to the wide child {shape}"
    );
    assert!(nums[4] < 40.0, "slotted credit bar grew {shape}");
    assert!(
        (nums[5] - 601.0).abs() < 2.0 && (nums[6] - 316.0).abs() < 2.0,
        "rail {shape}"
    );
    assert_eq!(parts[7], "block", "post display {shape}");
}

/// `.m\:block` / `.m\:hidden` live in an adopted sheet, and the host is
/// narrower than 960px. The query is the viewport, so a 1280px window
/// shows the desktop span and hides the hamburger even though the host
/// column is 200px.
#[test]
fn an_adopted_sheet_matches_m_block_against_the_viewport() {
    let (mut engine, view) = loaded(
        r#"<!doctype html><style>body{margin:0}</style><faceplate-header id="host" style="display:block;width:200px"></faceplate-header>"#,
    );
    engine
        .execute_script(
            view,
            r#"
class FaceplateHeader extends HTMLElement {
  constructor() {
    super();
    var root = this.attachShadow({ mode: 'open' });
    var sheet = new CSSStyleSheet();
    sheet.replaceSync('span{display:block}.hidden{display:none}@media (min-width:960px){.m\\:block{display:block}.m\\:hidden{display:none}}');
    root.adoptedStyleSheets = [sheet];
    root.innerHTML = '<span id="desk" class="hidden m:block" style="width:2019px;height:32px"></span><span id="burger" class="m:hidden" style="width:44px;height:40px"></span>';
  }
}
customElements.define('faceplate-header', FaceplateHeader);
"#,
        )
        .expect("upgrade");
    let at = |engine: &mut Engine, width: u32| {
        engine
            .resize_view(
                view,
                Bounds {
                    x: 0,
                    y: 0,
                    width,
                    height: 800,
                },
            )
            .expect("resize");
        eval(
            engine,
            view,
            r#"
            var root = document.getElementById('host').shadowRoot;
            var desk = root.getElementById('desk').getBoundingClientRect();
            var burger = root.getElementById('burger').getBoundingClientRect();
            [desk.width, burger.width].join('|')
            "#,
        )
    };
    let narrow = at(&mut engine, 800);
    assert_eq!(
        narrow, "0|44",
        "below 960 the desktop span kept a box {narrow}"
    );
    let wide = at(&mut engine, 1280);
    assert!(
        wide.starts_with("2019|0") || wide.starts_with("2019.0|0"),
        "desktop span lost its box inside the narrow host {wide}"
    );
}

/// Reddit's feed shell, from the public page (1 Oct 2026 snapshot).
///
/// `m:` is `@media (min-width: 1200px)`, and that same query holds
/// `--flex-nav-width` (272px, or 315px with `.flex-nav-upsell`),
/// `grid-template-columns: var(--flex-nav-width) 1fr`, and
/// `max-width: calc(100vw - var(--flex-nav-width, 0px))` on
/// `.subgrid-container`. The container is also `m:w-[1120px]` with 24px
/// padding. Chrome at 1280 with the upsell: sub 315×965, shell 339×917,
/// feed 577. Without the clamp the 1120px width's content box is 1072 and
/// the feed track is 732. At 1060 every `m:` rule drops: the shell is the
/// full viewport with 16px padding, the feed is 688, the hamburger
/// (`m:hidden`) is 44px, and the search is the flex remainder, not 560px.
#[test]
fn the_reddit_shell_clamps_to_the_viewport_minus_the_nav() {
    let html = r#"<!doctype html><style>
html,body{margin:0}
.grid{display:grid;width:100%}
@media (min-width:1200px){
.grid-container:not(.grid-full),.grid-container:not(.grid-full).flex-nav-expanded{--flex-nav-width:272px}
.grid-container:not(.grid-full).flex-nav-expanded.flex-nav-upsell{--flex-nav-width:315px}
.grid-container:not(.grid-full){display:grid;width:100%;grid-template-columns:var(--flex-nav-width) 1fr}
.grid-container:not(.grid-full) .subgrid-container{max-width:calc(100vw - var(--flex-nav-width, 0px))}
}
.box-border{box-sizing:border-box}
.w-full{width:100%}
.hidden{display:none}
.flex{display:flex}
.flex-col{display:flex;flex-direction:column}
.order-2{order:2}
.mx-auto{margin-left:auto;margin-right:auto}
.flex-1{flex:1 1 0%}
nav{display:flex;align-items:center;height:56px;width:100%}
#logo{width:120px;height:40px;flex:none}
#burger{width:44px;height:40px;flex:none}
#feed,#side{height:40px}
.main-container{display:grid;width:100%;gap:1.5rem;grid-template-columns:minmax(0,1fr)}
@media (min-width:768px){
  .xs\:px-md{padding-left:1rem;padding-right:1rem}
}
@media (min-width:960px){
  .s\:\[\&\.fixed-sidebar\]\:grid-cols-\[minmax\(0\,756px\)_minmax\(0\,316px\)\].fixed-sidebar{
    grid-template-columns:minmax(0,756px) minmax(0,316px)
  }
}
@media (min-width:1200px){
  .m\:col-start-2{grid-column-start:2}
  .m\:w-\[1120px\]{width:1120px}
  .m\:px-lg{padding-left:1.5rem;padding-right:1.5rem}
  .m\:hidden{display:none}
  .m\:block{display:block}
  .m\:w-\[560px\]{width:560px}
  .m\:mx-auto{margin-left:auto;margin-right:auto}
}
</style>
<div class="grid-container theme-rpl grid flex-nav-expanded flex-nav-upsell">
  <div style="position:fixed;width:315px;height:40px"></div>
  <div id="sub" class="subgrid-container m:col-start-2 box-border flex flex-col order-2 w-full m:w-[1120px] xs:px-md m:px-lg mx-auto">
    <div id="shell" class="main-container fixed-sidebar s:[&.fixed-sidebar]:grid-cols-[minmax(0,756px)_minmax(0,316px)]">
      <div id="feed"></div><div id="side"></div>
    </div>
  </div>
</div>
<header><nav>
  <button id="burger" class="m:hidden">menu</button>
  <div id="logo"></div>
  <div id="search" class="flex-1 flex">
    <div class="flex w-full">
      <div id="box" class="flex w-full m:block m:w-[560px] m:mx-auto"><div id="bar" class="w-full" style="height:40px">Search</div></div>
    </div>
  </div>
</nav></header>"#;
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        })
        .expect("view");
    engine.load_html(view, html).expect("load");
    let measure = |engine: &mut Engine| {
        eval(
            engine,
            view,
            r#"
            function r(id){var b=document.getElementById(id).getBoundingClientRect();return [b.x,b.width].join(' ')}
            [r('sub'),r('shell'),r('feed'),r('side'),r('burger'),r('box'),r('search'),innerWidth].join('|')
            "#,
        )
    };
    let wide = measure(&mut engine);
    let near = |got: &str, expect: &[f64]| {
        let nums = numbers(&got.replace([' ', '|'], ","));
        nums.len() == expect.len()
            && nums
                .iter()
                .zip(expect)
                .all(|(g, e)| (*g - *e).abs() < 2.0)
    };
    assert!(
        near(
            &wide,
            &[
                315.0, 965.0, 339.0, 917.0, 339.0, 577.0, 940.0, 316.0, 0.0, 0.0, 420.0, 560.0,
                120.0, 1160.0, 1280.0,
            ],
        ),
        "1280 shell {wide}"
    );
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1060,
                height: 800,
            },
        )
        .expect("resize");
    let narrow = measure(&mut engine);
    assert!(
        near(
            &narrow,
            &[
                0.0, 1060.0, 16.0, 1028.0, 16.0, 688.0, 728.0, 316.0, 0.0, 44.0, 164.0, 896.0,
                164.0, 896.0, 1060.0,
            ],
        ),
        "1060 shell {narrow}"
    );
}

/// Reddit's button font is `font: var(--button-font)`, and the custom
/// property chain ends at `normal 600 0.875rem/1.25rem <family>`. The 600 is
/// the weight. Reading it as the size made every `.button-medium` 600px and
/// the join button 1528 wide, so the wrapping credit span collapsed to 0 and
/// stacked to 827. The same text, measured as max-content on the resize
/// relayout, was the search row's automatic minimum.
#[test]
fn a_button_font_variable_is_14px_semibold_not_600px() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>"#,
        r#":root{--font-sans:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;"#,
        r#"--font-14-20-semibold:normal 600 0.875rem/1.25rem var(--font-sans);"#,
        r#"--font-12-16-semibold:normal 600 0.75rem/1rem var(--font-sans);"#,
        r#"--font-button-md:var(--font-14-20-semibold);"#,
        r#"--font-button-sm:var(--font-12-16-semibold)}"#,
        r#".button{font:var(--button-font);display:inline-flex;align-items:center;height:32px;margin:0;padding:0 8px;border:0}"#,
        r#".button-medium{--button-font:var(--font-button-md)}"#,
        r#".button-small{--button-font:var(--font-button-sm)}"#,
        r#"#host{font-size:18px}#bad{--button-font:600}"#,
        r#"#cyc{--a:var(--b);--b:var(--a)}"#,
        r#"body{margin:0}header{display:flex;align-items:center;width:100%;height:56px}"#,
        r#"#search{display:flex;flex:1 1 auto;min-width:auto;height:40px;align-items:center}"#,
        r#"#row{display:flex;width:545px;align-items:flex-start}"#,
        r#"#credit{display:flex;flex-wrap:wrap;flex:1 1 auto;align-items:center;min-height:32px}"#,
        r#"</style><body>"#,
        r#"<header id="hdr"><a id="skip" class="button button-medium">Skip to main content</a>"#,
        r#"<div id="search"><button id="find" class="button button-medium">Search</button></div></header>"#,
        r#"<div id="row"><button id="join" class="button button-small">Join</button>"#,
        r#"<span id="credit"><span>u/name</span><span>1h</span><span>award</span></span></div>"#,
        r#"<p id="host"><button id="bad" class="button">Bad</button></p>"#,
        r#"<button id="miss" class="button">Miss</button>"#,
        r#"<p id="cyc"><button id="cycle" class="button" style="--button-font:var(--a)">Cycle</button></p>"#,
        r#"</body>"#,
    ));
    let fonts = eval(
        &mut engine,
        view,
        r#"[getComputedStyle(document.getElementById('find')).fontSize, getComputedStyle(document.getElementById('find')).fontWeight, getComputedStyle(document.getElementById('join')).fontSize, getComputedStyle(document.getElementById('join')).fontWeight].join('|')"#,
    );
    assert_eq!(
        fonts, "14px|600|12px|600",
        "button font shorthand parsed the weight as the size: {fonts}"
    );
    let invalid = eval(
        &mut engine,
        view,
        r#"[getComputedStyle(document.getElementById('bad')).fontSize, getComputedStyle(document.getElementById('miss')).fontSize, getComputedStyle(document.getElementById('cycle')).fontSize].join('|')"#,
    );
    assert_eq!(
        invalid, "18px|16px|16px",
        "a font var() that is not a shorthand must inherit, not keep a partial size: {invalid}"
    );
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1060,
                height: 800,
            },
        )
        .expect("resize");
    let search = rect(&mut engine, view, "search");
    let skip = rect(&mut engine, view, "skip");
    let join = rect(&mut engine, view, "join");
    let credit = rect(&mut engine, view, "credit");
    assert!(
        search.2 < 1060.0 && search.2 > 40.0,
        "search flex item kept a content minimum from the 600px face: {search:?}"
    );
    assert!(
        skip.2 < 400.0,
        "skip link is the 600px face: {skip:?}"
    );
    assert!(
        join.2 < 200.0,
        "join button squeezed the credit span: {join:?}"
    );
    assert!(
        credit.3 < 40.0 && credit.2 > 100.0,
        "credit span stacked one item per line: {credit:?} join {join:?}"
    );
}
/// Netflix's hero card is a column flex item, `width: 100%`,
/// `align-self: center`, `max-width: 31.75rem` from 960px. 31.75rem is 508px.
/// The card must stay that wide; the heading and the email form wrap inside
/// it rather than at the min-content of one word.
#[test]
fn the_netflix_hero_card_stays_508_wide_at_1060() {
    let (mut engine, view) = loaded(concat!(
        r#"<!doctype html><style>body{margin:0}"#,
        r#".hero{display:flex;align-items:flex-end;justify-content:center;width:100%}"#,
        r#".col{display:flex;flex-direction:column;flex:1}"#,
        r#".wrap{display:flex;flex-direction:column;width:100%;max-width:120rem;"#,
        r#"justify-content:center;padding:0 2rem 2rem 2rem}"#,
        r#".card{display:flex;flex-direction:column;width:100%;margin-left:auto;"#,
        r#"margin-right:auto;text-align:center;align-self:center;position:relative;"#,
        r#"max-width:100%}"#,
        r#"@media all and (min-width:600px){.card{max-width:27rem}}"#,
        r#"@media all and (min-width:960px){.card{max-width:31.75rem}}"#,
        r#"@media all and (min-width:1280px){.card{max-width:36.75rem}}"#,
        r#"h1{font-size:2.5rem;font-weight:700;margin:0}"#,
        r#"form{width:100%;max-width:36.625rem;margin:1rem auto 0}"#,
        r#"</style><body><div class="hero"><div class="col"><div class="wrap">"#,
        r#"<div class="card" id="card"><h1 id="h">Unlimited movies, TV shows, and more</h1>"#,
        r#"<form id="f"><h3>Ready to watch? Enter your email to create or restart your membership.</h3>"#,
        r#"<input id="email" type="email"></form></div></div></div></div></body>"#,
    ));
    engine
        .resize_view(
            view,
            Bounds {
                x: 0,
                y: 0,
                width: 1060,
                height: 800,
            },
        )
        .expect("resize");
    let card = rect(&mut engine, view, "card");
    let heading = rect(&mut engine, view, "h");
    let form = rect(&mut engine, view, "f");
    assert!(
        (card.2 - 508.0).abs() < 2.0,
        "hero card is not the 31.75rem max-width: {card:?} h1 {heading:?} form {form:?}"
    );
    assert!(
        heading.2 > 400.0 && form.2 > 400.0,
        "hero text wrapped to the min-content width: h1 {heading:?} form {form:?}"
    );
}
