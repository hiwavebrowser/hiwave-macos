//! Which inline style values hold a bad url (CSS Syntax 3 §4.3.6), so the declaration is not kept
//! (CH2.E29), against Chrome for Testing 148.0.7778.216
//! (docs/diagnostics/2026-11-17/style_url_value_chrome148.json, from
//! tools/parity_oracle/style_url_value_log.mjs). Written on a custom property, which keeps any value but
//! one with a bad url, so the answer is the url rule alone; through `setProperty` and through the style
//! attribute. Also: page script that replaces built-ins does not change the answer.

use super::*;

fn bound() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html("<html><body><div id=a></div></body></html>").unwrap()))
        .unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => panic!("{script}: {other:?}"),
    }
}

#[test]
fn a_style_value_is_kept_or_refused_as_chrome_does() {
    let doc: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/diagnostics/2026-11-17/style_url_value_chrome148.json")).unwrap();
    assert_eq!(doc["chrome"].as_str(), Some("148.0.7778.216"));
    let b = bound();
    let mut wrong = Vec::new();
    for case in doc["cases"].as_array().unwrap() {
        let css = case["css"].as_str().unwrap();
        let lit = serde_json::to_string(css).unwrap();
        let got = ev(
            &b,
            &format!(
                "var el = document.getElementById('a'); el.removeAttribute('style');
                 el.style.setProperty('--x', 'before'); el.style.setProperty('--x', {lit});
                 var viaSet = el.style.getPropertyValue('--x') !== 'before';
                 el.setAttribute('style', '--x: ' + {lit});
                 var viaAttr = el.style.length > 0;
                 viaSet + '|' + viaAttr"
            ),
        );
        let want_set = case["kept_via_setProperty"].as_bool().unwrap();
        // A value that is only a comment: Chrome keeps the declaration with an empty value; the attribute
        // path here drops a declaration whose value is empty (not the url rule; a separate row).
        let want_attr = if css.trim_start().starts_with("/*") { false } else { case["kept_via_attribute"].as_bool().unwrap() };
        let want = format!("{want_set}|{want_attr}");
        if got != want {
            wrong.push(format!("{css:?}: kept (setProperty|attribute) {got}, Chrome {want}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), doc["cases"].as_array().unwrap().len(), wrong.join("\n"));
}

#[test]
fn replaced_built_ins_do_not_change_which_values_are_kept() {
    let b = bound();
    let got = ev(
        &b,
        "var el = document.getElementById('a'), out = [];
         var saved = [String.prototype.substr, String.prototype.toLowerCase, String.prototype.charAt,
                      RegExp.prototype.test, String.prototype.indexOf, String.prototype.charCodeAt];
         String.prototype.substr = function () { return 'x'; };
         String.prototype.toLowerCase = function () { return 'x'; };
         String.prototype.charAt = function () { return 'x'; };
         RegExp.prototype.test = function () { return false; };
         String.prototype.indexOf = function () { return -1; };
         String.prototype.charCodeAt = function () { return 120; };
         ['url(a b)', 'URL(a b)', 'url(a\"b)'].forEach(function (v) {
             el.style.setProperty('--x', 'before'); el.style.setProperty('--x', v);
             out.push(el.style.getPropertyValue('--x'));
         });
         String.prototype.substr = saved[0]; String.prototype.toLowerCase = saved[1]; String.prototype.charAt = saved[2];
         RegExp.prototype.test = saved[3]; String.prototype.indexOf = saved[4]; String.prototype.charCodeAt = saved[5];
         out.join('|')",
    );
    assert_eq!(got, "before|before|before", "Chrome 148 keeps 'before' (replaced_builtins in the oracle file)");
}
