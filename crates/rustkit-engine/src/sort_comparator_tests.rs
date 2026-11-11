//! JS2: `Array.prototype.sort` with a comparator that is not a total order
//! must not panic. Boa's `sort_indexed_properties` handed the page's
//! comparator to `Vec::sort_by`, which since Rust 1.81 panics ("user-provided
//! comparison function does not correctly implement a total order") when it
//! notices an inconsistent answer. Real pages ship such comparators (a
//! boolean `a > b`, a constant, `Math.random() - 0.5`): walmart.com panicked
//! on 17 of 17 loads. ECMA-262 leaves the result implementation-defined for
//! them; it does not allow the engine to die.

#![cfg(feature = "headless")]

use super::*;

fn run(script: &str) -> String {
    let mut engine = Engine::new(EngineConfig::default()).expect("engine");
    let id = engine
        .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
        .expect("view");
    engine.load_html(id, "<html><body></body></html>").expect("load_html");
    engine.execute_script(id, script).expect("script")
}

/// 200 numbers, enough to leave the small-slice insertion sort that never
/// notices an inconsistent comparator.
const ARRAY: &str = "var a = []; for (var i = 0; i < 200; i++) a.push((i * 7919) % 211);";

fn len_after(comparator: &str, method: &str) -> String {
    run(&format!(
        "{ARRAY} var r = a.{method}({comparator}); String(r.length + ':' + a.length)"
    ))
}

#[test]
fn a_boolean_comparator_does_not_panic() {
    assert_eq!(len_after("function (x, y) { return x > y; }", "sort"), "String(\"200:200\")");
}

#[test]
fn a_constant_comparator_does_not_panic() {
    assert_eq!(len_after("function () { return 1; }", "sort"), "String(\"200:200\")");
    assert_eq!(len_after("function () { return -1; }", "sort"), "String(\"200:200\")");
}

#[test]
fn a_random_comparator_does_not_panic() {
    assert_eq!(len_after("function () { return Math.random() - 0.5; }", "sort"), "String(\"200:200\")");
}

#[test]
fn to_sorted_and_typed_arrays_do_not_panic() {
    assert_eq!(len_after("function () { return 1; }", "toSorted"), "String(\"200:200\")");
    let typed = run(
        "var t = new Float64Array(200); for (var i = 0; i < 200; i++) t[i] = (i * 7919) % 211; \
         t.sort(function () { return -1; }); String(t.length)",
    );
    assert_eq!(typed, "String(\"200\")");
}

#[test]
fn a_consistent_comparator_still_sorts_stably() {
    let out = run(&format!(
        "{ARRAY} a.sort(function (x, y) {{ return x - y; }}); \
         var ok = true; for (var i = 1; i < a.length; i++) if (a[i - 1] > a[i]) ok = false; \
         var p = []; for (var j = 0; j < 100; j++) p.push({{ k: j % 5, i: j }}); \
         p.sort(function (x, y) {{ return x.k - y.k; }}); \
         var stable = true; for (var m = 1; m < p.length; m++) if (p[m - 1].k === p[m].k && p[m - 1].i > p[m].i) stable = false; \
         String(ok + ',' + stable)"
    ));
    assert_eq!(out, "String(\"true,true\")");
}

#[test]
fn a_throwing_comparator_stops_the_sort_and_rethrows() {
    let out = run(&format!(
        "{ARRAY} var calls = 0; var msg = ''; \
         try {{ a.sort(function () {{ calls++; if (calls === 5) throw new Error('boom'); return 0; }}); }} catch (e) {{ msg = e.message; }} \
         String(msg + ':' + calls)"
    ));
    // No further comparator calls after the abrupt completion (ECMA-262 SortIndexedProperties).
    assert_eq!(out, "String(\"boom:5\")");
}

#[test]
fn sorting_two_hundred_thousand_numbers_stays_fast() {
    let started = std::time::Instant::now();
    let out = run("var a = []; for (var i = 0; i < 200000; i++) a.push((i * 7919) % 100003); \
                   a.sort(function (x, y) { return x - y; }); String(a[0] + ',' + a[199999])");
    assert_eq!(out, "String(\"0,100002\")");
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "{:?}", started.elapsed());
    eprintln!("SORT200K {:?}", started.elapsed());
}
