# JIT plan: measurements first, then a staged decision

Plan only, no engine code. Author: Talos. Date: 2026-11-15. Base: develop 55547c08 (V1.5.0). For Pete's decision (Atlas's brief, 2026-11-14: "scope a baseline JIT ... design-and-measurement plan ... Pete decides on that plan").

## The answer first

**Do not start a JIT yet. Spend the next stage on the interpreter's runtime, with a measured go/no-go before any machine code.** Three measurements drive this:

1. **A baseline JIT is a small step, even in V8.** On 16 JetStream 2.2 workloads, V8's baseline JIT (Sparkplug) over its own interpreter (Ignition) is **1.34x** (geometric mean; 1.1x to 1.6x per workload). The large factors come from the optimizing tiers: Maglev adds **6.1x** on top of Sparkplug, Turbofan another 1.3x. A "template/baseline JIT over Boa's bytecode" is a Sparkplug: it removes dispatch and keeps every operation's runtime cost.
2. **Our gap is in the runtime, not in dispatch.** Our interpreter is **9.5x slower than V8's interpreter** (geometric mean over 13 workloads; 3.4x to 18.8x; 12.4x if OfflineAssembler's regex pathology is counted). Isolated operations show where: a loop turn and local arithmetic are 1.5x to 2.4x Ignition, but an own-property read that hits the inline cache is ~10x, a call ~11.5x, an object literal ~20x, a method found two prototype levels up ~49x.
3. **So a baseline JIT over today's runtime buys about 1.1x to 1.2x.** A trivial opcode costs ~5.2 ns here (dispatch included); the average opcode on richards costs ~20 ns, on Babylon ~38 ns. Removing dispatch can only take part of the ~5 ns from each opcode. The CodeMirror profile from the Mac (2026-11-13) agrees: interpreter dispatch ~25%, the rest is value refcounting, property lookup, allocation and calls.

The order that the evidence supports: **S0 runtime work (where the 10x is) → measure → S1 baseline JIT only if dispatch has become the dominant cost → S2 optimizing tier as a separate decision**, which is where V8's big factor lives and which is a multi-quarter project with a different risk profile.

## How it was measured

