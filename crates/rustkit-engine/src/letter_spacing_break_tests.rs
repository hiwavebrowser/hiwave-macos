//! LETTER-SPACING-BREAK: a line of text that carries `letter-spacing` or `word-spacing` breaks
//! where its spaced width no longer fits (CSS Text 3 §8: the spacing is part of the content's
//! width). RustKit fitted lines with the shaper's widths and added the spacing when it shaped the
//! runs afterwards, so a line it thought fit could be wider than its box: `1 2` at
//! `letter-spacing:1em` in a 5em Ahem box stayed on one line (WPT
//! css-text/letter-spacing/letter-spacing-200, box 3 206px tall where Chrome's is 266).
//! Chrome for Testing 148.0.7778.216 (win64), docs/diagnostics/2026-11-26/letter_spacing_break_cft148.json
//! (tools/parity_oracle/letter_spacing_break_log.mjs, the same markup). Each case compares `#p`'s
//! border-box height and painted lines: the characters on each line, in order, whitespace dropped.

use super::*;

const AHEM: &[u8] = include_bytes!("../../rustkit-text/tests/fixtures/Ahem.ttf");
const SENTENCE: &str = "The quick brown fox jumps over the lazy dog and keeps on running far away";
const AH: &str = "body{margin:0} #p{font:20px/1 Ahem;width:100px}";
const AR: &str = "body{margin:0} #p{font:16px/1.25 Arial;width:200px}";
const WPT: &str = "body{margin:0} .contain{font:20px/1 Ahem;width:5em;border:solid blue;margin:1em;float:left} \
    span,.test{letter-spacing:1em} .pre,.control p{white-space:pre-wrap} p{margin:0}";
const HEAD: &str = "<p><span>1 2</span></p><p>1 2 \u{4e09}<span>456</span>\u{4e03} 8 9</p>";

type Case<'a> = (&'a str, String, String, f64, &'a [&'a str]);

