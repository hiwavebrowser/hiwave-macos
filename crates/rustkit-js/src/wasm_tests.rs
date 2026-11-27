//! WASM W1 phase 1 (docs/design/WASM-W1.md): `WebAssembly` on wasmi, with
//! validate, Module, Instance, numeric exports and imports, the error
//! classes, the promise forms, and JS -> wasm -> JS re-entry.
//!
//! Expected values: Chrome for Testing 148.0.7778.216, headless
//! (docs/diagnostics/2026-11-24/wasm_phase1_cft148.json and
//! wasm_reentry_cft148.json, from tools/parity_oracle/wasm_phase1_probe.mjs).
//! Error messages are V8's own wording: the class is pinned, and the message
//! only where it is the spec's (a trap's `unreachable`, the stack overflow).

use crate::{JsRuntime, JsValue};

/// (module (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add))
const ADD: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,7,1,96,2,127,127,1,127,3,2,1,0,7,7,1,3,97,100,100,0,0,10,9,1,7,0,32,0,32,1,106,11])";
/// (module (func (export "big") (result i64) i64.const -1))
const BIG: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,5,1,96,0,1,126,3,2,1,0,7,7,1,3,98,105,103,0,0,10,6,1,4,0,66,127,11])";
/// (module (import "env" "f" (func (param i32) (result i32))) (func (export "g") (param i32) (result i32) local.get 0 call 0))
const IMP: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,6,1,96,1,127,1,127,2,9,1,3,101,110,118,1,102,0,0,3,2,1,0,7,5,1,1,103,0,1,10,8,1,6,0,32,0,16,0,11])";
/// (module (func (export "t") unreachable))
const TRAP: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,5,1,1,116,0,0,10,5,1,3,0,0,11])";
/// (import "env" "cb" (func (param i32) (result i32))), inner(x) = x + 1, outer(x) = cb(x)
const REENTRY: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,6,1,96,1,127,1,127,2,10,1,3,101,110,118,2,99,98,0,0,3,3,2,0,0,7,17,2,5,105,110,110,101,114,0,1,5,111,117,116,101,114,0,2,10,16,2,7,0,32,0,65,1,106,11,6,0,32,0,16,0,11])";

fn rt() -> JsRuntime {
    let mut rt = JsRuntime::new().expect("runtime");
    rt.enable_webassembly().expect("WebAssembly");
    rt
}

fn eval(rt: &mut JsRuntime, src: &str) -> String {
    match rt.evaluate_script(src).expect("script") {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

/// Chrome: typeof "object", toString [object WebAssembly], every
/// constructor's length 1, the three error classes subclass Error.
#[test]
fn the_namespace_is_there_with_chromes_shape() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, "[typeof WebAssembly, Object.prototype.toString.call(WebAssembly), \
            ['Module','Instance','Memory','Table','Global'].map(n => n + ':' + WebAssembly[n].length).join(','), \
            ['CompileError','LinkError','RuntimeError'].map(n => n + ':' + (WebAssembly[n].prototype instanceof Error) + ':' + WebAssembly[n].name).join(',')].join('|')"),
        "object|[object WebAssembly]|Module:1,Instance:1,Memory:1,Table:1,Global:1|CompileError:true:CompileError,LinkError:true:LinkError,RuntimeError:true:RuntimeError"
    );
}

/// Chrome: validate true / false; bad bytes are a CompileError; Module
/// without new is a TypeError; Module.exports / imports.
#[test]
fn validate_compile_errors_and_module_reflection() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var r = [WebAssembly.validate({ADD}), WebAssembly.validate(new Uint8Array([0,97,115,109,2,0,0,0]))]; \
            try {{ new WebAssembly.Module(new Uint8Array([1,2,3])); }} catch (e) {{ r.push(e instanceof WebAssembly.CompileError); }} \
            try {{ WebAssembly.Module({ADD}); }} catch (e) {{ r.push(e.constructor.name); }} \
            r.push(JSON.stringify(WebAssembly.Module.exports(new WebAssembly.Module({ADD})))); \
            r.push(JSON.stringify(WebAssembly.Module.imports(new WebAssembly.Module({IMP})))); r.join('|')")),
        r#"true|false|true|TypeError|[{"name":"add","kind":"function"}]|[{"module":"env","name":"f","kind":"function"}]"#
    );
}

/// Chrome: exports is frozen with a null prototype; i32 wraps
/// (2^31-1 + 1 = -2^31) and arguments go through ToInt32; an exported
/// function is named by its index, its length is its arity; i64 is a BigInt.
#[test]
fn exports_and_the_number_boundary() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var ex = new WebAssembly.Instance(new WebAssembly.Module({ADD})).exports; \
            [Object.getPrototypeOf(ex) === null, Object.isFrozen(ex), ex.add(2, 3), ex.add(0x7fffffff, 1), ex.add('4', 1.9), \
             ex.add.name, ex.add.length, typeof ex.add, \
             typeof new WebAssembly.Instance(new WebAssembly.Module({BIG})).exports.big(), \
             String(new WebAssembly.Instance(new WebAssembly.Module({BIG})).exports.big())].join('|')")),
        "true|true|5|-2147483648|5|0|2|function|bigint|-1"
    );
}

