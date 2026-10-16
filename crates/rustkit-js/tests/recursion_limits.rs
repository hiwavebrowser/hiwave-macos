//! Call depth (src/stack.rs, third_party/boa_engine/PATCHES.md "stack"):
//! script recurses about as deep as in Chrome, one call beyond the limit
//! throws a `RangeError` the script can catch, and recursion through host
//! calls (getters, callbacks), which uses the native stack, throws before
//! that stack overflows, on whatever thread the runtime lives.

use rustkit_js::stack::RECURSION_LIMIT;
use rustkit_js::{JsRuntime, JsValue};

fn number(runtime: &mut JsRuntime, source: &str) -> f64 {
    match runtime.evaluate_script(source) {
        Ok(JsValue::Number(n)) => n,
        other => panic!("{source}: expected a number, got {other:?}"),
    }
}

fn string(runtime: &mut JsRuntime, source: &str) -> String {
    match runtime.evaluate_script(source) {
        Ok(JsValue::String(s)) => s,
        other => panic!("{source}: expected a string, got {other:?}"),
    }
}

const DEPTH: &str = "function depth(n) { return n === 0 ? 0 : depth(n - 1) + 1; }";

/// Classifies what `f(n)` did, as a string.
const OUTCOME: &str = r#"
    function outcome(f, n) {
        try { return 'ok ' + f(n); }
        catch (e) { return (e instanceof RangeError ? 'RangeError: ' : 'other: ') + e.message; }
    }
"#;

#[test]
fn recursion_reaches_chrome_like_depth_and_one_beyond_is_a_catchable_range_error() {
    let mut runtime = JsRuntime::new().unwrap();
    runtime.evaluate_script(DEPTH).unwrap();
    runtime.evaluate_script(OUTCOME).unwrap();

    // The deepest `depth(n)` that returns, called from one `outcome` frame
    // under the script's own frame.
    let max = number(
        &mut runtime,
        r#"
        (function () {
            let lo = 0, hi = 100000;
            while (lo < hi) {
                const mid = (lo + hi + 1) >> 1;
                if (outcome(depth, mid).startsWith('ok')) lo = mid; else hi = mid - 1;
            }
            return lo;
        })()
        "#,
    ) as usize;
    // Script frame + the IIFE + `outcome` + depth(max)..depth(0).
    assert_eq!(max + 4, RECURSION_LIMIT, "the limit is the frame count");
    assert!(max >= 9_990, "about Chrome's depth, got {max}");

    let at = string(
        &mut runtime,
        &format!("(function () {{ return outcome(depth, {max}); }})()"),
    );
    assert_eq!(at, format!("ok {max}"));
    let beyond = string(
        &mut runtime,
        &format!("(function () {{ return outcome(depth, {}); }})()", max + 1),
    );
    assert_eq!(beyond, "RangeError: Maximum call stack size exceeded");

    // `finally` runs, and the runtime is usable afterwards.
    let finally = string(
        &mut runtime,
        "let ran = 'no'; try { try { depth(1e6) } finally { ran = 'yes' } } catch (e) {} ran",
    );
    assert_eq!(finally, "yes");
    assert_eq!(number(&mut runtime, "depth(100)"), 100.0);
}

#[test]
fn uncaught_overflow_is_a_script_error_not_a_crash() {
    let mut runtime = JsRuntime::new().unwrap();
    let err = runtime
        .evaluate_script("function f() { return f() + 1; } f()")
        .expect_err("unbounded recursion throws");
    assert!(
        err.to_string().contains("Maximum call stack size exceeded"),
        "{err}"
    );
    assert_eq!(number(&mut runtime, "1 + 1"), 2.0);
}

/// Host-call recursion uses the native stack: without the floor these
/// overflow a 2 MiB test thread long before 10,000 frames and abort the
/// process.
fn native_recursion_throws_range_error() {
    let mut runtime = JsRuntime::new().unwrap();
    runtime.evaluate_script(OUTCOME).unwrap();
    for (name, source) in [
        (
            "getter",
            "const o = { get x() { return this.x + 1; } }; outcome(() => o.x, 0)",
        ),
        (
            "setter",
            "const s = { set x(v) { this.x = v; } }; outcome(() => { s.x = 1; }, 0)",
        ),
        (
            "map",
            "function m() { return [1].map(m)[0]; } outcome(m, 0)",
        ),
        (
            "proxy",
            "const p = new Proxy({}, { get(t, k, r) { return r[k]; } }); outcome(() => p.x, 0)",
        ),
    ] {
        let got = string(&mut runtime, &format!("{{ {source} }}"));
        assert_eq!(
            got, "RangeError: Maximum call stack size exceeded",
            "{name}"
        );
    }
    assert_eq!(
        number(
            &mut runtime,
            "[1, 2, 3].map(x => x * 2).reduce((a, b) => a + b)"
        ),
        12.0
    );
}

#[test]
fn native_recursion_on_the_default_test_thread() {
    native_recursion_throws_range_error();
}

#[test]
fn native_recursion_on_a_small_thread() {
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(native_recursion_throws_range_error)
        .unwrap()
        .join()
        .expect("no stack overflow on a 1 MiB thread");
}

#[test]
fn native_recursion_on_a_big_thread_goes_deeper() {
    // parity-capture runs script on a 16 MiB thread: the floor follows the
    // thread, so a getter chain gets far deeper than on a small one.
    let depth = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let mut runtime = JsRuntime::new().unwrap();
            number(
                &mut runtime,
                "let d = 0; const o = { get x() { d++; return this.x; } }; try { o.x } catch (e) {} d",
            )
        })
        .unwrap()
        .join()
        .expect("no stack overflow on a 16 MiB thread");
    assert!(
        depth > 1_000.0,
        "deep getter chain on a big thread, got {depth}"
    );
}

#[test]
fn recursion_inside_promise_machinery_rejects() {
    // Boa issue 4515's case: the overflow happens inside a promise's
    // `then` lookup, so it becomes a rejection the page can observe.
    let mut runtime = JsRuntime::new().unwrap();
    runtime
        .evaluate_script(
            r#"
            var seen = 'pending';
            async function* f() {}
            f().return({ get then() { return this.then; } })
                .then(() => { seen = 'resolved'; },
                      (e) => { seen = e instanceof RangeError ? 'RangeError' : 'other'; });
            "#,
        )
        .unwrap();
    runtime.run_jobs().unwrap();
    assert_eq!(string(&mut runtime, "seen"), "RangeError");
}
