//! `innerHTML` on a raw text element (`script`, `style`) or an escapable
//! raw text one (`title`, `textarea`): HTML §13.4 parses the string with
//! no tags, so it becomes one text node. Seen on yahoo.com (10-22): Next.js
//! writes a `beforeInteractive` script's code with `script.innerHTML =
//! code`; the code was cut at its first `i<n` and threw a SyntaxError.

use super::*;

fn bound() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html("<html><head></head><body></body></html>").unwrap());
    b.set_document(doc).unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(x) => x.to_string(),
        JsValue::Number(n) => format!("{n}"),
        other => format!("{other:?}"),
    }
}

#[test]
fn a_scripts_inner_html_keeps_a_less_than_as_text() {
    let b = bound();
    let code = "for(var i=0,e=3;i<e;i++)window.n=i;<b>x</b>&amp;";
    let out = ev(
        &b,
        &format!(
            "var s = document.createElement('script'); s.innerHTML = {code:?}; \
             s.childNodes.length + '|' + s.textContent"
        ),
    );
    assert_eq!(out, format!("1|{code}"));
}

#[test]
fn a_styles_inner_html_is_one_text_node() {
    let b = bound();
    let out = ev(
        &b,
        "var s = document.createElement('style'); s.innerHTML = 'a>b{color:red}<p>'; \
         s.childNodes.length + '|' + s.textContent",
    );
    assert_eq!(out, "1|a>b{color:red}<p>");
}

#[test]
fn a_textareas_inner_html_decodes_references_but_makes_no_elements() {
    let b = bound();
    let out = ev(
        &b,
        "var t = document.createElement('textarea'); t.innerHTML = '<b>1 &lt; 2</b>'; \
         t.childNodes.length + '|' + t.textContent",
    );
    assert_eq!(out, "1|<b>1 < 2</b>");
}

#[test]
fn a_divs_inner_html_still_parses_elements() {
    let b = bound();
    let out = ev(
        &b,
        "var d = document.createElement('div'); d.innerHTML = '<b>x</b>y'; \
         d.childNodes.length + '|' + d.firstChild.tagName",
    );
    assert_eq!(out, "2|B");
}