/// `#p`'s border-box height and painted lines, top to bottom: the text commands inside its box
/// grouped by the y of their middle (`y` is the top of the face's content area, which with Arial
/// is above the line box's), each line its characters left to right with whitespace dropped.
fn lines(style: &str, body: &str) -> (f64, Vec<String>) {
    use base64::Engine as _;
    let face = format!(
        "@font-face{{font-family:Ahem;src:url(data:font/ttf;base64,{})}}",
        base64::engine::general_purpose::STANDARD.encode(AHEM)
    );
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine
        .load_html(view, &format!("<!doctype html><style>{face} {style}</style>{body}"))
        .expect("load_html");
    let file = std::env::temp_dir().join(format!(
        "letter_spacing_break_{}_{}.json",
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
    let b = &find(&json["root"], "#p").expect("#p")["border_box"];
    let (top, height) = (b["y"].as_f64().unwrap(), b["height"].as_f64().unwrap());
    let list = engine.views[&view].display_list.as_ref().expect("display list");
    let mut runs: Vec<(f64, f32, String)> = list
        .commands
        .iter()
        .filter_map(|c| match c {
            rustkit_layout::DisplayCommand::Text { text, x, y, font_size, .. } => {
                let chars: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                let mid = ((*y + *font_size / 2.0) as f64).round();
                (!chars.is_empty() && mid >= top && mid < top + height).then_some((mid, *x, chars))
            }
            _ => None,
        })
        .collect();
    runs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut out: Vec<(f64, String)> = Vec::new();
    for (y, _, chars) in runs {
        match out.last_mut() {
            Some((ly, s)) if *ly == y => s.push_str(&chars),
            _ => out.push((y, chars)),
        }
    }
    (height, out.into_iter().map(|(_, s)| s).collect())
}

/// The cases whose `(height, lines)` differ from Chrome's.
fn wrong(cases: &[Case]) -> Vec<String> {
    cases
        .iter()
        .filter_map(|(label, style, body, height, chrome)| {
            let got = lines(style, body);
            let ok = (got.0 - height).abs() < 0.5 && got.1 == *chrome;
            (!ok).then(|| format!("{label}: Chrome ({height}, {chrome:?}), got {got:?}"))
        })
        .collect()
}

#[test]
fn letter_spaced_and_word_spaced_ahem_breaks_where_the_spaced_width_no_longer_fits() {
    let block = |extra: &str| format!("{AH} #p{{{extra}}}");
    let cases: Vec<Case> = vec![
        ("letter-spacing 1em, \"1 2\"", block("letter-spacing:1em"), "<div id=p>1 2</div>".into(), 40.0, &["1", "2"]),
        (
            "letter-spacing 1em, \"1 2 3 4\"",
            block("letter-spacing:1em"),
            "<div id=p>1 2 3 4</div>".into(),
            80.0,
            &["1", "2", "3", "4"],
        ),
        (
            "letter-spacing 1em on a span, \"1 2\"",
            format!("{AH} span{{letter-spacing:1em}}"),
            "<div id=p><span>1 2</span></div>".into(),
            40.0,
            &["1", "2"],
        ),
        ("word-spacing 40px, \"1 2 3\"", block("word-spacing:40px"), "<div id=p>1 2 3</div>".into(), 40.0, &["12", "3"]),
    ];
    let wrong = wrong(&cases);
    assert!(wrong.is_empty(), "Ahem 20px/1, 100px wide:\n{}", wrong.join("\n"));
}

#[test]
fn letter_spaced_and_word_spaced_arial_breaks_where_the_spaced_width_no_longer_fits() {
    let block = |extra: &str| format!("{AR} #p{{{extra}}}");
    let body = format!("<div id=p>{SENTENCE}</div>");
    let cases: Vec<Case> = vec![
        (
            "letter-spacing 2px",
            block("letter-spacing:2px"),
            body.clone(),
            80.0,
            &["Thequickbrownfox", "jumpsoverthelazy", "dogandkeepson", "runningfaraway"],
        ),
        (
            "letter-spacing 0.2em",
            block("letter-spacing:0.2em"),
            body.clone(),
            100.0,
            &["Thequickbrown", "foxjumpsoverthe", "lazydogandkeeps", "onrunningfar", "away"],
        ),
        (
            "word-spacing 8px",
            block("word-spacing:8px"),
            body.clone(),
            80.0,
            &["Thequickbrownfox", "jumpsoverthelazy", "dogandkeepson", "runningfaraway"],
        ),
        (
            "letter-spacing -1px",
            block("letter-spacing:-1px"),
            body.clone(),
            60.0,
            &["Thequickbrownfoxjumpsover", "thelazydogandkeepson", "runningfaraway"],
        ),
        (
            "no spacing (control)",
            AR.to_string(),
            body.clone(),
            60.0,
            &["Thequickbrownfoxjumps", "overthelazydogand", "keepsonrunningfaraway"],
        ),
    ];
    let wrong = wrong(&cases);
    assert!(wrong.is_empty(), "Arial 16px/1.25, 200px wide:\n{}", wrong.join("\n"));
}

#[test]
fn letter_spacing_200_boxes_2_and_3_as_written() {
    let chrome: &[&str] =
        &["1", "2", "12\u{4e09}", "456", "\u{4e03}89", "123", "4", "5", "678", "123", "4", "5", "678"];
    let box2 = format!(
        "<div class=\"contain\" id=p>{HEAD}<p>1 2 3<span><br>4 5<br></span>6 7 8</p>\
         <p class=\"pre\">1 2 3<span>\n4 5\n</span>6 7 8</p></div>"
    );
    let box3 = format!(
        "<div class=\"contain\" id=p>{HEAD}<p>1 2 3<br><span>4 5</span><br>6 7 8</p>\
         <p class=\"pre\">1 2 3\n<span>4 5</span>\n6 7 8</p></div>"
    );
    let cases: Vec<Case> = vec![("box 3", WPT.to_string(), box3, 266.0, chrome)];
    let wrong = wrong(&cases);
    assert!(wrong.is_empty(), "letter-spacing-200:\n{}", wrong.join("\n"));
    // Box 2 up to its pre-wrap paragraph, `1 2 3<span>\n4 5\n</span>6 7 8`: there a preserved
    // newline at the end of the span does not end the line for the text after it (unspaced too:
    // Chrome 148 `123` / `45` / `678`, RustKit `123` / `45678` at 300px), a row of its own.
    let (_, got) = lines(WPT, &box2);
    let want: Vec<String> = chrome[..9].iter().map(|s| s.to_string()).collect();
    assert_eq!(got.get(..9), Some(&want[..]), "letter-spacing-200 box 2, its first three paragraphs: {got:?}");
}
