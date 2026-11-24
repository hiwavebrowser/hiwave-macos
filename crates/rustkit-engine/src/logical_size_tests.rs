//! LOGICAL-SIZE: the flow-relative size properties (css-logical-1 §4.1) `inline-size`,
//! `block-size` and their `min-` / `max-` forms are `width`, `height`, `min-width`, `min-height`,
//! `max-width` and `max-height` in horizontal-tb. RustKit did not know them, so a box sized with
//! them was laid out auto (WPT css-position/position-absolute-in-inline-007b: its container laid
//! out 800 wide and auto tall). A logical and a physical declaration of the same size are one
//! property for the cascade: the later one wins. `getComputedStyle` reads the same value under both
//! names. Vertical writing modes are not mapped (RustKit lays out horizontal-tb only).
//!
//! Oracle: Chrome for Testing 148.0.7778.216 (win64) through
//! `tools/parity_oracle/logical_size_log.mjs`, receipt
//! `docs/diagnostics/2026-11-24/logical_size_cft148.json`: `#b` in a 400x300 `#p`, holding one
//! 50x50 block, an 800x600 viewport.

use super::*;
use crate::script_net_tests::serve_routes;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// `#b`'s border-box `width x height`, then getComputedStyle's `inline-size | block-size |
/// min-inline-size | max-inline-size | min-block-size | max-block-size`.
fn read(outer: &str, own: &str) -> String {
    read_with(
        outer,
        "",
        own,
        "String(r.width+'x'+r.height+' '+['inline-size','block-size','min-inline-size',\
         'max-inline-size','min-block-size','max-block-size']\
         .map(function(p){return c.getPropertyValue(p)}).join(' | '))",
    )
}

/// `read` with a stylesheet for `#b`, reading `expr` (with `b`, its rect `r` and its computed style
/// `c` bound).
fn read_with(outer: &str, sheet: &str, own: &str, expr: &str) -> String {
    let page = format!(
        "<!doctype html><style>body{{margin:0}} {sheet}</style><div id=p style=\"{outer}\">\
         <div id=b style=\"{own}\"><div style=\"width:50px;height:50px\"></div></div></div>"
    );
    let server = serve_routes(vec![("/", "text/html", page)]);
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        })
        .expect("view");
    let url = Url::parse(&format!("http://127.0.0.1:{}/", server.port)).unwrap();
    rt().block_on(engine.load_url(view, url)).expect("load_url");
    let value = engine
        .execute_script(
            view,
            &format!("var b=document.getElementById('b'),r=b.getBoundingClientRect(),c=getComputedStyle(b);{expr}"),
        )
        .unwrap();
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

const OUTER: &str = "width:400px;height:300px";

