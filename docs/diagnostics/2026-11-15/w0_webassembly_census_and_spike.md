# W0: WebAssembly census and the memory-aliasing spike

Author: Talos. Date: 2026-11-15. Base: develop c5cfddac. Plan: `docs/diagnostics/2026-11-13/str3_webassembly_plan.md` (W0, "check before building").
Evidence: the pinned JetStream checkout on this machine (`git rev-parse HEAD` = `332d8ee1e4d5c40d9492c56f6865e64a9525ee9f`, the pin in the plan), wasmi 2.0.0 and its source, the vendored Boa in this tree. No repo code and no dependency is added by this change: the spike is a standalone crate under `w0_spike/`.

## The answer

1. **Option A works on wasmi, with one small Boa change.** One Boa `ArrayBuffer`'s storage can back a wasmi memory, stay aliased in both directions (including while an imported function reads it mid-call), and survive `memory.grow` in place with no copy. The Boa change: a way to make an `ArrayBuffer` with a **detach key**. Today `JsArrayBuffer::from_byte_block` always uses an undefined key, and the spike shows script can then `transfer()` the buffer out from under the running memory (case 11 below), which is a use-after-free hazard as well as a spec violation. Boa already has the `[[ArrayBufferDetachKey]]` slot and a public `detach(&key)`; only the constructor is missing.
2. **No wasm feature beyond the MVP is needed by any of the five workloads.** No SIMD, bulk memory, atomics or threads, exceptions, tail calls, multi-value, reference types or multiple tables. No `i64` or `v128` at the JS boundary, so BigInt conversion is not needed to run them (the spec still wants it, and W1 keeps it).
3. **Recommendation: wasmi, as the plan said.** All five binaries validate, compile and instantiate in wasmi (1.6 ms or less each, with stub imports). Its normal dependency tree, with default features off, is 8 crates, all MIT or Apache-2.0 dual-licensed.

## Census: the five pinned wasm workloads

Binaries are old Emscripten output (asm2wasm era, wasm binary version 1). Script: `w0_census.py` (pure Python, reads the sections and decodes every function body).

| | bytes | functions | imported funcs | memory (imported, pages) | table (imported) | start | custom sections |
|---|---|---|---|---|---|---|---|
| quicksort | 12,512 | 31 | 10 | 1024 (64 MiB), min = max | 6, min = max | none | none |
| richards | 12,886 | 25 | 7 | 1280 (80 MiB), min = max | 8, min = max | none | none |
| gcc-loops | 171,404 | 561 | 27 | 1024 (64 MiB), min = max | 481, min = max | none | none |
| HashSet | 196,315 | 115 | 16 | 2048 (128 MiB), min = max | 34, min = max | none | none |
| tsf | 185,298 | 392 | 23 | 800 (50 MiB), min = max | 145, min = max | none | none |

