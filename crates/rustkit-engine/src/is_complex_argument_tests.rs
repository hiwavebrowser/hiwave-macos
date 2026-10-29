//! `:is()`, `:where()` and `:not()` with a complex selector (one with a
//! combinator) as an argument.
//!
//! The matcher used to read each argument as a compound on the element
//! itself, and an argument with a combinator never matched
//! (`keys_for_compound`, `any_compound_in_list_matches`); now such a member
//! is a prepared complex selector with the element as its subject
//! (`ListMember::Complex`). Tailwind v4 writes its dark variant as
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

/// What `shown` reads, from a headless view's current layout (the restyle
/// path after a script).
fn shown_in_view(engine: &Engine, view: EngineViewId) -> String {
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
    texts(engine.views[&view].layout.as_ref().expect("layout"), &mut out);
    out.join(" ")
}

const COMBINATORS: &str = r#"<!doctype html>
<html><head><style>
nav { display: none; }
.d:is(.a .b) { display: block; }
.c:is(.a > .b) { display: block; }
.s:where(.p + .b) { display: block; }
.n:not(.a .b) { display: block; }
.nest:is(:is(.a .b)) { display: block; }
.nn:not(:is(.a .b)) { display: block; }
.y.z { display: none; }
.y:is(#x .y, .z) { display: block; }
.anc:is(.a *) > nav { display: block; }
.sib:is(.a *) ~ nav { display: block; }
.tw:is(.a *):is(.q + *) { display: block; }
</style></head><body>
<div class="a">
  <nav class="b d">d-in</nav>
  <div><nav class="b c">c-deep</nav></div>
  <nav class="b c">c-direct</nav>
  <nav class="p"></nav><nav class="b s">s-adjacent</nav>
  <nav class="b n">n-in</nav>
  <nav class="b nest">nest-in</nav>
  <nav class="b nn">nn-in</nav>
  <div id="x"><nav class="y z">yz</nav></div>
  <div class="anc"><nav>under-anc</nav></div>
  <nav class="sib"></nav><nav>after-sib</nav>
  <nav class="q"></nav><nav class="tw">tw</nav>
</div>
<div class="o">
  <nav class="b d">d-out</nav>
  <nav class="b s">s-out</nav>
  <nav class="b n">n-out</nav>
  <nav class="b nest">nest-out</nav>
  <nav class="b nn">nn-out</nav>
  <nav class="y z">yz-out</nav>
  <div class="anc"><nav>under-anc-out</nav></div>
  <nav class="sib"></nav><nav>after-sib-out</nav>
</div>
</body></html>"#;

#[test]
fn each_combinator_inside_is_where_and_not_reads_the_chain_and_the_siblings() {
    // `.c:is(.a > .b)`: `c-deep` has a div between it and `.a`.
    // `:not(.a .b)` shows the nav outside `.a` only; `:not(:is(.a .b))` the same.
    // `:is(#x .y, .z)` is (1,1,0) whichever member matches (Selectors 4 §17:
    // the most specific argument), so it beats `.y.z` (0,2,0) outside `#x` too.
    // Left of a combinator: `.anc:is(.a *) > nav` and `.sib:is(.a *) ~ nav`
    // decide the member against the ancestor's or sibling's own chain.
    assert_eq!(
        shown(COMBINATORS).join(" "),
        "d-in c-direct s-adjacent nest-in yz under-anc after-sib tw n-out nn-out yz-out"
    );
}

#[test]
fn tailwind_dark_variant_follows_the_dark_class_on_html_through_restyles() {
    // Tailwind v4 as served: `.dark\:bg-black:is(.dark *)`, the escape as
    // the sheet parser keeps it.
    let page = r#"<!doctype html>
<html><head><style>
nav { display: none; }
.dark\:bg-black:is(.dark *) { display: block; }
.dark\:hidden:is(.dark *) { display: none; }
.day { display: block; }
</style></head><body>
<nav class="dark:bg-black">dusk</nav>
<nav class="day dark:hidden">day</nav>
</body></html>"#;
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    engine.load_html(view, page).expect("load_html");
    assert_eq!(shown_in_view(&engine, view), "day");
    engine.execute_script(view, "document.documentElement.classList.add('dark')").expect("add");
    assert_eq!(shown_in_view(&engine, view), "dusk", "adding dark on <html> restyles the variants");
    engine.execute_script(view, "document.documentElement.classList.remove('dark')").expect("remove");
    assert_eq!(shown_in_view(&engine, view), "day", "removing it restyles them back");
}

#[test]
fn query_selector_sees_complex_members_too() {
    let page = r#"<!doctype html><html class="dark"><body>
<div class="a"><p id="in"></p></div><p id="out"></p></body></html>"#;
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let view = engine.create_headless_view(Bounds { x: 0, y: 0, width: 400, height: 300 }).expect("view");
    engine.load_html(view, page).expect("load_html");
    let got = engine
        .execute_script(
            view,
            "[document.querySelectorAll('p:is(.a *)').length, \
              document.querySelectorAll('p:not(.a p)').length, \
              document.getElementById('in').matches(':is(.dark .a > p)'), \
              document.getElementById('out').matches(':where(.a + p)')].join(' ')",
        )
        .expect("script");
    assert_eq!(got, "String(\"1 1 true true\")");
}
