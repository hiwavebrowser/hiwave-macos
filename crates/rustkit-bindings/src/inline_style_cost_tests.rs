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
/// taken from the kept list. (The attribute is set again with a space in front: different text, the same
/// declarations, so the second read parses it.)
#[test]
fn a_read_after_an_odd_write_matches_the_attribute() {
    let b = bound();
    for (name, value) in [("a;b", "1px"), ("x:y", "2px"), ("transform", "rotate(3deg]"), ("color", "red /* c */"), ("left", "4px\u{1}")] {
        let script = format!(
            "var e = document.getElementById('t'); e.setAttribute('style', 'left: 1px; top: 2px'); var s = e.style; \
             s.setProperty({name:?}, {value:?}); \
             var kept = s.cssText + '|' + s.length + '|' + s.getPropertyValue({name:?}); \
             e.setAttribute('style', ' ' + e.getAttribute('style')); \
             var fresh = s.cssText + '|' + s.length + '|' + s.getPropertyValue({name:?}); \
             kept === fresh ? 'same' : kept + ' != ' + fresh"
        );
        assert_eq!(ev(&b, &script), "same", "{name}: {value}");
    }
}

/// What the kept declarations and the property-name table remember is made without the page's built-ins:
/// a page that replaces them for a while (and puts them back) leaves nothing behind. Reads and writes made
/// while they are replaced may fail; every one after must be right.
#[test]
fn replaced_built_ins_leave_no_trace_in_inline_style() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var e = document.getElementById('t'), s = e.style; \
             var rp = String.prototype.replace, tl = String.prototype.toLowerCase; \
             String.prototype.replace = function () { return 'width'; }; String.prototype.toLowerCase = function () { return 'width'; }; \
             try { s.outlineColor = 'green'; } catch (x) {} \
             String.prototype.replace = rp; String.prototype.toLowerCase = tl; \
             var saved = [[Object.prototype, 'hasOwnProperty'], [Array.prototype, 'every'], [Array.prototype, 'map'], \
               [Array.prototype, 'filter'], [Array.prototype, 'push'], [String.prototype, 'replace'], \
               [String.prototype, 'toLowerCase'], [String.prototype, 'indexOf'], [String.prototype, 'charCodeAt'], \
               [RegExp.prototype, 'exec'], [RegExp.prototype, 'test'], [WeakMap.prototype, 'get'], [WeakMap.prototype, 'set']] \
               .map(function (p) { return [p[0], p[1], p[0][p[1]]]; }); \
             Object.prototype.hasOwnProperty = function () { return true; }; \
             Array.prototype.every = function () { return true; }; Array.prototype.map = function () { return []; }; \
             Array.prototype.filter = function () { return []; }; Array.prototype.push = function () { return 0; }; \
             String.prototype.replace = function () { return 'width'; }; String.prototype.toLowerCase = function () { return 'width'; }; \
             String.prototype.indexOf = function () { return -1; }; String.prototype.charCodeAt = function () { return 97; }; \
             RegExp.prototype.exec = function () { return null; }; RegExp.prototype.test = function () { return true; }; \
             WeakMap.prototype.get = function () { return undefined; }; WeakMap.prototype.set = function () { return this; }; \
             try { s.paddingLeft = '7px'; } catch (x) {} try { s.marginTop = '8px'; } catch (x) {} \
             try { s.setProperty('a;b', '1px'); } catch (x) {} try { var r1 = s.left + s.top + s.borderTopWidth; } catch (x) {} \
             saved.forEach(function (p) { p[0][p[1]] = p[2]; }); \
             e.setAttribute('style', 'left: 1px; top: 2px'); \
             s.paddingLeft = '4px'; s.marginTop = '5px'; s.borderTopWidth = '6px'; s.outlineColor = 'green'; \
             var kept = s.cssText + '|' + s.length + '|' + s.paddingLeft + '|' + s.marginTop + '|' + s.left + '|' + s.outlineColor; \
             e.setAttribute('style', ' ' + e.getAttribute('style')); \
             var fresh = s.cssText + '|' + s.length + '|' + s.paddingLeft + '|' + s.marginTop + '|' + s.left + '|' + s.outlineColor; \
             kept === fresh ? kept : kept + ' != ' + fresh",
        ),
        "left: 1px; top: 2px; padding-left: 4px; margin-top: 5px; border-top-width: 6px; outline-color: green;|6|4px|5px|1px|green"
    );
}

/// A write the element answers by changing its own style again (a custom element's
/// attributeChangedCallback) is read back as the attribute now stands, not as the write left it.
#[test]
fn a_style_changed_again_inside_the_write_is_read_as_it_stands() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var seen = 0; \
             customElements.define('x-restyle', class extends HTMLElement { \
               static get observedAttributes() { return ['style']; } \
               attributeChangedCallback() { if (seen++ === 0) this.setAttribute('style', 'color: blue; width: 9px'); } }); \
             var e = document.createElement('x-restyle'); document.body.appendChild(e); \
             e.style.color = 'red'; \
             [e.getAttribute('style'), e.style.color, e.style.width, e.style.length].join('|')",
        ),
        "color: blue; width: 9px|blue|9px|2"
    );
}
