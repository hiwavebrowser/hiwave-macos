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

// ---- Phase 2: Memory (docs/design/WASM-W1.md). Chrome 148:
// docs/diagnostics/2026-11-24/wasm_memory_cft148.json (tools/parity_oracle/wasm_memory_probe.mjs;
// module bytes from tools/parity_oracle/wasm_asm.py).

/// (import "env" "memory" (memory 1 2)); load(p), store(p, v), size(), grow(n).
const MEMMOD: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,15,3,96,1,127,1,127,96,2,127,127,0,96,0,1,127,2,16,1,3,101,110,118,6,109,101,109,111,114,121,2,1,1,2,3,5,4,0,1,2,0,7,30,4,4,108,111,97,100,0,0,5,115,116,111,114,101,0,1,4,115,105,122,101,0,2,4,103,114,111,119,0,3,10,31,4,7,0,32,0,40,2,0,11,9,0,32,0,32,1,54,2,0,11,4,0,63,0,11,6,0,32,0,64,0,11])";
/// A module-defined memory (1 page) exported as "mem"; size().
const OWNMEM: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,5,1,96,0,1,127,3,2,1,0,5,3,1,0,1,7,14,2,4,115,105,122,101,0,0,3,109,101,109,2,0,10,6,1,4,0,63,0,11])";

/// Chrome: byteLength 65536 (initial, whatever the maximum), the same buffer
/// each read, an ArrayBuffer script can't detach (transfer() and a
/// structuredClone transfer are TypeErrors), toStringTag; {initial: 0} is 0.
#[test]
fn a_memory_is_an_undetachable_array_buffer_of_its_initial_pages() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, "var m = new WebAssembly.Memory({ initial: 1, maximum: 2 }), r = [m.buffer.byteLength, m.buffer === m.buffer, \
            m.buffer.constructor.name, Object.prototype.toString.call(m)]; \
            try { m.buffer.transfer(); r.push('ok'); } catch (e) { r.push(e.constructor.name); } \
            r.push(new WebAssembly.Memory({ initial: 0 }).buffer.byteLength); r.join('|')"),
        "65536|true|ArrayBuffer|[object WebAssembly.Memory]|TypeError|0"
    );
}

/// Chrome: JS and wasm see each other's writes (no copy); memory.size 1;
/// grow(0) answers 1 and keeps the buffer.
#[test]
fn js_and_wasm_share_the_bytes() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var m = new WebAssembly.Memory({{ initial: 1, maximum: 2 }}); \
            var ex = new WebAssembly.Instance(new WebAssembly.Module({MEMMOD}), {{ env: {{ memory: m }} }}).exports; \
            new Int32Array(m.buffer)[4] = 1234; var r = [ex.load(16)]; ex.store(32, -7); r.push(new Int32Array(m.buffer)[8]); \
            var b = m.buffer; r.push(ex.size(), m.grow(0), m.buffer === b, m.buffer.byteLength); r.join('|')")),
        "1234|-7|1|1|true|65536"
    );
}

/// Chrome: no initial is a TypeError, initial > maximum a RangeError; a
/// non-Memory import or one whose maximum exceeds the module's is a LinkError.
#[test]
fn memory_descriptor_and_import_errors() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var r = []; \
            try {{ new WebAssembly.Memory({{}}); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Memory({{ initial: 2, maximum: 1 }}); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Instance(new WebAssembly.Module({MEMMOD}), {{ env: {{ memory: {{}} }} }}); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Instance(new WebAssembly.Module({MEMMOD}), {{ env: {{ memory: new WebAssembly.Memory({{ initial: 1, maximum: 3 }}) }} }}); r.push('ok'); }} \
            catch (e) {{ r.push(e.constructor.name); }} r.join('|')")),
        "TypeError|RangeError|LinkError|LinkError"
    );
}