#[test]
fn the_logical_size_properties_size_the_box_as_chrome_does() {
    // (label, #p's style, #b's style, Chrome)
    let rows: [(&str, &str, &str, &str); 14] = [
        (
            "inline-size and block-size",
            OUTER,
            "inline-size:200px;block-size:50px",
            "200x50 200px | 50px | 0px | none | 0px | none",
        ),
        ("percentages", OUTER, "inline-size:50%;block-size:20%", "200x60 200px | 60px | 0px | none | 0px | none"),
        ("em", OUTER, "font-size:10px;inline-size:12em;block-size:3em", "120x30 120px | 30px | 0px | none | 0px | none"),
        (
            "min-inline-size over width",
            OUTER,
            "width:100px;min-inline-size:300px",
            "300x50 300px | 50px | 300px | none | 0px | none",
        ),
        ("max-inline-size on an auto width", OUTER, "max-inline-size:150px", "150x50 150px | 50px | 0px | 150px | 0px | none"),
        ("min-block-size over the content", OUTER, "min-block-size:80px", "400x80 400px | 80px | 0px | none | 80px | none"),
        (
            "max-block-size under height",
            OUTER,
            "height:100px;max-block-size:30px",
            "400x30 400px | 30px | 0px | none | 0px | 30px",
        ),
        (
            "max-inline-size: none after a limit",
            OUTER,
            "max-width:120px;max-inline-size:none",
            "400x50 400px | 50px | 0px | none | 0px | none",
        ),
        (
            "a later inline-size wins over width",
            OUTER,
            "width:100px;inline-size:200px",
            "200x50 200px | 50px | 0px | none | 0px | none",
        ),
        (
            "a later width wins over inline-size",
            OUTER,
            "inline-size:200px;width:100px",
            "100x50 100px | 50px | 0px | none | 0px | none",
        ),
        (
            "block-size: auto after height",
            OUTER,
            "height:90px;block-size:auto",
            "400x50 400px | 50px | 0px | none | 0px | none",
        ),
        (
            "inline-size: inherit",
            "width:400px;height:300px;inline-size:250px",
            "inline-size:inherit",
            "250x50 250px | 50px | 0px | none | 0px | none",
        ),
        // Under box-sizing: border-box Chrome reads inline-size / block-size as 200px / 60px;
        // RustKit reads the content box (170px / 30px) for width / height too, a separate gap
        // (BOX-SIZING-COMPUTED), so only the box is compared here.
        (
            "box-sizing: border-box",
            OUTER,
            "box-sizing:border-box;padding:10px;border:5px solid;inline-size:200px;block-size:60px",
            "200x60",
        ),
        ("inline-size: fit-content", OUTER, "inline-size:fit-content", "50x50 50px | 50px | 0px | none | 0px | none"),
    ];
    let wrong: Vec<String> = rows
        .iter()
        .filter_map(|(label, outer, own, chrome)| {
            let got = read(outer, own);
            // A row that gives only the box compares only the box.
            let got = if chrome.contains(' ') { got } else { got.split(' ').next().unwrap_or("").to_string() };
            (got != *chrome).then(|| format!("{label} ({own}): Chrome {chrome:?}, got {got:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "#b in a 400x300 box:\n{}", wrong.join("\n"));
}

/// Prometheus's R1 nits on #1442: `inherit` into a box that does not fill its parent (a float), a
/// stylesheet declaration under an inline one and `!important` over it, and `CSS.supports`.
#[test]
fn the_logical_size_properties_inherit_cascade_and_are_supported_as_in_chrome() {
    const SIZES: &str =
        "String(r.width+'x'+r.height+' '+c.getPropertyValue('inline-size')+' '+c.getPropertyValue('block-size'))";
    // (label, #p's style, stylesheet, #b's inline style, Chrome)
    let rows: [(&str, &str, &str, &str, &str); 6] = [
        ("inline-size: inherit on a float", "width:300px;height:300px", "", "float:left;inline-size:inherit", "300x50 300px 50px"),
        ("block-size: inherit", OUTER, "", "block-size:inherit", "400x300 400px 300px"),
        ("a sheet inline-size under an inline width", OUTER, "#b{inline-size:200px}", "width:100px", "100x50 100px 50px"),
        ("a sheet width under an inline inline-size", OUTER, "#b{width:100px}", "inline-size:200px", "200x50 200px 50px"),
        (
            "a sheet inline-size !important over an inline width",
            OUTER,
            "#b{inline-size:200px !important}",
            "width:100px",
            "200x50 200px 50px",
        ),
        (
            "a sheet width !important over an inline inline-size",
            OUTER,
            "#b{width:100px !important}",
            "inline-size:200px",
            "100x50 100px 50px",
        ),
    ];
    let mut wrong: Vec<String> = rows
        .iter()
        .filter_map(|(label, outer, sheet, own, chrome)| {
            let got = read_with(outer, sheet, own, SIZES);
            (got != *chrome).then(|| format!("{label}: Chrome {chrome:?}, got {got:?}"))
        })
        .collect();
    let supports = read_with(
        OUTER,
        "",
        "",
        "String([['inline-size','10px'],['block-size','10px'],['min-inline-size','10px'],\
         ['max-inline-size','none'],['min-block-size','10px'],['max-block-size','none'],['inline-size','red']]\
         .map(function(a){return CSS.supports(a[0],a[1])}).join(' '))",
    );
    if supports != "true true true true true true false" {
        wrong.push(format!("CSS.supports: Chrome \"true true true true true true false\", got {supports:?}"));
    }
    assert!(wrong.is_empty(), "#b in its parent:\n{}", wrong.join("\n"));
}
