//! `:is()`, `:where()` and `:not()` with a complex selector (one with a
//! combinator) as an argument.
//!
//! The matcher reads each argument as a compound on the element itself, and
//! an argument with a combinator never matches (`keys_for_compound`,
//! `any_compound_in_list_matches`). Tailwind v4 writes its dark variant as
//! `.dark\:hidden:is(.dark *)`: on railway.com the page puts `class="dark"`
//! on `<html>`, and Chrome hides the day hero image and shows the dusk one;
//! RustKit shows the day image and every `dark:` rule is lost (Pete's hand
//! test 10-27).
//!
//! Every `nav` is `display: none` until its own rule shows it. The expected
//! row is what the pinned Chrome 148 computes for this page.

use super::*;

const PAGE: &str = r#"<!doctype html>
<html class="dark"><head><style>
nav { display: none; }
.a:is(.dark *) { display: block; }
.b:where(.dark, .dark *) { display: block; }
.c:is(.x, .dark *) { display: block; }
.d:is(.dark > body > .col > *) { display: block; }
.e:is(.light *) { display: block; }
.f:not(.dark *) { display: block; }
:is(.col .g) { display: block; }
.h:is(.col > .i *) { display: block; }
</style></head><body>
<div class="col">
<nav class="a">a</nav><nav class="b">b</nav><nav class="c">c</nav><nav class="d">d</nav><nav class="e">e</nav>
<nav class="f">f</nav><nav class="g">g</nav><div class="i"><nav class="h">h</nav></div>
</div>
</body></html>"#;

fn shown(html: &str) -> Vec<String> {
    let document = Rc::new(Document::parse_html(html).expect("html"));
    let engine = Engine::new(EngineConfig::default()).expect("engine");
    let layout = engine.build_layout_from_document(&document, &[]);
    fn texts(b: &LayoutBox, out: &mut Vec<String>) {
        if let BoxType::Text(t) = &b.box_type {
            if !t.trim().is_empty() {
                out.push(t.trim().to_string());
            }
        }
        for c in &b.children {
            texts(c, out);
        }
    }
    let mut out = Vec::new();
    texts(&layout, &mut out);
    out
}

#[test]
fn a_complex_selector_inside_is_where_and_not_matches_against_the_ancestors() {
    assert_eq!(shown(PAGE).join(" "), "a b c d g h");
}
