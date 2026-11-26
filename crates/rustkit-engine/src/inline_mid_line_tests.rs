//! INLINE-MID-LINE: an inline element whose text does not fit the rest of the current line starts
//! on that line, after the content before it, and wraps from there (CSS2 §9.4.2: an inline box
//! is split across line boxes). RustKit moved the whole span down to the next line when it did
//! not fit; only a bare text run could start mid-line (the phase-5 split). The settings page's
//! privacy notice is this shape: `<b>…</b> <span style="font-size:0.8rem;line-height:1.5">long
//! text</span>`. Chrome for Testing 148.0.7778.216 (win64),
//! docs/diagnostics/2026-11-21/inline_mid_line_cft148.json
//! (tools/parity_oracle/inline_mid_line_log.mjs, the same markup).

use super::*;

const TEXT: &str = "HiWave analytics are 100% local and private. We track only domain names, never full URLs, \
    page titles, or search queries. All data stays on your device and is never sent to any server.";
const SMALL: &str = "font-size:12.8px;line-height:1.5";

/// (block height, span y, span height, x and y of `#t` after the span or -1) from the layout
/// export the boards read: a wrapped inline reports `fragment_union_border_box`, the union of its
/// line fragments, which is what Chrome's `getBoundingClientRect()` is.
fn measure(family: &str, inner: &str) -> (f64, f64, f64, f64, f64) {
    let page = format!(
        "<!doctype html><style>body{{margin:0}}</style><div id=d style=\"font:16px {family};width:500px\">{inner}</div>"
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 400 })
        .expect("view");
    engine.load_html(view, &page).expect("load_html");
    let file = std::env::temp_dir().join(format!(
        "inline_mid_line_{}_{}.json",
        std::process::id(),
        inner.len() * 7 + family.len()
    ));
    engine.export_layout_json(view, file.to_str().unwrap()).expect("export");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let _ = std::fs::remove_file(&file);
    fn find<'a>(v: &'a serde_json::Value, sel: &str) -> Option<&'a serde_json::Value> {
        if v.get("selector").and_then(|t| t.as_str()) == Some(sel) {
            return Some(v);
        }
        v.get("children")?.as_array()?.iter().find_map(|c| find(c, sel))
    }
    let root = &json["root"];
    let rect = |sel: &str| -> Option<(f64, f64, f64)> {
        let b = find(root, sel)?;
        let r = b.get("fragment_union_border_box").unwrap_or(&b["border_box"]);
        Some((r["x"].as_f64()?, r["y"].as_f64()?, r["height"].as_f64()?))
    };
    let block = rect("#d").expect("the block");
    let span = rect("#s").expect("the span");
    let after = rect("#t").map_or((-1.0, -1.0), |r| (r.0, r.1));
    (block.2, span.1, span.2, after.0, after.1)
}

type Chrome = (f64, f64, f64, f64, f64);

fn wrong_rows(family: &str, rows: &[(&str, String, Chrome)]) -> Vec<String> {
    rows.iter()
        .filter_map(|(label, inner, chrome)| {
            let got = measure(family, inner);
            let close = |a: f64, b: f64| (a - b).abs() < 0.1;
            let ok = close(got.0, chrome.0)
                && close(got.1, chrome.1)
                && close(got.2, chrome.2)
                && close(got.3, chrome.3)
                && close(got.4, chrome.4);
            (!ok).then(|| {
                format!("{family} {label}: Chrome (block h, span y, span h, after x, after y) {chrome:?}, got {got:?}")
            })
        })
        .collect()
}

fn rows(chrome: [Chrome; 5]) -> Vec<(&'static str, String, Chrome)> {
    let bold = format!("{}<b>bold words</b>{}", &TEXT[..60], &TEXT[60..]);
    vec![
        ("span after text", format!("<b id=n>Lead</b> <span id=s>{TEXT}</span>"), chrome[0]),
        (
            "span after text, text after it",
            format!("<b id=n>Lead</b> <span id=s>{TEXT}</span> <b id=t>tail</b>"),
            chrome[1],
        ),
        (
            "smaller span after text",
            format!("<b id=n>Lead</b> <span id=s style=\"{SMALL}\">{TEXT}</span>"),
            chrome[2],
        ),
        ("span after a bare text run", format!("Lead in <span id=s>{TEXT}</span>"), chrome[3]),
        ("span with a bold child after text", format!("<b id=n>Lead</b> <span id=s>{bold}</span>"), chrome[4]),
    ]
}

