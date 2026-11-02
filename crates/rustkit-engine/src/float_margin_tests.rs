//! Plan row C10 (a), ledger #474: a float's margins do not collapse with the
//! block before it. Expected `y` of `#f` and `#n` are Chromium 143.0.7499.4 on
//! the same pages (`trench/z/tools/c10/float_margin_probe.mjs` on the hub).

use super::*;

const CASES: &[(&str, &str, &str)] = &[
    ("block mb, float mt", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div>", "60 -"),
    ("block mb smaller than float mt", "<div id=a style=\"height:20px;margin-bottom:10px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:30px\"></div>", "60 -"),
    ("block mb, float none", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;\"></div>", "50 -"),
    ("block none, float mt", "<div id=a style=\"height:20px;\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div>", "30 -"),
    ("negative block mb, float mt", "<div id=a style=\"height:20px;margin-bottom:-5px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div>", "25 -"),
    ("block mb, negative float mt", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:-10px\"></div>", "40 -"),
    ("then a block with mt", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div><div id=n style=\"height:10px;margin-top:40px\"></div>", "60 60"),
    ("then a block with smaller mt", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div><div id=n style=\"height:10px;margin-top:5px\"></div>", "60 50"),
    ("then a block with no mt", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;\"></div><div id=n style=\"height:10px;\"></div>", "50 50"),
    ("float mb then block", "<div id=a style=\"height:20px;\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-bottom:30px\"></div><div id=n style=\"height:10px;margin-top:5px\"></div>", "20 25"),
    ("right float", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:right;width:50px;height:50px;margin-top:10px\"></div>", "60 -"),
    ("two floats", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div><div id=n style=\"float:left;width:50px;height:50px;margin-top:20px\"></div>", "60 70"),
    ("clear after float", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div><div id=n style=\"height:10px;clear:both;margin-top:5px\"></div>", "60 110"),
    ("in a parent", "<div style=\"margin-top:7px\"><div id=a style=\"height:20px;margin-bottom:30px\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div></div>", "67 -"),
    ("float first in a parent with mt", "<div style=\"margin-top:7px\"><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div><div id=n style=\"height:10px;\"></div></div>", "17 7"),
    ("float first in body", "<div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div><div id=n style=\"height:10px;margin-top:15px\"></div>", "25 15"),
    ("empty block between", "<div id=a style=\"height:20px;margin-bottom:30px\"></div><div style=\"margin:12px 0\"></div><div id=f style=\"float:left;width:50px;height:50px;margin-top:10px\"></div>", "60 -"),
];

#[test]
fn a_float_keeps_its_margin_and_the_blocks_as_chromium_places_them() {
    let wrong: Vec<String> = CASES
        .iter()
        .filter_map(|(name, body, want)| {
            let mut engine = Engine::new(EngineConfig::default()).expect("engine");
            let view = engine
                .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
                .expect("view");
            let page = format!("<!doctype html><style>body{{margin:0}}</style>{body}");
            engine.load_html(view, &page).expect("load");
            let got = engine
                .execute_script(
                    view,
                    "['f', 'n'].map(function (i) { var e = document.getElementById(i); \
                     return e ? e.getBoundingClientRect().y : '-'; }).join(' ')",
                )
                .expect("script");
            let got = got.trim_start_matches("String(\"").trim_end_matches("\")").to_string();
            (got != *want).then(|| format!("{name}: {got}, Chromium {want}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{} of {} wrong:\n{}", wrong.len(), CASES.len(), wrong.join("\n"));
}

/// An empty block's margins collapse through it, with what is pending before
/// it and what comes after. It sat at its top margin with only its bottom one
/// pending, so `20 + 30`, then 12 more. Expected are the `y` of `#e` and `#n` and
/// the height of `#p`, Chromium 143 (`trench/z/tools/c10/empty_through_probe.mjs`).
const THROUGH: &[(&str, &str, &str)] = &[
    ("then a block", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div><div id=n style=\"height:10px;\"></div>", "50 50 -"),
    ("then a block with a larger mt", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div><div id=n style=\"height:10px;margin-top:40px\"></div>", "50 60 -"),
    ("larger bottom than the block before", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0 45px\"></div><div id=n style=\"height:10px;\"></div>", "50 65 -"),
    ("larger top than the block before", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:45px 0 12px\"></div><div id=n style=\"height:10px;\"></div>", "65 65 -"),
    ("two empties", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div><div style=\"margin:35px 0\"></div><div id=n style=\"height:10px;\"></div>", "50 55 -"),
    ("negative bottom", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:0 0 -5px\"></div><div id=n style=\"height:10px;\"></div>", "50 45 -"),
    ("nothing before", "<div id=e style=\"margin:12px 0\"></div><div id=n style=\"height:10px;\"></div>", "12 12 -"),
    ("height 0", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;height:0\"></div><div id=n style=\"height:10px;\"></div>", "50 50 -"),
    ("min-height", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;min-height:1px\"></div><div id=n style=\"height:10px;\"></div>", "50 63 -"),
    ("border stops it", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;border-top:1px solid\"></div><div id=n style=\"height:10px;\"></div>", "50 63 -"),
    ("padding stops it", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;padding-bottom:1px\"></div><div id=n style=\"height:10px;\"></div>", "50 63 -"),
    ("overflow hidden stops it", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;overflow:hidden\"></div><div id=n style=\"height:10px;\"></div>", "50 62 -"),
    ("display flex stops it", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;display:flex\"></div><div id=n style=\"height:10px;\"></div>", "50 62 -"),
    ("height 1 stops it", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;height:1px\"></div><div id=n style=\"height:10px;\"></div>", "50 63 -"),
    ("clear none after no float", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0;clear:both\"></div><div id=n style=\"height:10px;\"></div>", "50 50 -"),
    ("holds an abspos box", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"><div style=\"position:absolute;width:5px;height:5px\"></div></div><div id=n style=\"height:10px;\"></div>", "50 50 -"),
    ("holds an empty block", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"><div style=\"margin:50px 0\"></div></div><div id=n style=\"height:10px;\"></div>", "70 70 -"),
    ("last in a parent", "<div id=p><div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div></div><div id=n style=\"height:10px;\"></div>", "50 50 20"),
    ("last in a parent with a border", "<div id=p style=\"border-bottom:1px solid\"><div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div></div><div id=n style=\"height:10px;\"></div>", "50 51 51"),
    ("last in a fixed-height parent", "<div id=p style=\"height:60px\"><div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div></div><div id=n style=\"height:10px;\"></div>", "50 60 60"),
    ("first in a parent", "<div id=p style=\"margin-top:7px\"><div id=e style=\"margin:12px 0\"></div><div id=n style=\"height:10px;\"></div></div>", "12 12 10"),
    ("first in a parent with a border", "<div id=p style=\"border-top:1px solid;margin-top:7px\"><div id=e style=\"margin:12px 0\"></div><div id=n style=\"height:10px;\"></div></div>", "20 20 23"),
    ("only child of a parent", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=p style=\"margin:5px 0\"><div id=e style=\"margin:12px 0\"></div></div><div id=n style=\"height:10px;\"></div>", "50 50 0"),
    ("then a float", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div><div id=n style=\"float:left;width:50px;height:50px;margin-top:10px\"></div>", "50 60 -"),
    ("then text", "<div style=\"height:20px;margin-bottom:30px\"></div><div id=e style=\"margin:12px 0\"></div><div id=n>x</div>", "50 50 -"),
];

#[test]
fn margins_collapse_through_an_empty_block() {
    let wrong: Vec<String> = THROUGH
        .iter()
        .filter_map(|(name, body, want)| {
            let mut engine = Engine::new(EngineConfig::default()).expect("engine");
            let view = engine
                .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
                .expect("view");
            let page = format!("<!doctype html><style>body{{margin:0}}</style>{body}");
            engine.load_html(view, &page).expect("load");
            let got = engine
                .execute_script(
                    view,
                    "var y = function (i) { var e = document.getElementById(i); \
                     return e ? e.getBoundingClientRect().y : '-'; }; \
                     var p = document.getElementById('p'); \
                     [y('e'), y('n'), p ? p.getBoundingClientRect().height : '-'].join(' ')",
                )
                .expect("script");
            let got = got.trim_start_matches("String(\"").trim_end_matches("\")").to_string();
            (got != *want).then(|| format!("{name}: {got}, Chromium {want}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{} of {} wrong:\n{}", wrong.len(), THROUGH.len(), wrong.join("\n"));
}

