# JS-PERF2: where Boa's interpreter cost can be cut without a rewrite

Plan only, no code. Author: Talos. Date: 2026-11-13. Base: develop 040cac4f; JS-PERF1 (PR #1107) is item 1.

## The profile this answers
CodeMirror's Highlight step, first full parse: ~190 ms here against 17.7 ms in Chrome 148 (~11x). 3,404 engine-thread samples, parity build, clock held (from Atlas, Mac): interpreter dispatch ~25%, value drops and refcounting ~17% (drop of `JsValue` alone 12%), property lookup ~12%, number keys turned into strings ~7% (JS-PERF1, done), malloc/free ~4%, GC ~1%, our bindings ~0%.

**What I could not do:** profile it myself. `perf` is installed on my Linux box but `kernel.perf_event_paranoid=4` blocks it, and changing that needs root; I have not. So every gain below is an **estimate from the bucket sizes and from reading the code**, not a measurement, and each item starts with a local reproduction (an allocation count or a micro-benchmark, as JS-PERF1 did) before any code.

## Read this first: the ceiling
Chrome runs this parse in V8's JIT tiers; Boa is a bytecode interpreter, and a plain interpreter is typically several times slower than a JIT on a tight parser loop. Stacking every item below is worth roughly **10 to 20% of the current 190 ms** (to ~150 to 170 ms), by my estimate. That is real, and it is not 11x. Closing most of the gap would need a different execution tier (a baseline JIT or a much faster interpreter design), which is not a row-sized change; I am naming it as the decision for Pete rather than hiding it inside a list of 3% wins.

## Items, in the order I would do them
| # | Item | Where | Bucket it draws on | Estimate | Risk | First measurement (local) |
|---|---|---|---|---|---|---|
| 1 | Whole-number double keys without a string | `property/mod.rs`, `value/mod.rs` | number keys ~7% | DONE (#1107): float-key reads -33% (435 to 291 ms per 2M, Linux), integer keys unchanged; CodeMirror first parse 213.3 to 188.5 ms median on the Mac (-11.6%, Atlas, 10 runs each) | low | allocation pin, in the PR |
| 2 | `obj[i]` / `obj[i] = v` fast path before any clone: dense array, typed array, string, with `Integer32` or whole `Float64` keys | `vm/opcode/get/property.rs` (`get_by_value` clones the key and the base, then calls `to_property_key`, and only then reaches its dense-array path), `set/property.rs` | drops/refcount ~17%, property lookup ~12% | 3 to 6% overall on an index-heavy parser (Lezer-style typed-array tables read this way constantly) | low | allocation and time per `a[i]` and `u16[i]` read, int and float key |
| 3 | Native calls without a heap-allocated argument list | `object/internal_methods/function.rs` (native call path pops the arguments into a `Vec`/`ThinVec`) | malloc/free ~4%, part of drops | 2 to 4%. Measured here: a native call such as `Math.floor(x)` costs exactly one allocation, a JS-to-JS call none | low to medium (borrowing the stack while `&mut Context` is held; a `SmallVec<[JsValue; 4]>` avoids it) | allocations per native call, before/after |
| 4 | Instrumentation, not an optimization: count `GetPropertyByName` inline-cache hits against slow-path lookups (and the same for `SetPropertyByName` and global reads) over the CodeMirror parse | a diagnostic counter behind an env flag | property lookup ~12% | none by itself; decides whether item 5 is worth doing | none | needs the Mac probe; 20 lines of code |
| 5 | Widen the inline caches where the counters say they miss: methods on a primitive receiver (`s.charCodeAt`, `arr.push`), prototype-chain depth over 1, property-adding stores (object-literal shape transitions) | `vm/opcode/get/property.rs`, `set/property.rs`, shapes | property lookup ~12% | 2 to 4% (guess until item 4 reports) | medium (cache invalidation) | the item 4 hit rate |
| 6 | Audit register `.clone()`s that could borrow, ranked by how often each opcode runs in the parse (comparison, `in`, string/strict-equality ops, `PushKey`, `GetLocator`); the binary arithmetic ops already take a no-clone fast path | `vm/opcode/**` | drops/refcount ~17% | 3 to 5% | medium (each is a borrow-checker restructure) | opcode frequency from the Mac probe |
| 7 | Take back my own global-lexical patch's cost: it adds one cached hash lookup per global-object name read once any script has a top-level `const`/`let`/`class` (+14% on a loop of pure global reads, release, Linux); resolve once per code-block binding instead | `environments/runtime/mod.rs`, `realm.rs` | dispatch / property lookup | 0 to 2% (zero on a page that reads few globals) | medium (invalidation when a later script adds a lexical) | the same 1M-iteration loop |
| 8 | Check what the interrupt/budget bookkeeping (our patches) costs per instruction | `vm/mod.rs` | dispatch ~25% | 0 to 2% | low | run with the check compiled out, compare |

## Not recommended now
- **Superinstructions** (fusing hot opcode pairs such as a property load plus its call) and a different dispatch scheme: the handler table and unchecked frame/register access are already in place upstream, so dispatch has little slack left without bytecode-compiler changes; revisit only if items 2 to 6 leave dispatch dominant.
- **GC (1%) and bindings (~0%):** nothing to win.

## What I need from the Mac
The chrome-look probe on item 4 (hit/miss counters) and a per-opcode execution count over the same parse; both are small. Without them items 5 and 6 stay guesses.
