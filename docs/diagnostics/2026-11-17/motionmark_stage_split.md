# MotionMark stage split: one frame by stage, HiWave vs Chrome (2026-11-17)

The measurement a "close the gap to Chrome" plan rests on. Last night's batch-97
receipts put HiWave at ~99 overall against Chrome for Testing 148 at ~10,010
headed (28x to 740x per subtest). Those are scores; this is where the time of one
frame goes, per stage, at fixed element counts, so the gap can be assigned to a
stage and a design change priced against it.

## What could not be measured, and why

- **The real window's present and the GPU's own time (HiWave).** Every HiWave
  number here is the headless engine (parity-capture's live loop, offscreen
  GPU). It has no present, and `render`/`execute` time the CPU side of the
  commit and the command encoding, not GPU completion. The `present_ms` and
  `acquire_ms` columns are 0 by construction. Those columns are left to the Mac
  lane's real-window pass; the app does not print the per-turn stage line yet
  (`run.py --frame-stages --engine hiwave` refuses and says so).
- **The canvas subtests' draw of the canvas (HiWave).** On Canvas Arcs, Canvas
  Lines and Paths the headless render executed no draw on any frame read
  (`execute_ms` 0, `render_ms` ~0.001, no relayout; the frame's display list is
  refreshed in ~0.1 ms). The replay of the canvas recording as tessellated
  vector commands, which the real window does draw, is therefore **not in these
  tables**. What is in them for canvas is the script and the CPU raster
  (tiny-skia) into the bitmap that `getImageData` reads, which every draw call
  pays inside the script.
- **Three HiWave cells at 1000 elements.**
  - *Images 1000*: no frame was presented in the interval. The live turns ran
    4.3, 4.0, 2.0 and 5.4 s of script each with no render, then the suite's own
    end-of-test analysis threw (too few samples) and the run was called hung.
  - *Suits 1000*: one 18.8 s turn while the frame document set up (18.1 s of it
    claimed by no stage, 0.3 s render), then no animation frame before the run
    ended. Not a frame time.
  - *Multiply 1000*: measured, but only 6 steady turns (about 1.15 s each), and
    the suite's analysis threw before it reported its own frames, so the "rAF
    ticks per presented frame" figure (2.0) is read off the turn lines (the one
    turn with a single round took half as long as the others), not from the
    suite.
- **Script is not split into JavaScript and DOM bindings.** `script` is the
  whole script turn: Boa running the page's code plus every host call it makes
  (style setters, canvas API calls), less the canvas raster, which is its own
  row. On the DOM subtests that one row is the gap; the next instrument splits
  it (see the end).
- **Style and box build are one row for HiWave.** RustKit computes style and
  builds boxes in one walk (`build_layout_tree`), which is also what Chrome's
  `UpdateLayoutTree` covers; the row compares those two.
- **Chrome's "render" row is work on other threads**, not time on the frame's
  critical path: the renderer's compositor thread, the GPU process main thread
  (where raster runs: Chrome rasterizes on the GPU), and the display compositor
  threads, summed per frame. It runs in parallel with the main thread.
- **Chrome's frame interval is vsync-bound.** At every count below 1000 (and at
  1000 for all but Design) Chrome holds 16.7 ms, so its frame-time column says
  "at least 60 fps", not what a frame costs; its stage rows say that.

## Method

- **Fixed complexity.** MotionMark's developer runner has a fixed controller
  ("keep at a fixed complexity") next to the ramp. `tools/stress/run.py
  --complexity N` selects it for one subtest (`--only`), so the subtest holds N
  elements for the whole test interval (10 s at 10 and 100, 15 s at 1000). Local
  suite copy only (MotionMark 1.3.2 at its pin), loopback server, nothing off the
  machine (each record's offline checks are clean).
- **HiWave:** `--engine hiwave-headless --frame-stages`, parity-capture built
  from this branch (parity profile, head `ec384f45`). Each live turn prints one
  `Frame stages ...` line: the turn's busy time, the wait before it, and the
  per-thread stage totals' difference across it (script, canvas raster, build,
  layout, display list, snapshot, svg, render with its acquire/execute/present,
  renders, other). A presented frame is a turn that rendered; the steady stretch
  is the longest run of such turns with no gap over 2 s, and its middle 60% is
  read. Medians per stage.
