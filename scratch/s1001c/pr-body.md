## What

Atlas queue item 3 (elliptical corners), from the 2026-09-30 outside review §5.

`border-radius` kept one scalar per corner from the parser to the rasterisers. So:

| declaration | develop | this PR (and Chrome) |
|---|---|---|
| `border-radius: 50px / 25px` | the horizontal half as a circle | a 50x25 quarter ellipse per corner |
| `border-top-left-radius: 30px 15px` | declaration dropped, square corner | 30x15 corner |
| `border-radius: 50%` on 200x100 | 50px pill ends | one whole ellipse |
| `border-radius: 40px 40px 0 0` on a 200x40 tab | each radius cut to 20px (half the shorter side) | 40px: nothing overlaps (§5.5) |
| `border-end-end-radius: 12px` | not parsed | bottom-right corner |
| radius 12px, borders 4px left / 10px top, `overflow: hidden` | inner clip radius 2px on both axes | 8px x 2px (§5.2) |

The micro case `rounded-corners` (which has all of the first three forms) goes **1.3221% -> __RC__%** against Chrome 148. Every other campaign case is bit-identical to develop.

## Change

- **rustkit-css:** `CornerRadius { horizontal, vertical }` on the four computed `border-*-radius` fields.
- **rustkit-engine:** the shorthand reads `h / v`, each side of the slash expanded by the 1-4 value rule. Longhands take one or two values. The flow-relative longhands (`border-start-start-radius` ...) map to the physical corners of a horizontal-tb, ltr box. Values are split at top-level whitespace, so `calc(...)` stays whole. A negative radius makes the declaration invalid.
- **rustkit-layout:** `BorderRadius` holds a `CornerRadius { h, v }` per corner. `border_radius_px` resolves `h` against the border box's width and `v` against its height, then `BorderRadius::fitted` applies the overlap reduction of CSS Backgrounds 3 §5.5: one factor `f = min(1, side / sum of the two radii on it)` over the four sides, applied to all eight radii. The overflow clip's inner radius is inset per axis by the border beside it (§5.2), square once a border swallows either axis.
- **rustkit-renderer:** the fill, the border ring, gradient cells and the rounded clip all draw quarter ellipses, and all measure a corner with one function (`ellipse_edge_distance`). The four copies of `min(r, w/2, h/2)` are gone; each rasteriser calls `fitted` on the rect it was given, which is a no-op for radii layout already fitted.
  - The fill paints its interior as horizontal bands split where a corner starts or ends. Before, corners of different radii on one side left an unpainted hole beside the smaller one.
  - Two diagonally opposite corners can overlap once radii are no longer cut to half the box (`100px 0` on 150x100, the leaf in the fixture). A pixel in both corner boxes is painted once, with the smaller coverage.
  - A corner's pixel grid is laid from the box's outer edge. Laid from the corner box's inner corner (as before), a right or bottom corner with a fractional radius (`25%` of 150px) sat half a pixel off the grid and painted a half-transparent notch down the side. Second commit; found by looking at the first build's frame.
  - A rounded clip under a non-uniform scale scales each axis of its radius.
- **Display-list JSON:** the four scalar radius keys stay and carry the horizontal radii; a `vertical` object carries the other axis. Radii in the dump are now the used (fitted) values.

**Circles are unchanged to the bit.** Four equal radii fit to exactly half the shorter side (`side * (r / sum)`, so equal radii give `side * 0.5`), and the edge distance takes the plain `r - sqrt(dx^2 + dy^2)` path when `h == v`. That is why 38 of the 39 campaign captures below are identical.

## Not in this PR

- **Box-shadow corners.** `DisplayCommand::BoxShadow` carries no corner radius at all today (the shadow's hole is the square border box). The queue item says "clip, border and shadow"; shadow corners are new work, not a representation change, and are proposed as the next PR.
- **Rounded clipping of images and glyphs.** Textured quads under a rounded `overflow: hidden` get only the rectangular clip today. Same PR as shadow corners.
- `vw`/`vh` radii still resolve to 0 (`Length::to_px` has no viewport here); logical radii under other writing modes or `direction: rtl`.
- The GPU gradient path (hard-disabled) is fed the horizontal radii and its shader is untouched.
- `scripts/paint_oracle_gate.py::parse_radius` still returns 0 for elliptical and percentage radii, so that gate does not measure the new cases.

## Tests

**Fail first:** the five engine pins below read the display list as text, so they compile on either tree. Inserted into develop f16ad4e's `windows_engine_pins` and run there: **5 failed, 14 passed** (the worktree was restored and clean after). On this branch all pass.

- **Engine** (`windows_engine_pins`): `an_elliptical_radius_reaches_paint_with_both_axes` (replaces `an_elliptical_radius_takes_the_horizontal_half`, which pinned the wrong behaviour and only counted commands), `a_two_value_corner_longhand_rounds_that_corner`, `a_logical_corner_longhand_rounds_its_physical_corner`, `a_percentage_radius_resolves_per_axis`, `overlapping_radii_are_reduced_by_one_factor`. Parser: `border_radius_shorthand_expands_one_to_four_values` (now also `h / v`, two slashes, an empty side, a negative), `a_corner_longhand_takes_one_or_two_radii`.
- **Layout:** `fitting_scales_every_radius_by_one_factor` (including: uniform radii give exactly half the shorter side for awkward fractional sizes), `unequal_borders_give_the_overflow_clip_an_elliptical_radius`.
- **Renderer** (all device-free): `an_elliptical_clip_follows_the_ellipse`, `an_elliptical_corner_loses_its_own_area_to_the_clip`, `top_only_radii_as_tall_as_the_box_are_not_cut_to_half_of_it`, `the_edge_distance_is_exact_for_a_circle_and_close_for_an_ellipse` (half a pixel along the normal reads as half a pixel to within 0.05 all the way round a 60x30 ellipse), `the_fill_and_the_clip_agree_on_an_elliptical_corner`, `a_fractional_corner_is_sampled_on_the_grid_of_its_outer_edge`, `opposite_corners_that_share_the_middle_both_cut_it`, `a_gradient_cell_is_tested_against_the_ellipse`, `a_clip_scaled_unevenly_scales_each_axis_of_its_radius`.

__SUITES__

## Campaign receipt

Arms: develop **f16ad4e** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first (the shared target judges freshness by mtime).

```
__SUMMARY__
```

__RATCHET__

<details><summary>Per-case diff_pct (all 26), develop f16ad4e vs fix __HEAD__</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct (builtins 5 and micro 13)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real sites

__REALSITE__

large-diff: one representation change through four crates (computed style, used values, display list, four rasterisers); about half of the diff is tests.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
