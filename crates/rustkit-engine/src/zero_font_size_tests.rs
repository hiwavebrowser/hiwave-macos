//! `font-size: 0`, against the pinned Chromium.
//!
//! The usual way to remove the spaces between inline-blocks is `font-size: 0`
//! on their parent, and a page hides a label the same way. `0` parses to
//! `Length::Zero`, and layout read every font size that is not `Px` as 16px:
//! a line holding only a 10px inline-block was 18 tall, and text meant to be
//! hidden took its full width and height.
//!
//! Each page has the border boxes Chromium 143 reports for its elements with
//! an id, as `id:x:y:width:height` (rounded `getBoundingClientRect`). No page
//! has text at a size other than 0, so no shaper decides anything and the
//! pages are checked on every OS, laid out as the nested layout matrix lays
//! its pages out.

use super::*;

const IB: &str = r#"<i style="display:inline-block;width:100px;height:10px"></i>"#;

/// (name, extra css, body with `{ib}` for the inline-block, Chromium's boxes)
const PAGES: &[(&str, &str, &str, &str)] = &[
    ("a block around an inline-block", "", "<div id=x>{ib}</div>", "x:0:0:1280:10"),
    (
        "a flex row around a block",
        "#x{display:flex}",
        "<div id=x><div id=m>{ib}</div></div>",
        "x:0:0:1280:10 m:0:0:100:10",
    ),
    (
        "an inline-block around a block",
        "#f{display:inline-block}",
        "<div id=x><div id=f><div id=a>{ib}</div></div></div>",
        "x:0:0:1280:10 f:0:0:100:10 a:0:0:100:10",
    ),
    (
        "an inline-block around a grid",
        "#f{display:inline-block}#m{display:grid}",
        "<div id=x><div id=f><div id=m><div id=a>{ib}</div></div></div></div>",
        "x:0:0:1280:10 f:0:0:100:10 m:0:0:100:10 a:0:0:100:10",
    ),
    (
        "text alone takes no room",
        "#x{float:left}",
        "<div id=x><span id=m>hidden words</span></div>",
        "x:0:0:0:0 m:0:0:0:0",
    ),
    (
        "text beside an inline-block takes no room",
        "#x{float:left}",
        "<div id=x>words {ib}</div>",
        "x:0:0:100:10",
    ),
    (
        "an em of zero is zero",
        "#f{float:left}#a{font-size:2em}",
        "<div id=x><div id=f><span id=a>gone</span></div></div>",
        "x:0:0:1280:0 f:0:0:0:0 a:0:0:0:0",
    ),
    (
        "zero from a style attribute under a sized parent",
        "",
        r#"<div id=x style="font-size:16px"><div id=m style="font-size:0">{ib}</div></div>"#,
        "x:0:0:1280:10 m:0:0:1280:10",
    ),
    (
        "zero on a ::before",
        r#"#x{font-size:16px}#m{font-size:0}#m::before{content:"";display:inline-block;width:5px;height:5px;font-size:0}"#,
        "<div id=x><div id=m></div></div>",
        "x:0:0:1280:5 m:0:0:1280:5",
    ),
    // An atomic inline inside an inline box. With no strut left the box
    // around it must still give the line its height: a linked logo image in
    // a `font-size: 0` row was 0 tall, and the next block was drawn over it.
    (
        "an image in a link, then a block",
        "",
        r#"<div id=x><a href=#><img style="width:40px;height:30px" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a></div><div id=y style="height:5px"></div>"#,
        "x:0:0:1280:30 y:0:30:1280:5",
    ),
    (
        "an inline-block in a span, then a block",
        "",
        r#"<div id=x><span>{ib}</span></div><div id=y style="height:5px"></div>"#,
        "x:0:0:1280:10 y:0:10:1280:5",
    ),
    (
        "an inline-block three inline boxes deep",
        "",
        r#"<div id=x><span><b><em>{ib}</em></b></span></div><div id=y style="height:5px"></div>"#,
        "x:0:0:1280:10 y:0:10:1280:5",
    ),
    (
        "a taller inline-block in a span beside one outside",
        "",
        r#"<div id=x>{ib}<span><i style="display:inline-block;width:50px;height:30px"></i></span></div><div id=y style="height:5px"></div>"#,
        "x:0:0:1280:30 y:0:30:1280:5",
    ),
    (
        "the margin box of an inline-block in a span",
        "",
        r#"<div id=x><span><i style="display:inline-block;width:50px;height:10px;margin:5px 0"></i></span></div><div id=y style="height:5px"></div>"#,
        "x:0:0:1280:20 y:0:20:1280:5",
    ),
    (
        "an inline-block in a span on a zero line-height at 16px",
        "#x{font-size:16px;line-height:0}",
        r#"<div id=x><span>{ib}</span></div><div id=y style="height:5px"></div>"#,
        "x:0:0:1280:10 y:0:10:1280:5",
    ),
    (
        "two lines of inline-blocks in a span",
        "#x{width:150px}",
        r#"<div id=x><span>{ib}{ib}</span></div><div id=y style="height:5px"></div>"#,
        "x:0:0:150:20 y:0:20:1280:5",
    ),
    (
        "an image in a link in a flex item",
        "#f{display:flex}",
        r#"<div id=f><div id=x><a href=#><img style="width:40px;height:30px" src="data:image/gif;base64,R0lGODlhAQABAAAAACw="></a></div></div><div id=y style="height:5px"></div>"#,
        "f:0:0:1280:30 x:0:0:40:30 y:0:30:1280:5",
    ),
];

/// Pages that do not match Chromium yet. One that starts to match fails the
/// test, so the list cannot go stale.
///
/// `two lines of inline-blocks in a span`: an inline box lays its children
/// out in one row and does not break across lines, so the second
/// inline-block stays beside the first, past the 150px block, and the block
/// is one line tall (10; Chromium 20).
const GAPS: &[&str] = &["two lines of inline-blocks in a span"];

#[test]
fn a_zero_font_size_is_zero_and_not_sixteen() {
    let engine = Engine::new(EngineConfig::default()).expect("engine");
    // The style memo is keyed by the document's address: keep every
    // document alive so no later page can reuse an earlier one's address.
    let mut documents = Vec::new();
    let mut wrong = Vec::new();
    let mut gaps_that_pass = Vec::new();
    for (name, css, body, chrome) in PAGES {
        let html = format!(
            "<!doctype html><html><head><style>body{{margin:0;font-size:0}}{css}</style></head><body>{}</body></html>",
            body.replace("{ib}", IB)
        );
        let ids: Vec<&str> = chrome.split(' ').map(|b| b.split(':').next().unwrap()).collect();
        let document = Document::parse_html(&html).expect("parse");
        let rects = super::nested_layout_matrix_tests::engine_rects(&engine, &document, &ids);
        documents.push(document);
        let got = ids
            .iter()
            .zip(&rects)
            .map(|(id, r)| match r {
                Some(r) => format!("{id}:{}:{}:{}:{}", r[0].round(), r[1].round(), r[2].round(), r[3].round()),
                None => format!("{id}:none"),
            })
            .collect::<Vec<_>>()
            .join(" ");
        match (got == *chrome, GAPS.contains(name)) {
            (false, false) => wrong.push(format!("{name}\n   engine {got}\n   chrome {chrome}")),
            (true, true) => gaps_that_pass.push(*name),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{} of {} wrong:\n{}", wrong.len(), PAGES.len(), wrong.join("\n"));
    assert!(
        gaps_that_pass.is_empty(),
        "listed as a gap but matches Chrome now, take it off the list: {gaps_that_pass:?}"
    );
}