/// W1 LIMITS, pinned so they are seen (docs/design/WASM-W1.md; W2 lifts them):
/// - grow(n > 0) fails: a RangeError from JS and -1 from wasm. Chrome 148
///   grows (grow(1) answers 1, the old buffer is detached, the new one is
///   131072 long) and fails only past the maximum (RangeError / -1);
/// - a module-defined memory's exported buffer is a TypeError naming the
///   limit; the module itself runs (memory.size 1). Chrome: 65536 bytes.
#[test]
fn w1_limits_grow_and_module_defined_memory() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var m = new WebAssembly.Memory({{ initial: 1, maximum: 2 }}); \
            var ex = new WebAssembly.Instance(new WebAssembly.Module({MEMMOD}), {{ env: {{ memory: m }} }}).exports, r = []; \
            try {{ m.grow(1); r.push('grew'); }} catch (e) {{ r.push(e.constructor.name); }} \
            r.push(ex.grow(1), m.buffer.byteLength); \
            var own = new WebAssembly.Instance(new WebAssembly.Module({OWNMEM})).exports; \
            r.push(Object.prototype.toString.call(own.mem), own.size()); \
            try {{ own.mem.buffer; r.push('buffer'); }} catch (e) {{ r.push(e.constructor.name); }} r.join('|')")),
        "RangeError|-1|65536|[object WebAssembly.Memory]|1|TypeError"
    );
}

// ---- Phase 3: Table and Global. Chrome 148: docs/diagnostics/2026-11-24/wasm_table_global_cft148.json
// (tools/parity_oracle/wasm_table_global_probe.mjs; bytes from tools/parity_oracle/wasm_asm.py).

/// Imports env.table (funcref 2..4), env.g (mutable i32), env.nan (immutable f64); seven() = 7;
/// dispatch(i) = call_indirect table[i]; getg() = env.g; nan() = env.nan; elem table[1] = seven.
const TABMOD: &str = "new Uint8Array([0,97,115,109,1,0,0,0,1,14,3,96,0,1,127,96,1,127,1,127,96,0,1,124,2,36,3,3,101,110,118,5,116,97,98,108,101,1,112,1,2,4,3,101,110,118,1,103,3,127,1,3,101,110,118,3,110,97,110,3,124,0,3,5,4,0,1,0,2,7,33,4,5,115,101,118,101,110,0,0,8,100,105,115,112,97,116,99,104,0,1,4,103,101,116,103,0,2,3,110,97,110,0,3,9,7,1,0,65,1,11,1,0,10,24,4,4,0,65,7,11,7,0,32,0,17,0,0,11,4,0,35,0,11,4,0,35,1,11])";

/// Chrome: a funcref table is null-filled; the module's element segment puts
/// its function in, and table.get answers the very export object;
/// call_indirect calls through it; a null entry and an out-of-range index are
/// RuntimeErrors; table.set takes an exported function (a plain JS function
/// is a TypeError); grow answers the old length; past the maximum, or get out
/// of range, is a RangeError.
#[test]
fn a_funcref_table_shared_with_js() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var t = new WebAssembly.Table({{ element: 'anyfunc', initial: 2, maximum: 4 }}), r = [t.length, t.get(0), Object.prototype.toString.call(t)]; \
            var g = new WebAssembly.Global({{ value: 'i32', mutable: true }}, 41); \
            var ex = new WebAssembly.Instance(new WebAssembly.Module({TABMOD}), {{ env: {{ table: t, g: g, nan: NaN }} }}).exports; \
            r.push(t.get(1) === ex.seven, ex.dispatch(1)); \
            try {{ ex.dispatch(0); }} catch (e) {{ r.push(e.constructor.name + ':' + e.message); }} \
            try {{ ex.dispatch(9); }} catch (e) {{ r.push(e.constructor.name + ':' + e.message); }} \
            t.set(0, ex.seven); r.push(ex.dispatch(0), t.grow(1), t.length); \
            try {{ t.grow(5); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ t.set(0, () => 1); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ t.get(99); }} catch (e) {{ r.push(e.constructor.name); }} r.join('|')")),
        "2||[object WebAssembly.Table]|true|7|RuntimeError:null function|RuntimeError:table index is out of bounds|7|2|3|RangeError|TypeError|RangeError"
    );
}

