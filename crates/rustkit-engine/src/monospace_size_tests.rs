//! TX2: the generic `monospace` family alone takes the fixed-font default
//! size, 13px, where every other family takes 16px.
//!
//! Blink keeps whether a font-size is absolute and which keyword it came
//! from. When an element's family is the bare generic `monospace` and its
//! parent's is not (or the other way round), a size that is not absolute is
//! recomputed: a keyword from the fixed-font table (medium 13, small 12,
//! large 16), anything else (em/% of a keyword) scaled by 13/16 or 16/13
//! (`FontBuilder::CheckForGenericFamilyChange`, `FontSizeFunctions`
//! `kStrictFontSizeTable`, Chromium 148). `monospace, monospace` or
//! `Menlo, monospace` is not the bare generic and keeps 16px.
//!
//! Every expected value is Chrome for Testing 148.0.7778.216's computed
//! `font-size`, headless, on the same markup (standards mode). Chrome's
//! 9px minimum logical font size (a `50%` monospace child computes 9px, not
//! 6.5px) is a separate rule and is not pinned here.

use super::*;

fn sizes(body: &str, ids: &[&str]) -> Vec<String> {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 800, height: 600 })
        .expect("view");
    engine
        .load_html(view, &format!("<!doctype html><html><body>{body}</body></html>"))
        .expect("load");
    ids.iter()
        .map(|id| {
            let script = format!("getComputedStyle(document.getElementById('{id}')).fontSize");
            let out = engine
                .execute_script(view, &format!("String(eval({script:?}))"))
                .expect("script");
            out.strip_prefix("String(\"")
                .and_then(|s| s.strip_suffix("\")"))
                .unwrap_or_else(|| panic!("{script} evaluated to {out}"))
                .to_string()
        })
        .collect()
}

fn assert_sizes(body: &str, expected: &[(&str, &str)]) {
    let ids: Vec<&str> = expected.iter().map(|(id, _)| *id).collect();
    let got = sizes(body, &ids);
    let want: Vec<String> = expected.iter().map(|(_, px)| px.to_string()).collect();
    assert_eq!(got, want, "ids {ids:?} in {body}");
}

/// Chrome 148: bare `monospace` 13px (a 4ch box 31.22px wide); `monospace,
/// monospace` 16px (38.41px); a longer family list ending in `monospace`
/// 16px.
#[test]
fn bare_monospace_takes_the_fixed_font_default_size() {
    assert_sizes(
        r#"<div id=bare style="font-family:monospace">PASS</div>
           <div id=twice style="font-family:monospace,monospace">PASS</div>
           <div id=list style="font-family:Menlo,monospace">PASS</div>"#,
        &[("bare", "13px"), ("twice", "16px"), ("list", "16px")],
    );
}

/// An explicit size is the author's: `font-size: 16px` stays 16px; a size
/// inherited from an explicit 20px stays 20px. The keyword `medium` is not
/// explicit: 13px.
#[test]
fn an_explicit_size_is_kept() {
    assert_sizes(
        r#"<div id=explicit style="font-family:monospace;font-size:16px">x</div>
           <div id=medium style="font-family:monospace;font-size:medium">x</div>
           <div style="font-size:20px"><span id=mono20 style="font-family:monospace">x</span></div>"#,
        &[("explicit", "16px"), ("medium", "13px"), ("mono20", "20px")],
    );
}

/// em and % resolve against the monospace parent's 13px (2em 26px, 80%
/// 10.4px). A monospace child of a `2em` (32px) parent scales by 13/16 to
/// 26px. A sans-serif child of a monospace parent is back to 16px.
#[test]
fn relative_sizes_follow_the_fixed_default() {
    assert_sizes(
        r#"<div style="font-family:monospace"><span id=em2 style="font-size:2em">x</span><span id=pct80 style="font-size:80%">x</span><span id=sans style="font-family:sans-serif">x</span></div>
           <div style="font-size:2em"><span id=mono2em style="font-family:monospace">x</span></div>"#,
        &[("em2", "26px"), ("pct80", "10.4px"), ("sans", "16px"), ("mono2em", "26px")],
    );
}

/// The UA's monospace elements: `<code>` inside a paragraph and `<pre>` are
/// 13px; `<code>` in a 20px block is 20px.
#[test]
fn code_and_pre_take_the_fixed_default() {
    assert_sizes(
        r#"<p><code id=code>x</code></p><pre id=pre>x</pre>
           <div style="font-size:20px"><code id=code20>x</code></div>"#,
        &[("code", "13px"), ("pre", "13px"), ("code20", "20px")],
    );
}

/// Size keywords on a monospace element come from the fixed-font row of
/// Blink's table: small 12px, large 16px; `large` on a proportional parent
/// (18px) gives a monospace child 16px.
#[test]
fn size_keywords_use_the_fixed_font_table() {
    assert_sizes(
        r#"<div id=small style="font-family:monospace;font-size:small">x</div>
           <div id=large style="font-family:monospace;font-size:large">x</div>
           <div style="font-size:large"><span id=monolarge style="font-family:monospace">x</span></div>"#,
        &[("small", "12px"), ("large", "16px"), ("monolarge", "16px")],
    );
}
