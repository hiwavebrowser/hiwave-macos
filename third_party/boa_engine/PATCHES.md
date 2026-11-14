# Vendored boa_engine 0.22.0 (and boa_parser, boa_ast), with HiWave's patches

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

## Patch, continued: a stop never poisons module state for later turns

**Why.** An interrupt ends the turn; later turns must not see it. R1 HOLD on
PR #709 (@ 4c92f5b6, present since M1, #699) found that a module whose
evaluation was stopped kept `Interrupted` as its `[[EvaluationError]]` and the
stopped turn's `[[TopLevelCapability]]`, whose reject the stop itself had
prevented. Every later `import()` of it (or of a module under it) never
settled, and the first one re-raised "the script was interrupted" in a later
turn, so any page script importing it waited forever. The same happened
around a top-level `await`: a module stopped in a resumed body, in its async
bookkeeping (`AsyncModuleExecutionFulfilled` returned early), or at the
reaction that resumes its body stayed `evaluating-async` for good.

**Rule.** A module the stop interrupted while it evaluated becomes
`evaluated` with an ordinary, catchable `Error: module evaluation was
interrupted`, and a top-level capability the stop kept from settling is
dropped. A later import rejects with that error (a `catch` takes it like any
other) instead of hanging; a module that finished evaluating before the stop
landed on its own `resolve` keeps its namespace. The stopped turn itself still
ends with `Interrupted`, and its own import promise stays pending, as before.

