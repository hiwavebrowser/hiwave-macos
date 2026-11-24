# STRESS-SPEED: where TodoMVC-Lit-Complex-DOM's time goes (2026-11-24)

Row STRESS-SPEED. Speedometer 3.1 on develop with nested browsing contexts on scores 0.52 against Chrome
148's 44.99 (STRESS-TRUTH-2026-11-21.md). This is the first workload's breakdown, measured before any
change, using only the engine's own timing.

## Short answer

TodoMVC-Lit-Complex-DOM takes **4,304 ms** (median of 3: 4,287 / 4,304 / 4,326). Chrome 148's
median is 9.68 ms, so we are about 445x slower. Two costs that are not JavaScript make up half
of it:

1. **Window named-property lookups: about 1.21 s (28%).** Script reads `window.parentNode` and
   `window.nodeType` 5,006 and 5,011 times. Each read falls through to HTML's named access on the
   Window object (`web_named_access.js`, then `HOOKS.windowNamed`, then the `__rustkit_dom_collect`
   `windowNamed` arm), and that walks the whole document. The Complex-DOM page is 704 KB of
   markup, so each walk costs about 0.12 ms.
2. **The inline SVG pre-pass: about 0.86 s (20%).** The Complex-DOM filler holds 769 inline
   `<svg>` elements, and every render re-runs the pre-pass over all of them, about 428 ms per
   render.

**Proposed first fix:** make a window named-property miss cheap without changing the answer. Keep
a per-document index of the `id` values and of the `name` values on embed/form/img/object, built
lazily and dropped on any mutation of the tree or of an `id`/`name` attribute. With the index, a
name that no element carries returns `undefined` without walking the tree. A hit returns exactly
the elements the walk returns today. That removes up to 1.2 s of this workload's 4.3 s.

## How it was measured

- **Binary:** `parity-capture`, Windows release build of develop `b3329bff` (batch 124), on
  Windows 11 with an RTX 4090 and 32 logical CPUs. Run as `--engine hiwave-headless` with
  `HIWAVE_NESTED_CONTEXTS=1`.
- **Command:** `tools/stress/run.py --suite speedometer --only TodoMVC-Lit-Complex-DOM --iterations 1
  --frame-stages`. Speedometer is pinned to `1386415b`; the suite is served from 127.0.0.1 only.
  One run at a time, with nothing else heavy running.
- **Windows workaround:** `run.py` uses macOS-only calls (`os.getloadavg`, `os.uname`, `SIGHUP`,
  `sandbox-exec`, `lsof`). The runs went through a local wrapper that stubs the first four and
  launches the engine without the sandbox. As a result the records are marked `invalid`
  ("0 of 7 socket samples answered", because `lsof` has no Windows equivalent). The engine's own
  request log shows every request went to 127.0.0.1, and the workload's own timing (the
  `sub_scores` value) is recorded either way. Making `run.py` run on Windows is a separate tooling
  row.
- **Where the numbers come from:**
  - The workload time is Speedometer's own value, from the record's `sub_scores`.
  - The stage split is the engine's per-turn `Frame stages` lines, summed over the turns between
    the workload's `suite` and `iteration-done` progress events.
  - Host calls are the engine's `Host calls` rows: cumulative counts and milliseconds per
    host function.
  - Per-call selectors come from a one-off, uncommitted `eprintln!` in `__rustkit_dom_collect`:
    kind, argument and microseconds. That run (`probe`, 4,539 ms) is not one of the three
    baseline runs.
- **Records:** in this machine's `P:\repos\hiwave-wt\_stress_runs`:
  `speedometer_hiwave-headless_devb332_20261123T221803`, `..._222434`, `..._222443`.

## Stage split inside the workload (sum over its turns, ms)

| stage | run 1 | run 2 | run 3 |
|---|---:|---:|---:|
| busy (all stages) | 3,437 | 3,471 | 3,448 |
| script, host calls included | 1,993 | 2,003 | 1,997 |
| of which host calls | 1,246 | 1,241 | 1,250 |
| svg (inline SVG pre-pass) | 859 | 874 | 864 |
| other | 361 | 365 | 362 |
| build | 71 | 74 | 72 |
| layout | 56 | 57 | 57 |
| render | 57 | 57 | 56 |
| wait (pacing between turns) | 783 | 1,126 | 784 |

`host_ms` is part of `script_ms` (`tools/stress/frame_stages.py`: `js_ms = script_ms - host_ms`).
So JavaScript itself, in Boa, is about 750 ms. Layout, style build, display list and render
together come to about 0.25 s.

## Top 5 costs

