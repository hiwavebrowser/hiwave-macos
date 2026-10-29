//! `getComputedStyle(element).content`. `computed_style_lines` had no
//! `content` row, so script read an empty string. netflix's hero form picks
//! its layout from it: the wrapper says `content: '1'`, and `content: '2'`
//! from its medium breakpoint up, and `useSplitForm` keeps the form stacked
//! (`data-issplitform="true"`) while the digits read `1`, or nothing at all.
//! Chrome 148 at 1280x800 reads `"2"` and puts the field and button in one
//! row (Pete's hand test 10-27, HT27 N2).

use super::*;

#[test]
#[cfg(all(target_os = "macos", feature = "headless"))]
fn computed_content_is_the_cascaded_string_or_normal() {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds::new(0, 0, 1280, 800))
        .expect("headless view");
    engine
        .load_html(
            id,
            r#"<!doctype html><style>#w{content:'1'} @media (min-width: 600px){#w{content:'2'}}
               #q{content:"a\"b"} #n{content:none} #l{content:"a\a b"}</style>
               <body><div id=w><form id=f>x</form></div><div id=q>x</div><div id=n>x</div><div id=d>x</div><div id=l>x</div></body>"#,
        )
        .expect("load_html");
    let value = engine
        .execute_script(
            id,
            "['w', 'f', 'q', 'n', 'd', 'l'].map(function (i) { return i + '=' + \
             getComputedStyle(document.getElementById(i)).content; }).join(' ')",
        )
        .expect("script");
    // Chrome 148.0.7778.216, same page; a resolved `\a` serialises back as
    // `\a ` (CSSOM), never as a raw line break that would split the row.
    let chrome = r#"w="2" f=normal q="a\"b" n=normal d=normal l="a\a b""#;
    assert_eq!(value, format!("String({chrome:?})"));
}
