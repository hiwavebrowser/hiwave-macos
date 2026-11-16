# A box that comes or goes, without a full build (design note, 2026-11-16)

Status: proposal, no code. This is LPF3's "option 2". Grounded in develop `d4455b45` plus #1226
at `4c98f509` (LPF3). `lib.rs` means `crates/rustkit-engine/src/lib.rs`. Code is named by
function rather than line, because the lines move.

**Bar for writing any of it:** the MotionMark Multiply receipt in batch 97. With LPF3, a box
that comes or goes costs one full build. If the receipt shows those frames still hold the
score down, this is the next step. If it does not, this note stays a note.

## 0. What happens today

The script restyle short path (`repaint_script_restyled`) builds only the written elements.
It compares each written root, as built now, with its box in the kept unlaid tree
(`ViewLayout::script_unlaid`):

| written root, kept → fresh | today |
|---|---|
| no box → no box | nothing is painted; skipped |
| box → box | paint is copied onto the laid-out tree (a hit), or a miss names what changed |
| no box → box, box → no box | miss `a box came or went`: full build, full layout |

A DOM insert or removal (`appendChild`, `remove`) never reaches the comparison. The trust
check (`script_restyle_roots`) refuses it, and the relayout is a full build. In the LPF3
fixture `a_tile_appended_or_removed_costs_one_full_build`, develop already takes the short
path on 54 of 60 relayouts. Only the frame that changes the tree pays.

After LPF3, both cases cost exactly that one frame. The full build keeps its tree, and the
next frame hits. So this note is only about the cost of that single frame, which is a whole
build plus a whole layout. On Multiply's shape the build was about 78% of a 112-122 ms frame
(`docs/diagnostics/2026-11-16/layout_per_frame.md`).

## 1. Which boxes qualify

Start with case A: a written element's `display` goes to or from `none`. The DOM is
unchanged, so the trust check already lets the write through, and the build of the written
root is already done. Case B (a DOM insert or removal) is section 4.

A box that comes or goes is local only when nothing else is laid out differently because of
it. That holds for an **absolutely positioned box whose containing block is its parent**,
under these conditions. The first failing condition names the miss, as today.

1. **Out of flow.** `position: absolute`, both in the box as built and in its kept or laid-out
   counterpart. It moves no sibling, and it adds nothing to its parent's auto height.
   `position: fixed` is left out: its containing block is the viewport, and INC5's
   `subtree_ok` already treats fixed boxes as escaping. Floats and in-flow boxes are left out.
2. **The parent is the containing block.** The parent's `position` is not `static`, so the
   box anchors to the parent's padding box (`abspos_containing_block`), which is already laid
   out.
3. **No static position.** On each axis at least one inset is not `auto`. A box with an auto
   inset on an axis takes its static position, which depends on its in-flow siblings and on
   flex placement (`place_static_position_children`). Leave those out.
4. **The parent's box children map one-to-one onto its element children.** No anonymous
   block wraps the children, and there are no text runs or pseudo-element boxes. Then the
   insertion index in the box tree is the count of earlier element siblings that have boxes.
   This rule is what keeps the splice in section 2 simple.
5. **The scroll extent stays the same.** An absolute box contributes to its scroll
   container's scrollable overflow. The new box's border box must lie inside the padding box
   of the nearest ancestor that is not `overflow: visible`, or no such ancestor may exist
   below the viewport and the box must lie inside the viewport's initial extent. Transforms
   are already outside scrollable overflow here (`known_divergence_a_transform_is_not_in_scrollable_overflow`).
   This is checked after the box is laid out; a box outside that area is a miss.
6. **No counter, quote or `:empty` effect.** Showing or hiding a box changes CSS counters,
   and could matter to a sheet whose selectors read box-generation state. Leave out any page
   whose sheets use `counter-increment`, `counter-reset` or `content: counter(...)`. A
   per-sheet flag at extraction is enough.
7. **A transform or opacity does not come with it.** LPF2 already lays out a transform or
   opacity that comes or goes. The new box may have one, but its own stacking context is part
   of laying out the one box, not a change to anything else.

Multiply's tiles meet all seven, if the fixture's shape is the page's. The fixture uses
absolute tiles with `left`/`top` in a positioned, clipped stage. That has not been checked
against the benchmark's own markup, and it is the first thing to check.

## 2. What the kept tree and the laid-out tree need

