//! JS-PERF2 item 2: `a[i]` and `a[i] = v` on an array, a typed array or a string skip the clone and
//! key conversion of the general path. The shortcut must answer what the general path answers, so
//! the cases below (holes, prototype index setters and getters, frozen and non-extensible arrays,
//! accessor and non-writable elements, subclasses, typed-array conversions, detached and resized
//! buffers, differing receivers, proxies, `arguments`) are compared with Chrome for Testing
//! 148.0.7778.216 (`tools/parity_oracle/js_cases_log.mjs index_fast_path_cases.json --write`).
//! They pass before the shortcut too; the speed pin at the end is the one that does not.

use rustkit_js::{JsRuntime, JsValue};
use std::time::Instant;

#[test]
fn the_index_shortcuts_answer_what_chrome_answers() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../tools/parity_oracle/index_fast_path_cases.json")).expect("cases");
    let mut wrong = Vec::new();
    for c in data["cases"].as_array().expect("cases") {
        let name = c["name"].as_str().unwrap();
        let chrome = c["chrome"].as_str().expect("run js_cases_log.mjs --write");
        // One realm per case: a case may change a prototype.
        let mut rt = JsRuntime::new().unwrap();
        let got = match rt.evaluate_script(c["js"].as_str().unwrap()) {
            Ok(JsValue::String(s)) => s,
            other => format!("{other:?}"),
        };
        if got != chrome {
            wrong.push(format!("{name}:\n  rustkit {got}\n  chrome  {chrome}"));
        }
    }
    assert!(wrong.is_empty(), "{} differ\n{}", wrong.len(), wrong.join("\n"));
}

/// Median of 7 runs of a 200,000-iteration loop with `body`, relative to the empty loop.
fn relative_cost(rt: &mut JsRuntime, body: &str) -> f64 {
    let time = |rt: &mut JsRuntime, b: &str| -> f64 {
        let src = format!("(function(){{ var s = 0; for (var k = 0; k < 200; k++) for (var i = 0; i < 1000; i++) {{ {b} }} return s; }})()");
        rt.evaluate_script(&src).unwrap();
        let mut v: Vec<f64> = (0..7)
            .map(|_| {
                let t = Instant::now();
                rt.evaluate_script(&src).unwrap();
                t.elapsed().as_secs_f64()
            })
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[3]
    };
    time(rt, body) / time(rt, "s += i;")
}

/// The speed pin. A typed-array read and write each cost a multiple of the empty loop's iteration;
/// measured (Linux) as the sum of the two relative costs: before the shortcut 5.24 in a release
/// build and 5.18 in a debug build, after it 2.94 and 4.08. The bound sits between, and the test
/// passes if any of three attempts is under it (a loaded machine inflates a run, never deflates
/// it below what the code costs).
#[test]
fn typed_array_reads_and_writes_skip_the_general_path() {
    let mut rt = JsRuntime::new().unwrap();
    rt.evaluate_script("var u = new Uint16Array(1000);").unwrap();
    let bound = if cfg!(debug_assertions) { 4.6 } else { 4.0 };
    let mut best = f64::MAX;
    for _ in 0..3 {
        let sum = relative_cost(&mut rt, "s += u[i];") + relative_cost(&mut rt, "u[i] = i;");
        best = best.min(sum);
        if best <= bound {
            return;
        }
    }
    panic!("typed-array read + write cost {best:.2} empty-loop iterations, expected under {bound}");
}