- **Imports** are `env.memory`, `env.table`, i32 globals (`tableBase`, `DYNAMICTOP_PTR`, `STACKTOP`, `STACK_MAX`), f64 globals `global.NaN` and `global.Infinity` (gcc-loops and tsf), and JS functions with only `i32` parameters and results: `abort`, `enlargeMemory`, `getTotalMemory`, `abortOnCannotGrowMemory`, `___setErrNo`, `___syscall*`, `_emscripten_memcpy_big`, `_gettimeofday`, `_getenv`, `_pthread_*` stubs, `___cxa_*` (JS-side C++ exception glue, not wasm exceptions), `_abort`, `_exit`, `___lock`, `___unlock`, `___map_file`, `_strftime_l`, `_emscripten_asm_const_i`.
- **Exports** are all functions (the Emscripten `_main`, `_malloc`, `_free`, `_memcpy`, `dynCall_*`, `stackAlloc` family and each program's own entry points); no exported memory, table or global.
- **Instruction families**: MVP only in all five (decoded, not assumed): no `0xFC` (bulk memory, saturating truncation), `0xFD` (SIMD), `0xFE` (atomics), `try`/`delegate`, tail calls, typed `select`, reference-type instructions (`ref.*`, `table.get/set`), sign-extension ops, or `call_indirect` on a table other than 0. No function type returns more than one value. `i64` appears only inside some functions (gcc-loops 18 of 561, HashSet 3 of 115, tsf 6 of 392); no exported or imported function has an `i64` or `v128` parameter or result. gcc-loops, HashSet and tsf also use `f32`/`f64` internally.
- **Memory never grows in practice.** Every memory has initial = maximum, and the glue's `enlargeMemory` calls `abortOnCannotGrowMemory`. The `memory.grow` path (`reallocBuffer`, in quicksort.js and gcc-loops.js) is not reached. Growth is a spec requirement and the spike covers it, but these five do not exercise it.

## What the JS glue calls (the `WebAssembly` surface the five need)

- `new WebAssembly.Memory({initial, maximum})` with initial = maximum, then `.buffer` once; `HEAP8`, `HEAPU8`, `HEAP32`, `HEAPF64` and friends are views on that buffer, and the glue calls `.set` and `.subarray` on them (including `HEAPU8.set(HEAPU8.subarray(src, src + num), dest)` for `_emscripten_memcpy_big`).
- `new WebAssembly.Table({initial, [maximum], element: "anyfunc"})`, kept in `Module.wasmTable`; no `get`, `set` or `grow` call on it was found in the glue.
- `WebAssembly.instantiate(binary, info)`, a Promise. The glue only takes the `instantiateStreaming` path when `Module.wasmBinary` is unset, and the JetStream driver sets it (from `read(path, "binary")` in the shell, or an XHR in the page), so `instantiateStreaming` is not needed for these five.
- The workloads' harness needs the host to provide `read(path, "binary")` returning binary data. That is harness, not engine, and is outside this stage.

## The spike: what was tested

`w0_spike/src/main.rs` drives a hand-assembled module (imported memory of 1 to 4 pages, an imported host function, `store32`, `load32`, `grow`, `size`, `call_host`) from Rust against a real Boa `Context` and `JsArrayBuffer`, with the wasmi memory made by `Memory::new_static` over the `ArrayBuffer`'s own storage. The storage is reserved at the memory's **maximum** (wasmi's `new_static` records the buffer's full length as capacity and `grow_static` extends the length in place up to it, zero-filling; this is what the plan marked "unverified"), and the `ArrayBuffer`'s length is the current size.

| # | check | result |
|---|---|---|
| 1 | `ArrayBuffer` of the initial size (65,536) over a 4-page reservation | ok; `resize` within capacity does not move the storage |
| 2 | `Memory::new_static` over that storage | ok; wasmi's data pointer equals the `ArrayBuffer`'s |
| 3 | write `[1,2,3,4]` through the `ArrayBuffer`, `load32(0)` in wasm | `0x04030201` |
| 4 | `store32(8, 0xdeadbeef)` in wasm, read the `ArrayBuffer` | `0xdeadbeef` |
| 5 | wasm calls an imported host function that reads the storage mid-call | `0xdeadbeef` |
| 6 | `memory.grow(1)` | returns 1; size 2 pages; wasmi's data pointer **unchanged** |
| 7 | detach the old `ArrayBuffer` and take its storage back | old `byteLength` 0; storage returned as an `AlignedVec` |
| 8 | resize in place (within the reservation) and make a new `ArrayBuffer` | `byteLength` 131,072; earlier contents kept; storage pointer unchanged |
| 8b | wasm store into the grown page made before the re-wrap (the zero-fill bug case) | survives the re-wrap with `set_len`; wiped with `resize(new_len, 0)` |
| 9 | wasm store in the new page, read through the new `ArrayBuffer` | `0x12345678` |
| 10 | grow past the maximum | returns -1, as the spec says |
| 11 | script calls `transfer()` on a buffer made by `from_byte_block` | **succeeds**: the buffer needs a detach key |

`w0_spike/src/bin/load5.rs <JetStream wasm dir>` loads each of the five real binaries into wasmi with stub imports (a stub function per imported signature returning zeros, a memory and table of the imported limits, zero globals): all five validate, compile and instantiate (release build; a debug build is 10 times slower: 2 to 20 ms); compile 0.2 ms (quicksort, richards) to 1.6 ms (gcc-loops); instantiation under 0.1 ms. wasmi translates lazily, so this is not a measure of run speed.

## Things W1 to W4 have to design around (found by the spike)

- **The Boa change (W2):** a public constructor for an `ArrayBuffer` with a detach key (the spec's key is the string "WebAssembly.Memory"), so script cannot detach or transfer a wasm memory's buffer, and host code can, with the key, when the memory grows. Everything else needed is already public: `JsArrayBuffer::detach(&key)` returns the `AlignedVec`, `from_byte_block` takes one back.
- **The aliasing is an accepted violation of Rust's aliasing rules, not a proof of soundness.** `Memory::new_static` takes a `&'static mut [u8]`, so wasmi holds a unique reference to the storage for the memory's whole life, while Boa hands out `data()` / `data_mut()` references to the same bytes. The lifetime is extended by an `unsafe` slice over the `AlignedVec`'s allocation. What the design relies on instead, and W1 must state and test: (a) one thread, and wasm calls are synchronous, so the two sides never touch the bytes at the same time; (b) the `ArrayBuffer` that owns the allocation outlives the wasmi memory (the `WebAssembly.Memory` and `Instance` objects hold it strongly); (c) nothing detaches or frees it except the grow path (the detach key); (d) **no `data()` or `data_mut()` guard on the buffer, shared or exclusive, is held across a call into wasm**: a shared guard alive during a wasm store is as much a data race on the same bytes as an exclusive one; (e) teardown order: the wasmi `Store` (and every `Memory` and `Instance` in it) is dropped or made unreachable before the buffer's storage is freed. The sanitizer run Atlas asked for (Miri or ASan on this spike before W1 merges) tests the single-thread claim; it cannot make the aliasing sound.
- **The grow re-wrap must not zero the grown region (a bug found in review, fixed in the spike).** The first version re-wrapped with `old.resize(new_len, 0)`, which zero-fills the new pages. A wasm store into the new pages after `memory.grow`, in the same call and before the re-wrap, would be wiped. The spike now stores into the grown page before the re-wrap (case 8b below) and the re-wrap only changes the length (`set_len`; the whole reservation is initialised when it is created). W1 keeps this as a test: wasm writes after a grow inside one call, then the buffer is re-wrapped, and the bytes are still there.
- **Host functions must be `Send + Sync`.** wasmi's `Linker::func_wrap` and `Func::wrap` require it, and Boa objects are not `Send`. Imported JS functions have to be reached through the store's data (`Caller<T>`), not through captured handles. The spike hit this and worked around it with a wrapper for a raw pointer; the real bridge needs the store data design.
- **Reserving the maximum is eager.** `AlignedVec` has no lazily-zeroed allocation, so a memory of 64 to 128 MiB is zeroed and committed when it is created (my understanding, not checked here, is that Chrome reserves address space and commits lazily instead). For these five, initial = maximum anyway, so the whole buffer is needed at once; the cost is the zeroing time and resident size per instance, not measured here.
- **Interruptibility (W3).** wasmi has fuel metering and a resumable out-of-fuel call (`ResumableCallOutOfFuel`, `TypedResumableCallOutOfFuel`), which is what a "run a slice, check the stop word, refuel, resume" loop needs. I read the API and did not run it.

## Licence and dependency footprint (for the W1 `THIRD_PARTY_NOTICES` entry)

With `default-features = false`, `cargo tree -e normal` for wasmi 2.0.0 is: `wasmi`, `wasmi_core`, `wasmi_collections`, `wasmi_ir` (all MIT/Apache-2.0), `wasmparser` 0.228.0 (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT), `bitflags` 2.13.2 (MIT OR Apache-2.0), `libm` 0.2.16 (MIT), `spin` 0.9.9 (MIT). Its default features add `wat` and friends (`wast`, `wasm-encoder`, `bumpalo`, `leb128fmt`, `memchr`, `unicode-width`); the text format is not needed outside tests. The exact set depends on the features W1 turns on (at least `std` and `validate`) and has to be re-listed then. The licence text of each goes into `THIRD_PARTY_NOTICES` in the same PR that adds the dependency (W1), not here.

## What was not checked

- **No workload was run.** The five run on the Mac harness with `read(path, "binary")` and the Emscripten syscall glue; the spike loads and instantiates them with stub imports only. So there is **no speed number**: an interpreter against V8 is far behind, and W5 measures it.
- **Chrome 148 values for the W1 to W3 pins** are not measured here (this stage had nothing to pin).
- **`WebAssembly.Table` as a JS object** (`get`, `set`, `grow`, `length`) is not used by the five's glue, so the census does not cover it; W2 covers it from the spec and Chrome.
- **The out-of-fuel resume** (above) and **wasmi on the Mac and Windows builds** were not run; the spike ran on Linux x86-64 only.
- **The `unsafe` aliasing** was exercised on a single thread; it was not run under Miri or a sanitizer.

## Where this leaves the plan

Option A (Boa's buffer backs the wasm memory) on wasmi is confirmed feasible, so no runtime change and no Boa `ArrayBuffer` redesign (option B) is needed. W1 can start: the new `rustkit-wasm` crate, with wasmi as the one new dependency behind a cargo feature that is off by default, and its `THIRD_PARTY_NOTICES` entry in the same PR. The decisions the plan left for Pete (a new third-party dependency, order against other rows) are for the live Atlas session or Pete to confirm before that PR.
