//! `super.x = v` sets `x` with `this` as the receiver: a data property found on the home object's
//! prototype is defined on `this`, never written on the prototype, however the site was first run.
//! Unmodified Boa 0.22 let a `SetPropertyByName` cache hit ignore the receiver, so a site whose
//! first call had `this` equal to that prototype later wrote the prototype. Expected values: Chrome
//! for Testing 148.0.7778.216 (`tools/parity_oracle/super_set_cases.json`, `js_cases_log.mjs
//! --write`). Each case runs on its own runtime and thread, so a panic is reported by name.

use rustkit_js::{JsRuntime, JsValue};

#[test]
fn super_property_writes_match_chrome() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/parity_oracle/super_set_cases.json")).expect("cases");
    let cases = data["cases"].as_array().expect("cases");
    let mut wrong = Vec::new();
    for c in cases {
        let name = c["name"].as_str().unwrap().to_owned();
        let chrome = c["chrome"].as_str().expect("run js_cases_log.mjs --write").to_owned();
        let js = c["js"].as_str().unwrap().to_owned();
        let got = std::thread::spawn(move || {
            let mut rt = JsRuntime::new().unwrap();
            match rt.evaluate_script(&js) {
                Ok(JsValue::String(s)) => s,
                other => format!("{other:?}"),
            }
        })
        .join()
        .unwrap_or_else(|_| "<panicked>".to_owned());
        if got != chrome {
            wrong.push(format!("{name}:\n  rustkit {got}\n  chrome  {chrome}"));
        }
    }
    assert!(wrong.is_empty(), "{} of {} differ\n{}", wrong.len(), cases.len(), wrong.join("\n"));
}
