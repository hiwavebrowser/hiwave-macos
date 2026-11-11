# Incremental layout: plan for PR 5 of INCREMENTAL_RESTYLE (2026-11-10)

Status: stages 1 and 2 implemented behind a flag that defaults to `off`;
stages 3 to 6 are plans. This splits PR 5 of `docs/design/INCREMENTAL_RESTYLE.md` (§3.4, §5)
into small PRs. Read §3.4, §4 and §6 of that note first; this file does not
repeat them.

## Why now

A hand test on 2026-11-10 measured live turns on an encyclopedia page spending
650 to 1840 ms in the layout stage on scroll and timer relayouts. The target in
the restyle note is about 10 ms. Every relayout lays the whole tree out from the
root: `lay_out_document` builds a fresh box tree and runs `layout_with_collapse`
over all of it. The `DirtySet` from restyle PR 1 is computed and only logged.

## The flag

`RUSTKIT_INCREMENTAL_LAYOUT=off|verify|on`, read once per process
(`incremental_layout::mode`), with a thread-local override for tests
(`incremental_layout::set_mode_for_test`).

- `off` (default): no retained state, no cost, today's behaviour.
- `verify`: every layout runs the full pass as today and, when the incremental
  path is eligible, the incremental pass too. Every box rect of the two trees is
  compared (content, padding, border and margin edges, to 0.01 px, plus the tree
  shape and each text box's lines: count, break points, width, x offset and
  justification space, which fix where every glyph run is drawn). The page is
  painted from the full tree. Verify does not compare scrollable overflow,
  `scrollbar_overflows` or stacking contexts, so "0 mismatches" is box and
  line equivalence, not full equivalence. Mismatches are counted on the view and logged as shapes only (the
  box's child-index path, box kind and which edge moved), never text or URLs.
  Under `cfg(test)` a mismatch panics, so any engine test run with
  `RUSTKIT_INCREMENTAL_LAYOUT=verify` is an equivalence test.
- `on`: the incremental result is painted when the path is eligible; anything
  else falls back to the full pass, and the fallback reason is counted.

Each stage below adds cases to the eligible set. A stage's flag stays `off`
until its verify run shows 0 mismatches on the engine suite and on the A/B
sites. The relayout log line gains `inc_layout=<outcome>` when the flag is not
`off`.

## Retained state

Per view, in `ViewLayout::incremental_layout` (not thread-local, so it moves
with the view; restyle note §6):

- the last unlaid box tree (as built, before `set_viewport`), to diff the next
  build against;
- the last laid-out tree: since stage 2, the very tree the view published
  (`incremental_layout::adopt`), taken back without a copy when nothing else
  holds it (the bindings hold theirs weakly; the view drops its own handle
  before the layout). The root post-passes (empty body filling the viewport,
  root covering the viewport) are undone from their recorded inputs and the
  engine's stamps (scroll offsets, sticky rules) cleared, so deltas apply to
  the tree as layout left it. A forced layout's tree is still held by its
  geometry closures and is copied;
- the key it was laid out under: viewport bounds, initial containing block
  width and the font-load generation (`rustkit_text::webfonts::generation()`,
  which moves whenever the face set text is measured with changes: a web
  font finishing its load, or another set installed on the thread). A
  changed key drops the state and takes the full path. Until batch 59 the
  key held the partition's face count, read outside the build, where it was
  always the opaque partition's (0): a font landing after the first layout
  left the retained tree's stale text measurements standing (cnn, 2963
  mismatches in one `font_loaded` relayout).

The state is dropped with the document. Its cost is one extra box tree per
view (the unlaid one, about 1.5 KB per element) and one copy of the fresh
build per full layout; the restyle note's §6 memory risk applies, and only
when the flag is on.

**Skipped builds (stage 2).** A relayout asked for by script writes alone
(causes `script_mutation`, `timer`, `scroll`, `observer_callback`) whose
`DirtySet` is not full and has no node in it changed no style and no box: the
restyle index's contract is that such writes (`data-*`/`aria-*` attributes no
selector reads, writes to disconnected nodes) are read by nothing in the
build. With the flag `on` the build is skipped and the published tree stands
(`skipped_build`). Under `verify` the build runs and must diff as unchanged; a
hint that was wrong is counted (`skip_violations`) and panics under
`cfg(test)`. Skipping the build for writes that do change boxes needs restyle
PR 3 (subtree build and splice) and is not attempted here.

## What changed: a box-tree diff, not the DirtySet (stage 1)

Stage 1 decides what changed by diffing the new unlaid tree against the
retained one, box by box, on every layout input the box carries (an exhaustive
destructure of `LayoutBox`, so a new field fails to compile until it is
classified as input or output). This is independent of the restyle
`DirtySet`, so it is correct whatever the invalidation index misses, and it
costs a walk of the tree with no layout. When restyle PR 3 lands (subtree box
reuse), the diff is replaced by the spliced roots the build already knows, and
this walk goes.

