# LAYOUT-PER-FRAME: measurement note (no fix)

MotionMark's Multiply on the Mac rebuilt the whole layout tree every frame: 72 ms for 11,176
elements, for 2-12 inline style writes a frame, so 13.5 frames per second. This note measures the
same page shape on the engine, pins Chrome on it, and says what invalidates the whole tree and what
an incremental path needs. It proposes PRs; it changes no engine code.

## The page

`layout_per_frame_probe.html` (Chrome) and `layout_per_frame_tests::layout_per_frame_multiply_shape`
(engine, ignored, a measurement):
- 11,176 absolutely positioned 12x12 tiles in a 1280x720 positioned stage with `overflow: hidden`,
  98 to a row, Multiply's way.
- An animation-frame loop that writes on 6 tiles a frame.
- Three arms:
  - `transform` + `background-color` (Multiply's writes);
  - `left` + `top`;
  - a `data-*` attribute that no selector reads.

## Chrome (CfT 148.0.7778.216, headless, 3 runs per arm, `layout_per_frame_probe_cft148.json`)

| arm | frames in 3000 ms | median gap | max gap |
|---|---|---|---|
| transform + background-color | 177-178 | 16.7 ms | 66.7 ms |
| left + top | 178-179 | 16.7 ms | 50-66.8 ms |
| data attribute | 179-180 | 16.7 ms | 33.3-50 ms |

Chrome keeps the frame rate on every arm (the one long gap per run is early in the run).

## Engine (Windows, parity profile, develop ce197324, 20 frames per arm after 3 to settle)

| arm | ms per frame (live turn) | relayouts | build | layout | relayout total | tree reused / layout skipped / paint-only |
|---|---|---|---|---|---|---|
| transform + background-color | 122.2 | 20 of 20 | 94.2 | 12.5 | 118.9 | 0 / 0 / 0 |
| left + top | 112.4 | 20 | 86.0 | 11.6 | 109.3 | 0 / 0 / 0 |
| data attribute | 115.7 | 20 | 91.0 | 11.5 | 114.4 | 0 / 0 / 0 |
| transform + background-color, `RUSTKIT_INCREMENTAL_LAYOUT=on` | 144.5 | 20 | 99.7 | 15.9 | 141.1 | 0 / 0 / 0 |
| left + top, incremental on | 128.5 | 20 | 87.7 | 15.7 | 125.3 | 0 / 0 / 0 |
| transform + background-color, 1,000 tiles | 13.7 | 20 | 9.7 | 0.9 | 11.7 | 0 / 0 / 0 |

(Windows' figures are ~1.6x the Mac's 72 ms; the split is what matters.)

**What the numbers say:**
- **The build is ~78% of a frame:** the cascade and box tree for all 11,176 elements, about 8.4 µs an element. It scales with the document, not with the 6 written tiles (1,000 tiles: 9.7 ms).
- **The layout stage is ~10%**, about 12 ms.
- **The remaining ~10%** is the display list, the geometry snapshot and the render, about 12 ms.
- **A write no selector reads still costs a full frame.** The `data-*` arm is as slow as the others.
- **Incremental layout does not engage**, and on costs more: it adds its diff and its fallback is the full path.

## What invalidates the whole tree

1. **Every relayout runs the full build.** `Engine::relayout` → `relayout_uncounted` →
   `lay_out_document`, the cascade and box tree for every element. That includes a relayout whose
   only cause is one inline style write.
2. **The precise dirty set is computed but only logged.**
   - `restyle_invalidation::DirtySet::compute` turns an inline style write into `STYLE | PAINT` on the written element. The path is `subtree`, not `full`.
   - But the relayout uses it only for the `would_path=` log field (restyle PR 1, "logged only").
3. **The skipped build (INC5 stage 2) needs an empty dirty set and the incremental flag on.**
   - A style write marks its node, so it never qualifies.
   - With the flag off (the default), even the `data-*` write that marks nothing is a full build.
4. **The only short path is the pointer one.** `Engine::repaint_restyled` builds only the elements whose `:hover`/`:active`/`:focus` changed, keeps the laid-out tree when the change is paint (`restyle_needs_layout`, `same_shape`), copies the paint onto it (`copy_changed_paint`) and rebuilds the display list.
   - It is reached only from pointer and focus changes, never from script writes.
5. **The paint-only property set excludes `transform` and `opacity`.**
   - `ComputedStyle::differs_only_in_paint` and `copy_paint_only` cover colours, borders, text decoration, shadows, outline and transitions.
   - So even the pointer path would fall back for Multiply's `transform`.
   - The engine's CSS-animation paint path (LAT2) moves `transform`/`opacity` on its own; script writes have no such path.
6. **`left`/`top` on an absolutely positioned box is a layout change.** Incremental layout's out-of-flow stage (stage 4 in `docs/design/INCREMENTAL_LAYOUT_PLAN.md`) is a plan, so even with the flag on it falls back.

## What an incremental path needs (proposed, one PR each, Chrome-pinned, with cost tests)

1. **Script style writes take the restyle short path (LPF1).** When the frame's `DirtySet` is not full and every node's bits are `STYLE | PAINT`:
   - build only those elements, using `repaint_restyled`'s `RESTYLE_BUILD` path walk with the dirty nodes as roots;
   - compare each built box with the laid-out box's own style (the DOM no longer has the old state, so "before" is the laid box);
   - patch with `copy_changed_paint` and rebuild the display list;
   - fall back on anything else, with a named reason on the relayout line, as the pointer path does.

   Expected: the build goes from O(document) to O(written elements); a frame becomes display list + snapshot + render (~12 ms here). That is the largest single win and covers `background-color` alone today.
2. **`transform` and `opacity` as paint when they do not change what the box establishes (LPF2).**
   - A change from one non-`none` transform to another, or between two opacities below 1, keeps the box's stacking context and its containing block for fixed and absolute descendants.
   - `none` ↔ a transform, or 1 ↔ below 1, does not, and falls back.
   - Scrollable overflow moves with a transform (Chrome counts transformed boxes in scrollable overflow): the patch must update the scroller's extent, or fall back when the box is in a scroller whose overflow is not clipped. Multiply's stage clips.
   - Pin Chrome on the overflow case first.

   With LPF1 this covers Multiply.
3. **Writes that restyle nothing skip the build by default (LPF3).** INC5 stage 2's skip is correct today behind the flag. Lift that one case to default, with the verify run's numbers, so the `data-*` arm costs nothing.
4. **`left`/`top` on an absolutely positioned box (LPF4)** = incremental layout stage 4 (re-anchor the out-of-flow box against its recorded containing block), on top of LPF1's restyle. This is not Multiply's write, but it is common in animation code.
5. **Then the display list.** Once 1 and 2 land, the frame is mostly the display list for 11,176 boxes plus the geometry snapshot. Measure it then. A patch-in-place display list is a later row, and only if it shows.

**Order:** LPF1, then LPF2 (Multiply), then LPF3, then LPF4. Each PR needs:
- a red test on the relayout stats (the build skipped, `trees_reused` or a new counter);
- a cost test that counts elements built, not time;
- this measurement re-run as its receipt.

## Not done here

- **The Mac:** the 72 ms is the Mac's; these numbers are Windows'.
- **MotionMark itself.**
- **The display list and snapshot share,** measured separately.
- **A headed Chrome run** (FRAMEPIN-HEADED).
