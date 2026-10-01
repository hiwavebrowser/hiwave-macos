# Queue item 3 (elliptical corners): map of the scalar representation

From a read-only sweep of develop e4a82f7 + #398's branch on 2026-10-01 (an Explore agent; nothing was
compiled). **Line numbers and "not found" claims are the agent's and were not re-checked. Verify each
before relying on it.**

Headline: radii never reach a live GPU shader. All rounding on the live path is CPU-side per-pixel
quads. The only WGSL that takes radii (`gradient.wgsl`) belongs to a GPU gradient path that is
hard-disabled (`gpu_gradients_enabled` forced false, renderer ~637).

## Parse (crates/rustkit-engine/src/lib.rs, `apply_style_property`)
- `"border-radius"` arm ~6326-6343; ~6336 drops everything after `/`
  (`value.split('/').next()`), then `parse_border_radius_shorthand` (~12207, returns
  `Option<[Length; 4]>`, `None` on `/`).
- Longhands ~6344-6363 call `parse_length` on the whole value, so `border-top-left-radius: 10px 20px`
  is DROPPED today.
- Logical longhands (`border-start-start-radius`): not found.

## Computed style (crates/rustkit-css/src/lib.rs)
- `border_{top_left,top_right,bottom_right,bottom_left}_radius: Length` (~2458-2461). Not inherited.
  No pair type exists; `TransformOrigin { x, y }` (~2181) is the nearest precedent.
- One production reader: `border_radius_px` (rustkit-layout lib.rs ~7054), called from
  `overflow_clip` (~7115), `render_background` (~7162), `render_borders` (~7597).

## Used values (crates/rustkit-layout/src/lib.rs)
- `pub struct BorderRadius { top_left, top_right, bottom_right, bottom_left: f32 }` (~5952),
  `Debug, Clone, Copy, Default`; helpers `uniform`, `is_zero` only. 4 struct literals in the workspace
  (5964, 7062, 7138, renderer 6167); about 30 direct field reads.
- `border_radius_px` resolves every corner against `border_rect.width` and with `to_px` (so `vw`/`vh`
  radii are 0).
- Inner radius: `overflow_clip` ~7131-7143, `r - max(adjacent borders)`.
- No overlap reduction in layout. FOUR copies of a non-spec clamp in the renderer
  (`.min(min(w/2, h/2))`): `draw_rounded_rect` ~2543, `point_in_rounded_rect` ~2610,
  `draw_rounded_border` ~2938, `clamped_radii` ~6313.
- `clip_entry_under` (~6166) scales radii by one `sqrt(|m0*m3|)`.

## Display list
`RoundedRect`, `BackdropFilter`, `LinearGradient`, `RadialGradient`, `ConicGradient`,
`PushClipRounded` (3 sites), `RoundedBorder` carry `BorderRadius`; `Button` carries an unused `f32`.
`BoxShadow` carries NO corner radius (renderer comment ~3118: the hole is the square border box).
`escapable_clips: Vec<(Rect, BorderRadius)>` (~6571).

## Renderer (crates/rustkit-renderer/src/lib.rs)
- `draw_rounded_rect` (~2534) -> `draw_rounded_corner` (~2664): 1px quads, circular SDF.
- `draw_rounded_border` (~2915): outer curve circular; inner curve ALREADY an ellipse
  (`(rad - vw, rad - hw)`, ~2992). The only elliptical math in the tree.
- Clip: `push_clip_rounded` (~5643) -> `clip_entry_under` -> `clip_quad_to_rounded` (~6454) ->
  `rounded_row_span` (~6331, analytic circle). No scissor, no mask. Textured quads (glyphs, images)
  get only the rectangular clip (~6256), so an `<img>` under a rounded `overflow:hidden` is not
  rounded today.
- Gradients test each cell with `point_in_rounded_rect` (~2593).
- Dead GPU path: `pipeline::GradientParams` is bytemuck `Pod`, 80 bytes, four f32 radii, size pinned
  by `test_gradient_params_size` (~7381) and mirrored in `gradient.wgsl` (~77-110). Cheapest safe
  option: keep feeding it the horizontal radii and leave the shader alone.

## JSON dump
`Engine::export_display_list_json` (engine ~9858), inner `fn radius` (~9884) emits four scalars under
`"radius"` / `"border_radius"`. `RoundedBorder`, `BackdropFilter`, `Button` dump as `"unknown"`.
No Python consumer of the radius keys found. `scripts/paint_oracle_gate.py::parse_radius` returns 0
for elliptical/percent radii, so that gate is blind to the new cases.

## Tests that pin today's behaviour
- engine: `border_radius_shorthand_expands_one_to_four_values` (~15586; pins `"50px / 25px" -> None`),
  `an_elliptical_radius_takes_the_horizontal_half` (~21695; count-only, would still pass: needs a
  value assertion).
- layout: ~8923, ~9382, ~9441, ~9754, ~9778 read `radius.top_left` etc.
- renderer: `fn radius(r)` helper (~6751), `a_scaled_clip_scales_its_corner_radius` (~6915),
  `radii_are_clamped_so_opposite_corners_cannot_overlap` (~7214),
  `the_clipped_area_matches_the_rounded_rect_area` (~7134), `test_gradient_params_size` (~7381).
- No percentage-radius test exists.

## Commit sequence (each compiling, no pixel change until 6)
1. Layout only: `PartialEq` + helpers (`shrink`, `scaled`, per-corner `(h, v)` getters); convert the
   literals and ~30 field reads to them.
2. Flip the field type to an `(h, v)` pair per corner with `h == v` everywhere; `is_zero` = either
   axis 0; JSON keeps scalar keys and adds pair keys (or bumps `version`); add a `RoundedBorder` arm.
3. ONE clamp: the spec factor `f = min(1, w/(h_tl+h_tr), w/(h_bl+h_br), ht/(v_tl+v_bl), ht/(v_tr+v_br))`
   in `border_radius_px`; the four renderer clamps become no-ops. Own commit, own campaign run
   (changes pixels only for non-uniform overflowing radii).
4. Computed style: four fields become a `(Length, Length)` pair; longhands take one or two values;
   the shorthand handles `/`. `border_radius_px` still feeds `.h` to both axes.
5. Renderer elliptical math fed circles: `draw_rounded_corner`, `point_in_rounded_rect`,
   `rounded_row_span` (`dx = h * sqrt(1 - (dy/v)^2)`), band limits in `clip_quad_to_rounded`, outer
   curve of `draw_rounded_border`. Tests ~7134/~7214 are the circle guard.
6. Switch on: h against width, v against height; per-axis inset in `overflow_clip`; `clip_entry_under`
   scales h by `|m0|` and v by `|m3|`. Rewrite the horizontal-half test; add percent and overlap tests.
7. Box-shadow corner radius (not carried at all today) is NOT in this list and is part of the queue
   item's wording ("clip, border and shadow"): budget it separately.

## Risks
1. `border-radius: 50%` on every non-square box changes (ellipse instead of today's clamped circle
   corners). Expect campaign cases to move, probably toward Chrome; say so in the receipt.
2. Fill, border, gradient and clip are four separate rasterisers with no device-free tests on three.
   If they drift by a pixel the clip cuts into the paint. Extract pure coverage functions first.
3. `GradientParams` Pod size / WGSL mirror (dead path, no pixel test would catch a mismatch).