| # | cost | ms | share of 4,304 | what it is |
|---|---|---:|---:|---|
| 1 | window named access for `parentNode` and `nodeType` | ~1,214 | 28% | 10,017 `__rustkit_dom_collect(windowNamed)` calls, ~0.12 ms each, each a full-document walk |
| 2 | inline SVG pre-pass | ~860 | 20% | 769 inline `<svg>` in the Complex-DOM filler, re-done on every render (~428 ms each, 2 renders in the workload) |
| 3 | JavaScript (Boa), host calls excluded | ~750 | 17% | the Lit app and the runner's steps |
| 4 | pacing wait between turns | ~780 | 18% | the live loop's wait between turns (PACE1); idle, not work |
| 5 | `other` | ~360 | 8% | outside the named stages; mostly one turn's ~336 ms (to split next) |

Next after those: other window named-access misses, `onclick` and `ontodo-add`/`-delete`/`-edit`
(500 calls, ~59 ms); `__rustkit_dom_info/parent` (521,465 calls, 92 ms of host time, plus the
script loop around them); everything else is under 20 ms.

### Host calls by argument, probe run (`__rustkit_dom_collect`, 11,463 calls, 1,283 ms)

| calls | ms | kind | argument |
|---:|---:|---|---|
| 5,006 | 612.7 | windowNamed | `parentNode` |
| 5,011 | 601.6 | windowNamed | `nodeType` |
| 200 | 23.5 | windowNamed | `onclick` |
| 100 | 12.1 | windowNamed | `ontodo-add` |
| 100 | 11.9 | windowNamed | `ontodo-delete` |
| 100 | 11.8 | windowNamed | `ontodo-edit` |
| 650 | 0.4 | selector | `todo-item,todo-list,todo-form,todo-footer,todo-app` |
| 200 | 1.2 | selector | `.toggle`, `.destroy` |

Every `querySelector` / `querySelectorAll` together comes to under 6 ms. The selector engine is
not the cost; the named lookups are.

## Proposed fixes, in order

1. **WINDOW-NAMED-INDEX (first).**
   - **What:** a window named-property miss in O(1). A per-document index of `id` values and of
     `name` values on embed/form/img/object, dropped on any tree mutation or `id`/`name`
     attribute change, rebuilt on the next lookup.
   - **Semantics:** unchanged. An element with `id="parentNode"` still makes
     `window.parentNode` that element, as in Chrome.
   - **Test, red first:** `window.parentNode` on a 10,000-element page under a call budget,
     plus Chrome 148's answers for hit, miss, `name` and several-match cases.
   - **Expected:** up to −1.2 s on this workload. Every workload that reads a missing global
     benefits.
   - **Also:** find the reader of `window.parentNode` / `nodeType` (a node walk that reaches
     `window`, probably an event path). If it's in our own bindings, stopping the walk at
     `window` is a second, smaller fix.
2. **SVG-PREPASS-CACHE.** Keep the pre-pass result of an inline SVG whose subtree and style have
   not changed since the last render. Expected up to −0.86 s here. All six Complex-DOM workloads
   (Angular, ES6-Webpack, Lit, Preact, React, Svelte) carry the same filler.
3. **Split `other`:** the ~336 ms turn.
4. **The pacing wait:** whether the headless live loop's inter-turn wait (PACE1) belongs inside a
   Speedometer measurement window at all, measured against Chrome's own frame pacing.

## Not done

- Only one workload (of 20) has been measured, and only on Windows headless. This is not a suite
  score.
- The caller of `window.parentNode` / `nodeType` is not identified.
- Nothing was changed.

## After the first fix (WINDOW-PATH-WALK)

The probe found where the reads come from: our own event dispatch. Every event that reaches the
Window asked it for `parentNode` / `nodeType` (shadow retargeting in `web_shadow.js` `rootNode` /
`hostOf` / `includes`; the body/frameset handler check in `dom.rs` `isBodyOrFrameset`) and for
`on<type>` (`dom.rs` `handlerOf`: `onclick`, `ontodo-add`, ...). The Window has none of those, so each
read fell to named access. The fix stops those helpers at the Window and reads the Window's own
`on<type>` without named access (head `2dbfd2b3`). WINDOW-NAMED-INDEX (a named-access miss that
does not walk the document) is still to do; with this fix the workload no longer exercises it.

A/B on this machine, same build settings, arms alternating, `run.py` made portable (STRESS-WINDOWS,
#1445), every record `status: ok` with the socket watch answering (0 failed samples):

| arm | TodoMVC-Lit-Complex-DOM ms (3 runs) | median |
|---|---|---:|
| develop `b3329bff` | 4,295.3 / 4,276.3 / 4,304.9 | 4,295.3 |
| this fix `2dbfd2b3` | 2,924.8 / 2,920.7 / 2,931.2 | **2,924.8** |

**-1,370 ms (-32%).** That is more than the 1.21 s of `parentNode` / `nodeType` walks: the
`on<type>` reads (`onclick`, `ontodo-*`) walked the document too. The first develop run started at
load 6.1 (just after the builds), so its record is marked not valid for load; its time matches
the other two. Records: `P:\repos\hiwave-wt\_stress_runs\ab\speedometer_hiwave-headless_lit-{base,fix}_*`.

The full Speedometer score for both arms is not measured yet: the run was stopped when the machine
ran low on memory (the develop arm's full run was in progress).
