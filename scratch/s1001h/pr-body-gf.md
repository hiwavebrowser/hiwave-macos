## What

A grid item that is a flex container did not fill its row, and did not align its items in the row's height.

Phase 9 of grid layout runs the item's flex layout to learn its content height. An auto-height flex container writes that content height over the grid area it was handed, and nothing gave a stretched item its row back unless the row-repair pass (Phase 9.5) happened to change some row. When that pass did stretch the item, its items stayed where the content-height pass had aligned them.

Reduced page (`scratch/s1001h/l0/stretch.html` on the hub branch): a grid row whose height comes from a `height: 100px` sibling, beside flex containers with `align-items: center` holding one 30x20 box. Pinned Chrome 148 against both arms:

```
box                           Chrome 148            develop f657cf2       fix
flex row item                 200 x 100 at y=10     200 x 38              200 x 100
  its 30x20 child             y=50                  y=19                  y=50
flex column item (justify)    200 x 100 at y=10     200 x 38              200 x 100
  its 30x20 child             y=50                  y=19                  y=50
flex item, one line of text   200 x 100 at y=120    200 x 38              200 x 100
  its text                    y=160                 y=129                 y=160
flex item, align-self: start  200 x 38              200 x 38              200 x 38
boxes more than 0.5px off     -                     6 of 11               0 of 11
```

`new_tab` has the second half: its `.shortcut` rows are flex rows in a grid, and the ones sharing a row with a taller shortcut had every child 5px above Chrome's.

## Change

`crates/rustkit-layout/src/grid.rs`, `flex.rs`:

- Phase 9, flex arm: the content height still goes to `real_heights` (what Phase 9.5 sizes rows from). If `align-self` resolves to `stretch` and the height is `auto` (`stretches_to_its_row`), the item keeps the area height Phase 8 gave it.
- Phase 9.8 (new, last): a flex-container item whose used height ended above its content height lays its items out again at that height, through `flex::layout_flex_container_at_used_height`. That is the entry point a stretched flex item that is itself a flex container already takes (#300); the grid never called it.
- An item with `align-self: start` (or any non-stretch alignment), `height: fit-content`, or an explicit height is untouched. An `aspect-ratio` item still gets its height from Phase 9.6 and is then laid out at it.

Cost: one more flex layout for each flex-container grid item that is shorter than its row. Items as tall as their row are not laid out again.

Not in this PR: a grid item that is itself a grid has the same loss of its stretched height (`layout_grid_container` re-derives an auto height from its rows). The page above has no such item; it wants its own test.

## Tests

Four in `grid.rs`:

- `a_flex_grid_item_fills_its_row_and_aligns_in_it`: beside a 100px sibling the flex item is 100 tall and its centred 20px child's top is at 40.
- `a_column_flex_grid_item_justifies_in_its_row`: the same for a column container with `justify-content: center`.
- `a_flex_grid_item_realigns_after_its_row_grows`: two flex items share an auto row that Phase 9.5 grows to 60; the short one fills it and centres its child at 20.
- `a_start_aligned_flex_grid_item_keeps_its_content_height`: the opt-out. This one is a guard and passes on develop.

Fail-first, with the two production changes switched off under the new tests (`scratch/s1001h/failfirst_gf.py`):

__FAILFIRST__

__SUITES__

## Campaign receipt

Arms: develop **__BASE__** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first.

```
__SUMMARY__
```

__RATCHET__

<details><summary>Per-case diff_pct (all 26), develop __BASE__ vs fix __HEAD__</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct (builtins, micro)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real sites

__FRAMES__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
