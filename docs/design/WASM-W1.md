# WASM W1: WebAssembly on wasmi

Row WASM. JetStream 2.2's exit metric needs a score. Its five wasm tests (HashSet, tsf,
quicksort, gcc-loops, richards) need `WebAssembly`, which RustKit deliberately does not define
today (`web_interfaces.js`: `typeof WebAssembly` is `"undefined"`, so a page takes its non-wasm
path). Once #1440 lands, the wasm group may be where the run stops. wasmi is approved (Pete,
2026-11-14), and it needs a `THIRD_PARTY_NOTICES.md` entry. wasmi 2.0.0 is the version in the
local registry.

## What JetStream actually calls

These are from the pinned checkout (`JetStream2.2` @ `332d8ee1`), `JetStreamDriver.js` and
`wasm/*.js`. All five are older Emscripten builds with the same glue:

- **The binary.** The driver fetches it with XHR (`responseType = 'arraybuffer'`) and sets
  `Module.wasmBinary`, so the glue takes `WebAssembly.instantiate(binary, info)`. It never takes
  `instantiateStreaming`, which is guarded by `!Module["wasmBinary"]`.
- **Memory.** `new WebAssembly.Memory({initial: N, maximum: N})` with **initial equal to
  maximum**: 52–134 MB fixed (TOTAL_MEMORY 52,428,800 to 134,217,728). The growth paths exist
  in the glue, but they can't succeed against that maximum.
- **Table.** `new WebAssembly.Table({initial, maximum, element: "anyfunc"})`, imported as
  `env.table`.
- **Imports.** `env.memory` and `env.table` are created in JS. `env.*` functions are JS
  functions the module calls: abort, the stack and heap helpers, time.
- **Exports.** Functions called from JS (`_main`, `stackAlloc`, ...). The glue makes typed
  views over `wasmMemory.buffer` (`HEAP8` ... `HEAPF64`) and reads and writes them between calls.
- **Score.** The driver's `WasmBenchmark` takes `reportCompileTime` and `reportRunTime`.

So W1 is what those five need, done to the spec where they touch it, with the rest stated.

## W1 surface

On `globalThis.WebAssembly`, in every realm (frames included):

| API | W1 |
|---|---|
| `validate(bytes)` | yes |
| `compile(bytes)` | yes. A promise, settled in a microtask; the work is done synchronously (stated) |
| `instantiate(bytes, imports)` | yes, giving `{module, instance}` |
| `instantiate(module, imports)` | yes, giving an `Instance` |
| `Module(bytes)` | yes, with `Module.exports(m)`, `Module.imports(m)` and `Module.customSections(m, name)` |
| `Instance(module, imports)` | yes, with `.exports` a frozen null-prototype object |
| `Memory({initial, maximum, shared?})` | yes, see Memory below. `shared` is a TypeError (no threads) |
| `Table({element, initial, maximum})` | `anyfunc`/`funcref` and `externref`, with `get`/`set`/`grow`/`length` |
| `Global({value, mutable}, v)` | i32, i64 (BigInt), f32, f64, externref, funcref, with `.value` and `valueOf()` |
| `CompileError`, `LinkError`, `RuntimeError` | yes, as `Error` subclasses |
| `compileStreaming`, `instantiateStreaming` | yes, over a `Response` (`arrayBuffer()` then the above) |

Not in W1 (stated: each is rejected at validation, as Chrome rejects a feature it doesn't support):

- threads and shared memory;
- exceptions (`Tag`/`Exception`);
- the JS Promise Integration API;
- **memory64.** It is in wasmi 2.0's default features, so W1 turns it off in the engine `Config`
  (`wasm_memory64(false)`); Chrome 148 ships it, so this is a stated gap, revisited after W1;
- **SIMD.** It is behind wasmi's non-default `simd` cargo feature, and W1 builds without it. None
  of JetStream 2.2's five wasm tests uses SIMD (checked by validating them with SIMD off, in phase
  5). It is a later row if a page needs it.

## Architecture

- **A new crate, `rustkit-wasm`,** wrapping wasmi: module compile, validate, instantiate, and
  the type mapping. `rustkit-js` stays free of wasmi.
- **`rustkit-bindings` installs `WebAssembly`** with host functions on the realm's host object,
  as the other web APIs do (`web_wasm.js` for the JS half, which holds the classes, promises and
  argument checks).
- **One wasmi `Store` per JS realm,** owned by the bindings. Modules, instances, memories, tables
  and globals are store handles, held by the JS objects that wrap them. A realm's store goes with
  its realm.

### The value boundary (JS API §ToWebAssemblyValue / ToJSValue)

| wasm type | JS to wasm | wasm to JS |
|---|---|---|
| i32 | ToInt32 | Number |
| i64 | ToBigInt64 | BigInt (Chrome) |
| f32 | ToNumber, then f32 | Number |
| f64 | ToNumber | Number |
| funcref | null, or an exported wasm function (else TypeError) | that function |
| externref | any value | the same value |

