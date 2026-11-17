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
