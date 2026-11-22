# Stress truth, 2026-11-21: Speedometer, MotionMark, JetStream vs Chrome 148

Pete's bar (11-12, restated 11-16): the three stress suites at or better than
Chrome for Testing 148.0.7778.216 on this Mac. This is where HiWave stands
today, what stops each suite, and which single fix moves which suite most.
It replaces the 11-11 plan rows STR-A, STR-1..STR-11 (stale: frames, canvas
image data, workers and a MotionMark completion landed since).

## Short answer

**No suite produces a score on the default HiWave engine.** With nested
browsing contexts on (`HIWAVE_NESTED_CONTEXTS=1`, off by default), none
finishes either: Speedometer stops in its second iteration, MotionMark is
stopped by the 5 s live-turn watchdog at the end of its first subtest, and
JetStream fails its first test.

| suite | completes? (default / frames flag) | HiWave headless | Chrome 148 headless, median (min-max), N | ratio |
|---|---|---|---|---|
| Speedometer 3.1 | no 0/1 / no 0/3 | no score | 44.99 (43.78-46.64), 3 | - |
| MotionMark 1.3.2 | no 0/1 / no 0/3 | no score; **125.31** with the watchdog raised to 60 s (1 diagnostic run, not a score) | 8082 (8043-8129), 3 | 1.6% (raised run) |
| JetStream 2.2 | not run / no 0/1 (traced) | no score | 433.89 (429.92-445.22), 3 | - |

