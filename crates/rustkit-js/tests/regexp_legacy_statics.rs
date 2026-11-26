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

/// The statics belong to a realm: a match made in one realm is not visible through another
/// realm's `RegExp` (frames run in their own realms on the same thread).
#[test]
fn a_match_in_one_realm_is_not_visible_from_another() {
    use boa_engine::{Context, Source};
    let mut context = Context::default();
    let other = context.create_realm().expect("realm");
    context
        .eval(Source::from_bytes(r#"/se(cr)et/.test("a secret here")"#))
        .expect("match in the first realm");
    let outer = context.enter_realm(other);
    let seen = context
        .eval(Source::from_bytes("[RegExp.lastMatch, RegExp.input, RegExp.$1].join('|')"))
        .expect("read in the second realm")
        .to_string(&mut context)
        .expect("string")
        .to_std_string_escaped();
    context.enter_realm(outer);
    assert_eq!(seen, "||", "the second realm sees nothing from the first realm's match");
    let first = context
        .eval(Source::from_bytes("RegExp.lastMatch"))
        .expect("read in the first realm")
        .to_string(&mut context)
        .expect("string")
        .to_std_string_escaped();
    assert_eq!(first, "secret", "the first realm still sees its own match");
}