#[test]
fn a_span_that_does_not_fit_the_rest_of_the_line_starts_on_it_arial() {
    let wrong = wrong_rows(
        "Arial",
        &rows([
            (54.0, 0.0, 53.0, -1.0, -1.0),
            (54.0, 0.0, 53.0, 377.09375, 36.0),
            (57.5625, 2.0, 53.375, -1.0, -1.0),
            (54.0, 0.0, 53.0, -1.0, -1.0),
            (54.0, 0.0, 53.0, -1.0, -1.0),
        ]),
    );
    assert!(wrong.is_empty(), "a span after text, 16px block, 500px wide:\n{}", wrong.join("\n"));
}

#[cfg(windows)]
#[test]
fn a_span_that_does_not_fit_the_rest_of_the_line_starts_on_it_segoe_ui() {
    // The smaller span's row also needs WRAPPED-SPAN-STRUT (#1418: under Segoe UI the strut's
    // ascent is larger than the span's, which sits the span 2px high on every line), so it is
    // left out here until that lands; the Arial test covers the smaller span meanwhile.
    let mut rows = rows([
        (63.0, 0.0, 63.0, -1.0, -1.0),
        (63.0, 0.0, 63.0, 369.796875, 42.0),
        (63.5625, 3.0, 59.375, -1.0, -1.0),
        (63.0, 0.0, 63.0, -1.0, -1.0),
        (63.0, 0.0, 63.0, -1.0, -1.0),
    ]);
    rows.remove(2);
    let wrong = wrong_rows("'Segoe UI'", &rows);
    assert!(wrong.is_empty(), "a span after text, 16px block, 500px wide:\n{}", wrong.join("\n"));
}

// Batch 124 found two WPT cases that the mid-line start moved away from Chrome:
// css-text/letter-spacing/letter-spacing-200 (a span that starts at a line end and holds a forced
// break lost a line) and css-position/position-absolute-in-inline-007b (a span that starts mid-line
// and is split by a block counted one line too many). Chrome for Testing 148.0.7778.216 (win64),
// docs/diagnostics/2026-11-23/inline_mid_line_breaks_cft148.json
// (tools/parity_oracle/inline_mid_line_breaks_log.mjs, the same markup, Ahem).

const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");

/// The border-box height of `#p` and the height of `#s`'s fragment union (what Chrome's
/// `getBoundingClientRect()` is) in `body`, laid out with Ahem.
fn heights_with_ahem(style: &str, body: &str) -> (f64, f64) {
    use base64::Engine as _;
    let face = format!(
        "@font-face{{font-family:Ahem;src:url(data:font/ttf;base64,{})}}",
        base64::engine::general_purpose::STANDARD.encode(AHEM)
    );
    let page = format!("<!doctype html><style>{face} {style}</style>{body}");
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, &page).expect("load_html");
    let file = std::env::temp_dir().join(format!(
        "inline_mid_line_breaks_{}_{}.json",
        std::process::id(),
        body.len() * 7 + style.len()
    ));
    engine.export_layout_json(view, file.to_str().unwrap()).expect("export");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let _ = std::fs::remove_file(&file);
    fn find<'a>(v: &'a serde_json::Value, sel: &str) -> Option<&'a serde_json::Value> {
        if v.get("selector").and_then(|t| t.as_str()) == Some(sel) {
            return Some(v);
        }
        v.get("children")?.as_array()?.iter().find_map(|c| find(c, sel))
    }
    let p = find(&json["root"], "#p").expect("#p");
    let s = find(&json["root"], "#s").expect("#s");
    let union = s.get("fragment_union_border_box").unwrap_or(&s["border_box"]);
    (
        p["border_box"]["height"].as_f64().expect("height"),
        union["height"].as_f64().expect("union height"),
    )
}