An exported function is a JS function object (`name` = the function index as a string, `length`
= its arity) whose call runs the wasmi function.

### Calls both ways (re-entrancy)

A JS import is a wasmi host function. A wasm export called from JS runs inside a Boa native call
holding `&mut Context`. When wasm calls a JS import, that host function needs the same context.
The store's data carries a pointer to the current Boa context, set by the export's call wrapper
for the call's duration and cleared after. It's only valid inside that call; a host function with
no context set traps. Nested JS → wasm → JS → wasm works the same way, the pointer saved and
restored around each level. An exception thrown by a JS import propagates as that exception
through the wasm frames, as in Chrome. A trap becomes a `WebAssembly.RuntimeError`.

### Memory: one copy, owned by the ArrayBuffer

A `WebAssembly.Memory` allocates `maximum` pages (or `initial` when there's no maximum) as an
`ArrayBuffer` that script cannot detach. That's Boa's existing `from_byte_block_with_detach_key`
patch (PATCHES.md "arraybuffer-detach-key"). wasmi runs over the same bytes with
`Memory::new_static`, so there's no copy and no sync at call boundaries. The `ArrayBuffer`'s
storage must never move while the store lives: the detach key and the store's ownership of the
`ArrayBuffer` object guarantee it.

- **byteLength.** It is `initial × 64 KiB` only when `initial == maximum`, which is JetStream's
  and every fixed-memory Emscripten build's case. W1 creates growable memories only with a
  maximum (and a cap below) and **does not grow them past `initial`**: `memory.grow(n)` with
  `n > 0` throws `RangeError` from JS and returns −1 to wasm. That's legal: a grow may fail.
  It's a stated limit. W2 is a Boa patch for an `ArrayBuffer` viewing external bytes, which lets
  a grow detach the old buffer and hand out a new one at the new size, as the spec says.
- **Cap.** A per-realm total of linear memory (1 GiB in W1, a constant), enforced with wasmi's
  `ResourceLimiter`. Past it, `Memory` throws `RangeError`, as Chrome does when its own reservation
  fails.
- **Allocation.** Zeroed lazily (`alloc_zeroed`): untouched pages cost no resident memory.

### The script deadline

A long wasm loop has to honour the live turn's deadline and Stop, the same as JS. wasmi fuel
metering is on. The export wrapper hands out fuel in slices; when a slice runs out, it checks the
deadline and the interrupt word (rustkit-js's), then refills or traps with the same uncatchable
stop a JS script gets. The slice size is set by measurement, as low as overhead allows.

## Tests (red first)

- **API shape and edges, against Chrome for Testing 148** (receipt in `docs/diagnostics/`):
  `typeof WebAssembly`, the constructors' `length`/`name`, `new WebAssembly.Memory({initial:1})`
  `.buffer.byteLength`, `validate` on good and bad bytes, `Instance.exports`' prototype and
  frozenness, an i64 export returning a BigInt, a JS import's exception passing through, a trap's
  error class and message pattern, a `Table` of `anyfunc`, a `Global`'s `value` and mutability.
- **Spec testsuite basics** (WebAssembly/testsuite, hand-picked modules as inline bytes:
  `i32`/`f64` arithmetic, `call_indirect` through a table, memory load/store at the boundary,
  traps), checked against Chrome's answers.
- **JetStream's own first calls:** quicksort's glue with its `.wasm`, run in the engine with
  frames on, to `reportRunTime`. Then the other four. The JetStream score comes from the batch's
  JetStream run.
- **A deadline test:** an infinite wasm loop is stopped at the budget, and the page lives on.
- **A navigation-release test:** an instantiated module's realm is released after navigation.
  This is #1440's lesson about GC roots in host closures, applied from the start.

## Phases (each red, then green)

1. **`rustkit-wasm` with `validate`/`Module`/`Instance`**, numeric exports and imports, and the
   error classes. No memory or table yet.
2. **Memory** (the `new_static` design), the cap, and `buffer` identity.
3. **Table and Global**, `call_indirect`, and funcref across the boundary.
4. **The promise forms and streaming,** and the deadline via fuel.
5. **JetStream's five wasm tests run in the engine;** their compile and run times are reported.

## Costs and risks, stated

- **Speed.** wasmi is an interpreter, so JetStream's wasm scores will be far below Chrome's JIT.
  W1 is about running and scoring at all. Speed is a later row: wasmi's register machine is among
  the fastest interpreters, and a JIT is out of scope.
- **Size.** wasmi and its dependencies add to the binary (measured and stated in the PR).
- **Security.** wasm is sandboxed by construction (wasmi is memory-safe Rust). The new surface is
  the boundary code: re-entrancy through a raw context pointer, scoped to one call, and memory
  over an undetachable buffer. Both get adversarial tests (a host function outliving its call, a
  detach attempt).