The diff descends while exactly one child subtree differs and that child is a
block box whose own inputs are equal. Where it stops is the changed box B: its
children changed (count, or more than one, or a child whose own inputs
changed). No difference at all reuses the retained laid-out tree as is.

## How dirty bits propagate (§3.4)

| Box that changed | What is dirty | Stage |
|---|---|---|
| in-flow block, height change | B re-laid; parent's later in-flow siblings move by the delta; parent's auto height grows by it; repeat at each ancestor to the root | 1 |
| in-flow block whose margins collapse through its top or bottom edge, or that collapses through itself (empty) | the parent re-laid from B on, and up while the parent's edge is open (§3.4's "parent's parent" rule, made exact by re-running the parent's child loop from B with the margin context saved at B) | 3 |
| out-of-flow box (absolute) | its containing block's positioned descendants re-anchored; nothing in flow moves | 4 |
| out-of-flow box (fixed) | the viewport's fixed boxes re-anchored | 4 |
| any in-flow change under a containing block of an absolute box with `bottom`, a percentage offset or height, or a static position after B | that absolute box re-anchored after the in-flow delta | 4 |
| float, or a block beside a float | the float's block formatting context root re-laid from the first box the float reaches; floats after B in the same context move by the delta | 5 |
| flex item | the flex container re-laid (line breaking and free space are global to it); its own height delta then propagates as an in-flow block | 6 |
| grid item | the grid container re-laid; then as above | 6 |
| relayout boundary (fixed `width` and `height`, `overflow` not visible) | propagation stops at the boundary; nothing outside moves | 2 |

Layout modes in order: block and inline in normal flow (stages 1 to 3), then
out-of-flow (4), floats (5), flex then grid (6). Tables, multi-column,
`-webkit-box`, writing modes and container-size queries stay on the full path
until a stage names them.

## Stages (one PR each, each behind the same flag)

1. **Harness and the simplest case (this PR).** The flag, retained state,
   the box-tree diff, verify mode with the rect comparison, the visit counter
   (`rustkit_layout::layout_visits`), and in-place relayout of one block box B
   whose own content changed, when all of these hold (anything else falls back
   and counts its reason):
   - B is a block box (`display: block`, list items included) in normal flow:
     `position: static`, no float, no `clear`, no percentage in its
     `height`/`min-height`/`max-height`, `overflow` visible;
   - no margin collapses across B's edges: each edge is closed by padding or a
     border, or the child next to it (before and after the change) is
     inline-level; and B is not empty before or after (no collapse-through);
   - B's subtree, before and after, holds no float, no `clear`, no box with a
     `position` other than `static`, and no percentage height;
   - every ancestor is a plain block (`display: block`, not a table, flex,
     grid, multi-column or `-webkit-box` container), in flow, `height`,
     `min-height` and `max-height` not set, `overflow` visible, no
     transform, filter or `will-change`/`contain` that holds fixed boxes, no
     size container, horizontal writing mode;
   - no float placed before B reaches below B's top edge;
   - every absolute box outside B has a positioned ancestor off the path from
     the root to B (so its containing block does not grow or move unless the
     whole of it moves); no fixed box comes after B;
   - no styled-scrollbar pass is pending for the view (no bar marks, no root
     bar), and the key (viewport, containing block width, fonts) is unchanged.

   B is laid out alone at its old position, against a margin context that
   says its top margin was already adjoined, so it lands where the full pass
   put it. Its border-box bottom delta then moves every later in-flow sibling
   subtree and grows every ancestor, up to the root, whose post-passes run
   again. Layout visits: B's subtree plus nothing; boxes moved: those after
   B.
2. **Hoisting, boundaries, several changed boxes (implemented).**
   - Several changed boxes in one layout: the diff collects every box whose
     children changed; nested ones fold into the outer one; each is laid out
     in reverse document order, so an earlier delta moves the later roots
     already laid out.
   - Hoisting: a changed box that cannot be laid out alone (not block-level,
     margins collapsing through an edge, a float or `clear` under a box that
     does not contain them, a fixed box under it) moves up to its parent,
     and so does one under an ancestor that does not just grow (flex, grid,
     table, columns, a float, an absolute box, a size container, a
     percentage or keyword min/max height, an `aspect-ratio` with an auto
     height, whose height comes from its width and grows with its content
     only past the ratio height). A relayout root may be a flex or
     grid container: it is laid out whole, and only its own height delta
     moves what follows. A root that would be the body (or the root box)
     falls back: the full layout is as cheap.
   - Boundaries: an in-flow block ancestor with a definite height (lengths,
     or a percentage of a definite parent) stops the delta. Its later
     children move; it, its ancestors and what follows it do not.
   - Ancestors with `min-height`/`max-height` lengths grow when the old and
     new content heights are both on the unclamped side; otherwise the
     layout falls back (`ancestor_min_max`). Ancestors with non-visible
     overflow grow like any block (a styled scrollbar is still
     `scrollbar_pass`).
   - Out-of-flow boxes anchor to their parent's padding box in RustKit
     (`reanchor_absolute_children`), so the check is now local: no absolute
     or fixed box may be a child of a growing ancestor or follow the path
     in a box whose later children move, and no fixed box may move. An
     absolute box under the relayout root is laid out with it. Relative
     offsets under the root are applied again after its layout.
   - The skipped build and the published-tree reuse above.