After this PR's fix (FRAME-MODULE-MAP) and #1423's 30 s stop: **Speedometer completes, 0.52 (0.52-0.53, N=3) vs 44.99; MotionMark completes, 146.44 (111.34-225.06, N=3) vs 8082.** See [After the fix](#after-the-fix).

## How it was measured

- HiWave: `parity-capture` release build of develop `20d87254` (batch 119),
  `--engine hiwave-headless`, `tools/stress/run.py` at the pins in
  `tools/stress/pins.json`, suites served from 127.0.0.1 only (every record's
  offline checks clean). Each run started at a 1-minute load below 3, one at a
  time, nothing else heavy on the machine (lane 1 idle).
- Chrome: Chrome for Testing 148.0.7778.216 headless, sha1 `d7c8cd59...`, the
  same binary today. Chrome's scores do not depend on our build, so the
  11-12/13 reference records are cited rather than re-run (coordinator's call,
  to free the machine): `~/zg-measure/stress-runs/`
  `speedometer_chrome_cft148_20261112T193901.json`, `..._20261112T215603.json`,
  `..._20261113T062605.json`; `motionmark_chrome_cft148_20261112T194553.json`,
  `..._20261113T055736.json`, `..._20261113T082441.json`;
  `jetstream_chrome_cft148_20261112T193923.json`, `..._20261113T042941.json`,
  `..._20261113T062647.json`. All started below load 3.
- HiWave records: `~/zg-measure/stress-runs/truth-1121/` (file names below).
- Not run: the real window (`--engine hiwave`, lane 1's lock). Headless
  frame rates are an upper bound on the app's (tools/stress/README.md).

## Speedometer 3.1

| run | flags | outcome | where |
|---|---|---|---|
| `speedometer_hiwave-headless_dev20d8-default_20261121T195431` | default | hung, 241 s | after `client-ready`: the first workload's iframe is never loaded (no nested contexts) |
| `..._dev20d8-frames_20261121T200135` | frames | hung, 329 s | iteration 1 of 10 completes (all 20 workloads, ~75 s); iteration 2: TodoMVC-WebComponents throws, then TodoMVC-Angular-Complex-DOM never starts |
| `..._dev20d8-frames_20261121T200732` | frames | hung, 328 s | same |
| `..._dev20d8-frames_20261121T201300` | frames | hung, 327 s | same |
| `..._dev20d8-frames_20261121T203441` (`--only` WebComponents + Angular-Complex, 2 iterations, `--trace`) | frames | hung, 97 s | iteration 1 both pass; iteration 2 WebComponents throws, Angular-Complex hangs after fetching its `main.js` |

The error, three times per run (once per test step), at 80-82 s:

```
TypeError: cannot convert 'null' or 'undefined' to object
    at getParent (resources/benchmark-runner.mjs:14)   // node.shadowRoot of a null node
    at querySelector / querySelectorAll ... tests.mjs:188, 196, 203 (TodoMVC-WebComponents)
```

**Cause (FRAME-MODULE-MAP).** Speedometer makes a new iframe for every
workload of every iteration. A same-origin frame is a new realm of the page's
JS runtime, and the module map was the runtime's, keyed by URL alone
(`rustkit-js/src/module.rs`: `begin_module` returned the existing handle for a
URL already started, and an import found the module already parsed in another
realm). So the second frame to load `todo-app.component.js` ran none of it:
no `customElements.define`, no upgrade, no shadow root, and the runner's
`getParent` reads `.shadowRoot` of null. Angular-Complex's three scripts are
all `type=module`: in iteration 2 Angular never boots and `prepare` waits for
`.new-todo` forever. HTML has one module map per document (environment
settings object). Chrome 148 on a reduced page
(`frame_module_realm_cft148.json`, by
`tools/parity_oracle/frame_module_realm_log.mjs`): a second frame at the same
module URLs runs them again, and a frame's import of a module the parent page
already ran runs it again in the frame.

Iteration time: HiWave's first iteration took ~75 s of wall clock; Chrome's
whole 10-iteration run takes ~20 s (~2 s per iteration). No per-workload
numbers exist until a run completes (Speedometer reports at the end).

Chrome's per-workload medians (ms, lower is better), for the next table:
Charts-chartjs 29.00, Charts-observable-plot 31.66, Editor-CodeMirror 15.66,
Editor-TipTap 48.26, NewsSite-Next 46.76, NewsSite-Nuxt 38.53, Perf-Dashboard
28.35, React-Stockcharts-SVG 51.70, TodoMVC-Angular-Complex-DOM 19.35,
TodoMVC-Backbone 15.69, TodoMVC-JavaScript-ES5 29.38,
TodoMVC-JavaScript-ES6-Webpack-Complex-DOM 25.84, TodoMVC-Lit-Complex-DOM
9.68, TodoMVC-Preact-Complex-DOM 7.40, TodoMVC-React-Complex-DOM 21.94,
TodoMVC-React-Redux 22.39, TodoMVC-Svelte-Complex-DOM 6.98, TodoMVC-Vue 13.52,
TodoMVC-WebComponents 9.89, TodoMVC-jQuery 69.69.

## MotionMark 1.3.2

| run | flags | outcome | where |
|---|---|---|---|
| `motionmark_hiwave-headless_dev20d8-default_20261121T195833` | `HIWAVE_NESTED_CONTEXTS=0` | hung, 181 s | at start: the test's iframe never loads |
| `..._dev20d8-frames_20261121T201827`, `..._202131`, `..._202434` | frames (the suite's own setting) | hung, 182-183 s, 3/3 | Multiply, its first subtest: ~40 s in, one timer callback runs past 5000 ms, the engine stops it (`Script stopped at the script budget source=timers elapsed_ms=5002`), the suite's loop is dead |
| `..._dev20d8-frames-raised_20261121T202739` | frames + `HIWAVE_STRESS_TURN_BUDGET_MS=60000` | **ok, 125.31** (bounds 101.5-249.3), 420 s | diagnostic only: not the default engine |

The stopped callback is MotionMark's end-of-test analysis (bootstrap
statistics over the test's samples, one long timer task); the 11-14 probe
measured 3-8 s for it on a Windows release build. Chrome has no such stop.

Per subtest, raised-budget run vs Chrome 148 (higher is better):

| subtest | HiWave (raised, N=1) | Chrome median (N=3) | ratio |
|---|---:|---:|---:|
| Multiply | 2 (floor; 3 of 10 ramps settled at 1) | 6420 | 0.03% |
| Canvas Arcs | 606.5 | 31338 | 1.9% |
| Leaves | 572.4 | 5399 | 10.6% |
| Paths | 2070.4 | 78997 | 2.6% |
| Canvas Lines | 1120.1 | 70572 | 1.6% |
| Images | 20.3 | 799.1 | 2.5% |
| Design | 44.0 | 827.0 | 5.3% |
| Suits | 42.3 | 4664 | 0.9% |
| **overall** | **125.31** | **8082** | **1.6%** |

Where the frame time goes per subtest is in
`docs/diagnostics/2026-11-17/motionmark_stage_split.md` (script on 7 of 8,
pipeline on Suits); the inline style write it found (~108 us per
`el.style.x =`) has since gone native (#1264), so those script rows are
stale; re-split before aiming at them.

## JetStream 2.2

| run | flags | outcome | where |
|---|---|---|---|
| `jetstream_hiwave-headless_dev20d8-frames_20261121T200704` (`--trace`) | frames | failed, 6.6 s | first test (WSL): "benchmark failed; JetStream stops the run here" |

The error: `unhandled rejection TypeError: cannot convert 'null' or
'undefined' to object at JetStreamDriver.js:323` (`runCode`). JetStream
builds every benchmark like this (JetStreamDriver.js 352-361):

```js
magic.contentDocument.body.innerHTML = "<iframe id=\"magicframe\" frameborder=\"0\">";
var magicFrame = magic.contentDocument.getElementById("magicframe");
magicFrame.contentDocument.open();
magicFrame.contentDocument.write("<!DOCTYPE html>...<script>...</script><script src=\"blob:...\"></script>...");
```

What it needs that is missing, in the order a fix meets them:

1. **A new iframe's `about:blank` document at once.** Here `contentDocument`
   is `null` until the inserting script returns (web_frames.js, "Stated
   limits of this stage"); HTML makes it at insertion. This is the throw.
2. **`document.open()` / `document.write()`** on that document, with its
   scripts run in order as the written parser reaches them (inline ones
   synchronously, `blob:` `src` ones blocking the parser), `window.onerror`
   and `top.currentResolve` reached from the frame. Not implemented at all:
   `Document.prototype` has neither (the only `write` is a no-op on the
   legacy stub document in rustkit-bindings/src/lib.rs).
3. **WebAssembly** (5 of 64 tests: HashSet-wasm, gcc-loops-wasm,
   quicksort-wasm, richards-wasm, tsf-wasm). Absent, JetStream skips them
   quietly (`runWasm = false`) and scores the rest; W0 (11-15) found wasmi
   sufficient, W1 is not built.
4. **Workers** for bomb-workers (`web_worker.js` exists; not reached yet).

## Blockers, ranked by expected score impact

| # | blocker | suite(s) | moves | size |
|---|---|---|---|---|
| 1 | **Module map per realm** (FRAME-MODULE-MAP): a frame re-runs module URLs another realm ran | Speedometer | no score -> first full score (expected ~1: one iteration ~75 s vs Chrome ~2 s; an estimate, not a measurement) | small, this PR |
| 2 | **The 5 s live-turn stop ends MotionMark's end-of-test analysis** | MotionMark | no score -> ~125 (the raised run) | either Boa faster on the analysis or the stop not ending a turn that is making progress: the second is a product decision (Pete) |
| 3 | **Nested contexts off by default** | all three | the default engine runs none of them; every number here needs the flag | flag flip after IFR-FLOAT (the open pre-flip row); priority call |
| 4 | **Synchronous about:blank + `document.open/write` with script execution** | JetStream | no score -> 59 of 64 tests run | medium-large |
| 5 | **WebAssembly** (W1-W4, wasmi approved) | JetStream | the last 5 tests | large, planned |
| 6 | **Speed**: Speedometer ~35x per iteration; MotionMark 0.03-10.6% per subtest (Multiply at the floor, Suits 0.9%, canvas 1.6-2.6%) | all | the actual gap to Chrome | the long road; re-split MotionMark first |

Why #1 first: it is the only blocker that turns a suite from no score into a
score with a small, local, spec-pinned change, and it is a correctness bug
any site with module scripts in two same-origin frames hits. #2 moves
MotionMark as far but needs either a deep Boa speed-up or Pete's decision on
the watchdog. #3 is a flag Pete's priorities flip. #4 is the JetStream
gate and is bigger.

## After the fix

Re-measured 2026-11-21 23:34 to 2026-11-22 00:34, in the quiet slot after
batch 121 (lane 1 idle). Each run started below a 1-minute load of 3
(1.7-2.4), one at a time, offline checks clean, headless,
`HIWAVE_NESTED_CONTEXTS=1`. Records: `~/zg-measure/stress-runs/truth-1121/after/`.

| suite | build | completes? | HiWave median (min-max), N | Chrome 148 median | ratio |
|---|---|---|---|---|---|
| Speedometer 3.1 | this PR (module map per realm), `c9067be2` | **yes, 3/3** (774 s each) | **0.52** (0.52-0.53), 3 | 44.99 | **1.2%** |
| MotionMark 1.3.2 | this PR + #1423 (30 s live stop), local merge `e5e93967` | **yes, 3/3**, default budget | **146.44** (111.34-225.06), 3 | 8082 | **1.8%** |
| JetStream 2.2 | - | not re-run (nothing here moves it: blocker #4) | - | 433.89 | - |

**Speedometer.** Before: 0/3 complete (iteration 2 threw, then hung). After:
3/3 complete, no page or engine errors, spread 1.7%. That number was checked
before it was reported: Editor-TipTap's ~15 s sits next to the app's 15 s
load script budget, but it is 14.4, 15.5 and 15.0 s across the runs and no
run logged a script stop (`log_findings` empty), so it is TipTap's own time.
A completion check on a busy machine before this re-measure (load 2-7,
`checks/speedometer_hiwave-headless_fmm-c9067be2-busycheck_20261121T210137`,
not a measurement) hung once in iteration 7 at Perf-Dashboard after six
clean iterations; the three quiet runs did not. That hang is unexplained and
worth a look if it comes back.

Per workload (ms per iteration, lower is better; sorted by the gap):

| workload | HiWave ms (median, N=3) | Chrome ms | HiWave / Chrome |
|---|---:|---:|---:|
| TodoMVC-Lit-Complex-DOM | 3935 | 9.68 | 407x |
| Editor-TipTap | 15010 | 48.26 | 311x |
| TodoMVC-Svelte-Complex-DOM | 2018 | 6.98 | 289x |
| TodoMVC-Preact-Complex-DOM | 1906 | 7.40 | 258x |
| NewsSite-Nuxt | 7432 | 38.53 | 193x |
| NewsSite-Next | 7753 | 46.76 | 166x |
| TodoMVC-Angular-Complex-DOM | 2632 | 19.35 | 136x |
| TodoMVC-React-Complex-DOM | 2866 | 21.94 | 131x |
| TodoMVC-JavaScript-ES6-Webpack-Complex-DOM | 2733 | 25.84 | 106x |
| TodoMVC-WebComponents | 949 | 9.89 | 96x |
| Editor-CodeMirror | 1236 | 15.66 | 79x |
| TodoMVC-React-Redux | 1505 | 22.39 | 67x |
| Perf-Dashboard | 1392 | 28.35 | 49x |
| TodoMVC-Vue | 659 | 13.52 | 49x |
| React-Stockcharts-SVG | 2232 | 51.70 | 43x |
| TodoMVC-jQuery | 2555 | 69.69 | 37x |
| TodoMVC-Backbone | 483 | 15.69 | 31x |
| Charts-chartjs | 834 | 29.00 | 29x |
| TodoMVC-JavaScript-ES5 | 798 | 29.38 | 27x |
| Charts-observable-plot | 817 | 31.66 | 26x |

**MotionMark.** Before: 0/3 at the default budget (stopped at 5 s in
Multiply's end-of-test analysis). With the 30 s stop (#1423, Pete's
2026-11-21 decision; the turn's 5 s allowance and pacing unchanged) the
suite completes at the default engine settings. The 78% spread is Multiply
alone (1 / 11 / 198): every other subtest is within about 30% across the
three runs, but Multiply sits near the floor, where MotionMark's ramp
settles anywhere, and the suite's geometric mean amplifies it. Read the
median with that in mind.

| subtest | HiWave median (N=3) | runs | Chrome median | ratio |
|---|---:|---|---:|---:|
| Multiply | 11 | 1 / 11 / 197.8 | 6420 | 0.17% |
| Canvas Arcs | 597.08 | 607.2 / 587.8 / 597.1 | 31338 | 1.91% |
| Leaves | 584.32 | 580.8 / 584.3 / 597.1 | 5399 | 10.82% |
| Paths | 1598 | 2121.2 / 1597.5 / 1521.7 | 78997 | 2.02% |
| Canvas Lines | 1060 | 1059.9 / 1100.3 / 1010.4 | 70572 | 1.50% |
| Images | 16.66 | 16.7 / 16.6 / 24.7 | 799.12 | 2.08% |
| Design | 40.36 | 35.5 / 40.4 / 49.7 | 827.02 | 4.88% |
| Suits | 49.32 | 50.3 / 47.6 / 49.3 | 4664 | 1.06% |

**What moved, in one line each.** Speedometer: no score -> 0.52 (Chrome
44.99, 87x behind). MotionMark: no score -> 146 (Chrome 8082, 55x behind).
JetStream: unchanged, no score.

**Next, by the ranking above.** #3 (the frames flag) is a gated batch trial
(Pete, 11-21). #4 (synchronous about:blank and `document.open/write`) is the
JetStream gate. Then speed: on Speedometer the largest gaps are
TodoMVC-Lit-Complex-DOM (407x), Editor-TipTap (311x),
TodoMVC-Svelte-Complex-DOM (289x) and TodoMVC-Preact-Complex-DOM (258x); in
absolute time, Editor-TipTap and the two NewsSite workloads are 30 of the
~60 s an iteration takes. On MotionMark, Multiply (floor) and Suits (1%).

*This section was added after the first review head; it needs a delta R1.*
