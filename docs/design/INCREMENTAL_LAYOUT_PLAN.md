# Incremental layout: plan for PR 5 of INCREMENTAL_RESTYLE (2026-11-10)

Status: stage 1 implemented behind a flag that defaults to `off`; stages 2 to 6
are plans. This splits PR 5 of `docs/design/INCREMENTAL_RESTYLE.md` (§3.4, §5)
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
  shape and each text box's line count and widths). The page is painted from the
  full tree. Mismatches are counted on the view and logged as shapes only (the
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
- the last laid-out tree, before the root post-passes (empty body filling the
  viewport, root covering the viewport), so deltas apply to raw heights;
- the key it was laid out under: viewport bounds, initial containing block
  width and web font count. A changed key drops the state and takes the full
  path.

The state is dropped with the document. Its cost is two box trees per view
(about 2 x 1.5 KB per element) and one clone of the result per incremental
layout; the restyle note's §6 memory risk applies, and only when the flag is on.

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
2. **Relayout boundaries and several changed boxes.** Stop propagation at a
   box whose size cannot depend on content (§3.4 list). Allow several changed
   boxes, each relaid in document order with the deltas applied in between.
   Allow ancestors with `min-height`/`max-height` (re-clamp at each level) and
   with non-visible overflow (re-run that scroller's overflow mark).
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

Full layout, with the reason counted on the view (`IncrementalLayoutStats`)
and shown on the relayout line: `no_retained`, `key_changed`,
`scrollbar_pass`, `root_changed`, `root_box` (the change is at the root),
`changed_not_block`, `changed_position`, `changed_float`, `changed_clear`,
`changed_percent_height`, `changed_overflow`, `subtree_out_of_flow`,
`subtree_float`, `subtree_positioned`, `subtree_percent_height`,
`margin_collapse_top`, `margin_collapse_bottom`, `collapse_through`,
`ancestor_not_block`, `ancestor_sized`, `ancestor_overflow`,
`ancestor_holds_fixed`, `ancestor_out_of_flow`, `float_beside`,
`abspos_on_path`, `fixed_after`, `geometry_moved`.

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
