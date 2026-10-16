# Vendored boa_engine 0.22.0, with one patch

Source: crates.io `boa_engine` 0.22.0 (upstream `boa-dev/boa` at `337a3668`,
`core/engine`), the version `Cargo.lock` pins. The first commit that added this
directory is the crate exactly as published; every change since is marked
`HIWAVE PATCH (interrupt)` and listed here. Wired in through `[patch.crates-io]`
in the workspace `Cargo.toml`. License as upstream (Unlicense OR MIT, see
`Cargo.toml`).

## Patch: a cross-thread interrupt word

**Why.** Boa 0.22 has no way for another thread to stop a running script
(`vm/runtime_limits.rs` has recursion, stack and loop-count limits only). A
page's `while (true) {}` that never calls the host therefore held its thread
for good: HiWave's host-call refusal (T0, PR #666) only stops script at its
next call into the host. The threads plan (`docs/design/THREADS_PLAN_2026-10.md`,
invariant 6, step M1) needs a stop that a watchdog thread or the user's Stop
can set.

**What.** About 50 lines in four files:

| file | change |
|---|---|
| `src/vm/mod.rs` | `Vm` gains `interrupt: Option<Arc<AtomicU64>>` and `interrupt_turn: u64`; `Context::check_interrupt` stops when the word is nonzero and equals `interrupt_turn` or `u64::MAX`; `check_runtime_limits` calls it first |
| `src/vm/opcode/iteration/loop_ops.rs` | `IncrementLoopIteration` (every loop back-edge) calls `check_interrupt` |
| `src/error/mod.rs` | `RuntimeLimitError::Interrupted` |
| `src/context/mod.rs` | `Context::set_interrupt_word`, `Context::set_interrupt_turn` |

`check_runtime_limits` runs on every ordinary and native function call
(`builtins/function/mod.rs`, `native_function/mod.rs`), and
`IncrementLoopIteration` on every back-edge of `for`, `for-in`, `for-of`,
`while` and `do-while`. Together they cover every loop and every call, including
the callbacks `forEach` and `reduce` make. The cost is one relaxed atomic load
per call and per back-edge, and nothing when no word is set.

The error is a `RuntimeLimitError`, so `JsError::is_catchable` is false: no
`catch` or `finally` runs and the VM unwinds to the Rust caller
(`Context::handle_error`), leaving the context usable.

A stop inside an `async` function after its first `await` reaches `run_jobs`'
caller too: see "No panic" below (the `Await` reactions).

**Not covered.** A single long native operation (a pathological regex,
`Array(1e8).fill()`) is not interrupted; it ends on its own.

**Tests.** `crates/rustkit-js/tests/vm_interrupt.rs`: a pure spin stopped from
another thread, `try`/`catch`/`finally` around it, nested calls and promise
jobs, the context usable afterwards, a stale turn id, `u64::MAX`, and a race
that sets and clears the word under load. They fail to compile without the
patch (`set_interrupt_word` does not exist upstream).

## Patch, continued: no panic where Boa assumed a call cannot fail

