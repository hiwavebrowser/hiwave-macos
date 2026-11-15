//! What reading and writing inline style properties costs (MotionMark's Multiply writes three inline
//! properties per tile per frame, thousands a frame; one frame ran past the 5 s script budget on a
//! Windows release build: 15000 reads 5116 ms, 15000 writes 3626 ms, against Chrome 148's 3.8 ms and 2.6 ms
//! on the same page). The work is counted, not timed: how many times an element's `style` attribute text
//! is parsed (`__rk.inlineStyleParses`). Reading the same unchanged text again, and reading after the
//! engine's own write, need no new parse.

use super::*;

fn bound() -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html("<html><body><div id=t style='left: 1px; top: 2px; width: 3px'></div></body></html>").unwrap(),
    ))
    .unwrap();
    b
}

fn parses(b: &DomBindings) -> f64 {
    match b.runtime.borrow_mut().evaluate_host_bookkeeping("return __rk.inlineStyleParses()").unwrap() {
        JsValue::Number(n) => n,
        other => panic!("{other:?}"),
    }
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

#[test]
fn reading_and_writing_inline_properties_parses_the_style_text_once_per_change() {
    let b = bound();
    let before = parses(&b);
    // 1000 reads of unchanged text, then 300 writes (each read back), as an animation frame does.
    assert_eq!(
        ev(
            &b,
            "var s = document.getElementById('t').style, out = ''; \
             for (var i = 0; i < 1000; i++) out = s.left + s.top; \
             for (var j = 0; j < 100; j++) { s.transform = 'rotate(' + j + 'deg)'; s.backgroundColor = 'hsla(' + j + ',100%,50%,0.5)'; s.opacity = '1'; } \
             out + '|' + s.transform + '|' + s.backgroundColor + '|' + s.getPropertyValue('width') + '|' + s.length",
        ),
        "1px2px|rotate(99deg)|hsla(99,100%,50%,0.5)|3px|6"
    );
    let n = parses(&b) - before;
    assert!(n <= 5.0, "{n} parses of the style text for 1000 reads and 300 writes");
}

#[test]
fn a_style_attribute_changed_by_script_is_read_afresh() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var e = document.getElementById('t'), s = e.style, a = s.left; \
             e.setAttribute('style', 'left: 9px; color: red'); \
             a + '|' + s.left + '|' + s.color + '|' + s.top + '|' + s.length",
        ),
        "1px|9px|red||2"
    );
}

/// What a read gives after a write is what the attribute now says, whatever the name or value written: a
/// write the parser would read differently (a `;` or `:` in the name, a comment, mismatched brackets) is not
/// taken from the kept list. (The attribute is set again with a space added: different text, the same
/// declarations, so the second read parses it.)
#[test]
fn a_read_after_an_odd_write_matches_the_attribute() {
    let b = bound();
    for (name, value) in [("a;b", "1px"), ("x:y", "2px"), ("transform", "rotate(3deg]"), ("color", "red /* c */"), ("left", "4px\u{1}")] {
        let script = format!(
            "var e = document.getElementById('t'); e.setAttribute('style', 'left: 1px; top: 2px'); var s = e.style; \
             s.setProperty({name:?}, {value:?}); \
             var kept = s.cssText + '|' + s.length + '|' + s.getPropertyValue({name:?}); \
             e.setAttribute('style', e.getAttribute('style') + ' '); \
             var fresh = s.cssText + '|' + s.length + '|' + s.getPropertyValue({name:?}); \
             kept === fresh ? 'same' : kept + ' != ' + fresh"
        );
        assert_eq!(ev(&b, &script), "same", "{name}: {value}");
    }
}
