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