**Why.** The interrupt makes calls throw that Boa treats as unable to throw:
calling a promise's default `resolve`/`reject` function, constructing
`%Promise%` for an internal capability, an async module body before its first
`await`. Each is a native call, and `check_runtime_limits` (above) runs first.
Boa answered with `.expect(...)` or `assert!`, so a stop at the wrong moment
panicked the process: PR #706 found `import()` of a missing module, finished in
a stopped turn, panicking at `vm/opcode/call/mod.rs` ("default `reject`
function cannot throw: RuntimeLimit(Interrupted)"). The recursion and stack
limits reach the same calls. A stopped page must recover, never crash.

**Rule.** The error is returned (`?`), so the interrupt ends the job or turn
as it does everywhere else. Where the operation runs no script at all, the
fallible call is replaced by one that cannot fail. Every change is marked
`HIWAVE PATCH (interrupt)`.

| file | site | change |
|---|---|---|
| `src/vm/opcode/call/mod.rs` | `load_dyn_import` (dynamic `import()`): reject on a failed load, the two phase rejections, `onRejected`, reject on a failed link, `resolve` with the namespace (6 × `.expect`) | `?`: the job or reaction ends with the error |
| `src/vm/opcode/call/mod.rs` | `ImportCall`: `NewPromiseCapability(%Promise%)` (`.expect`) | `PromiseCapability::new_intrinsic` |
| `src/builtins/promise/mod.rs` | new `PromiseCapability::new_intrinsic`, `Promise::fulfilled` | build a `%Promise%` capability, or a fulfilled promise, without running code |
| `src/builtins/promise/mod.rs` | `NewPromiseReactionJob` with no capability: `assert!(handler_result.is_ok())` | return the error. The engine's own handlers (`onRejected` above, module evaluation) fail only this way |
| `src/vm/opcode/await/mod.rs` | `Await` reactions dropped the completion of the body they resume | an uncatchable completion (the async body's own handlers take every catchable one) is returned, so the stop reaches `run_jobs`' caller |
| `src/builtins/async_generator/mod.rs` | `AsyncGenerator::resume`: `assert!(!result.is_throw_completion())` | return the error (`vm/completion_record.rs`: `is_throw_completion` is now unused, `allow(dead_code)`) |
| `src/module/mod.rs` | `Module::load`: `NewPromiseCapability` (`.expect`) | `new_intrinsic` |
| `src/module/mod.rs` | `Module::inner_load`: `resolve` once the graph is loaded (`.expect`) | returns `JsResult`. `Module::load` returns a promise, not a result, so a stop there leaves that load pending; the embedder's turn is over |
| `src/module/mod.rs` | `Module::load_link_evaluate`: `JsPromise::then` twice (`.expect`) | `PerformPromiseThen` with an intrinsic capability. Same result for these engine-made promises, without the species lookup and capability construction a stop can fail |
| `src/module/source.rs` | `SourceTextModule::inner_load` | returns `JsResult` (it loads already-loaded modules through `Module::inner_load`) |
| `src/module/source.rs` | `execute_async`: `NewPromiseCapability` and `execute(...)` ("async modules cannot directly throw", `.expect`) | `new_intrinsic`; returns `execute`'s error. A stop before a top-level `await` is that error |
| `src/module/synthetic.rs` | `SyntheticModule::load`: `JsPromise::resolve` (`.expect`) | a fulfilled promise built directly |
| `src/builtins/atomics/futex.rs` | `Atomics.waitAsync` notify: `resolve` (`.expect`) | returns the error after cancelling the timeout |
| `src/error/mod.rs` | `From<PanicError> for JsError` | `js_expect` (about 30 promise and module sites: `JsPromise::then`/`resolve`/`reject`, async generators, `Array.fromAsync`, module evaluation) wraps the error of a call it assumes cannot fail in an `EnginePanic`. A runtime-limit error passes through as itself, so the embedder sees `Interrupted`, not "engine panic" |
| `src/builtins/iterable/mod.rs` | `IteratorRecord::close` with a throw completion | an uncatchable error from `return()` wins over the completion being closed, so a `catch` cannot swallow a stop that lands in `return()` (`Array.from` with a throwing mapper did) |

Two operand-stack leaks, found by the 200-iteration race tests:

| file | change |
|---|---|
| `src/vm/mod.rs` | `handle_error`, uncatchable path: also truncate the stack to the exit-early frame, as `handle_throw` does. Its callers pop that frame without truncating, so every stopped turn left the frame's values behind. About 1,700 stops filled the stack and every later script failed with "reached the maximum stack size" |
| `src/module/source.rs` | `initialize_environment` (module link): truncate the stack when popping the link frame, as `SyntheticModule::evaluate` does. This one is not about the interrupt: every linked module leaked its link frame's values, so about 2,000 modules (dynamic imports included) broke the page the same way |

**Not changed.** `.expect` and `js_expect` calls whose operand cannot be an
error from script or a runtime limit (downcasts of promises Boa just made,
bytecode invariants) are left as upstream.

**M1's guarantees hold.** A pure spin still stops at its back-edges, a
`catch` or `finally` still cannot take the stop, and a word naming an ended
turn still stops nothing (the existing tests, unchanged except that the async
case now requires the error to reach `run_jobs`).

**Tests.** `crates/rustkit-js/tests/vm_interrupt_promises.rs`. Each case stops
a turn exactly at one site (script calls `arm()`, which sets the word, so the
engine's next call fails), then 200 times with the word set from another
thread after a varied delay. Each asserts no panic; that a failed turn failed
with `Interrupted` (not `EnginePanic`); and that the same context then runs a
new turn correctly (plain code, promise jobs, an async function, a dynamic
import), with the stopped turn's id still in the word. The cases: promise
resolve, promise reject, a failed dynamic import (PR #706's report), a
successful import's evaluation, an async function and async generator await, a
microtask drain, `finally` and iterator cleanup, a module's top-level await, and
3 × 2,500 stops plus 5,000 imports on one page (the leaks). On the unpatched
code, three of the nine tests panic in Boa (`vm/opcode/call/mod.rs:370`,
`module/mod.rs:413`, `module/source.rs:1521`), four fail their assertions
(`EnginePanic` instead of `Interrupted`, an async stop that never reaches
`run_jobs`, a `catch` that takes the stop, the stack limit after many stops),
and two (reject, microtask drain) pass and stay as guards. Taken out one at a
time, the async generator and `IteratorClose` hunks each fail their case too.

**Upgrading Boa.** Re-apply the marked lines to the new version, or drop this
directory once upstream has an equivalent hook (to be offered upstream).
