//! Z plan CH2.E31: a `{`, `}` or `;` inside a CSS string is text, not
//! structure. The rule parser ended a style rule at a `}` inside a quoted
//! declaration value (`a{content:"}"}`) and opened one at a `{` inside an
//! attribute selector's string, so the rule and everything after it went
//! wrong, in a plain `<style>` and in a `<style media>` alike.
//!
//! Expected colours are Chrome for Testing 148.0.7778.216's on the same
//! pages (headless, `getComputedStyle`): for every sheet below the `a`
//! is red and the following `b` is blue, with or without `media="all"`.

use super::*;

const RED: &str = "rgb(255, 0, 0)";
const BLUE: &str = "rgb(0, 0, 255)";

const SHEETS: &[(&str, &str)] = &[
    (r#"a{content:"}";color:rgb(255,0,0)} b{color:rgb(0,0,255)}"#, ""),
    (r#"a{content:"{;";color:rgb(255,0,0)} b{color:rgb(0,0,255)}"#, ""),
    ("a{content:'}';color:rgb(255,0,0)} b{color:rgb(0,0,255)}", ""),
    (r#"a{background-image:url("x}.png");color:rgb(255,0,0)} b{color:rgb(0,0,255)}"#, ""),
    (r#"a[title="}"]{color:rgb(255,0,0)} b{color:rgb(0,0,255)}"#, "}"),
    (r#"a[title="{"]{color:rgb(255,0,0)} b{color:rgb(0,0,255)}"#, "{"),
    (r#"a[title=";"]{color:rgb(255,0,0)} b{color:rgb(0,0,255)}"#, ";"),
    (r#"@media all{a{content:"}";color:rgb(255,0,0)} b{color:rgb(0,0,255)}}"#, ""),
    (r#"i{& a{content:"}";color:rgb(255,0,0)}} b{color:rgb(0,0,255)}"#, ""),
];

fn color_of(engine: &mut Engine, id: EngineViewId, tag: &str) -> String {
    let value = engine
        .execute_script(id, &format!("getComputedStyle(document.querySelector('{tag}')).color"))
        .expect("script");
    value
        .strip_prefix("String(\"")
        .and_then(|v| v.strip_suffix("\")"))
        .unwrap_or(&value)
        .to_string()
}

#[test]
fn braces_and_semicolons_in_css_strings_keep_the_rule_and_the_next_one() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let mut wrong = Vec::new();
    for media in ["", " media=\"all\""] {
        for (css, title) in SHEETS {
            let html = format!(
                "<style{media}>{css}</style><i><a title=\"{title}\">x</a></i><b>y</b>"
            );
            let id = engine.create_headless_view(Bounds::new(0, 0, 400, 300)).expect("view");
            engine.load_html(id, &html).expect("load");
            let (a, b) = (color_of(&mut engine, id, "a"), color_of(&mut engine, id, "b"));
            if a != RED || b != BLUE {
                wrong.push(format!("<style{media}>{css}: a {a}, b {b}"));
            }
            let _ = engine.destroy_view(id);
        }
    }
    assert!(wrong.is_empty(), "{} wrong\n{}", wrong.len(), wrong.join("\n"));
}