3. **Margin collapsing.** Save each block's incoming `MarginCollapseContext`
   and cursor on the laid-out box (output fields), so a parent's child loop
   can be resumed at B instead of B being laid out alone. That lifts the
   closed-edge and non-empty conditions: the parent re-runs from B to its end
   (later siblings that are clean are moved, not re-laid, once the loop's
   cursor and pending margin match the old ones at that sibling).
4. **Out-of-flow boxes.** Record each absolute box's containing block on the
   laid-out tree (`containing_block_index` is there and unused). After the
   in-flow pass, re-anchor the absolute boxes whose containing block is on
   the dirty path or whose static position moved; fixed boxes against the
   viewport.
5. **Floats.** Re-lay the block formatting context root's children from the
   first box a changed float can reach (the float's top), with the float
   context rebuilt from the floats before it, which are kept on the laid-out
   tree. A float after B moves with the delta.
6. **Flex, then grid.** A change inside a flex item re-lays the container
   (its algorithm is global), and the container's height delta propagates as
   in stage 1. Grid follows the same shape.

Then the DirtySet replaces the diff (after restyle PR 3), and the soak (50k
randomized sequences, restyle note §4 test 2) runs before the flag defaults to
`on`.

## Fallbacks (always correct, restyle note I4)

Full layout, with the reason counted on the view (`incremental_layout::Stats`)
and shown on the relayout line. Since stage 2 a reason is the last one met
while hoisting, when the root would have reached the body: `no_retained`,
`key_changed`, `laid_not_kept`, `scrollbar_pass`, `root_changed`, `root_box`,
`body_box` (the change is in the body's own children), `changed_not_block`,
`changed_position`, `changed_float`, `changed_clear`, `changed_percent_height`,
`subtree_fixed`, `subtree_float`, `margin_collapse_top`,
`margin_collapse_bottom`, `collapse_through`, `ancestor_not_block`,
`ancestor_flex`, `ancestor_grid`, `ancestor_table`, `ancestor_columns`,
`ancestor_container`, `ancestor_out_of_flow`, `ancestor_sized`,
`ancestor_aspect_ratio`,
`ancestor_min_max`, `float_beside`, `abspos_on_path`, `relative_percent`,
`fixed_after`, `geometry_moved`.

## What the batch 59 A/B showed (2026-11-11)

The 19-site A/B in verify mode, stage 1 on develop: no relayout on any real
site took the incremental path. The fallbacks, by count: `unchanged` 45 (the
retained tree stood; one of these was the font-key bug above),
`no_retained` 19 (each page's first layout), `subtree_positioned` 13,
`subtree_out_of_flow` 10, `changed_percent_height` 9, `changed_not_block` 8.
Real pages put positioned and out-of-flow boxes inside the boxes that
change, which stage 1 refuses and stage 2 handles only when they lay out
inside the relayout root; stage 4 (out-of-flow re-anchoring) is the one
that opens real sites, and `changed_percent_height` and
`changed_not_block` (inline-level and table boxes changing) come after.
Stage 2's own counts on these sites come from the next batch A/B.

## Tests

- 6000-element article, a timer rewrites one paragraph's text so it wraps to
  more lines: with the flag `on`, layout visits are the paragraph's subtree
  (single digits) where the full pass visits every box, and the painted tree
  equals the full pass's (verify mode, 0 mismatches).
- The websuite cases and the `fixtures/` pages, each mutated paragraph by
  paragraph (text grown and shrunk), in verify mode: 0 mismatches, with the
  eligible and fallback counts printed.
- Fallback pins: a change next to a float, inside an absolute box, under an
  absolute box anchored to the path, with a margin collapsing through the
  changed box's edge, and inside a flex item each take the full path with
  their reason.
- Any engine test filter run with `RUSTKIT_INCREMENTAL_LAYOUT=verify` panics on
  a mismatch.

## Timing

Stage 1 removes layout from the eligible relayouts; the build (restyle PRs 2
and 3), the diff walk and the result clone remain O(n). The relayout line's
stage split (`relayout_cause::take_stage_ms`) shows the layout stage alone.
The live encyclopedia figures are re-measured by the batch A/B with the flag
in verify, then on, once the stages that lift the abspos and float fallbacks
land: the page's absolute and floated boxes are expected to keep stage 1 from
applying to most of its relayouts.
