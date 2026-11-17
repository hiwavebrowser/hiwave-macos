//! The second set of string paths held to Chrome's maximum length (2^29 - 24 code units): a result
//! longer than that is refused with `RangeError: Invalid string length`, as Chrome for Testing
//! 148.0.7778.216 does for every case below (docs/diagnostics/2026-11-16/string_paths2_chrome148.json),
//! and results under the limit are unchanged. The first set is in `string_length_limit.rs`.
//! (`escape` and the HTML methods such as `anchor` are Annex B, which this build does not enable.)

use rustkit_js::{JsRuntime, JsValue};

fn run(src: &str) -> String {
    let mut rt = JsRuntime::new().unwrap();
    let wrapped = format!(
        "(function () {{ try {{ var v = (function () {{ {src} }})(); return 'value ' + v; }} \
         catch (e) {{ return e.name + ': ' + e.message; }} }})()"
    );
    match rt.evaluate_script(&wrapped).unwrap() {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

const REFUSED: &str = "RangeError: Invalid string length";

#[test]
fn json_text_past_the_limit_is_refused() {
    assert_eq!(run("var a = new Array(2 ** 15).fill('x'.repeat(2 ** 14)); return JSON.stringify(a).length;"), REFUSED);
    assert_eq!(
        run("var o = {}, big = 'x'.repeat(2 ** 14); for (let i = 0; i < 2 ** 15 + 1; i++) o['k' + i] = big; return JSON.stringify(o).length;"),
        REFUSED
    );
    // Chrome calls every toJSON first, then refuses.
    assert_eq!(
        run("var n = 0, big = 'x'.repeat(2 ** 14), a = []; for (let i = 0; i < 2 ** 16; i++) a.push({ toJSON() { n++; return big; } });
            try { JSON.stringify(a); } catch (e) { return e.name + ' after ' + n; }"),
        "value RangeError after 65536"
    );
    // Under the limit, unchanged (Chrome 148's lengths).
    assert_eq!(run("return JSON.stringify(new Array(2 ** 10).fill('x'.repeat(2 ** 10))).length;"), "value 1051649");
    assert_eq!(
        run("var deep = [[[[[[[[[[1]]]]]]]]]]; for (let i = 0; i < 12; i++) deep = [deep, deep]; return JSON.stringify(deep, null, 10).length;"),
        "value 15429666"
    );
}

#[test]
fn string_raw_past_the_limit_is_refused() {
    assert_eq!(run("var raw = new Array(2 ** 15 + 1).fill('x'.repeat(2 ** 14)); return String.raw({ raw }).length;"), REFUSED);
    assert_eq!(run("var s = 'x'.repeat(2 ** 28); return String.raw({ raw: ['', '', ''] }, s, s).length;"), REFUSED);
}

#[test]
fn expanding_conversions_past_the_limit_are_refused() {
    assert_eq!(run("return encodeURI('\\u0800'.repeat(2 ** 26)).length;"), REFUSED);
    assert_eq!(run("return encodeURIComponent('\\u0800'.repeat(2 ** 26)).length;"), REFUSED);
    assert_eq!(run("return '\\u00df'.repeat(2 ** 28).toUpperCase().length;"), REFUSED);
    assert_eq!(run("return '\\u00df'.repeat(2 ** 28).toLocaleUpperCase().length;"), REFUSED);
    // Under the limit, unchanged.
    assert_eq!(run("return 'stra\\u00dfe'.toUpperCase();"), "value STRASSE");
    assert_eq!(run("return encodeURIComponent('a b\\u0800');"), "value a%20b%E0%A0%80");
    assert_eq!(run("return '\\ufdfa'.normalize('NFKD').length;"), "value 18");
}