/// Chrome: an import is called (7 -> 70); a JS import's exception passes
/// through unchanged; a missing import module is a TypeError, a
/// non-callable function import a LinkError; a trap is a RuntimeError
/// "unreachable".
#[test]
fn imports_their_errors_and_traps() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var m = new WebAssembly.Module({IMP}), r = []; \
            r.push(new WebAssembly.Instance(m, {{ env: {{ f: x => x * 10 }} }}).exports.g(7)); \
            try {{ new WebAssembly.Instance(m, {{ env: {{ f: () => {{ throw new TypeError('from js'); }} }} }}).exports.g(1); }} catch (e) {{ r.push(e.constructor.name + ':' + e.message); }} \
            try {{ new WebAssembly.Instance(m, {{}}); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Instance(m, {{ env: {{ f: 5 }} }}); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Instance(new WebAssembly.Module({TRAP})).exports.t(); }} catch (e) {{ r.push(e.constructor.name + ':' + (e instanceof WebAssembly.RuntimeError) + ':' + e.message); }} \
            r.join('|')")),
        "70|TypeError:from js|TypeError|LinkError|RuntimeError:true:unreachable"
    );
}

/// Chrome: instantiate(bytes) resolves {module, instance}; instantiate(module)
/// resolves an Instance; compile resolves a Module.
#[test]
fn the_promise_forms() {
    let mut rt = rt();
    rt.evaluate_script(&format!("var out = []; \
        WebAssembly.instantiate({ADD}).then(r => out.push(Object.keys(r).join(',') + ':' + r.instance.exports.add(1, 1))); \
        WebAssembly.instantiate(new WebAssembly.Module({ADD})).then(i => out.push(Object.prototype.toString.call(i) + ':' + (i instanceof WebAssembly.Instance))); \
        WebAssembly.compile({ADD}).then(m => out.push(m instanceof WebAssembly.Module));")).expect("script");
    rt.run_jobs().expect("jobs");
    assert_eq!(eval(&mut rt, "out.join('|')"), "module,instance:2|[object WebAssembly.Instance]:true|true");
}

/// Re-entry (a W1 requirement: richards' timed loop runs one level in).
/// Chrome: outer(1) -> cb(1) -> outer(10) -> cb(10) -> outer(100) -> cb(100)
/// -> inner(100) = 101, trail 1,10,100. An unbounded JS <-> wasm recursion is
/// a catchable RangeError "Maximum call stack size exceeded", and the
/// instance still works after it.
#[test]
fn js_to_wasm_to_js_re_entry_three_levels_deep_and_its_bound() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var ex, depth = 0, trail = [], bytes = {REENTRY}; \
            ex = new WebAssembly.Instance(new WebAssembly.Module(bytes), {{ env: {{ cb: x => {{ trail.push(x); return depth++ < 2 ? ex.outer(x * 10) : ex.inner(x); }} }} }}).exports; \
            var r = [ex.outer(1), trail.join(',')]; var ex2; \
            ex2 = new WebAssembly.Instance(new WebAssembly.Module(bytes), {{ env: {{ cb: x => ex2.outer(x) }} }}).exports; \
            try {{ ex2.outer(1); r.push('returned'); }} catch (e) {{ r.push(e.constructor.name + ':' + e.message); }} \
            r.push(ex.inner(41)); r.join('|')")),
        "101|1,10,100|RangeError:Maximum call stack size exceeded|42"
    );
}


/// #1453 R1 nit 1: a start function that calls a JS import runs it during
/// instantiation, and an exception it throws comes out unchanged. Chrome 148
/// (docs/diagnostics/2026-11-24/wasm_start_cft148.json): called once;
/// "SyntaxError:from start".
#[test]
fn a_start_function_calls_its_js_import() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, "var bytes = new Uint8Array([0,97,115,109,1,0,0,0,1,4,1,96,0,0,2,7,1,1,101,1,102,0,0,3,2,1,0,8,1,1,10,6,1,4,0,16,0,11]), calls = 0, r = []; \
            new WebAssembly.Instance(new WebAssembly.Module(bytes), { e: { f: () => { calls++; } } }); r.push(calls); \
            try { new WebAssembly.Instance(new WebAssembly.Module(bytes), { e: { f: () => { throw new SyntaxError('from start'); } } }); r.push('none'); } \
            catch (e) { r.push(e.constructor.name + ':' + e.message); } r.join('|')"),
        "1|SyntaxError:from start"
    );
}

/// (import "env" "cb" (func)); spin(); count(n) loops n times and returns 0; nest() calls env.cb.
const LOOPS: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,9,2,96,0,0,96,1,127,1,127,2,10,1,3,101,110,118,2,99,98,0,0,3,4,3,0,1,0,7,23,3,4,115,112,105,110,0,1,5,99,111,117,110,116,0,2,4,110,101,115,116,0,3,10,31,3,7,0,3,64,12,0,11,11,16,0,3,64,32,0,65,1,107,34,0,13,0,11,32,0,11,4,0,16,0,11])";

/// A long wasm loop runs on a script thread without growing the native
/// stack. Built without wasmi's loop dispatch, wasmi used tail-call dispatch,
/// which grew the stack per executed instruction when the optimizer did not
/// turn the calls into jumps: count(100000) overflowed a 16 MiB thread and
/// aborted the process.
#[test]
fn a_long_wasm_loop_does_not_grow_the_native_stack() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut rt = rt();
            let _ = tx.send(eval(&mut rt, &format!("var ex = new WebAssembly.Instance(new WebAssembly.Module({LOOPS}), \
                {{ env: {{ cb: () => 0 }} }}).exports; String(ex.count(20000000))")));
        })
        .expect("thread");
    assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(120)).expect("finished"), "0");
}