| file | change |
|---|---|
| `src/module/source.rs` | new `interrupted_module_error`, `drop_top_level_capability`, `abandon_interrupted` (an `evaluating-async` module, its cycle root and the modules waiting on it become evaluated with that error and no capability; nothing is called, so it works in the stopped turn) and `abandon_interrupted_module_body` |
| `src/module/source.rs` | `SourceTextModule::evaluate`, abrupt completion: an uncatchable one caches `interrupted_module_error` on each stack module and drops the capability, then returns the stop; a stop of the `reject` call drops the capability. Normal completion: a stop of the `resolve` call drops the capability |
| `src/module/source.rs` | `async_module_execution_fulfilled`: a stop of the module's `resolve` drops the capability and abandons the modules waiting on it; a stop in a waiting module's `execute`/`execute_async` (was an early return through `into_opaque`/`?`), or of its `resolve`, abandons it and every module not run yet |
| `src/module/source.rs` | `async_module_execution_rejected`: a stop of the `reject` call drops the capability and still marks the waiting modules evaluated with the error; the first stop is returned |
| `src/module/mod.rs` | re-exports `abandon_interrupted_module_body` to the VM |
| `src/vm/opcode/await/mod.rs` | `Await` reactions: when the resumed body is a module's top level (its frame's code block is the module's) and the stop ends it, that module is abandoned |
| `src/builtins/promise/mod.rs` | `NewPromiseReactionJob`: a reaction with no capability is the engine's own (script's `then` always has one); its handler is entered even in a stopped turn (`Vm::interrupt_exempt_once`), so the `Await` and `ExecuteAsyncModule` handlers can record the stop. Stopped at its entry instead, a module's body was never resumed and the module stayed `evaluating-async` |
| `src/native_function/mod.rs` | a native call consumes `interrupt_exempt_once`: it then makes only the recursion and stack checks |
| `src/vm/mod.rs` | `Vm::interrupt_exempt_once`; `check_runtime_limits` split into `check_interrupt` + `check_depth_limits` (unchanged checks); `Context::run` checks the interrupt on entry, so no bytecode runs in a stopped turn, even a body an engine reaction resumes: it fails as its first opcode would |
| `src/vm/opcode/call/mod.rs`, `src/object/builtins/jspromise.rs` | the other no-capability engine handlers (`import()`'s steps, `JsPromise::await_native`) call `check_interrupt` first, so they behave as before: not entered in a stopped turn (a module whose `import()` was only queued stays linked and evaluates normally later). The async generator's await-return did too; it now abandons the generator instead (last section) |

One more operand-stack leak, which the executor below exposed:

| file | change |
|---|---|
| `src/object/operations.rs`, `src/vm/mod.rs` | `JsObject::call`/`construct`: a call refused before its frame exists (the interrupt, or another runtime limit, at an ordinary function's entry) left `this`, the function and the arguments on the stack; they are now truncated away (`Stack::len`/`truncate`). A stopped turn refuses every job left in it this way, so a few hundred stopped turns filled the stack and every later script failed with "reached the maximum stack size" |

The job executor matters: rustkit-js' `HostJobExecutor` runs every queued job
of a stopped turn (each fails at its entry) and returns the first error, so
the engine reactions above run and record the stop. Boa's default
`SimpleJobExecutor` drops the whole queue at the first error; with it, a
module suspended at a top-level `await` when the turn was stopped is never
resumed. The tests use an executor with `HostJobExecutor`'s policy.

**Tests** (`crates/rustkit-js/tests/vm_interrupt_promises.rs`): the hold's
input (`leaf`/`mid`), with both modules imported in two later turns, their
namespace awaited in an `async` function whose `catch` takes the error, and
an unrelated turn after; a module stopped at its evaluation's `resolve`
(namespace kept), through `import()` and `Module::evaluate`; top-level
`await` stopped before the first `await`, in a resumed body (and the module
over it), in `AsyncModuleExecutionFulfilled` for a plain and an async
waiting module, and as the body settles; the stop moved across every job of a
module's top-level `await` (16 positions, each imported twice later); that
an engine reaction entered after the stop runs no script; and 1,000 stopped
turns each refusing 40 queued jobs (fails at the 86th without the
`JsObject::call` fix). The races now re-import every module of the previous
iteration in a later turn (each settles) and require the leftover-job drain of a later turn to succeed (it
accepted `Interrupted`, which was this defect). On the code before this patch
the hold's test and the TLA tests fail (an import never settles); without the
`interrupt_exempt_once` entry the job-position test and the TLA race fail the
same way; without the `run` entry check the no-script test fails.

**Not covered.** `Array.fromAsync` / `JsPromise::await_native`
(`object/builtins/jspromise.rs`, around `JsPromise::resolve(value)?` and
`Break(Err)`): an error there returns with the coroutine's frame still pushed
and the stacks swapped. Only reachable with Boa's `experimental` feature,
which HiWave does not enable (`Array.fromAsync` is undefined here); revisit
before enabling it. Separately, upstream's `async_module_execution_fulfilled`
passes `module`, not `m`, to `AsyncModuleExecutionRejected` when a waiting
module throws (spec: `m`), leaving `m` `evaluating-async`; that is upstream
behaviour for ordinary errors, not the interrupt's, and is left as is.

**M1's guarantees still hold**, now with one more: once the stop has landed,
no more bytecode runs in that turn (the `run` entry check; before, a later
job's body without calls or loops could still run). The stop still never
reaches a `catch` or `finally`, a pure spin still stops at its back-edges, and
a word naming an ended turn still stops nothing.

## Patch, continued: a stopped async generator settles its calls

**Why.** R1 HOLD @ `d627b635` on #709: an async generator whose body the
interrupt stopped stayed `executing`. `AsyncGenerator::resume` and both
`Await` reactions returned the stop without changing its state, so the
request being served, any queued behind it, and every later `next`,
`return` or `throw` waited forever (`inner_next` only queues while the state
is `executing`).

**What.**

| file | change |
|---|---|
| `src/builtins/async_generator/mod.rs` | new `AsyncGenerator::abandon_interrupted`: the generator becomes `completed` (its context dropped), and every queued request, the stopped one included, is rejected with an ordinary, catchable `Error: async generator was interrupted`. The reject functions only enqueue jobs and run no script, so each is called while settling (`Promise::reject_interrupted`, last section), which lets this run in the stopped turn; a depth limit there leaves that request pending, as the stopped turn's own work does. `resume` calls it on an uncatchable completion |
| `src/vm/opcode/await/mod.rs` | both `Await` reactions call it on an uncatchable completion of an async generator's body (after restoring its context, as before) |

Later calls then behave as on a finished generator: `next()` resolves
`{ done: true }`, `return(v)` resolves `{ value: v, done: true }`, `throw(e)`
rejects with `e`, and `for await` ends.

**Tests.** `an_async_generator_stopped_while_it_runs_settles_later_calls`:
a body stopped when `next()` resumes it, one stopped after an `await`, and
one stopped going on after a `yield` with requests queued behind it; in later
turns the stopped and queued requests reject with the error above and
`next`/`return`/`throw`/`for await` settle as on a finished generator. Then
200 raced turns of ten generators with twelve requests each, every request
of a turn settling in the next. Without this patch the test fails with
"`stopped2` never settled".

## Patch, continued: nothing a stopped turn suspended stays stuck

**Why.** The same task as the hold above asked for every other state the
interrupt can freeze to be checked: sync generators, async functions,
async-from-sync iterators, iterator close and `for await`. Probes (now the
tests below) found that sync generators, async-from-sync iterators and sync
`IteratorClose` already end `completed` and answer later calls. Four things
stayed pending for good:

- an **async function** stopped in a body an `await` resumed: its promise
  (so every `await` of it, every `for await` loop in it, and `then(async ...)`);
- an **async generator in `return(v)`**: stopped while awaiting `v` (the
  reaction refused, a thenable's `then` or a `constructor` getter stopped),
  it stayed `draining-queue` and queued every later call forever;
- a **promise whose job the stopped turn refused**: a reaction job's derived
  promise when its script handler (or its own `resolve`) was refused, a
  promise resolved with a thenable whose `then` job was refused, and a
  promise whose `resolve` was stopped while getting `then`; so an async
  function awaiting one of them never resumed;
- **`Promise.all`/`allSettled`/`any`/`race`** over inputs that settle in the
  stopped turn: their element functions (and `race`'s resolve) run as
  reaction jobs, which were refused.

**Rule.** The engine's own promise bookkeeping still runs in a stopped turn;
script does not. A promise the stop kept from settling is rejected with an
ordinary, catchable error (`async function was interrupted`, `async generator
was interrupted` or `promise job was interrupted`), so later turns that wait
on it go on. A generator the stop froze becomes `completed`.

| file | change |
|---|---|
| `src/native_function/mod.rs`, `src/object/mod.rs` | `NativeFunctionObject::settles_promises`, set by `FunctionObjectBuilder::settles_promises`: the engine's promise bookkeeping, which runs no script. A native call to one while `Vm::settling_promises` is set makes only the depth checks. The other builders (`builtins/builder.rs`, `object/builtins/jsfunction.rs`, `ConstructorBuilder`) set it to `false` |
| `src/vm/mod.rs` | `Vm::settling_promises` |
| `src/builtins/promise/mod.rs` | marked `settles_promises`: a promise's resolving functions (`CreateResolvingFunctions`) and the element functions of `Promise.all`, `allSettled` (both) and `any`. Each only records a value and calls a capability's resolve or reject; a capability that is not the engine's (a subclass's) is script and is still stopped at its entry |
| `src/builtins/promise/mod.rs` | new `Promise::settling` (runs a closure with `settling_promises` set) and `Promise::reject_interrupted` (calls a reject function while settling, with `Error: {message}`; the result is dropped: a subclass's reject or a depth limit leaves that promise pending) |
| `src/builtins/promise/mod.rs` | `NewPromiseReactionJob`: a handler marked `settles_promises` is called while settling; the capability's resolve/reject is called while settling; a handler the interrupt refused or stopped, and a resolve/reject it refused, reject the capability with `promise job was interrupted`, then the stop ends the job |
| `src/builtins/promise/mod.rs` | `NewPromiseResolveThenableJob`: a `then` refused or stopped rejects the promise the same way. Promise resolve function: stopped while getting `then` (the promise is already resolved, so no reject function can settle it any more), it rejects the promise directly with that error and returns the stop |
| `src/vm/opcode/await/mod.rs` | new `async_function_reject` (the reject function in a suspended async function's capability registers; `None` for an async generator and for a module's top level, which has no function object); both `Await` reactions call `Promise::reject_interrupted` with it on an uncatchable completion |
| `src/builtins/async_generator/mod.rs` | `abandon_interrupted` rejects through `Promise::reject_interrupted` (was `interrupt_exempt_once`); new `await_return_step` for both closures of `AsyncGeneratorAwaitReturn`: if the turn is stopped, or a step is, the generator is abandoned (was `check_interrupt()?`, which left it `draining-queue`); it does nothing on a generator already abandoned, instead of failing step a.'s `assert_eq!` |
| `src/builtins/async_generator/mod.rs` | `await_return`: an uncatchable `PromiseResolve` error abandons the generator, and so does a catchable one whose rejection the stop refused (step 8's `complete_step` returning the stop; R1 HOLD @ `2144eedf`: the direct `return()` path otherwise left it `draining-queue`). `drain_queue` is a wrapper that abandons it when the drain (upstream's steps, now `drain_queue_steps`) is stopped. `complete_step`: an uncatchable completion rejects the request with the ordinary error before returning the stop, and a resolve/reject the interrupt refused rejects it the same way (`settle_refused`) |

The stopped turn still ends with `Interrupted`. In it, no script runs: the
exemption covers only marked natives, every script function is still checked
at its entry, and `Context::run` checks on entry. A promise chain or combinator
whose inputs settle in the stopped turn therefore settles there; a script
handler in it is dropped, and the promise it would have settled is rejected.
One visible change: a reaction job's own `resolve` (a `then()` with no
handler, or after the handler returned) is no longer where a stop lands, so a
turn whose only remaining work is that bookkeeping can end `Ok` with the word
set; script's own calls to `resolve`/`reject` are still stopped.

**Tests** (`crates/rustkit-js/tests/vm_interrupt_promises.rs`):
- `the_holds_async_generator_stopped_mid_body_answers_later_calls`: the
  hold's input, stopped after an `await` and before the first; in two later
  turns `next()` is `{done:true}` and `return(7)` is `{value:7,done:true}`.
- `an_async_generator_stopped_in_return_or_delegation_answers_later_calls`:
  await-return refused, stopped in a thenable's `then`, stopped in a
  `constructor` getter, a `constructor` getter throwing after the stop (on a
  suspended-start and on a completed generator); `return()` into a `finally`
  that awaits; `yield*` to an
  async and to a sync generator stopped mid-body.
- `a_stopped_async_function_settles_its_promise`: after an `await`, a method,
  an arrow, nested functions, awaiting a refused job, awaiting `Promise.all`,
  `for await` over an array, an async generator and a sync generator
  (stopped in it, and a refused async-from-sync job), closing an async
  iterator by `break` with its `finally` stopped, and `then(async ...)`.
- `a_sync_generator_or_iterator_stopped_mid_body_completes`: mid-body, in a
  `finally` through `return()`, in a `yield*` inner generator, and closed by a
  `for-of` `break`.
- `promise_combinators_settle_in_a_stopped_turn`: all four, with no script
  handler run after the stop.
- `every_suspended_state_a_raced_stop_leaves_settles_later`: 200 turns that
  each make async functions, async and sync generators, `for await` loops,
  iterator closes and combinators, stopped by another thread after a delay of
  up to 40 ms and, separately, at the `tick()` numbered `K`, which steps
  through all 252 of them (80 in the script, the rest in its jobs). The next
  turn checks that every promise settles and every generator answers `next()`
  and `return()`.
- `interrupt_during_promise_resolve`: its reaction case now asserts the chain
  settles; a new executor case keeps script's own `resolve` stopped.

On the code before this patch, the async function, return/delegation and
combinator tests and the race fail ("`p` never settled"), and the resolve
case fails its new assertion; the hold's test fails without the previous
section.

**Not covered.** A promise whose capability is not the engine's (a `Promise`
subclass with its own resolve/reject) stays pending if the stop refused the
job that settles it: its reject function is script. A promise an earlier,
healthy turn holds stays pending if the work that settles it was a script
handler or `import()` step the stopped turn dropped (R1 HOLD @ `d627b635`:
`p1 = import('X')` from turn 1, X's body stopped in turn 2); later work on
the same object (a re-import, a later `next()`) settles. Upstream, with no
interrupt involved: `import('D')` while D is `evaluating-async` under another
cycle root never settles, because `SourceTextModule::evaluate`
(`source.rs`, around lines 1052-1101) does not store the new capability on D
(spec step 7); left as upstream.

**Upgrading Boa.** Re-apply the marked lines to the new version, or drop this
directory once upstream has an equivalent hook (to be offered upstream).

## Patch, continued: recursion limits a real page can use

**Why.** Boa's default recursion limit is 512, and `check_depth_limits`
added `host_call_depth` to the frame count. A setter, getter or other host
re-entry both increments `host_call_depth` (around `Context::run`) and
pushes a frame, so it counted twice and a Lit-style setter chain died near
256 calls. Reddit's shreddit bundles (custom elements plus a fetch-wrapping
canary) tripped `RuntimeLimitError::Recursion` inside the load turn's
microtask drain, with budget left (`limit=Recursion`, about 6s of a 15s
load turn).

The frame counter does not leak across an uncatchable stop, an interrupt or
the grace turn: `call` decrements `host_call_depth` and pops the frame on
the error path too. `HostJobExecutor::run_jobs` did re-enter. A host call
during a job that asked for another checkpoint drained the new jobs on top
of the outer job's frames. HTML says a microtask checkpoint does not nest.

**What.** Marked `HIWAVE PATCH (recursion)`.

| file | change |
|---|---|
| `src/vm/runtime_limits.rs` | frame limit 8192, host limit 1024, stack slots `64 * 1024` (the old 10240 would turn a deep call into `StackSize`) |
| `src/vm/mod.rs` | the two limits are checked separately (`limit <= frames`, `host_limit <= host_call_depth`); every check samples the depth onto the thread, and `max_frame_depth` is the per-script high-water mark |
| `src/context/mod.rs` | `Context::reset_recursion_max` |

`recursion_sample()` is what the interrupt log reads: the hook has no
`Context`. The host cap stays lower than the frame cap because each
re-entry nests `Context::run` on the native stack.

The job executor (`crates/rustkit-js/src/executor.rs`, not this crate)
returns immediately when `run_jobs` is already running. The outer loop
takes the jobs the nested call queued, after the current job returns.

**Tests.** `uncatchable_stops_do_not_leak_recursion_depth` (thirty
recursion stops, twenty setter stops, thirty interrupt stops, then a
recursion that does not fit if a stop kept a frame or a host count),
`a_nested_microtask_checkpoint_does_not_reenter`, and
`a_deep_setter_chain_and_call_fit_the_default_recursion_limit` (400
setters, a call of 1000). The existing `recursion_runtime_limit` and
setter tests still fail through the frame check at the limit they set.

## Host-stack names

`host_call_depth` counted re-entries and did not say which function was
entered. While `set_host_stack_armed` is set, `call` and `construct` push
that function's name. The first time a climb crosses 256, 512, and 1000,
`host_stack` calls the installed hook with a histogram and the innermost
20 names. The console log arms it. A disarmed process does not allocate
the names.

**What.** Marked `HIWAVE PATCH (host stack)`.

| file | change |
|---|---|
| `src/vm/host_stack.rs` | the stack, the thresholds, the hook |
| `src/vm/mod.rs` | `host_frames`, `host_stack_logged` |
| `src/object/operations.rs` | `call` and `construct` push and pop around `Context::run` |

## Patch, continued: a stop runs the `finally` blocks it unwinds through

**Why.** A stop never reached a `catch` or a `finally` (M1, above). A
scheduler that posts its next task from a `finally` (React's
`performWorkUntilDeadline`: `try { flushWork() } finally { if (more)
post(); else running = false; }`) was then left with its "running" flag
set once the watchdog stopped one task: it never posted again, and the page
ran no more timers or requests (2026-10-22). The stop must still end the
script; the page's cleanup needs a moment.

**Rule.** With `Context::set_stop_finally_grace(Some(grace))` (off by
default; `rustkit-js` sets the 50 ms stop grace), a stop by the interrupt
word or the loop limit (`rustkit-js` reports its host-call deadline as the
loop limit) that unwinds through a `try` with a `finally` runs that
`finally` block, innermost first, in the stopped frame and its callers up to
the frame the Rust caller entered. No `catch` runs. Each block ends by
rethrowing the stop, so the next one out runs, and then the stop reaches
the Rust caller as before. For the grace, counted from the first block, the
word naming the stopped turn does not stop script; past it, the stop is
final as upstream's, and so is `u64::MAX` (Stop), which runs no `finally`
at all. The block cannot end the stop: a `return` in it, a throw out of it
(caught around the `try` or by a caller), and a `break` or `continue` out of
it each go on with the stop instead. A `catch` inside the block, or in a
function it calls, is the page's own. A body the run entry check stops
before it starts (a job resumed in a stopped turn) runs no `finally`.
Marked `HIWAVE PATCH (stop finally)`.

| file | change |
|---|---|
| `src/vm/code_block.rs` | `Handler::stop_finally`: where the `finally` of the `try` the handler belongs to is (`StopFinally`: body from `entry` to `exit`, rethrow up to `exit_end`) |
| `src/bytecompiler/jump_control.rs`, `src/bytecompiler/statement/try.rs` | the try block's handler (and a `catch` block's, with a `finally`) records it; the engine's own handlers record none |
| `src/vm/mod.rs` | `Vm::stop_finally_grace`, `Vm::stop_unwind` (`StopUnwind`: the stop, its grace's end, the blocks running); `handle_error` enters the innermost `finally` around the stop (`enter_stop_finally`) and claims a catchable throw that would leave the running block (`stop_claims_throw`); `handle_return` goes on with the stop from the block's frame; `check_interrupt` lets the block run through the grace and stops code that left it by `break`; the run entry check enters no `finally`; the state ends when a stop leaves the outermost frame |
| `src/vm/opcode/control_flow/throw.rs` | `Throw` and `ReThrow` at the end of the running block, or out of it, go on with the stop (`resume_stop`) |
| `src/context/mod.rs` | `Context::set_stop_finally_grace`, `Context::stop_finally_running` (the host answers the block's calls), `Context::end_stop_unwind` |

**Not covered.** A `yield` or `await` in the block suspends it as usual; the
grace's end stops what runs after it in the turn. If the script the Rust
caller started then ends normally (`handle_return`/`handle_yield` at the
outermost frame), the stop's state is cleared, so the next script is not
stopped by what is left of the grace.

**Tests.** `crates/rustkit-js/tests/vm_stop_finally.rs` (innermost first and
no `catch`; a `finally` that schedules the next task; Stop runs none; no
grace, none; `return`, a throw caught around it, and `break` do not end the
stop; a `catch` inside the block is the page's; a block that spins is
stopped at the grace and the next turn runs; the loop limit; a generator and
an async function), `watchdog_turns::try_catch_around_the_spin_cannot_swallow_the_stop`
(now: the `finally` runs once, the `catch` never), and in `rustkit-engine`
`live_timer_slice_tests` (a MessageChannel scheduler that re-arms in
`finally` runs its next task after a stop; a spinning `finally` is stopped).

## Patch, continued: sort with a comparator that is not a total order (JS2)

**Why.** `SortIndexedProperties` passed the page's comparator to `Vec::sort_by`,
which since Rust 1.81 panics ("user-provided comparison function does not
correctly implement a total order") once it notices an inconsistent answer. A
page's comparator can be anything (`Math.random() - 0.5`, a boolean `a > b`);
ECMA-262 makes the result implementation-defined, not fatal. walmart.com
panicked the page thread on 17 of 17 loads.

**What.** `src/builtins/array/mod.rs`: `sort_indexed_properties` calls
`merge_sort_fallible`, a stable merge sort that never panics for any comparator
and stops at the first abrupt completion (marked `HIWAVE PATCH (sort)`; replaced by the V8 port below). It
serves `Array.prototype.sort`/`toSorted` and the typed-array `sort`/`toSorted`.
Pinned in `crates/rustkit-engine/src/sort_comparator_tests.rs`.

## Patch: `boa_parser` binding list across a newline (STR-5)

`third_party/boa_parser` is crates.io `boa_parser` 0.22.0 exactly as published (first commit), wired
through `[patch.crates-io]`. One change, marked `HIWAVE PATCH (STR-5)` in
`src/parser/statement/declaration/lexical.rs`: after a `const`/`let` binding the list continues on a
comma even when a line terminator precedes it. Upstream asked `peek_semicolon` first, which answers
"found" for any token after a newline, so `const a = 1` then a line starting `, b = 2` threw
`unexpected token ','` (Speedometer 3's Lit bundles are written that way). Pin:
`crates/rustkit-js/tests/const_comma_newline.rs`.
The vendored crate's own parser tests are not run by the workspace, and `cargo test --manifest-path`
on it fails from inside the tree ("believes it's in a workspace"). To run them, copy the directory out
of the repository and run `cargo +<the version in rust-toolchain.toml> test --lib` there (312 pass,
1 ignored, with the STR-5 patch).

## Patch: a native-stack budget for built-ins that recurse in Rust (native recursion)

A page could kill the process: `var a=[]; for (...) a=[a]; JSON.stringify(a)` recursed in Rust until the
real stack overflowed (so did `flat(Infinity)`, `join`/`toString`/`==` through nested arrays, Proxy and
prototype chains, deeply nested source in the parser). Chrome throws a catchable `RangeError`.

`src/native_stack.rs`: the stack address where the outermost engine entry on a thread began
(`Context::run`, `JsObject::call`, `Script::parse`, `Module::parse`), and `check(budget)`, which throws `RangeError:
Maximum call stack size exceeded` once more than the budget has been used since. Checked in
`CallValue::resolve` (every call, native or JS), `[[Get]]`/`[[Set]]`/`[[HasProperty]]`/`try_get` (Proxy
and prototype chains), `JSON.stringify`'s `serialize_json_property` and `flatten_into_array`.
`RuntimeLimits::set_native_stack_budget` (default 1 MiB, which fits any 2 MiB thread); `rustkit-js`'s
`set_thread_native_stack_budget` sets it from the stack size a thread was spawned with (the content
thread and `spawn_script_thread` use half of it). A thread that never sets it gets 1 MiB: in a debug
build the parser needs several KiB per nesting level, so roughly 100 levels of `[[[...]]]` fit.

`third_party/boa_parser` (marked `HIWAVE PATCH (native recursion)`): `Cursor::check_depth` at
`AssignmentExpression`, `UnaryExpression`, `Statement` and `StatementListItem`, using the engine's probe
(`set_stack_probe`) or the parse's own start when there is none; and `MAX_CHAIN` (8192) on the loops that
build a left-nested chain (`1+1+...`, `a.b.c...`, `f()()()...`), because the finished tree is dropped,
analysed and compiled recursively and a 30 000-link chain overflowed a 2 MiB thread. The error is
`Error::is_stack_overflow`; `From<ParseError> for JsNativeError` and the `Function` constructor turn it into
a `RangeError`.

Pin: `crates/rustkit-js/tests/native_recursion.rs` (each case in a child process, since an overflow
aborts; 40 cases, expected values from Chrome for Testing 148.0.7778.216). Where Chrome answers and this
engine throws a RangeError instead, or where `JSON.parse` keeps its old SyntaxError ceiling, the test names
the case and the answer it accepts.

Follow-up (#1099 R1 MED): `Module::parse` is an entry too, and the probe answers `None` (no origin) on a
thread with no outermost entry, so such a parse is measured from its own start. The probe is process-wide
and was installed by the first entry and never cleared; it answered "not over" without an entry, so a parse
outside one (a module parse) was never measured and a deeply nested module source overflowed the thread.
Pin: `crates/rustkit-js/tests/module_parse_depth.rs` (5 nestings at 200 000 levels and a shallow control,
Chrome for Testing 148.0.7778.216: `RangeError`, and the page runs on).

## Patch: a catchable stack overflow (STR-10)

Opt in with `RuntimeLimits::set_catchable_stack_overflow(true)` (off by default, so upstream's tests
keep their uncatchable `RuntimeLimitError::Recursion`; the page runtime, `JsRuntime::new`, turns it
on). Running out of call frames or operand-stack slots then throws a `RangeError` ("Maximum call
stack size exceeded") that `try`/`catch` sees. Host re-entry (`host_recursion`) stays uncatchable.
Second change, in `handle_throw` (always on): when a caller's `catch` takes an exception from a
callee, the unwound callee's stack values are dropped, as its return would; they stayed until the
caller returned, so a caught overflow left ~64K slots behind and the next call hit the slot limit.
Pin: `crates/rustkit-js/tests/stack_overflow_catchable.rs`. The vendored crate's own `vm::tests`
are not in the workspace and were not run here.

## Patch: `boa_ast` `this` through nested arrow functions (arrow this)

`third_party/boa_ast` is crates.io `boa_ast` 0.22.0 exactly as published (first commit of its PR), wired
through `[patch.crates-io]`. Two small changes, marked `HIWAVE PATCH (arrow this)`: `Scope` gets an `arrow`
flag (`scope.rs`, set from `visit_function_like` in `scope_analyzer.rs`), and
`escape_this_in_enclosing_function_scope` keeps climbing past an arrow's scope instead of stopping at the
first enclosing function scope. Upstream stopped at the outer *arrow*, so in
`m() { return () => () => this.name; }` the method was never told its `this` is used, kept it out of its
function environment, and the inner arrow read the wrong one (strict and sloppy; breaks TipTap). It only
showed when the outer arrow did not use `this` itself. Pin: `crates/rustkit-js/tests/arrow_this_capture.rs`
(35 function bodies, expected values from Chrome for Testing 148.0.7778.216). The vendored crate's own
unit tests run on a copy outside the tree (`cargo +<rust-toolchain.toml version> test --lib`): 38 pass.

## Patch: interpreter statistics (`vm-stats`, JS-PERF2 item 4)

**Why.** To say where the interpreter's property lookups go slow on a real page (CodeMirror's first
parse) without guessing: which opcodes run how often, and which property accesses miss their inline
cache, on what kind of receiver, for which name.

**What.** `src/vm/stats.rs` (new). Off unless `RUSTKIT_VM_STATS=1` is in the environment (read once
per process): `Context::run` picks `run_impl::<true>`, a copy of the loop that counts every opcode,
only then, and the miss hooks in `get_by_name`, `set_by_name` and `GetNameGlobal` (all on the path
that already missed) are one branch on that flag. `rustkit_js::vm_stats_report()` returns the text:
the top 40 opcodes with counts and shares, the misses per site split into cacheable (cold, or the
shape changed) and not cacheable (misses every time, e.g. a store that adds a property), and the top
40 (site, receiver kind, name) misses. Hits are the opcode's count minus its misses.
`RUSTKIT_VM_STATS_FILE=<path>` also rewrites the report to `<path>.t<thread number>` every 1M opcodes and when the
thread ends, for a probe that exits without calling the function. Pin:
`crates/rustkit-js/tests/vm_stats.rs` (red first, against a stub report). Privacy: the report carries shapes, not data; property names are recorded only when identifier-like
and at most 48 characters (anything else is one bucket), and computed keys are never recorded.
Cost with the flag off,
measured (release, Linux, min of 11, two rounds each way): within +/-1.5% of develop on property
reads, a script call and arithmetic loops.
## Patch: whole-number double keys without a string (`js-perf1`)

**Why.** `obj[n]` with `n` a double (`Math.floor(x / 2)`, `i + 0.5 - 0.5`, a parser's offset
arithmetic) converted the number to a string and parsed it back to an array index
(`ToPropertyKey`'s Float64 arm: `JsString::from(float).into()`), an allocation and a parse per
read. CodeMirror's first parse spent about 7% of 3,404 engine-thread samples there.

**What.** `PropertyKey::from_f64_key` in `src/property/mod.rs`, used by `Value::to_property_key`
(`src/value/mod.rs`, marked `HIWAVE PATCH (js-perf1)`): a whole number from 0 to 2^32 - 2 (`-0`
included, whose string is "0") is that index; every other Number keeps `Number::toString`'s
string (`NaN`, `1.5`, `-1`, `4294967295`, `2^32`, `1e+21`, `Infinity`). Pins:
`crates/rustkit-js/tests/float_index_keys.rs` (correctness for those cases, which passed before
the change too) and `float_index_keys_allocs.rs` (allocations per read: a float key 1.14 before,
0.14 after, an integer key 0.12; red before). Measured (release, Linux): 2M reads of `a[x]` with a
double `x`, 435 ms to 291 ms; an integer key is unchanged at 224 ms.

## Patch: `a[i]` and `a[i] = v` without the general path (`js-perf2`, item 2)

**Why.** `get_by_value` cloned the key and the base, then converted the key, and only then reached
its dense-array path; typed arrays and strings read through `__get__`; `SetPropertyByValue` cloned
four registers before its dense-array store, and typed arrays had no shortcut at all. A parser's
inner loops are mostly `table[i]` on a typed array or array.

**What.** Marked `HIWAVE PATCH (js-perf2)`: `fast_index` and `fast_get_by_index` in
`src/vm/opcode/get/property.rs` take a whole-number key (int32 or whole double, `0..=2^32-2`) on an
array's dense element, a typed array's element or a string's code unit straight off the registers;
`SetPropertyByValue` in `src/vm/opcode/set/property.rs` does the same for an extensible array's
dense element and for a typed array when the receiver is the base itself. Anything else (holes,
sparse arrays, accessors, a proxy, `arguments`, another receiver) takes the general path as before.
`typed_array_get_element` and `typed_array_set_element` are `pub(crate)` for it. Pins:
`crates/rustkit-js/tests/index_fast_paths.rs` (30 cases from Chrome 148.0.7778.216 in
`tools/parity_oracle/index_fast_path_cases.json`, which pass before the change too, and a speed pin
that does not). Measured (release, Linux, cost of the operation relative to an empty loop
iteration, median of 7): array read 0.87 to 0.67, array write 0.77 to 0.50, Uint16Array read 1.38
to 1.01, Uint16Array write 1.86 to 0.93.

`js-perf2` follow-up (R1 on #1112): an index past the end of a string primitive is answered by
`String.prototype` (`String.prototype[5] = 'P'; 'abc'[5]` is 'P'), and a dense-array store only
happens when the receiver is the array itself (`super[i] = v` on an array home object and
`Reflect.set(arr, i, v, other)` define the element on the receiver). Both holes were in the general
path before this patch too (its string shortcut answered `undefined` past the end, its array store
ignored the receiver); fixed in both paths, with the five cases added to the Chrome-148 set.

## Patch: global lexical scope across scripts

**Why.** ES2024 GlobalDeclarationInstantiation: every classic script of a realm shares one global
declarative environment, so a function compiled in an earlier script sees a top-level
`const`/`let`/`class` a LATER script declares. Boa resolves a name no enclosing scope declares to the
global OBJECT at compile time, so the earlier function threw `ReferenceError` for it forever
(Speedometer 3's Perf-Dashboard: a function from one `<script>` reads `RemoteAPI`, a class declared by
the next).

**What.** Marked `HIWAVE PATCH (global-lexical-scope)`:
`src/environments/runtime/mod.rs` (`find_runtime_binding` re-points a global-object locator at the
realm's lexical binding of that name, in `resolve_late_global_lexical`), `src/realm.rs`
(`late_global_lexical`, a per-realm name cache that is dropped when the global scope gains bindings, so
a read of `Math` or `document` costs one hash lookup), and `src/vm/opcode/set/name.rs`
(`verify_initialized` throws TypeError when such a late-resolved binding is a `const`, which the
compiler could not know). Pins: `crates/rustkit-js/tests/global_lexical_scope.rs` (runtime) and
`crates/rustkit-engine/src/script_global_lexical_tests.rs` (page scripts). Cost: one cached hash lookup
per global-object name read once any script has a top-level lexical declaration; a 1M-iteration loop
of three global reads with 300 top-level consts went 1.41 s to 1.6 s (release, Linux).

`GetNameOrUndefined` (`src/vm/opcode/get/name.rs`) takes its "undeclared global, so `undefined`" decision
after the locator has been re-pointed, so `typeof` of a later script's binding before its declaration
has run is a ReferenceError as in Chrome, not `"undefined"` (#1101 follow-up).

## Patch: sort in V8's order (`sort-v8`)

**Why.** With a comparator that is not a consistent total order (`(a, b) => a > b`, a NaN or
`undefined` result, `-1` always, `Math.random() - 0.5`) ECMA-262 leaves the order unspecified, and
pages see V8's. observable-plot keeps a different top-N when ours differs. The merge sort above never
panics but orders such input differently, and calls the comparator in a different sequence.

**What.** `src/builtins/array/v8_timsort.rs` (new) ports V8's `array-sort.tq` (TimSort: run
detection, binary insertion under the minimum run, galloping merges) and its
`typed-array-sort.tq` top-down merge sort. `sort_indexed_properties` partitions off the `undefined`s
(V8 never passes one to the comparator) and puts them last. Chrome 148.0.7778.216 measured two
facts the Torque source does not show: `Array.prototype.sort` on fewer than 8 elements goes straight
to binary insertion, and `toSorted` does not. Also fixed in the typed-array comparator: a result of
`0`, `-0` or `false` was taken as "greater" (`is_sign_positive`). `merge_sort_fallible` is gone.
Pins: `crates/rustkit-js/tests/sort_v8_order.rs` compares 180 cases (sizes 2 to 500, ten comparator
shapes, plus typed arrays) with Chrome's sorted result AND its comparator call log, call for call
(`tools/parity_oracle/sort_nan_cases.json`, regenerated by `sort_cases_log.mjs --write`), and fuzzes
stability and totality with a seeded generator. Not pinned: comparators with side effects other
than logging, arrays over ~500 elements, and proxies or array-likes with getters (the algorithm does
not depend on them, but they were not measured).

## Patch: a left operand is read before the right one can assign (`eval-order`)

**Why.** Boa's `compile_binary` used a local variable's own register as the left operand of
`lhs op rhs` (no copy into a temporary), so the operator read it AFTER the right operand had run.
`a + --a` and `a < (a = 5)` used the new value; ECMA-262 reads the left operand first. The same in the
fused comparison branch of a loop or `if` condition (`while (a < (a = a + 2))`). Chrome 148 gives the
old value in all of them.

**What.** `ByteCompiler::compile_expr_operand_before(lhs, rhs, ...)` (`src/bytecompiler/mod.rs`,
marked `HIWAVE PATCH (eval-order)`): the register is used directly only when `rhs` contains no
assignment or update expression (`may_assign`, a conservative AST visitor); otherwise the left operand
is copied into a temporary first. Used by `compile_binary` (arithmetic, bitwise, relational) and
`try_fused_comparison_branch`. Pins: `crates/rustkit-js/tests/eval_order.rs` (800 generated programs,
3 of which differ from Chrome without the fix, and 34 hand-written cases, 3 of which differ, all from
Chrome 148.0.7778.216).

## Patch: assigning a `let` or `const` in its temporal dead zone (`tdz-assign`)

**Why.** `z = 1` compiled before `let z` in the same function (or block) succeeded silently: the
compiler's `access_set` called `insert_binding`, which gave the not-yet-declared local a fresh
register of its own, and the declaration later took another. Chrome 148 throws ReferenceError, after
evaluating the right side, for a `const` too. A read, a compound assignment and `++` already threw
(they read first).

**What.** `access_set` in `src/bytecompiler/mod.rs` (marked `HIWAVE PATCH (tdz-assign)`): a lexical
local with no register yet is in its dead zone at that point of the source; the assignment emits the
same "uninitialized binding" ReferenceError a read of it does, before the constness check. Pin:
`crates/rustkit-js/tests/tdz_assign.rs` (10 cases from Chrome 148.0.7778.216 in
`tools/parity_oracle/tdz_assign_cases.json`; 6 differed before). Not covered: a `let` declared in one
`switch` case and assigned in a later one (compiled after the declaration, so no error; unchanged).

## Patch: cyclic ToPrimitive and Array join (cyclic to-primitive)

`JsObject::ordinary_to_primitive` carried upstream's `RecursionLimiter`: an object already being
converted answered the hint's default (`0` / `""`). So a user `toString() { return String(o); }` returned
`""` after one level where Chrome recurses to a `RangeError`. The limiter is removed. What it was
protecting, cyclic arrays, is V8's join stack now: `Array.prototype.join` and `toLocaleString` keep a
thread-local list of the arrays they are running on, and one reached again contributes `""` (`a=[1];
a[1]=a; String(a)` is `"1,"`). `join` no longer treats "the receiver itself as an element" as empty (a
plain object that is its own element prints `[object Object]`, as in Chrome). `toLocaleString` joins
with `","` as V8 does, not ICU's unit-list `", "`. Pin: `crates/rustkit-js/tests/cyclic_to_primitive.rs`
(19 cases, Chrome for Testing 148.0.7778.216). A runaway user recursion ends in the host re-entry
cap's uncatchable `RuntimeLimitError`, not Chrome's catchable `RangeError`: the test names that.

## Patch: fewer register copies in the bytecode compiler (`js-perf2`, item 6)

**Why.** The compiler copied a local variable into a fresh temporary (`Move`) before almost every
use as an operand, updated a `var` local through three copies, and wrote `x = rhs`, `var x = init`
and `x op= rhs` through a temporary and a copy back. On CodeMirror's first parse `Move`,
`PushFromRegister` and `This` were about 32% of the opcodes executed (the `vm-stats` counters).

**What.** Marked `HIWAVE PATCH (js-perf2 item 6)` in `src/bytecompiler/mod.rs` and
`expression/update.rs`, each only where nothing evaluated in between can assign the local
(`may_assign`, shared with `eval-order`):
- member access reads the object and key from a local's own register (`access_get`,
  `emit_get_property_by_name_from`);
- `x++`/`x--` on a mutable non-lexical local (`var`, a parameter) is one in-place `Inc`/`Dec`;
- a statement `x op= rhs` on a mutable register local is one instruction on its register
  (`compile_local_compound_assign`);
- a statement `x = rhs` and `var x = init` compile straight into the local's register when the right
  side writes its destination only with its last instruction (`writes_dst_last`: literals,
  identifiers, arithmetic/bitwise/relational operators, simple member reads, calls, `new`);
- a statement `obj.p = v` / `obj[k] = v` reads object, key and value from locals directly
  (`compile_member_assign_direct`).
Pins: `crates/rustkit-js/tests/compiled_moves.rs` (opcodes executed by a parser-shaped loop, from
the `vm-stats` counters: `Move`s 1460 -> 7, all opcodes 3790 -> 2337) and `eval_order.rs` (1,200
generated programs and 56 hand cases from Chrome 148.0.7778.216).
## Patch: `++`/`--` convert their operand, and a throwing conversion keeps it (`update-conversion`)

**Why.** Two bugs found by an independent R1 on #1149, confirmed against Chrome 148. (1) The in-place
fast path for a register local emitted `Move(dst, local); Inc(local, local)` for a postfix used as a
value, so the value was the old value UNconverted: `let s = '5'; s++` gave `'5'` (Chrome 5),
`let u; u++` gave `undefined` (NaN), `n--` on `'5'` then `+ 1` gave `'51'` (6). (2) `Inc`/`Dec` take
their source register before ToNumeric and did not put it back when the conversion threw, so
`x++` with a throwing `valueOf` (or a Symbol) left `x` as `undefined`.

**What.** `expression/update.rs` (marked `HIWAVE PATCH (update-conversion)`): the postfix value form is
`Move(dst, local); Inc(local, dst)`, so `Inc` writes the converted old value to dst and the new value
to the local, which is untouched if the conversion throws. `vm/opcode/unary_ops/increment.rs` and
`decrement.rs`: on a conversion error the taken value is put back before the error propagates. Pin:
`crates/rustkit-js/tests/update_conversion.rs` (29 cases from Chrome 148.0.7778.216 for `let`, `var`,
a parameter, a global and a member; 12 differed before).
## Patch: a write to an array's `length` is never inline-cached (`array-length-ic`)

**Why.** Unmodified Boa 0.22 bug, reported by Atlas from engine-side JS on develop e8ed58da (a
request queue emptied with `queue.length = 0` lost elements after the second reset). An array's
`length` is an ordinary shape slot, so the first `a.length = n` at a site found it and
`SetPropertyByName` cached the slot; every later write from that site stored `n` straight into the
slot and skipped ArraySetLength. So nothing was truncated (old elements stayed readable: `q[0]` after
`q.length = 0`), later `q[i] = v` writes did not grow `length`, and `q.length = '2'` stored a string.

**What.** `builtins/array/mod.rs` `array_exotic_define_own_property` (marked
`HIWAVE PATCH (array-length-ic)`): after ArraySetLength, the slot is marked `NOT_CACHEABLE`, so a
`length` write always takes the exotic path. Reads of `length` are still cached. Pin:
`crates/rustkit-js/tests/array_length_reset.rs` (30 cases from Chrome 148.0.7778.216: push, shift,
pop, unshift, index writes, holes, closures, a queue pump, string and growing lengths, a frozen
array; 6 differed before).

## Patch: a dead zone for `let`/`const`/`class` declared in a `switch` case (`switch-tdz`)

**Why.** A binding declared in one `switch` case is reached by jumps that skip its declaration
(`case 0: let z = 1; break; case 1: z`). Boa kept it in a register, and registers have no run-time
dead zone: compilation follows source order, so the later case's access compiled as initialised and
read whatever the reused register last held (another local's value, the switch value), where Chrome
throws ReferenceError. A non-local `const` there had the same hole through `const_binding_cache`.

**What.** In vendored `boa_ast` (`src/scope_analyzer.rs`, `visit_switch_mut` of the escape analysis,
marked `HIWAVE PATCH (switch-tdz)`): a switch scope's bindings always escape, so they live in a
declarative environment (as they already did under direct `eval`), whose uninitialised binding throws.
In `src/bytecompiler/mod.rs` and `statement/switch.rs`: a `const` of a switch scope is not cached in a
register (`switch_scope_ids`). The cost is limited to `switch` statements that declare a lexical
binding in a case. Pin: `crates/rustkit-js/tests/switch_tdz.rs` (14 cases from Chrome 148.0.7778.216,
including fallthrough, same-case access, hoisted functions, closures and a fresh binding per loop
iteration; 9 differed before).

## Patch: ArrayBuffer transfer without `experimental` (arraybuffer transfer)

`ArrayBuffer.prototype.transfer`, `transferToFixedLength` and the `detached` getter were behind Boa's
`experimental` feature, so no page saw them. The four `#[cfg(feature = "experimental")]` gates in
`src/builtins/array_buffer/mod.rs` are removed (the rest of `experimental` stays off). A structured-clone
transfer detaches the sender's buffer through them. Pin: `crates/rustkit-js/tests/arraybuffer_transfer.rs`
(18 cases, Chrome for Testing 148.0.7778.216).

**Follow-up (R1 on #1157).** The write cache could still be filled by an ordinary object that has an
array's shape (an Array subclass instance is shaped like `Object.create(Q.prototype)` given a writable
own `length`), and a later hit wrote a real array's `length` directly. `vm/opcode/set/property.rs`
`set_by_name`: a cache hit writes the slot only when the object's [[DefineOwnProperty]] is the ordinary
one, or the object is an array and the name is not `length` (so other named writes on arrays stay
cached). `object/operations.rs` `length_of_array_like`: an array's length is read and the borrow
released before it is converted. With the cache guard in place every array's `length` slot holds a
number, so this second change has no failing case of its own; it is defensive. Pin:
`crates/rustkit-js/tests/array_length_shapes.rs` (15 cases from Chrome 148.0.7778.216: subclass,
`Object.create([])`, `Object.create(Array.prototype)`, `++`, `+=`, sloppy frozen and sealed,
non-writable `length` sloppy and strict, a non-configurable element, `length` given as an object;
2 differed at #1157's head).
