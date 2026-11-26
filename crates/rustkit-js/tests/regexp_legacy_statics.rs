//! The legacy RegExp statics (`regexp-legacy-statics`, see third_party/boa_engine/PATCHES.md;
//! REGEXP-LEGACY-STATICS). JetStream 2.2's first test, WSL, reads `RegExp.lastMatch.length` after
//! every `test` in its lexer; without the statics it stopped at 5.2 s with "cannot convert 'null'
//! or 'undefined' to object".
//!
//! Expected values: Chrome for Testing 148.0.7778.216,
//! docs/diagnostics/2026-11-24/regexp_legacy_statics_cft148.txt.

use rustkit_js::{JsRuntime, JsValue};

fn eval(rt: &mut JsRuntime, code: &str) -> JsValue {
    rt.evaluate_script(code).expect("evaluation succeeds")
}

fn eval_string(rt: &mut JsRuntime, code: &str) -> String {
    match eval(rt, code) {
        JsValue::String(s) => s,
        other => panic!("{code}: expected a string, got {other:?}"),
    }
}

#[test]
fn the_legacy_statics_follow_the_last_successful_match() {
    let mut rt = JsRuntime::new().expect("runtime");
    assert_eq!(eval_string(&mut rt, "RegExp.lastMatch"), "");
    eval(&mut rt, r#"/b(c)(d)?/.test("abcz")"#);
    let read = |rt: &mut JsRuntime, name: &str| eval_string(rt, &format!("RegExp[{name:?}]"));
    assert_eq!(read(&mut rt, "lastMatch"), "bc");
    assert_eq!(read(&mut rt, "$&"), "bc");
    assert_eq!(read(&mut rt, "$1"), "c");
    assert_eq!(read(&mut rt, "$2"), "");
    assert_eq!(read(&mut rt, "$3"), "");
    assert_eq!(read(&mut rt, "lastParen"), "");
    assert_eq!(read(&mut rt, "leftContext"), "a");
    assert_eq!(read(&mut rt, "rightContext"), "z");
    assert_eq!(read(&mut rt, "input"), "abcz");
    assert_eq!(read(&mut rt, "$_"), "abcz");
    // A failed match leaves them as they were.
    eval(&mut rt, r#"/x/.test("nomatch")"#);
    assert_eq!(read(&mut rt, "lastMatch"), "bc");
    // Only input/$_ can be set; the other setters change nothing.
    eval(&mut rt, r#"RegExp.input = "set"; RegExp.lastMatch = "ignored";"#);
    assert_eq!(read(&mut rt, "input"), "set");
    assert_eq!(read(&mut rt, "lastMatch"), "bc");
}

#[test]
fn the_statics_are_configurable_non_enumerable_accessors_with_setters() {
    let mut rt = JsRuntime::new().expect("runtime");
    assert_eq!(
        eval_string(
            &mut rt,
            "var d = Object.getOwnPropertyDescriptor(RegExp, 'lastMatch'); \
             [d.enumerable, d.configurable, typeof d.get, typeof d.set].join()"
        ),
        "false,true,function,function"
    );
    // Chrome answers whatever the this value: no TypeError.
    eval(&mut rt, "Object.getOwnPropertyDescriptor(RegExp, 'lastMatch').get.call({})");
}

#[test]
fn the_wsl_lexer_pattern_reads_the_match_length() {
    let mut rt = JsRuntime::new().expect("runtime");
    assert_eq!(
        eval_string(&mut rt, "var t = '  foo'; /^\\s+/.test(t); String(RegExp.lastMatch.length)"),
        "2"
    );
}