/// Chrome: a Global's value and valueOf; a mutable imported Global is shared
/// (a JS write is seen by wasm); a plain Number gives an immutable f64 import
/// (NaN); i64 is a BigInt, f32 rounds; setting an immutable Global is a
/// TypeError; a plain Number for a mutable global, or a non-Number for an f64
/// one, is a LinkError; a non-Table table import is a LinkError.
#[test]
fn globals_and_global_imports() {
    let mut rt = rt();
    assert_eq!(
        eval(&mut rt, &format!("var t = new WebAssembly.Table({{ element: 'anyfunc', initial: 2, maximum: 4 }}); \
            var g = new WebAssembly.Global({{ value: 'i32', mutable: true }}, 41), r = [g.value, g.valueOf(), Object.prototype.toString.call(g)]; \
            var ex = new WebAssembly.Instance(new WebAssembly.Module({TABMOD}), {{ env: {{ table: t, g: g, nan: NaN }} }}).exports; \
            r.push(ex.getg()); g.value = 5; r.push(ex.getg(), String(ex.nan())); \
            var ig = new WebAssembly.Global({{ value: 'i32' }}, 3); try {{ ig.value = 4; }} catch (e) {{ r.push(e.constructor.name); }} \
            r.push(typeof new WebAssembly.Global({{ value: 'i64' }}, 5n).value, new WebAssembly.Global({{ value: 'f32' }}, 1.1).value); \
            try {{ new WebAssembly.Instance(new WebAssembly.Module({TABMOD}), {{ env: {{ table: t, g: 41, nan: NaN }} }}); r.push('ok'); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Instance(new WebAssembly.Module({TABMOD}), {{ env: {{ table: t, g: g, nan: 'x' }} }}); r.push('ok'); }} catch (e) {{ r.push(e.constructor.name); }} \
            try {{ new WebAssembly.Instance(new WebAssembly.Module({TABMOD}), {{ env: {{ table: {{}}, g: g, nan: NaN }} }}); }} catch (e) {{ r.push(e.constructor.name); }} r.join('|')")),
        "41|41|[object WebAssembly.Global]|41|5|NaN|TypeError|bigint|1.100000023841858|LinkError|LinkError|LinkError"
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

// ---- Phase 4: the live turn's deadline reaches wasm (docs/design/WASM-W1.md: fuel slices through
// call_resumable; when a slice runs out, the deadline and the interrupt word are checked, and the
// call ends with the same uncatchable stop a JS script gets). RustKit's own budget semantics:
// nothing to measure in Chrome, whose "page unresponsive" is not a script deadline.

/// Run `f` on its own thread with its own runtime: a wasm loop the deadline
/// doesn't stop would otherwise hang the test binary.
fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .expect("thread");
    rx.recv_timeout(std::time::Duration::from_secs(secs)).expect("the wasm call was not stopped by the deadline")
}

/// An infinite wasm loop, called at the top level or from inside a JS import
/// (richards' shape), is stopped at the deadline; the stop is the turn's
/// (take_deadline_hit), uncatchable (the script's try/catch doesn't see it),
/// and the runtime goes on (a later turn runs wasm normally).
#[test]
fn the_deadline_stops_a_wasm_loop_top_level_and_nested() {
    let got = within(20, || {
        let mut rt = rt();
        rt.evaluate_script(&format!("var caught = false, ex; ex = new WebAssembly.Instance(new WebAssembly.Module({LOOPS}), \
            {{ env: {{ cb: () => ex.spin() }} }}).exports;")).expect("setup");
        let mut out = Vec::new();
        for call in ["ex.spin()", "ex.nest()"] {
            rt.set_execution_deadline(Some(std::time::Instant::now() + std::time::Duration::from_millis(100)));
            let stopped = rt.evaluate_script(&format!("try {{ {call}; }} catch (e) {{ caught = true; }}")).is_err();
            out.push(format!("{stopped}:{}", rt.take_deadline_hit()));
            rt.set_execution_deadline(None);
        }
        out.push(eval(&mut rt, "String(caught) + ':' + ex.count(10)"));
        out.join("|")
    });
    assert_eq!(got, "true:true|true:true|false:0");
}

/// A finite loop longer than one fuel slice, nested inside a JS import,
/// completes: an out-of-fuel suspension is resumed, not turned into a trap
/// (wasmi's plain call would trap; #1451 delta R1).
#[test]
fn a_long_nested_wasm_loop_runs_past_its_fuel_slices() {
    let got = within(60, || {
        let mut rt = rt();
        eval(&mut rt, &format!("var result = -1, ex; ex = new WebAssembly.Instance(new WebAssembly.Module({LOOPS}), \
            {{ env: {{ cb: () => {{ result = ex.count(20000000); }} }} }}).exports; ex.nest(); String(result)"))
    });
    assert_eq!(got, "0");
}
