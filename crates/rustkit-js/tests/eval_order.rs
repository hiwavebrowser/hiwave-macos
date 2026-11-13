//! JS-PERF2 item 6: the compiler reads a local variable's register directly (no copy into a
//! temporary) when nothing evaluated in between can assign to it, and updates `var` locals in
//! place. Evaluation order is where that goes wrong (`a[a = 1]`, `x += (x = 5)`, `a + a++`), so
//! 400 generated programs full of exactly those hazards (`tools/parity_oracle/gen_eval_order_cases.py`,
//! seeded) are compared with Chrome for Testing 148.0.7778.216 (`js_cases_log.mjs eval_order_cases.json
//! --write`): locals declared with `var` and `let`, a parameter, objects and arrays, side-effecting
//! keys, compound assignment, `++`/`--`, a closure that mutates a captured variable. They pass
//! before the change too; they are what would catch an order mistake in it.

use rustkit_js::{JsRuntime, JsValue};

#[test]
fn generated_evaluation_order_programs_match_chrome() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/parity_oracle/eval_order_cases.json")).expect("cases");
    let cases = data["cases"].as_array().expect("cases");
    let mut wrong = Vec::new();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let chrome = c["chrome"].as_str().expect("run js_cases_log.mjs --write");
        let mut rt = JsRuntime::new().unwrap();
        let got = match rt.evaluate_script(c["js"].as_str().unwrap()) {
            Ok(JsValue::String(s)) => s,
            other => format!("{other:?}"),
        };
        if got != chrome {
            wrong.push(format!("{name}:\n  js      {}\n  rustkit {got}\n  chrome  {chrome}", c["js"].as_str().unwrap().replace('\n', " ")));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ\n{}", wrong.len(), cases.len(), wrong.iter().take(4).cloned().collect::<Vec<_>>().join("\n"));
}