- **Chrome:** Chrome for Testing 148.0.7778.216 (the repo's pin), headless,
  `--frame-stages`: its own startup trace (JSON; devtools.timeline, toplevel,
  v8, blink, cc, gpu, viz). A frame is the renderer main thread from one
  `ProxyMain::BeginMainFrame` to the next; middle 60% of the longest steady run.
  Main-thread time is self time classed by event name (script: function calls,
  animation frame and timer callbacks, V8 and GC; style: UpdateLayoutTree and
  style recalc; layout; paint: prepaint, paint, layerize; the rest "other").
  Other threads count each task's whole time in the frame it started in.
- **Per element** is the row divided by N. At 10 elements fixed per-frame costs
  dominate; the 100 and 1000 columns are the per-element costs to read.
- **rAF ticks per presented frame.** When a frame tree's live turn spends its
  frame's worth of time in timers, HiWave runs the tree's animation frame
  callbacks once more before the turn returns (#776). MotionMark counts each
  callback as a frame, so on the heavy cells one presented HiWave frame carries
  about two of the suite's frames (the tables say how many). HiWave's script row
  is per rAF tick (the turn's script divided by the ticks), so it compares with
  Chrome's one tick per frame; every other HiWave row is per presented frame.
- **Machine:** each run started at a 1-minute load below 3 (run.py's rule; 51
  runs, 1.96 to 3.0 at start), one run at a time, no cargo job beside a run,
  real-window lock checked before each run and each build. N = 1 run per cell.

## The read

**The gap is script, on 7 of the 8 subtests.** Share of HiWave's busy time per
presented frame (script includes the canvas raster; "pipeline" is build +
layout + display list + render):

| subtest | N | HiWave busy ms/presented frame | script | of which canvas raster | pipeline | other |
|---|---:|---:|---:|---:|---:|---:|
| Multiply | 100 | 101.0 | 94% | 0% | 4.5% | 1.4% |
| Multiply | 1000 | 1148.7 | 93% | 0% | 7.2% | 0.2% |
| Canvas Arcs | 1000 | 63.8 | 99% | 61% | 0.2% | 0.8% |
| Leaves | 1000 | 922.8 | 98% | 0% | 1.4% | 0.2% |
| Paths | 1000 | 12.2 | 96% | 28% | 0.4% | 3.4% |
| Canvas Lines | 1000 | 41.1 | 99% | 34% | 0.2% | 1.1% |
| Images | 100 | 128.0 | 96% | 0.8% | 2.9% | 0.6% |
| Design | 1000 | 688.4 | 71% | 0% | 26% | 3.8% |
| Suits | 100 | 64.4 | 4.3% | 0% | 90% | 4.9% |

Per element, from the 100 to the 1000 column (the slope, so fixed costs drop
out), script per rAF tick:

- Multiply: HiWave (531.7 - 48.5) / 900 = 537 µs per tile, Chrome
  (0.506 - 0.069) / 900 = 0.49 µs: ~1,100x.
- Leaves: HiWave (448.7 - 42.2) / 900 = 452 µs per leaf, Chrome 0.50 µs: ~900x.
- Design: HiWave (244.8 - 21.8) / 900 = 248 µs per text element, Chrome
  0.56 µs: ~440x.
- Canvas Arcs: HiWave script less raster (12.6 - 2.2) / 900 = 11.6 µs per arc,
  raster (20.0 - 3.3) / 900 = 18.5 µs; Chrome 0.18 µs per arc in script.

The DOM subtests' script is a handful of style writes per element per frame;
half a millisecond per element is not JavaScript interpretation speed alone and
is the first thing to take apart (below).

**What a compositor-layer design would remove** (draw an animated subtree once
into a texture, then only move or fade it each frame): the per-frame build,
layout, display list and render of that subtree, not the script that writes
the new transform or opacity.

- Suits 100: 2.2 + 0.03 + 39.4 + 16.4 = 58.1 ms of 64.4 ms (90%). This is the
  one subtest where layers are the lever: what remains is ~2.8 ms of script and
  ~3.2 ms other, ~6 ms a frame, against Chrome's ~1.8 ms of total work.
- Design 1000: 82.7 + 67.8 + 23.1 + 2.4 = 176 ms of 688 ms (26%), if the text
  is laid out once and only moved; 490 ms of script per presented frame stays.
- Multiply 1000: 80.8 + 0.9 + 0.4 + 0.4 = 82.4 ms of 1149 ms (7%).
- Leaves 1000: 11.2 + 0.3 + 1.1 + 0.5 = 13.0 ms of 923 ms (1.4%).
- Images 100: 1.6 + 0.04 + 0.2 + 1.9 = 3.7 ms of 128 ms (2.9%).
- Canvas subtests: nothing measured here (no relayout or draw happens per
  frame headless; the canvas content changes every frame, so a layer of it
  would be redrawn every frame anyway).

**What a GPU 2D canvas would remove** (draw calls recorded and rasterized on
the GPU instead of tiny-skia on the CPU inside each call): the canvas raster
row, per rAF tick at 1000 elements:

- Canvas Arcs: 20.0 ms of 32.7 ms of script (61%); 12.6 ms of script remains
  against Chrome's 0.20 ms (64x).
- Canvas Lines: 7.3 ms of 21.1 ms (34%); 13.8 ms remains vs Chrome 0.25 ms (56x).
- Paths: 3.4 ms of 11.8 ms (29%); 8.3 ms remains vs Chrome 0.12 ms (68x).
- Plus whatever the real window's replay of the recording costs today, which
  these tables do not have (above).

**Two more things the split shows.**

- *Half of HiWave's suite frames are never presented on the heavy cells.* At
  100 elements and up, a presented HiWave frame carries ~2 rAF ticks (1.74 to
  2.01 in the tables): the second animation round runs the page's callbacks
  again before anything is drawn. MotionMark scores frames by callback, so the
  score reads about twice the presented frame rate, and the second round's
  script is spent on a frame nobody sees. Doubles the script cost per presented
  frame.
- *Design leaves 36-40% of its frame unclaimed at 10 and 100 elements* (2.5 ms,
  30.8 ms `other`). Something per text element runs outside every timed stage
  (hover hit-testing after layout, font or text work outside the build); it
  needs its own stage before Design can be priced fully. Paths at 10 shows a
  1.3 ms `other` floor too.

**What to measure next, in order.**

1. Split `script` into JavaScript and host calls by binding (style setter,
   classList, canvas path/arc/fill, image src), same per-turn line. That says
   whether the ~0.5 ms per element on Multiply/Leaves is Boa or the bindings'
   style path.
2. The app's per-turn stage line, and the real-window pass at the same fixed
   counts: present, acquire and the canvas replay cost.
3. The unclaimed time on Design.

## Tables

Rows: HiWave ms per presented frame (script and canvas raster per rAF tick) and
µs per element; Chrome ms per frame and µs per element; ratio HiWave / Chrome.
"Work per presented frame" is HiWave's whole turn and Chrome's main-thread busy
time plus its other threads' work. "Frame interval" is presented frame to
presented frame; "MotionMark's own frame length" is the suite's mean frame
length at that count (per callback).

### Multiply

**10 elements** (HiWave: 421 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 4.871 | 487.10 | 0.122 | 12.20 | 40x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 3.678 | 367.80 | 0.176 | 17.60 | 21x |
| layout | 0.009 | 0.90 | 0.001 | 0.10 | - |
| display list / paint | 0.014 | 1.40 | 0.309 | 30.90 | 0.05x |
| render (commit + GPU execute) | 0.094 | 9.40 | 2.026 | 202.65 | 0.05x |
| other | 0.889 | 88.90 | 0.262 | 26.20 | 3.4x |
| **work per presented frame** | 9.542 | 954.20 | 2.921 | 292.05 | 3.3x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 69 presented frames read, 1.96 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 48.524 | 485.24 | 0.069 | 0.69 | 703x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 4.356 | 43.56 | 0.102 | 1.02 | 43x |
| layout | 0.012 | 0.12 | 0.000 | 0.00 | Chrome ~0 |
| display list / paint | 0.050 | 0.50 | 0.174 | 1.74 | 0.3x |
| render (commit + GPU execute) | 0.113 | 1.13 | 0.446 | 4.46 | 0.3x |
| other | 1.440 | 14.40 | 0.063 | 0.63 | 23x |
| **work per presented frame** | 100.960 | 1009.60 | 0.857 | 8.57 | 118x |
| frame interval (presented) | 101.1 | | 16.7 | | 6.1x |
| MotionMark's own frame length | 51.7 | | 16.7 | | 3.1x |

**1000 elements** (HiWave: 6 presented frames read, 2.00 rAF ticks each; Chrome: 614 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 531.670 | 531.67 | 0.506 | 0.51 | 1051x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 80.825 | 80.83 | 0.626 | 0.63 | 129x |
| layout | 0.893 | 0.89 | 0.000 | 0.00 | Chrome ~0 |
| display list / paint | 0.363 | 0.36 | 1.071 | 1.07 | 0.3x |
| render (commit + GPU execute) | 0.360 | 0.36 | 1.067 | 1.07 | 0.3x |
| other | 2.543 | 2.54 | 0.077 | 0.08 | 33x |
| **work per presented frame** | 1148.748 | 1148.75 | 3.357 | 3.36 | 342x |
| frame interval (presented) | 1150.0 | | 16.7 | | 69x |
| MotionMark's own frame length | - | | 16.7 | | - |

### Canvas Arcs

**10 elements** (HiWave: 432 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 0.218 | 21.80 | 0.032 | 3.20 | 6.8x |
| canvas raster (per rAF tick) | 0.229 | 22.90 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.005 | 0.50 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.10 | - |
| display list / paint | 0.008 | 0.80 | 0.039 | 3.90 | 0.2x |
| render (commit + GPU execute) | 0.001 | 0.10 | 0.547 | 54.70 | 0.00x |
| other | 0.357 | 35.70 | 0.117 | 11.75 | 3.0x |
| **work per presented frame** | 0.814 | 81.35 | 0.742 | 74.20 | 1.1x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 432 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 2.216 | 22.16 | 0.034 | 0.34 | 65x |
| canvas raster (per rAF tick) | 3.333 | 33.33 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.003 | 0.03 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.01 | - |
| display list / paint | 0.024 | 0.24 | 0.023 | 0.23 | 1.0x |
| render (commit + GPU execute) | 0.001 | 0.01 | 0.506 | 5.06 | 0.00x |
| other | 0.636 | 6.36 | 0.087 | 0.87 | 7.3x |
| **work per presented frame** | 6.254 | 62.54 | 0.655 | 6.55 | 9.5x |
| frame interval (presented) | 16.6 | | 16.6 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**1000 elements** (HiWave: 159 presented frames read, 1.93 rAF ticks each; Chrome: 614 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 12.645 | 12.64 | 0.197 | 0.20 | 64x |
| canvas raster (per rAF tick) | 20.011 | 20.01 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.004 | 0.00 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.00 | - |
| display list / paint | 0.120 | 0.12 | 0.028 | 0.03 | 4.3x |
| render (commit + GPU execute) | 0.002 | 0.00 | 0.962 | 0.96 | 0.00x |
| other | 0.513 | 0.51 | 0.124 | 0.12 | 4.1x |
| **work per presented frame** | 63.761 | 63.76 | 1.315 | 1.31 | 49x |
| frame interval (presented) | 63.8 | | 16.7 | | 3.8x |
| MotionMark's own frame length | 33.1 | | 16.7 | | 2.0x |

### Leaves

**10 elements** (HiWave: 432 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 4.362 | 436.25 | 0.128 | 12.80 | 34x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 1.142 | 114.15 | 0.115 | 11.50 | 9.9x |
| layout | 0.014 | 1.40 | 0.002 | 0.20 | Chrome ~0 |
| display list / paint | 0.021 | 2.10 | 0.227 | 22.65 | 0.09x |
| render (commit + GPU execute) | 0.083 | 8.25 | 1.929 | 192.95 | 0.04x |
| other | 0.505 | 50.55 | 0.265 | 26.50 | 1.9x |
| **work per presented frame** | 6.168 | 616.85 | 2.680 | 268.05 | 2.3x |
| frame interval (presented) | 16.6 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 84 presented frames read, 1.98 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 42.160 | 421.60 | 0.067 | 0.67 | 629x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 1.980 | 19.80 | 0.079 | 0.79 | 25x |
| layout | 0.033 | 0.33 | 0.005 | 0.05 | Chrome ~0 |
| display list / paint | 0.120 | 1.20 | 0.666 | 6.67 | 0.2x |
| render (commit + GPU execute) | 0.132 | 1.32 | 0.819 | 8.19 | 0.2x |
| other | 0.637 | 6.38 | 0.111 | 1.11 | 5.7x |
| **work per presented frame** | 86.424 | 864.24 | 1.748 | 17.48 | 49x |
| frame interval (presented) | 86.4 | | 16.7 | | 5.2x |
| MotionMark's own frame length | 43.7 | | 16.7 | | 2.6x |

**1000 elements** (HiWave: 21 presented frames read, 2.01 rAF ticks each; Chrome: 614 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 448.691 | 448.69 | 0.513 | 0.51 | 875x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 11.214 | 11.21 | 0.591 | 0.59 | 19x |
| layout | 0.259 | 0.26 | 0.014 | 0.01 | 18x |
| display list / paint | 1.105 | 1.10 | 0.988 | 0.99 | 1.1x |
| render (commit + GPU execute) | 0.465 | 0.47 | 2.477 | 2.48 | 0.2x |
| other | 2.137 | 2.14 | 0.489 | 0.49 | 4.4x |
| **work per presented frame** | 922.792 | 922.79 | 5.086 | 5.09 | 181x |
| frame interval (presented) | 923.3 | | 16.7 | | 55x |
| MotionMark's own frame length | 459.6 | | 16.7 | | 28x |

### Paths

**10 elements** (HiWave: 429 presented frames read, 1.00 rAF ticks each; Chrome: 435 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 0.802 | 80.20 | 0.016 | 1.60 | 50x |
| canvas raster (per rAF tick) | 0.111 | 11.10 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.003 | 0.30 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.10 | - |
| display list / paint | 0.037 | 3.70 | 0.023 | 2.30 | 1.6x |
| render (commit + GPU execute) | 0.002 | 0.20 | 0.381 | 38.10 | 0.01x |
| other | 1.282 | 128.20 | 0.078 | 7.80 | 16x |
| **work per presented frame** | 2.251 | 225.10 | 0.503 | 50.30 | 4.5x |
| frame interval (presented) | 16.6 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 432 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 0.899 | 8.99 | 0.032 | 0.32 | 28x |
| canvas raster (per rAF tick) | 0.318 | 3.18 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.004 | 0.04 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.01 | - |
| display list / paint | 0.011 | 0.11 | 0.028 | 0.28 | 0.4x |
| render (commit + GPU execute) | 0.001 | 0.01 | 0.483 | 4.83 | 0.00x |
| other | 0.360 | 3.60 | 0.101 | 1.01 | 3.6x |
| **work per presented frame** | 1.592 | 15.92 | 0.646 | 6.46 | 2.5x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**1000 elements** (HiWave: 605 presented frames read, 1.00 rAF ticks each; Chrome: 614 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 8.328 | 8.33 | 0.122 | 0.12 | 68x |
| canvas raster (per rAF tick) | 3.437 | 3.44 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.004 | 0.00 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.00 | - |
| display list / paint | 0.051 | 0.05 | 0.028 | 0.03 | 1.8x |
| render (commit + GPU execute) | 0.001 | 0.00 | 0.673 | 0.67 | 0.00x |
| other | 0.413 | 0.41 | 0.117 | 0.12 | 3.5x |
| **work per presented frame** | 12.239 | 12.24 | 0.946 | 0.95 | 13x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

### Canvas Lines

**10 elements** (HiWave: 432 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 0.355 | 35.50 | 0.019 | 1.95 | 18x |
| canvas raster (per rAF tick) | 0.284 | 28.40 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.003 | 0.30 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.10 | - |
| display list / paint | 0.010 | 1.00 | 0.019 | 1.90 | 0.5x |
| render (commit + GPU execute) | 0.001 | 0.10 | 0.389 | 38.95 | 0.00x |
| other | 0.373 | 37.30 | 0.074 | 7.40 | 5.0x |
| **work per presented frame** | 1.020 | 102.05 | 0.506 | 50.60 | 2.0x |
| frame interval (presented) | 16.7 | | 16.6 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 429 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 1.579 | 15.79 | 0.035 | 0.35 | 45x |
| canvas raster (per rAF tick) | 0.896 | 8.96 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.002 | 0.02 | - |
| layout | 0.000 | 0.00 | 0.000 | 0.00 | - |
| display list / paint | 0.019 | 0.19 | 0.016 | 0.16 | 1.2x |
| render (commit + GPU execute) | 0.001 | 0.01 | 0.334 | 3.34 | 0.00x |
| other | 0.416 | 4.16 | 0.064 | 0.64 | 6.5x |
| **work per presented frame** | 2.910 | 29.10 | 0.453 | 4.53 | 6.4x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.8 | | 16.7 | | 1.0x |

**1000 elements** (HiWave: 240 presented frames read, 1.92 rAF ticks each; Chrome: 614 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 13.808 | 13.81 | 0.248 | 0.25 | 56x |
| canvas raster (per rAF tick) | 7.256 | 7.26 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.004 | 0.00 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.00 | - |
| display list / paint | 0.066 | 0.07 | 0.033 | 0.03 | 2.0x |
| render (commit + GPU execute) | 0.001 | 0.00 | 0.802 | 0.80 | 0.00x |
| other | 0.435 | 0.43 | 0.127 | 0.13 | 3.4x |
| **work per presented frame** | 41.077 | 41.08 | 1.218 | 1.22 | 34x |
| frame interval (presented) | 41.1 | | 16.6 | | 2.5x |
| MotionMark's own frame length | 21.4 | | 16.7 | | 1.3x |

### Images

**10 elements** (HiWave: 429 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 8.545 | 854.49 | 0.120 | 12.00 | 71x |
| canvas raster (per rAF tick) | 0.045 | 4.49 | - | - | - |
| style + box build | 0.000 | 0.00 | 0.004 | 0.40 | - |
| layout | 0.000 | 0.00 | 0.001 | 0.10 | - |
| display list / paint | 0.020 | 2.00 | 0.037 | 3.70 | 0.5x |
| render (commit + GPU execute) | 0.001 | 0.10 | 0.436 | 43.60 | 0.00x |
| other | 0.401 | 40.10 | 0.177 | 17.70 | 2.3x |
| **work per presented frame** | 9.310 | 931.00 | 0.775 | 77.45 | 12x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 47 presented frames read, 1.74 rAF ticks each; Chrome: 432 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 70.478 | 704.78 | 1.113 | 11.13 | 63x |
| canvas raster (per rAF tick) | 0.568 | 5.68 | - | - | - |
| style + box build | 1.585 | 15.85 | 0.026 | 0.27 | 60x |
| layout | 0.041 | 0.41 | 0.006 | 0.06 | Chrome ~0 |
| display list / paint | 0.220 | 2.20 | 0.218 | 2.18 | 1.0x |
| render (commit + GPU execute) | 1.881 | 18.81 | 2.090 | 20.89 | 0.9x |
| other | 0.722 | 7.22 | 1.259 | 12.59 | 0.6x |
| **work per presented frame** | 128.012 | 1280.12 | 4.723 | 47.23 | 27x |
| frame interval (presented) | 128.1 | | 16.7 | | 7.7x |
| MotionMark's own frame length | 73.8 | | 16.7 | | 4.4x |

**1000 elements**: HiWave NOT MEASURED (1 presented frame(s) in the interval; run hung)

### Design

**10 elements** (HiWave: 432 presented frames read, 1.00 rAF ticks each; Chrome: 436 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 1.909 | 190.90 | 0.017 | 1.70 | 112x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 1.547 | 154.70 | 0.085 | 8.50 | 18x |
| layout | 0.007 | 0.70 | 0.000 | 0.00 | - |
| display list / paint | 0.192 | 19.20 | 0.108 | 10.80 | 1.8x |
| render (commit + GPU execute) | 0.076 | 7.60 | 0.569 | 56.90 | 0.1x |
| other | 2.478 | 247.80 | 0.061 | 6.10 | 41x |
| **work per presented frame** | 6.266 | 626.55 | 0.844 | 84.40 | 7.4x |
| frame interval (presented) | 16.6 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 86 presented frames read, 1.99 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 21.772 | 217.72 | 0.097 | 0.97 | 224x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 8.497 | 84.97 | 0.825 | 8.25 | 10x |
| layout | 0.019 | 0.19 | 0.001 | 0.01 | Chrome ~0 |
| display list / paint | 2.293 | 22.93 | 0.824 | 8.24 | 2.8x |
| render (commit + GPU execute) | 0.378 | 3.78 | 1.902 | 19.02 | 0.2x |
| other | 30.777 | 307.77 | 0.098 | 0.98 | 314x |
| **work per presented frame** | 84.989 | 849.88 | 3.785 | 37.85 | 22x |
| frame interval (presented) | 84.9 | | 16.7 | | 5.1x |
| MotionMark's own frame length | 42.7 | | 16.7 | | 2.6x |

**1000 elements** (HiWave: 24 presented frames read, 2.00 rAF ticks each; Chrome: 532 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 244.838 | 244.84 | 0.600 | 0.60 | 408x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 82.739 | 82.74 | 7.782 | 7.78 | 11x |
| layout | 67.754 | 67.75 | 0.001 | 0.00 | Chrome ~0 |
| display list / paint | 23.078 | 23.08 | 8.704 | 8.70 | 2.7x |
| render (commit + GPU execute) | 2.409 | 2.41 | 12.446 | 12.45 | 0.2x |
| other | 25.913 | 25.91 | 0.199 | 0.20 | 130x |
| **work per presented frame** | 688.366 | 688.37 | 29.780 | 29.78 | 23x |
| frame interval (presented) | 688.1 | | 18.1 | | 38x |
| MotionMark's own frame length | 343.9 | | 19.1 | | 18x |

### Suits

**10 elements** (HiWave: 360 presented frames read, 1.00 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 0.240 | 24.00 | 0.050 | 5.00 | 4.8x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 1.180 | 118.00 | 0.050 | 5.00 | 24x |
| layout | 0.013 | 1.30 | 0.007 | 0.70 | Chrome ~0 |
| display list / paint | 3.284 | 328.45 | 0.086 | 8.55 | 38x |
| render (commit + GPU execute) | 1.486 | 148.60 | 0.921 | 92.10 | 1.6x |
| other | 0.572 | 57.20 | 0.128 | 12.75 | 4.5x |
| **work per presented frame** | 6.891 | 689.10 | 1.235 | 123.45 | 5.6x |
| frame interval (presented) | 16.7 | | 16.7 | | 1.0x |
| MotionMark's own frame length | 16.7 | | 16.7 | | 1.0x |

**100 elements** (HiWave: 93 presented frames read, 1.99 rAF ticks each; Chrome: 434 frames)

| stage | HiWave ms/frame | HiWave µs/elt | Chrome ms/frame | Chrome µs/elt | ratio |
|---|---:|---:|---:|---:|---:|
| script (JS, per rAF tick) | 1.387 | 13.87 | 0.100 | 1.00 | 14x |
| canvas raster (per rAF tick) | 0.000 | 0.00 | - | - | - |
| style + box build | 2.239 | 22.39 | 0.107 | 1.07 | 21x |
| layout | 0.027 | 0.27 | 0.006 | 0.06 | Chrome ~0 |
| display list / paint | 39.405 | 394.05 | 0.605 | 6.05 | 65x |
| render (commit + GPU execute) | 16.401 | 164.01 | 0.860 | 8.60 | 19x |
| other | 3.170 | 31.70 | 0.083 | 0.83 | 38x |
| **work per presented frame** | 64.382 | 643.82 | 1.772 | 17.73 | 36x |
| frame interval (presented) | 64.3 | | 16.7 | | 3.9x |
| MotionMark's own frame length | 32.4 | | 16.7 | | 1.9x |

**1000 elements**: HiWave NOT MEASURED (1 presented frame(s) in the interval; run hung)

