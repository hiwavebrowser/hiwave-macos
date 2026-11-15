# STR-3: WebAssembly. A written plan, no code

Author: Talos. Date: 2026-11-13. Base: develop 03e4d100. For Pete to read before any code (Atlas's ask).

## What is wrong today
`WebAssembly` does not exist in the engine, so any page that touches it throws, and JetStream 2.2's wasm workloads (the pinned suite: `WebKit/JetStream` `JetStream2.2` @ `332d8ee1`, `tools/stress/pins.json`) cannot run at all. The first deliverable is therefore not speed: it is that they run and give correct answers, which turns a missing score into a measured one.

**What I could not check.** The suite is not on this machine (the pinned harness fetches it on the Mac), so everything below about what JetStream's wasm tests use comes from my memory of the suite and is marked *unverified*. Task W0 below is the check.

## Facts checked today (docs.rs, 2026-11-13)
- `wasmi` 2.0.0 (crates.io, 2026-09-01): pure-Rust interpreter, `no_std`-capable, "100% WebAssembly spec testsuite compliance", built-in **fuel metering**, configurable recursion depth and stack height, SIMD, bulk memory, multi-value, reference types, tail calls, memory64 and multi-memory available as `Config` switches. `Memory` has `data`, `data_mut`, `data_ptr`, `data_size`, `grow`, and **`Memory::new_static(ctx, ty, &'static mut [u8])`**: a linear memory over a buffer the host supplies.
- `wasmtime` 50.0.1 (2026-11-05): JIT (Cranelift) runtime with a **`MemoryCreator` / `LinearMemory`** trait, which lets the host own a linear memory's bytes.

## The design question that decides the runtime: who owns the bytes
`WebAssembly.Memory.buffer` is an `ArrayBuffer` that must alias the module's linear memory (JetStream's Emscripten glue builds `Uint8Array`/`Int32Array`/`Float64Array` views on it and reads and writes through them while wasm runs). Boa's `ArrayBuffer` owns its bytes (`Option<AlignedVec<u8>>`, detachable); `SharedArrayBuffer` is an `Arc` of atomics. So one side has to lend the other its storage:

- **A. Boa's buffer backs the wasm memory.** The runtime is handed a pointer into an `ArrayBuffer`'s storage (wasmi `new_static`, or wasmtime `MemoryCreator`). `memory.grow` allocates a new buffer, copies, detaches the old `ArrayBuffer` and creates a new one (what the spec requires). This is the one I recommend if the spike confirms it. wasmi's `new_static` takes a fixed `&'static mut [u8]`, so growth would need the buffer reserved at its maximum size up front (virtual reservation; pages beyond the current size stay inaccessible to wasm): *unverified that wasmi accepts that*.
- **B. The runtime's memory backs the `ArrayBuffer`.** Needs a new backing kind in vendored Boa's `ArrayBuffer` (an externally owned region with its own detach rule). A larger Boa change; the cleanest fit if the runtime cannot take a host buffer.
- **C. Copy in and out around every call.** Wrong whenever an imported JS function (called from wasm) reads or writes the memory mid-call, which Emscripten glue does constantly (`printf`, `memcpy_big`, heap growth). Not an option.

## Runtime: decide with a spike, default wasmi
| | wasmi | wasmtime |
|---|---|---|
| Execution | interpreter | Cranelift JIT (and an interpreter, Pulley) |
| Speed vs Chrome | far behind (an interpreter against V8's Liftoff/TurboFan) | much closer once compiled |
| Size and build time | small, quick to build | large dependency tree and long build (matters for the batch gate and the Linux/Windows ports) |
| macOS | nothing special | JIT needs an executable-memory entitlement (hardened runtime): a release and signing question that is Pete's call |
| Host-owned memory | `new_static` | `MemoryCreator` |
| Stopping a runaway loop | fuel metering built in | epoch interruption (*from memory, unverified*) |

Both are Apache-2.0-family (*check the exact terms against our dual licence before adding either*). Recommendation: **W1 to W4 on wasmi** for correctness and a small footprint; **W5 measures** the JetStream wasm scores, and only then decides whether a JIT tier (wasmtime) is worth its size, its entitlement and its build cost. That is the same honest ceiling as JS-PERF2: an interpreter will not match Chrome on a compute benchmark.

## Where it lives
A new portable crate `rustkit-wasm` (no macOS code, no window): compile, instantiate, link, call, memory, table, global, traps, limits, and the unit tests that run on Linux and Windows. The JS-facing `WebAssembly` namespace (`Module`, `Instance`, `Memory`, `Table`, `Global`, `validate`, `compile`, `instantiate`, `CompileError`, `LinkError`, `RuntimeError`) in `rustkit-bindings`, as host functions over `rustkit-wasm`, plus the small Boa change for the `ArrayBuffer` aliasing (A or B).

## Stages, each red-then-green against Chrome 148 values measured first (`tools/parity_oracle`, Playwright, the pinned Chrome for Testing)
- **W0, check before building.** Fetch the pinned JetStream and take a census of every `WebAssembly.*` call, import, export, wasm feature (SIMD? bulk memory? threads/atomics?) and glue function the five wasm workloads use (*I believe: HashSet-wasm, tsf-wasm, quicksort-wasm, gcc-loops-wasm, richards-wasm; unverified*). Then a time-boxed spike of the memory-aliasing question on wasmi: can one `ArrayBuffer`'s storage back a module's memory, survive `memory.grow`, and stay valid while an imported function reads it? Output: a one-page answer (A, B or a runtime change) and the feature list the plan must cover.
- **W1.** `WebAssembly.validate`, `compile`, `instantiate`, `Module`, `Instance`, exported functions over i32/i64/f32/f64 (an i64 argument or result is a BigInt at the JS boundary), start functions, no memory. Pins: small hand-assembled modules whose results and errors come from Chrome.
- **W2.** `Memory`, `Table`, `Global`: imported and exported memory, `memory.buffer` aliasing, `grow` detaching the old buffer, bounds, tables of functions and references, imported JS functions called from wasm and their exceptions crossing back.
- **W3.** Errors and limits as Chrome reports them: `CompileError` and `LinkError` messages, `RuntimeError` traps (unreachable, out-of-bounds, division by zero, stack overflow as a catchable `RangeError`), a memory ceiling, a module-size ceiling, and **interruptibility**: a wasm infinite loop must stop on the view's stop word like script does (JS1's rule), checked by a pin that was red first.
- **W4.** The Emscripten glue end to end on each of the five workloads: each runs, and its own correctness check passes. Any wasm feature the census found beyond W1 to W3 is added here.
- **W5.** Measure. JetStream's wasm scores against Chrome on the Mac in the stress harness's own report, then the JIT-tier decision with numbers.

Not in this plan: threads and `SharedArrayBuffer` atomics from wasm (unless the census says a workload needs them), WASI, exceptions and GC proposals, `WebAssembly.instantiateStreaming` (add if a workload's loader uses it), and anything that needs the real window.

## Decisions for Pete
1. **Interpreter first, JIT later, or JIT from the start?** I recommend interpreter first: it is small and gives correct results quickly; the JIT costs build size, a macOS entitlement and signing complexity, and should be justified by W5's numbers.
2. **Is a new third-party runtime dependency acceptable?** Either runtime is a large new dependency (wasmi small, wasmtime very large); it affects the Windows and Linux ports and the licence audit.
3. **Order against the other stress rows.** Per the 2026-11-12 priority change this comes after iframes and fetch (Speedometer's first score) and Canvas 2D (MotionMark); I would start W0 only when Atlas calls it.