**The splice.** For a box that comes (kept: none, fresh: a box), insert the fresh box into
the kept unlaid tree under its parent, at the index from rule 4. Lay out a copy of it alone
against the parent's laid-out padding box: the layout entry point for one box, then
`reanchor_absolute`. Insert that copy into the laid-out tree at the same index. For a box
that goes, remove it from both trees at that index. Rule 4 makes the two trees' child lists
correspond, so the index is the same in both. A debug assertion should check that the
neighbours' `node_id`s match.

**Paint order.** Absolute boxes with `z-index: auto` paint in tree order among their
stacking context's positioned descendants. Inserting at the build's index gives the full
build's order. Nothing else needs re-sorting, if the display list is built from the tree
each frame. Today `display_list_for` builds from the laid-out tree each time.

**Element numbers.** A build numbers elements in the order it reaches them
(`attach_identity`, a counter per build), and it stores each element's selector path in
`ElementIdentity`. That path counts same-tag siblings (`nth-of-type`), so it can name a
sibling too. After a box comes or goes, a full build renumbers every later element, and can
change later siblings' selector paths. The short path would keep the old numbers. That
matters for three readers:
  - `LayoutBox::element_id` keys the intrinsic-size cache (`intrinsic_cache`, keyed by
    element id and style pointer) and `layout_memo`.
  - `identity` is the geometry oracle's join key.
  - `adopt_build_ids` assumes the kept tree's numbers are the full build's.

The safe answer is to renumber after the splice. Walk the laid-out tree and the kept tree in
document order, give each element box the number a full build would, and recompute the
selector path of the parent's later same-tag children. That is one walk with no styling and
no layout, which is cheap next to a build, and it makes both trees equal to a full build's
apart from layout. Still to confirm in the red test: does a `display: none` element take a
number? I read `attach_identity` as running only for a box that is built, so it does not.

**Geometry published to script.** `publish_geometry` runs after every short path and reads
the laid-out tree, so `getBoundingClientRect`, `offsetTop` and hit testing see the new box
with no extra step. The element scroll extents are shared through `share_element_scrolls`,
and rule 5 keeps them unchanged.

**The idle and backoff state** (LPF1, LPF3) needs nothing new. A splice is a hit.

## 3. How the cross-check covers it

The cross-check is what makes this safe: every frame is painted with the path on and off,
and the display lists are compared (`cross_check_go`, `tiles_that_come_and_go`). For this
change it should also compare, every frame:

- `elementFromPoint` at the box that came or went, and at one of its neighbours;
- `getBoundingClientRect()` of the shown box, and the stage's `scrollWidth` and
  `scrollHeight` (rule 5);
- each box's `element_id` and `identity` in the laid-out tree, path on against path off
  (the renumbering).

Tests, each red first:

- `a_written_box_that_comes_or_goes_costs_one_full_build` (LPF3) tightens to **no** full
  build: zero of the counted relayouts.
- One test per rule in section 1 that falls back. A static-position tile (rule 3), a tile in
  a non-positioned parent (rule 2), a text node beside the tiles (rule 4), and a tile that
  pokes out of a scroller (rule 5) each still take one full build and paint the same.
- A counter page (rule 6) takes one full build.
- `RUSTKIT_INCREMENTAL_LAYOUT=verify` (INC5's verify mode) is unaffected: this path
  publishes its own tree, and INC5 runs only on full builds.

## 4. Case B: a DOM insert or removal

This builds on case A. The trust check has to let a child-list mutation through, and that
needs a structural-selector check: no rule in the page's sheets uses `:nth-*`, `:first-*`,
`:last-*`, `:only-*`, `:empty`, `:has()` or a sibling combinator in a way that reaches the
parent's children. The restyle invalidation index already answers part of this for pointer
restyles (`PointerCompound::reach`). The inserted subtree is then built as a written root,
and section 2 applies unchanged. A removal also has to drop the node's per-element state:
scroll offsets, focus, the hover chain. This is a bigger step than case A, and the receipt
may not ask for it. In the LPF3 fixture, inserts and removals already cost only their own
frame.

## 5. Order of work, if the receipt asks for it

1. Check Multiply's markup against rules 1-7 (pinned Chrome CfT 148.0.7778.216, page source
   only).
2. Case A behind the existing `RUSTKIT_SCRIPT_RESTYLE` switch: red tests first (section 3),
   then the splice and the renumbering.
3. A Multiply receipt on the Mac at that head: hits, misses by name, and the score.
4. Case B only if a site or a benchmark shows DOM inserts holding a frame rate down.

Not covered here: `position: fixed`; boxes whose static position matters; any in-flow box
(an in-flow box moves its siblings, which is INC5's job, not this path's).