- **Workloads**: 16 JetStream 2.2 workloads (pin `332d8ee1`, `tools/stress/pins.json`, fetched by `tools/stress/fetch.sh`), chosen to cover object-oriented code (richards, delta-blue, raytrace), numeric loops (navier-stokes, crypto, sha256), parsers and compilers (Babylon, Basic, OfflineAssembler, Air), collections and GC (splay, hash-map, cdjs), and search (ai-astar, UniPoker). Babylon is the nearest to CodeMirror's Lezer parse.
- **Harnesses** (committed): `crates/rustkit-js/examples/jsbench.rs` (a fresh `JsRuntime`; the bindings' own `web_legacy.js` and `web_encoding.js` as pages get them, plus `performance.now`) and `docs/diagnostics/2026-11-15/jsbench.mjs` (Node 24.22.2, V8 13.6.233.17, scripts in the main context). Both load the workload's files, create one `Benchmark`, and time one `runIteration()` call: **the timer wraps the iteration only, no load or parse.** Steady state is the median of iterations 2..N (N = 6 here, 15 on V8).
- **V8 tiers** by flag: `--jitless` (Ignition only), `--always-sparkplug --no-maglev --no-turbofan` (Ignition + Sparkplug), `--no-maglev --no-turbofan` (Sparkplug at its normal tier-up), `--no-turbofan` (+ Maglev), no flags (all tiers).
- **Machine**: Linux, Intel i7-10750H, release profile (`opt-level 3`, thin LTO, one codegen unit), Rust 1.91.0. Runs were sequential with no other build running.
- **Not available**: a sampling profiler. `perf` is blocked here (`kernel.perf_event_paranoid=4`, changing it needs root, not done). The time split below therefore comes from **timed micro-loops plus the interpreter's own opcode counters** (`RUSTKIT_VM_STATS=1`, #1116), not from samples. A Mac `sample`/Instruments run of three workloads through `jsbench` would confirm it (asked below).
- Raw data: `jit_plan_tiers.jsonl` (every iteration), `tiers_table.py` (the table below), `micro/*.js`.

## Where the time goes

### Per workload (steady ms per iteration)

| workload | RustKit | V8 interp | V8 +Sparkplug (lazy) | V8 +Sparkplug (always) | V8 +Maglev | V8 all tiers | RustKit / V8 interp | V8 interp / +Sparkplug(always) | V8 interp / all tiers |
|---|---|---|---|---|---|---|---|---|---|
| richards | 1133.5 | 105.1 | 88.0 | 95.8 | 5.1 | 4.3 | 10.8x | 1.1x | 24.4x |
| delta-blue | 868.1 | 73.2 | 60.0 | 60.1 | 3.6 | 2.8 | 11.9x | 1.2x | 26.2x |
| raytrace | 1129.4 | 98.6 | 65.8 | 68.3 | 9.4 | 4.7 | 11.5x | 1.4x | 20.8x |
| navier-stokes | 257.1 | 76.2 | 56.5 | 59.0 | 4.8 | 3.9 | 3.4x | 1.3x | 19.8x |
| crypto | 580.0 | 87.8 | 58.1 | 58.6 | 4.5 | 3.1 | 6.6x | 1.5x | 28.1x |
| splay | 602.4 | 54.5 | 44.7 | 45.8 | 19.2 | 15.9 | 11.0x | 1.2x | 3.4x |
| earley-boyer | fails | 50.6 | 35.4 | 34.6 | 6.2 | 5.7 | - | 1.5x | 8.9x |
| Babylon | 459.8 | 37.1 | 26.3 | 26.0 | 4.0 | 4.1 | 12.4x | 1.4x | 9.1x |
| Air | 412.6 | 21.9 | 17.3 | 16.1 | 8.5 | 6.9 | 18.8x | 1.4x | 3.2x |
| Basic | fails | 26.6 | 20.6 | 20.5 | 5.8 | 4.7 | - | 1.3x | 5.6x |
| cdjs | 1700.3 | 188.6 | 152.7 | 146.6 | 26.1 | 21.1 | 9.0x | 1.3x | 8.9x |
| hash-map | 1644.4 | 162.9 | 125.5 | 133.4 | 12.0 | 6.9 | 10.1x | 1.2x | 23.5x |
| UniPoker | 462.2 | 49.5 | 33.8 | 33.4 | 10.9 | 9.3 | 9.3x | 1.5x | 5.3x |
| ai-astar | 770.2 | 60.0 | 42.1 | 43.4 | 9.8 | 6.8 | 12.8x | 1.4x | 8.8x |
| stanford-crypto-sha256 | 481.5 | 89.6 | 74.5 | 71.6 | 6.8 | 5.6 | 5.4x | 1.3x | 15.9x |
| OfflineAssembler | 50680.2 | 117.0 | 76.9 | 75.1 | 34.4 | 32.3 | 433.0x | 1.6x | 3.6x |

Geometric means: RustKit / V8 interpreter **9.5x** over the 13 workloads that run (12.4x with OfflineAssembler); RustKit / V8 all tiers **114x** (13 workloads). V8 interpreter / +Sparkplug **1.34x**; +Sparkplug / +Maglev **6.1x**; +Maglev / all tiers **1.27x** (16 workloads).

Two workloads fail here and are interpreter correctness rows, not speed (below): **Basic** (legacy `RegExp.lastMatch` / `RegExp.rightContext` are missing) and **earley-boyer** (our call depth limit is 6,551 frames for a one-argument recursive function; Chrome 148 allows 9,583).

### Per operation (micro-loops, 2M turns, `micro/*.js`, ns per turn above an empty loop)

| operation | RustKit | V8 Ignition | ratio | opcodes here |
|---|---|---|---|---|
| empty loop turn (the floor: compare, increment, jump) | 20.8 (whole turn) | 8.5 | 2.4x | 4 per turn → **~5.2 ns per trivial opcode** |
| three local arithmetic ops | 20.5 | 13.2 | 1.5x | |
| own property read, cache hit | 22.5 | 2.3 | ~10x | 1 `GetPropertyByName` |
| own property write, cache hit | 25.5 | 4.6 | 5.5x | 1 `SetPropertyByName` |
| dense array element read | 26.5 | 7.0 | 3.8x | |
| method two prototype levels up | 167 | 3.4 | ~49x | 1 `GetPropertyByName`, not cacheable |
| call to an empty one-argument function | 160 | 13.8 | 11.5x | 13 opcodes per call |
| method call reading `this.x` | 183 | 17.8 | 10x | |
| two-property object literal | 347 | 17.7 | ~20x | `StoreEmptyObject` + 2 `DefineOwnPropertyByName` + 3 `Move` |

### What the opcode counters say (one iteration, `RUSTKIT_VM_STATS=1`)

- **Babylon** (12.5M opcodes, ~38 ns each): `GetPropertyByName` 17%, `PushFromRegister` 15%, `This` 11%, `PopIntoRegister` 5%, `Move` 5%, `Call` 4%. **Over a third of all opcodes move values for calls** (pushing arguments, `this`, popping results). 10% of property reads miss their cache *every time*: the parser's methods (`this.next`, `this.eat`, `this.finishToken`) live more than one prototype level up, and node-building stores (`node.start = …`, `.end`, `.loc`, `.type`) add properties, which our cache does not handle.
- **richards** (56.4M opcodes, ~20 ns each): `GetPropertyByName` 18%, `This` 16%, `PushFromRegister` 10%; the caches almost never miss (178 cold misses). This is the "everything hits and is still ~10x" case: the hit path itself is expensive.
- **navier-stokes** (27.3M opcodes): `Move` 32%, then `GetPropertyByValue`, `Inc`, `Add`, `Mul` ~12% each. Numeric code is the nearest to V8's interpreter (3.4x) because arithmetic is cheap here; the `Move`s are what is left of item 6 (#1149) on `var`-heavy loops.
- **OfflineAssembler** (24.5M opcodes in 51.7 s, **~2.1 µs each**): the interpreter is not where it goes. It executes `StoreRegexp` 403k times; a regex literal costs ~5.5 µs to create here against <0.1 µs in V8's interpreter (the pattern is compiled on every evaluation; V8 keeps it per site), and `exec` is ~11x. That explains part, not all, of the 433x; without a profiler I am naming it as the likely cause, not attributing the whole.
- **MotionMark's end-of-test analysis** (Athena, 2026-11-15, Windows): `Array.prototype.sort` itself is fine; plain JS loops are 55-100x Chrome. Same conclusion: the interpreter's per-operation cost.

## Expected gain per stage, with the evidence for each

| stage | what | expected gain on these workloads | evidence | confidence |
|---|---|---|---|---|
| **S0** | runtime work in the interpreter (list below) | **2x to 3x** overall (from 9.5x Ignition toward 3x to 4x) | the per-operation table: calls, allocation, property hits and deep prototype reads are 10x to 49x Ignition while dispatch is 2.4x; those operations are most of the opcode mix | medium: each item gets its own red-first measurement, as JS-PERF1/2 did |
| **S1** | baseline JIT over Boa's bytecode (Sparkplug-like) | **1.1x to 1.2x today; ~1.3x after S0** | V8's own Sparkplug/Ignition ratio is 1.34x with a lean runtime; here the trivial-opcode floor (~5.2 ns, dispatch included) is a quarter or less of an average opcode (~20 to 38 ns) | medium-high for the bound; S1 cannot exceed the dispatch share |
| **S2** | optimizing tier: type feedback, speculation, inlining, deoptimization | V8 shows 6x over its baseline; for us, unknown until S0/S1 exist | V8 Maglev/Sparkplug 6.1x geometric mean | low: this is a compiler project, not a row |

### S0 items (each its own PR, red first, measured with `jsbench` and the counters)

1. **Inline caches for prototype depth > 1 and for property-adding stores** (shape transitions). Babylon's every-time misses are exactly these; a method read two levels up is ~49x, an object literal ~20x. JS-PERF2 item 5.
2. **The call path**: arguments and `this` go through `PushFromRegister`/`PopIntoRegister` and a heap frame; 13 opcodes per call and ~11.5x Ignition. Register-based argument passing and a cheaper frame push.
3. **The cache-hit path**: a hit still clones the object handle, borrows, and walks the IC entry list (~22 ns vs 2.3). Monomorphic fast check first, no refcount traffic on a hit.
4. **Value refcount traffic**: every copy of an object or string value increments a count (boa_gc roots by count). JS-PERF2 item 6 removed most `Move`s on `let`/`const` locals; `var`-heavy numeric loops still spend a third of their opcodes in `Move`.
5. **Regex literals compiled once per site** (OfflineAssembler) and regex `exec` (~11x). Not interpreter work, but the biggest single factor found.

**Go/no-go for S1** (measured with this harness, same workloads): RustKit / V8 Ignition geometric mean **≤ 4x**, *and* the trivial-opcode floor is **≥ 40%** of an average opcode's cost on richards and Babylon (i.e. dispatch has become the thing to remove). If S0 stalls above that, S1's ceiling stays ~1.2x and it is not worth its security and maintenance cost.

## S1 design: a baseline JIT, if S0 says go

**Shape (Sparkplug's).** Compile each function's bytecode, unchanged, to machine code that performs the same operations in the same order on the same frame: the register file stays where the interpreter keeps it, each opcode becomes either a few inline instructions (moves, jumps, int32 fast paths) or a direct call to the existing opcode handler. No speculation, so:

- **Deopt / bailout**: none needed. The frame layout is the interpreter's, so leaving JIT code at any bytecode boundary is a jump into the interpreter at that `pc` (and entering JIT code from the interpreter at a loop header is the reverse, for long-running loops). A pc → native-offset table per function is the only map.
- **Exceptions**: an opcode that fails returns its error to a per-function stub that calls the existing `handle_error` with the faulting `pc`; the code block's handler table decides, and execution resumes at the handler's native offset (or returns to the caller), exactly as the interpreter does.
- **GC**: boa_gc finds roots by reference count, not by scanning stacks, so JIT code needs **no stack maps** as long as it keeps values in the frame's register file (not in machine registers across a call that can allocate) and performs the same `clone`/`drop` count changes as the handlers. The handlers it calls already do.
- **Generators / async**: frames are saved and resumed by `pc`; resuming enters JIT code through the same pc → native table.
- **Our own patches stay in force**: the interrupt word and recursion checks at calls and loop back-edges (`IncrementLoopIteration`), the native-stack budget, `vm-stats` (interpreter-only), the stop-finally rule. Each is a call or a compare in the generated code, pinned by the existing `vm_interrupt*`, `watchdog_turns`, `native_recursion`, `stack_overflow_catchable` tests running in JIT mode.
- **Tier-up**: a function is compiled after N calls or loop back-edges (V8's Sparkplug compiles very early because it is cheap); the counter lives on the code block.

**Code generation backends** (choose at S1 start; both need a THIRD_PARTY_NOTICES entry):
- **Cranelift** (Apache-2.0 WITH LLVM-exception; Wasmtime's backend): x86-64 and arm64 from one IR, mature, but compiles slower than a template JIT needs for very cold code.
- **A template assembler** (`dynasm-rs`, MPL-2.0, or hand-written encoders): fastest compile, closest to Sparkplug, but every opcode template is written per architecture.
- Recommendation: start arm64 (the Mac, where users are) with templates for the hot simple opcodes and handler calls for the rest; x86-64 later needs the same templates re-encoded (Linux/Windows), roughly half the S1 work again.

## Security posture: what a JIT changes

Today the engine maps no executable memory. A JIT makes writable-then-executable memory and generated code part of the attack surface (a bug in code generation becomes memory corruption with code execution; JIT spraying). Required before any default-on:

- **W^X everywhere, never RWX.** macOS arm64: `mmap(MAP_JIT)` and `pthread_jit_write_protect_np` toggled per thread around writes, `sys_icache_invalidate` after; under the hardened runtime the app needs the **`com.apple.security.cs.allow-jit`** entitlement (not `allow-unsigned-executable-memory`), which is a signing and notarisation change Pete owns. Linux: map RW, write, `mprotect` to RX (and clear the instruction cache on arm64). Windows: `VirtualAlloc` RW then `VirtualProtect` RX, `FlushInstructionCache`; with Control Flow Guard on, generated entry points must be registered as valid call targets. (API names from memory; each is re-checked against the vendor documentation at S1 start.)
- **One executable-memory module.** All code allocation, permission flips and freeing go through one small module with its own tests; nothing else maps executable memory.
- **Kill switch, interpreter-only by default.** `EngineConfig::js_jit` off by default, an environment override for testing, and a runtime switch that stops compiling and runs everything in the interpreter. Interpreter-only stays the default until S1's differential suite and fuzzing have run clean for an agreed period and Pete flips it.
- **Policy decisions for Pete**: whether to offer a user-facing "disable JIT" setting (as browsers' lockdown modes do), and whether the JIT stays off for some content.

## Test strategy

- **Differential: interpreter vs JIT on everything we already pin.** Every Chrome-measured case file in `tools/parity_oracle/*_cases.json` (eval order's 1,200 generated programs and hand cases, TDZ, update conversion, array length, super set, …) runs in both modes and must agree with Chrome and with each other; the rustkit-js test suite runs twice (a `--features jit` job); the 16 JetStream workloads must produce the same results.
- **A generated-program fuzzer** in the style of `gen_eval_order_cases.py`, run interpreter vs JIT with any difference a failure, in CI and as a long-running job.
- **Tier-boundary tests**: entering and leaving JIT code mid-loop, exceptions thrown from JIT code into interpreter handlers and the reverse, generators resumed in both modes, the interrupt and recursion tests in JIT mode.
- **test262** is not vendored today; S1 should run its language/ subset in both modes before default-on (licence: BSD-3-Clause, notices entry).

## Milestones, each with its go/no-go measurement

| # | milestone | measure | go if |
|---|---|---|---|
| M0 | this plan + harness (this PR) | the tables above, reproducible | Pete picks a stage order |
| M1 | S0 items 1-3 (caches, call path, hit path) | `jsbench` geometric mean vs Ignition; per-op micro table | RustKit / Ignition drops from 9.5x to ≤ 6x |
| M2 | S0 items 4-5 + whatever the counters show next | the same | ≤ 4x, and the trivial-opcode floor ≥ 40% of an average opcode |
| M3 | S1 prototype on arm64: handler-call JIT for all opcodes, inline templates for moves/jumps/int32 | differential suite green; `jsbench` | ≥ 1.2x over the M2 interpreter on the geometric mean, no workload slower |
| M4 | S1 hardening: W^X module, entitlement, kill switch, fuzzing, test262 | security review (Prometheus + an independent read) | clean fuzz period agreed with Pete |
| M5 | x86-64 port of S1 | the same suite on Linux/Windows | same bar as M3 |
| M6 | S2 decision | Maglev-class gains need type feedback and deopt; separate plan | Pete |

## Rows found while measuring (not JIT work; correctness first)

- **REGEX1**: regex literals are compiled on every evaluation (~5.5 µs here vs <0.1 µs in V8's interpreter); `exec` ~11x. OfflineAssembler is 433x Ignition. Likely the largest single real-site factor in this list.
- **REGEXP-LEGACY**: `RegExp.lastMatch`, `RegExp.rightContext` (and the other legacy static properties every browser has) are missing; JetStream's Basic throws a TypeError in its lexer.
- **DEPTH2**: call depth 6,551 frames here vs 9,583 in Chrome 148 for `function d(n){return n===0?0:1+d(n-1)}` (bare runtime; the page runtime's budget to be checked); earley-boyer overflows.

## What I need from the Mac

`sample` (or Instruments) on three workloads through `jsbench` (richards, Babylon, hash-map), release build, so the split above (from micro-loops and counters) is checked against real samples, and the same tier table on arm64 (Apple Silicon may move the dispatch share).
