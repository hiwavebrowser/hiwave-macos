# Vendored boa_engine 0.22.0, with two patches

Source: crates.io `boa_engine` 0.22.0 (upstream `boa-dev/boa` at `337a3668`,
`core/engine`), the version `Cargo.lock` pins. The first commit that added this
directory is the crate exactly as published; every change since is marked
`HIWAVE PATCH (interrupt)` or `HIWAVE PATCH (stack)` and listed here. Wired in through `[patch.crates-io]`
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

One upstream behaviour is left as is: the `Await` reactions
(`src/vm/opcode/await/mod.rs`) drop the completion of an async body they resume,
so a stop inside an `async` function after its first `await` ends that job
without `run_jobs` returning the error. The spin still ends, no `catch` sees it,
and the jobs after it stop at their first call.

**Not covered.** A single long native operation (a pathological regex,
`Array(1e8).fill()`) is not interrupted; it ends on its own.

**Tests.** `crates/rustkit-js/tests/vm_interrupt.rs`: a pure spin stopped from
another thread, `try`/`catch`/`finally` around it, nested calls and promise
jobs, the context usable afterwards, a stale turn id, `u64::MAX`, and a race
that sets and clears the word under load. They fail to compile without the
patch (`set_interrupt_word` does not exist upstream).

## Patch: call-depth errors are catchable, and a native stack floor

**Why.** Boa's default recursion limit is 512 frames, and past it (or past the
operand-stack limit) it throws a `RuntimeLimitError`, which no `catch` sees.
x.com's React startup goes deeper than 512 and died with "reached the maximum
number of recursive calls on this execution". V8 allows on the order of 10,000
small frames and throws a catchable `RangeError: Maximum call stack size
exceeded`. Raising the limit alone is not safe: script frames live on the heap
(`Vm::frames` is a `Vec`; 10,000 plain recursive calls use under 6 KiB of
native stack), but every host call that re-enters the VM (`JsObject::call` and
`construct`: getters, setters, `map`/`forEach` callbacks, Proxy traps) runs a
nested `Context::run` on the native stack, about 1.2 KiB per level in release
and 6.5 KiB in debug. 10,000 such levels overflow an 8 MiB main thread.

**What.** About 40 lines in two source files, plus their tests:

| file | change |
|---|---|
| `src/vm/mod.rs` | `check_runtime_limits` throws `RangeError: Maximum call stack size exceeded` (catchable) for the recursion and stack-size limits, and also when the native stack pointer is below `Vm::native_stack_floor` (0 = unchecked) |
| `src/context/mod.rs` | `Context::set_native_stack_floor` |
| `src/vm/tests.rs` | the three recursion tests expect the `RangeError` |

The embedder (`crates/rustkit-js/src/stack.rs`) sets the recursion limit to
10,000, the operand stack to 2^20 values, and the floor from the running
thread's real stack bounds (`pthread_getattr_np` / `pthread_get_stackaddr_np`)
plus a reserve of a quarter of the stack (256 KiB to 1 MiB) for native work
between two calls. The interrupt stays uncatchable.

**Not covered.** Native code that recurses without making a function call is
checked by nothing new; the reserve is all it has. Known cases: the parser on
deeply nested source, `JSON.stringify` of a deeply nested array, a Proxy whose
target is another Proxy forwarding with no trap (`[[Get]]` and friends recurse
natively down the chain), `[[Get]]` up a very long prototype chain of proxies or
exotic objects, and `Array.prototype.flat` with a large depth on deeply nested
arrays.

**Tests.** `crates/rustkit-js/tests/recursion_limits.rs`: the deepest call that
returns is the limit, one deeper throws a catchable `RangeError`, `finally`
runs, getter/setter/`map`/Proxy recursion throws instead of overflowing on 1
MiB, 2 MiB and 16 MiB threads (with the floor off, the 1 MiB case aborts the
process with a stack overflow), and the Boa 4515 case rejects its promise.
Boa's own `src/vm/tests.rs` cannot run here (not a workspace member).

**Upgrading Boa.** Re-apply the marked lines to the new version, or drop this
directory once upstream has an equivalent hook (to be offered upstream).
