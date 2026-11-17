# GPU 2D canvas: what it would take (design note, 2026-11-17)

For the owner. A decision note, not a plan of record: what moving the canvas 2D
raster from the CPU to the GPU would take, what it would remove according to
the measurements we have, what it risks, and what to measure before choosing.

## Where canvas time goes today

A canvas 2D draw does three things on the content thread, inside the page's
script turn:

1. **The bindings' JavaScript** (`web_canvas.js`): the member's argument and
   receiver checks, then one host call (`__rustkit_canvas`).
2. **The host call** (`web_canvas.rs`, `rustkit-canvas`): the state change or
   draw on the context, a recording (`DrawCommand`, used now for the text
   overlay and partial clears), and for a draw **the CPU raster**: tiny-skia
   fills the path (a stroke is first turned into its outline) into the
   canvas's premultiplied RGBA pixmap.
3. Nothing else until the frame: when the display list is built, the bitmap
   is copied out (the canvas store's `pixels` copy → `CanvasFrame`, a new image key
   for every new bitmap), and the presenter uploads it as a texture
   (`upload_display_list_images`) and draws it in the canvas's box, with the
   text overlay drawn over it as vector text.

Measured on MotionMark at 1000 elements (headless, the SCRIPT-SPLIT build of
#1263; `docs/diagnostics/2026-11-17/motionmark_stage_split.md`, "Script split",
on #1258/#1263):

| subtest | rAF ticks per presented frame | script ms/presented frame | CPU raster ms | raster share of script | other host ms | JS ms | script ms per tick | Chrome script ms/frame |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Canvas Arcs | 1.94 | 64.73 | 38.27 | 59% | 3.84 | 21.30 | 33.4 | 0.197 |
| Canvas Lines | 1.92 | 41.76 | 13.97 | 33% | 3.31 | 24.34 | 21.8 | 0.248 |
| Paths | 1.00 | 12.29 | 3.46 | 28% | 1.04 | 7.78 | 12.3 | 0.122 |

(A presented HiWave frame on the heavy cells carries ~2 of MotionMark's
frames; per tick is what compares with Chrome's per-frame column.)

Per draw: a stroke is 21.7 µs on Arcs (7.2-7.5 µs on Lines and Paths), a fill
16.3 µs; every other canvas call is 0.21-0.46 µs of host. A scratch
microbenchmark of 1000 MotionMark arcs (tiny-skia at the parity profile, 1200x700)
splits one draw as: building the path 0.17 µs, stroking it into an outline
0.89 µs, **filling the outline with anti-aliasing 21.2 µs** (the same fill
without anti-aliasing: 2.7 µs), ~1,300 pixels touched per draw (~16 ns per
pixel). tiny-skia anti-aliases by 4x4 supersampling and blends each partially
covered run through its pipeline one run at a time; that, not path building or
stroking, is the raster cost.

CANVAS-SHIM (the canvas row's PR, `atlas/canvas-shim`) takes ~1.1 µs of Boa off
each hot canvas call; the JS column above is before it.

## What moving the raster to the GPU would remove

- **The raster share of script**: 59% of Arcs, 33% of Lines, 28% of Paths at
  1000 elements, and the per-frame copy of the bitmap into a new `CanvasFrame`
  and its upload as a new texture (not separately measured; see below).
- **What it would not remove**: the bindings' JS and the host call itself
  (0.2-0.5 µs a call), the recording, and the CPU side of a GPU draw: building
  the path, stroking it to an outline (0.9 µs per arc), tessellating or
  encoding it, and recording the GPU commands. Even with the raster at zero,
  Arcs would be ~13.7 ms of script per MotionMark frame against Chrome's
  0.2 ms: the gap is ~70x, not ~170x. The canvas subtests need both this and the JS work.

## Ways to do it

**A. Replay the recording on the GPU, rasterize on the CPU only for reads.**
The engine already has a vector paint of the recording (`canvas_paint.rs`,
CH2.E10 phase 1: fills tessellated into polygons, strokes as polylines, the
clip as a rect), which drew canvases before STR-7 added the bitmap. The
recording would be drawn into an offscreen wgpu texture per canvas (so a frame
redraws only what changed since the last clear), and the tiny-skia bitmap
would be built **lazily**, by replaying the recording, only when script reads
pixels (getImageData, drawImage of the canvas, createPattern of it,
toDataURL when it lands).
- Removes: the raster from every frame that does not read pixels, which is
  every MotionMark canvas frame.
- Keeps: getImageData byte-identical to today (the same tiny-skia raster, in
  the same order), provided the recording carries everything the raster
  takes: today it holds a subset (style, transform, line width and dash, a
  rectangular clip), not line caps and joins, the miter limit, the clip path,
  globalAlpha or the composite operation; those would be added. The
  `canvas_raster_cases` pins then stay as they are.
- Costs: the vector path is not the bitmap: anti-aliasing, stroke joins and
  caps, curve flattening, gradients, patterns and compositing modes would be
  drawn by the renderer's code, not tiny-skia's, so **what the screen shows
  and what getImageData returns can differ by edge pixels**. The recording must
  be kept until the next full clear or a bound (it is already bounded by
  bytes, `RECORDING_BYTE_BUDGET`; past it, rasterize and drop, as today).
- Smallest step from today; no new dependency.

**B. A GPU 2D rasterizer for both display and reads.**
Draw into a wgpu texture with GPU anti-aliasing (MSAA, or analytic coverage
in a shader), and answer getImageData by reading the texture back.
- Removes the raster and the upload; one source of truth for screen and
  reads, as in Chrome (whose GPU canvas reads back its own surface).
- Costs: a readback is a GPU round trip (map, wait, copy); a canvas read every
  frame stalls the pipeline. Chrome handles this by moving such a canvas to
  the CPU (`willReadFrequently`, and its own demotion after repeated reads).
  MotionMark Images does 163 getImageData and 163 putImageData per frame at
  100 elements: that canvas must stay on the CPU path.
- All of AA, joins, caps, dashes, gradients, patterns, clips, composite
  operations and text have to be written for the GPU and pinned against
  Chrome again.

**C. A GPU 2D renderer crate (for example Vello, on wgpu).**
Same shape as B with the rasterizer taken from a library (Vello renders 2D
paths with analytic anti-aliasing in compute shaders and has a CPU sibling
for reads). A new dependency and a wgpu version to keep in step with ours
(wgpu 24 here): that is an owner decision (THIRD_PARTY_NOTICES, review of
the code, binary size, shader compile time at startup).

**D (CPU, for comparison). An analytic-coverage CPU rasterizer** in place of
tiny-skia's supersampled fill (one pass computing exact edge coverage per
pixel, as Skia's own analytic AA and font rasterizers do). The microbenchmark
says the supersampling is ~8x the cost of a non-AA fill of the same outlines,
so this could take a large share of the 21 µs per arc on the CPU, and edges
would move toward Chrome's (analytic) rather than away. It changes every
edge pixel, so it is not a byte-identical change either; it needs the same
re-pinning as B, without the readback problem.

A and B/C are not exclusive: A first takes the raster out of script on every
frame that does not read; B or C later replace the vector replay with real
GPU AA.

## Risks

- **Readback for getImageData.** B and C pay a GPU round trip per read; A
  pays a lazy CPU raster of everything since the last clear on the first read
  after drawing. A read-heavy canvas (Images, charting libraries that hit-test
  by reading pixels) needs a CPU mode either way; decide the switch
  (`willReadFrequently`, a read count) before building.
- **Two pictures of one canvas (A only).** Screen and getImageData from
  different rasterizers. Pages that read back what they drew and compare
  (games, hit testing by colour) work on exact colours in the interior, not
  on edges; the visible difference is anti-aliasing at edges. Board rows that
  diff canvas pixels against Chrome would move.
- **Anti-aliasing parity with Chrome.** Today's pins (`canvas_raster_cases`,
  `web_canvas_raster_tests.rs`) compare tiny-skia's 4x4 supersampling with
  Chrome for Testing 148, which anti-aliases analytically: covered pixels
  match exactly, edge pixels within a tolerance (no channel off by more than
  80, mean at most 14 over painted pixels; thin curved strokes are the worst
  at ~13). A GPU path draws edges differently from both; every canvas pin and
  board row with curves or thin lines would need re-measuring against Chrome,
  and Chrome itself draws differently on its GPU and software paths.
- **Text.** Canvas text is not in the bitmap today: it is a vector overlay
  drawn over it, so a later draw does not cover text the way it does in
  Chrome. A GPU canvas would draw text into the texture through the same
  glyph cache, in order with the other draws: closer to Chrome, and another
  set of pins to re-measure.
- **Headless and CI.** The headless engine renders offscreen on a real
  adapter; CI runners without a GPU would need a software adapter for the
  canvas tests, or the CPU path kept as the test path.
- **Memory.** A texture per canvas (plus a CPU bitmap while one is held),
  against today's pixmap plus a per-frame upload.

## What to measure first

1. **The upload and copy we have now**: per frame on the canvas subtests, the
   bitmap copy (`pixels`), the `CanvasFrame` insert and the texture upload
   (the stage split's render column did not show a canvas draw headless;
   the instrument should count canvas uploads and their bytes per frame).
2. **The vector replay's cost and pixels (A)**: one Canvas Arcs frame at 1000
   elements through `canvas_paint`'s vector path into an offscreen texture:
   CPU time to build the commands (tessellation, polylines), GPU time, and a
   pixel diff against today's bitmap and against Chrome's frame.
3. **How often pages read canvases**: getImageData / drawImage-of-a-canvas /
   createPattern-of-a-canvas calls per frame on the board's pages and the top
   sites list (the host call rows count them), to size the CPU mode.
4. **What stays on the CPU per draw under any GPU design**: path build +
   stroke outline + encoding, per draw type, so the gain is priced before the
   build (the microbenchmark above is a start: ~1.1 µs per arc before
   encoding).
5. **Chrome's own split for the same frame**: its trace's raster and GPU
   threads for Canvas Arcs at 1000 (the stage split has them in
   `raster_workers` and `gpu_main`), to know what Chrome spends where we would
   be spending.

## First measurements (2026-11-17, same machine)

Taken after the note, to decide between A, B, C and D without building any
of them. Scratch tools (a microbenchmark inside rustkit-canvas, a Chrome probe
through `tools/parity_oracle`'s pinned launch), not committed; numbers below.

### 1. How often pages read canvas pixels back

From the SCRIPT-SPLIT host rows (#1263) for all 8 MotionMark subtests at 100
and 1000 elements: **only Images reads**: 163 `getImageData` and 163
`putImageData` per presented frame at 100 elements (1.72 MotionMark frames
each: ~95 of each per MotionMark frame), 7.1 and 9.8 µs per call. Every other
subtest makes no `getImageData`, no `drawImage` of a canvas and no
`createPattern` of one (`toDataURL` is not implemented).

Not measured: the board's pages (the browser's own UI: no canvas) and the
A/B's 19 live sites (no live loads outside the batch's A/B tool, which does
not print host rows). Speedometer's chart suites (Chart.js, Observable Plot)
carry `toDataURL` in their library code, but the headless runs did not get
past loading (no nested contexts in that run), so their reads are not
counted.

**For the decision:** MotionMark's canvas subtests never read back, so a
lazy read path (A) or a GPU readback (B, C) costs them nothing; Images is a
read-every-frame canvas and needs the CPU mode under B or C.

### 2. What the raster costs, by draw kind and size

One draw on a 200x200 canvas, solid opaque colour, default caps and joins,
parity profile, 400 repetitions each, µs per draw. "Context" is the whole
canvas call as the host runs it (tiny-skia AA, recording included); the next
three fill the same outline three ways; build and outline are the CPU work
that stays with any design (path build, stroke to outline); px is the pixels
with any coverage.

| draw | context (AA) | tiny-skia AA fill | tiny-skia non-AA fill | analytic sketch | build | outline | px |
|---|---:|---:|---:|---:|---:|---:|---:|
| arc stroke r20 w1 | 17.0 | 15.2 | 1.7 | 3.1 | 0.19 | 1.34 | 183 |
| arc stroke r20 w5 | 19.2 | 16.6 | 2.2 | 4.9 | 0.19 | 1.29 | 530 |
| arc stroke r60 w1 | 43.8 | 42.6 | 4.1 | 19.4 | 0.22 | 1.68 | 568 |
| arc stroke r60 w5 | 48.0 | 46.4 | 3.7 | 23.1 | 0.19 | 1.62 | 1,571 |
| arc stroke r60 w20 | 51.7 | 50.2 | 4.5 | 38.3 | 0.19 | 1.54 | 5,358 |
| arc stroke r90 w5 | 69.3 | 67.3 | 5.7 | 45.1 | 0.18 | 1.51 | 2,381 |
| arc stroke r90 w20 | 74.8 | 74.0 | 6.7 | 64.8 | 0.19 | 1.55 | 8,063 |
| arc fill r20 | 14.1 | 11.5 | 1.3 | 4.8 | 0.20 | - | 886 |
| arc fill r60 | 38.3 | 33.5 | 3.2 | 37.2 | 0.18 | - | 7,685 |
| arc fill r90 | 56.6 | 49.2 | 4.3 | 78.0 | 0.18 | - | 17,192 |
| line 205 px long, w1 | 36.6 | 37.1 | 2.8 | 24.8 | 0.05 | 0.16 | 482 |
| line 205 px long, w5 | 38.2 | 38.3 | 2.6 | 28.7 | 0.05 | 0.14 | 1,306 |
| line 205 px long, w20 | 43.0 | 43.2 | 3.2 | 40.6 | 0.05 | 0.14 | 4,428 |
| line 36 px long, w5 | 8.6 | 8.2 | 0.9 | 1.8 | 0.05 | 0.14 | 228 |
| rect 120x91, fractional | 22.0 | 18.8 | 2.1 | 43.5 | 0.09 | - | 11,011 |
| rect 100x100, aligned | 6.1 | 6.8 | 2.2 | 40.3 | 0.09 | - | 10,000 |
| quadratic stroke w1 | 42.8 | 43.1 | 3.2 | 19.9 | 0.05 | 0.44 | 558 |
| quadratic stroke w5 | 45.6 | 45.5 | 3.6 | 23.6 | 0.05 | 0.60 | 1,518 |

- **tiny-skia's AA cost follows the shape's extent and edges, not its
  painted pixels.** A 1 px diagonal line of 482 painted pixels costs 37 µs
  (~77 ns per painted pixel); an aligned 100x100 rectangle of 10,000 pixels
  costs 6.8 µs (0.7 ns per pixel). The 4x supersampled scan over every edge
  row is the cost; the same fill without AA is 8-15x cheaper on curves and
  lines (3-9x on rectangles).
- **The CPU work that stays under any design** (path build + stroke outline)
  is 0.2-1.9 µs per draw: under 10% of today's draw.
- **The analytic sketch** is a 60-line signed-area accumulator (font-rs
  style, scalar, every pixel of the bounding box resolved and blended, curves
  flattened to fixed steps). It is 1.1-5x faster than tiny-skia on thin
  strokes and lines, but slower on large fills and rectangles (it resolves the
  whole box; a real one keeps sparse spans and fills interiors by run). It
  prices D's shape, not D: a production analytic rasterizer would beat it on
  fills.

### 3. How far edges are from Chrome's, today and per option

The same 18 draws, read back with `getImageData`, against Chrome for Testing
148.0.7778.216 with the pins' launch (`--disable-accelerated-2d-canvas`: its
software Skia, analytic AA). Alpha channel (one colour, so coverage), over the
pixels either side painted: share of painted pixels off by more than 2,
largest difference, mean difference. "Chrome GPU" is the same Chrome launched
with Metal and GPU rasterization on (ANGLE Metal, Apple M2 Max), read back the
same way; whether each 200x200 canvas took Chrome's GPU raster is not
visible from the page, but its pixels differ from the software run's.

| draw kind | tiny-skia AA (today) | Chrome GPU | analytic sketch | tiny-skia non-AA |
|---|---|---|---|---|
| arc strokes (7) | 19.7%, max 138, mean 5.34 | 19.2%, 109, 4.12 | 20.5%, 147, 5.55 | 19.8%, 217, 12.38 |
| arc fills (3) | 3.8%, 79, 0.58 | 4.1%, 113, 1.06 | 4.5%, 179, 2.41 | 4.3%, 214, 2.65 |
| lines (4) | 21.6%, 48, 3.50 | 20.7%, 68, 3.47 | 18.8%, 47, 3.06 | 23.2%, 153, 12.50 |
| quadratic strokes (2) | 46.4%, 116, 9.96 | 49.5%, 115, 15.54 | 55.0%, 160, 22.33 | 50.7%, 206, 31.55 |
| rects (2) | 0.4%, 11, 0.05 | 0.6%, 39, 0.08 | 0.6%, 13, 0.08 | 1.4%, 128, 1.14 |
| all 18 | 9.5%, 138, 2.13 | 9.5%, 115, 2.14 | 10.1%, 179, 3.17 | 10.2%, 217, 6.25 |

- **Chrome's own GPU path is as far from Chrome's software path as
  tiny-skia is** (mean 2.14 against 2.13 over all painted pixels). A GPU
  canvas that draws like Chrome's GPU canvas would sit at today's distance
  from the pins; the pins compare with Chrome's software raster.
- **Non-AA is out**: 3x today's mean difference, and up to 217 on a channel.
- The analytic sketch is no closer than tiny-skia (its curve flattening and
  the join at the arc fills' centre are crude); a production one would be
  measured again.

### 4. The per-frame copy and upload

The engine's copy of a 1200x700 canvas bitmap into a new `CanvasFrame`
(premultiplied rows, ~3.4 MB): **0.13 ms per frame**. The texture upload is
not measured: the headless render executed no canvas draw on these frames
(#1258's note); it needs the real window.

### 5. What Chrome spends on the same frames

From #1258's Chrome traces at 1000 elements (per frame, ms): Canvas Arcs main
thread script 0.20, compositor 0.16, **GPU process main thread 0.63**,
display 0.18, raster workers 0; Lines 0.25 / 0.17 / 0.37 / 0.25 / 0; Paths
0.12 / 0.16 / 0.34 / 0.18 / 0. Chrome's canvas raster is in the GPU process
(its GPU main thread), under 0.7 ms of CPU a frame for 1000 arcs, against
~20 ms of tiny-skia per MotionMark frame here.

### What this says about A, B, C, D

- Reads are rare where the time is (MotionMark canvas: none) and constant
  where they happen (Images: every frame): every option needs a CPU mode for
  read-heavy canvases, and none pays readback on the canvas subtests.
- The raster cost is tiny-skia's supersampled AA, 15-74 µs a draw; what any
  design keeps on the CPU is under 2 µs a draw. GPU options (A, B, C) can take
  most of the 59/33/28% raster share; D (CPU analytic) has to be a serious
  rasterizer, not the sketch, to win on fills.
- On edges, a GPU canvas lands where Chrome's own GPU canvas does: as far from
  the software-Chrome pins as tiny-skia is now. A's split pictures (GPU on
  screen, tiny-skia for reads) would differ from each other by about that
  much at edges, not more.