#[test]
fn a_span_that_starts_at_a_line_end_with_a_forced_break_or_split_by_a_block_keeps_its_lines() {
    const LS: &str =
        "body{margin:0} .c{font:20px/1 Ahem;width:5em} span{letter-spacing:1em} p{margin:0} .pre{white-space:pre-wrap}";
    const POS: &str = "body{margin:0} .c{font:20px/1 Ahem;inline-size:16em} .rel{position:relative} \
        .block{inline-size:8em;block-size:1em;background:black}";
    const SPLIT: &str = "XX\n  <span id=s class=\"rel\">\n    XXXX<br>\n    <div class=\"block\"></div>\n    \
        XXXXX<br>\n    XXXXXXXX<br>\n    XXXXX<br>\n    XXXXX\n  </span>";
    // The rows INLINE-MID-LINE must get right. In the same boxes Chrome also gives 80 for
    // `1 2 3<span><br>4 5<br></span>6 7 8`, `1 2 3<br><span>4 5</span><br>6 7 8` and, under
    // pre-wrap, `1 2 3\n<span>4 5</span>\n6 7 8` (the letter-spaced "4 5" wraps to two lines);
    // RustKit gives 60 for those with or without INLINE-MID-LINE (develop b9b7a4ec too), a
    // separate gap: the letter spacing is not counted where the span wraps.
    // position-absolute-in-inline-007b: two columns 3em tall with a 5px border; Chrome is 70 tall.
    // The WPT file sizes the boxes with inline-size / block-size, which RustKit does not support
    // (the container is laid out 800 wide and auto tall, 110 on develop b9b7a4ec), so this row uses
    // width / height, which Chrome lays out identically. Develop and INLINE-MID-LINE both give 70.
    // Not asserted: without columns Chrome is 120 and RustKit 100 on both arms (the line after the
    // block is lost), and a block-split span's fragment union reports its first fragment (20 tall,
    // Chrome 120 and 60) on both arms.
    let phys = POS.replace("inline-size", "width").replace("block-size", "height");
    let phys_cols = format!("{phys} .cols{{height:3em;columns:2;column-fill:auto;column-gap:0;border:5px solid gold}}");
    // (label, style, body, Chrome: height of #p, and of #s's fragment union where asserted)
    let rows: [(&str, &str, String, (f64, Option<f64>)); 2] = [
        (
            "span starting at a line end, holding newlines (pre-wrap)",
            LS,
            "<div class=c><p id=p class=pre>1 2 3<span id=s>\n4 5\n</span>6 7 8</p></div>".into(),
            (80.0, Some(40.0)),
        ),
        (
            "007b: span after text, split by a block, two columns",
            &phys_cols,
            format!("<div class=\"c cols\" id=p>{SPLIT}</div>"),
            (70.0, None),
        ),
    ];
    let wrong: Vec<String> = rows
        .iter()
        .filter_map(|(label, style, body, chrome)| {
            let got = heights_with_ahem(style, body);
            let union_off = chrome.1.is_some_and(|u| (got.1 - u).abs() >= 0.1);
            ((got.0 - chrome.0).abs() >= 0.1 || union_off)
                .then(|| format!("{label}: Chrome (height, span union height) {chrome:?}, got {got:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "Ahem 20px/1:\n{}", wrong.join("\n"));
}

// Prometheus (HOLD at 61853644): in WPT css-text/letter-spacing/letter-spacing-200's box 3 the
// mid-line start put a span on the wrong line without always changing a height: the pre-wrap
// row's `4 5` drawn at the end of `1 2 3`, and `456` (with Arial) or `七` (with Ahem) drawn on
// the line before its own. So these pin the lines themselves, paragraph by paragraph: each line's
// characters, in order, whitespace dropped. Chrome for Testing 148.0.7778.216 (win64),
// docs/diagnostics/2026-11-26/inline_mid_line_rows_cft148.json
// (tools/parity_oracle/inline_mid_line_rows_log.mjs, the same markup; the paragraphs' ids are
// added here to find them in the layout export).

/// letter-spacing-200's box 3 as written, in `face`, its paragraphs `#r1`..`#r4`.
fn letter_spacing_200_box_3(face: &str) -> String {
    format!(
        "<style>body{{margin:0}} .contain{{font:20px/1 {face};width:5em;border:solid blue;margin:1em;float:left}} \
         span,.test{{letter-spacing:1em}} .pre,.control p{{white-space:pre-wrap}} p{{margin:0}}</style>\
         <div class=\"contain\"><p id=r1><span>1 2</span></p><p id=r2>1 2 \u{4e09}<span>456</span>\u{4e03} 8 9</p>\
         <p id=r3>1 2 3<br><span>4 5</span><br>6 7 8</p><p id=r4 class=\"pre\">1 2 3\n<span>4 5</span>\n6 7 8</p></div>"
    )
}

/// Each of `#r1`..`#r4`'s painted lines, top to bottom: the text commands inside the paragraph's
/// box grouped by their middle's y, each line its characters left to right with whitespace dropped.
fn painted_rows(page: &str) -> Vec<Vec<String>> {
    use base64::Engine as _;
    let face = format!(
        "<style>@font-face{{font-family:Ahem;src:url(data:font/ttf;base64,{})}}</style>",
        base64::engine::general_purpose::STANDARD.encode(AHEM)
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine.load_html(view, &format!("<!doctype html>{face}{page}")).expect("load_html");
    let file = std::env::temp_dir().join(format!("inline_mid_line_rows_{}_{}.json", std::process::id(), page.len()));
    engine.export_layout_json(view, file.to_str().unwrap()).expect("export");
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let _ = std::fs::remove_file(&file);
    fn find<'a>(v: &'a serde_json::Value, sel: &str) -> Option<&'a serde_json::Value> {
        if v.get("selector").and_then(|t| t.as_str()) == Some(sel) {
            return Some(v);
        }
        v.get("children")?.as_array()?.iter().find_map(|c| find(c, sel))
    }
    let bands: Vec<(f64, f64)> = (1..=4)
        .map(|i| {
            let b = &find(&json["root"], &format!("#r{i}")).expect("paragraph")["border_box"];
            let y = b["y"].as_f64().unwrap();
            (y, y + b["height"].as_f64().unwrap())
        })
        .collect();
    let list = engine.views[&view].display_list.as_ref().expect("display list");
    let mut runs: Vec<(f64, f32, String)> = list
        .commands
        .iter()
        .filter_map(|c| match c {
            // Banded by the glyphs' middle: `y` is the top of the face's content area, which
            // with Arial (22.3px at 20px) sits above a 20px line box's top.
            rustkit_layout::DisplayCommand::Text { text, x, y, font_size, .. } => {
                let chars: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                (!chars.is_empty()).then(|| (((*y + *font_size / 2.0) as f64).round(), *x, chars))
            }
            _ => None,
        })
        .collect();
    runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    bands
        .iter()
        .map(|&(top, bottom)| {
            let mut lines: Vec<(f64, String)> = Vec::new();
            for (y, _, chars) in runs.iter().filter(|r| r.0 >= top && r.0 < bottom) {
                match lines.last_mut() {
                    Some((ly, s)) if ly == y => s.push_str(chars),
                    _ => lines.push((*y, chars.clone())),
                }
            }
            lines.into_iter().map(|(_, s)| s).collect()
        })
        .collect()
}

fn rows_of(chrome: &[&[&str]]) -> Vec<Vec<String>> {
    chrome.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect()
}

#[test]
fn letter_spacing_200_box_3_keeps_each_span_on_its_own_lines_arial() {
    let got = painted_rows(&letter_spacing_200_box_3("Arial"));
    let chrome = rows_of(&[
        &["12"],
        &["12\u{4e09}", "456", "\u{4e03}89"],
        &["123", "45", "678"],
        &["123", "45", "678"],
    ]);
    assert_eq!(got, chrome, "letter-spacing-200 box 3, Arial: each paragraph's lines");
}

/// With Ahem the second paragraph is the mid-line start's: `456` and `七 8 9` each on a line of
/// their own. The other three paragraphs are left out: RustKit keeps `1 2` and `4 5` on one line
/// where Chrome breaks them at the space (each is 120px with its letter-spacing in a 100px box),
/// on develop as on this branch; that is a letter-spacing row of its own, not this change's.
#[test]
fn letter_spacing_200_box_3_keeps_each_span_on_its_own_lines_ahem() {
    let got = painted_rows(&letter_spacing_200_box_3("Ahem"));
    let chrome = rows_of(&[&["12\u{4e09}", "456", "\u{4e03}89"]]);
    assert_eq!(got[1..2], chrome[..], "letter-spacing-200 box 3, Ahem: the second paragraph's lines (all: {got:?})");
}
