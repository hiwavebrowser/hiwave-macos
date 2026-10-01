## What

The second half of Atlas queue item 3 (2026-09-30 outside review §5), the part #401 left out and named: **shadow corners** and **rounded clipping of images and glyphs**. Plus the bug the second one ran into: **images were drawn without blending**.

| page has | develop | this PR (and Chrome) |
|---|---|---|
| `border-radius: 20px; box-shadow: 0 0 0 6px blue` (a focus ring) | a square frame around the rounded box | a ring with 26px corners |
| `border-radius: 45px; box-shadow: 0 6px 16px rgba(0,0,0,.45)` | a square halo with square corners showing past the pill | the halo follows the pill |
| a transparent rounded box with a spread shadow | square hole | the hole is the box's own curve |
| `<img>` in `border-radius: 50%; overflow: hidden` (an avatar) | the whole square image | a disc |
| `<img>` or a background tile in a rounded `overflow: hidden` card | square corners | cut at the card's corners, antialiased |
| a PNG with a transparent surround (most logos) over a colour | the logo in a **black box** | the colour shows through |

Fixture and frames: develop, this PR and Chrome 148 are in **Frames** below.

## Change

**1. Images are composited, not copied (`fix(renderer): composite images source-over`).**
The image batch was drawn with the blit pipeline, whose blend is `REPLACE`: every texel overwrote the pixel under it, alpha included. A transparent texel is `(0, 0, 0, 0)` in most PNGs, so it painted black. `create_image_pipeline` is the same shader with `ALPHA_BLENDING` (decoded images are straight alpha); the colour-glyph pipeline and it now share one constructor. Opaque images paint the same pixels as before. This is in this PR because the rounded clip below antialiases its edge through vertex alpha, which `REPLACE` ignores.

**2. A shadow takes its shape from the box (`fix: box shadows and textured quads follow rounded corners`).**
- `DisplayCommand::BoxShadow` carries `border_radius`, the box's used border-box radii (the same `border_radius_px` the background, border and clip use). The display-list JSON gains `border_radius` on `box_shadow`.
- `CornerRadius::spread` / `BorderRadius::spread`: the shadow's radii are the box's plus the spread; a radius smaller than the spread grows by `spread * (1 + (r/spread - 1)^3)`, and a square corner stays square (CSS Backgrounds 3 §6.1.1). A negative spread shrinks them.
- `rounded_difference_pieces(shape, shape_radius, hole, hole_radius)`: the shadow shape minus the border box, both rounded. The plane is cut along the edges of both rects and their corner boxes; a cell outside every corner box is painted whole or not at all, a cell inside one is painted a pixel at a time with runs of full pixels joined. The hole's coverage is `1 - ` the fill's own corner coverage, so the two meet on the curve.
- The blur is still the stack of expanding layers it was (an approximation, unchanged); each layer's radii grow with it.
- **Two square shapes return `rect_minus` exactly**, so a box without `border-radius` gets the rects it always did.

**3. A rounded clip reaches textured quads.**
- `clip_textured_pieces_under`: the rectangular cut as before (`clip_textured_under`), then the same decomposition colour quads get (`clip_quad_to_rounded`), each piece taking the texels that were under it. Every textured site (glyphs, colour glyphs, `<img>`, background tiles) goes through `Renderer::textured_pieces`.
- A quad whose top and bottom edges are inside every rounded constraint is passed through whole (a rounded rect is convex). Text in a pill sits in the corner band but between the arcs, so it stays one quad per glyph.
- Without a rounded clip on the stack the output is the one piece it always was.

## Not in this PR

- **Inset shadows** do not use the radii yet. (They are also wrong in a bigger way on develop: an unblurred inset shadow fills the whole inner rect instead of the band between the padding edge and the offset shape. `rounded_difference_pieces` is the tool for it; next PR.)
- **A real blur.** The layered approximation is unchanged; only its shape is.
- Rounded clips under a rotation or skew stay rectangular, as for colour quads.
- `border-radius` on the `<img>` itself (no `overflow: hidden` wrapper) is still not applied to the image. Third box of the fixture's second row.
- Gradient text does not go through the clip at all (pre-existing).

## Tests

**Fail first.** The two engine tests were inserted into develop 473a047 and run there: **2 failed, 19 passed** (worktree restored and clean after). `a_shadow_carries_the_corner_radii_of_its_box` reads the display list as text. `image_alpha_rounded_image_clips_and_rounded_shadows_reach_the_frame` serves a page over HTTP, renders it headless and reads pixels; on develop its first probe is `[0, 0, 0]` where the page is `[0, 0, 255]` (the transparent PNG painted black). On this branch both pass.

- **Engine:** the two above. The frame test probes seven pixels: blue through a transparent image; the centre and two cut corners of an image in a circular clip; the top of a circular box's spread ring, the corner of the ring's bounding square, and the inside of the box.
- **Layout:** `a_shadows_corner_radius_grows_with_the_spread`.
- **Renderer** (device-free): `a_rounded_box_gets_a_rounded_ring` (point probes, no pixel painted twice, area within 2% of the difference of the two rounded rects), `an_offset_shadow_shows_through_the_corner_notch_of_its_box`, `blurred_layers_of_a_rounded_shadow_are_rounded`, `a_square_box_gets_exactly_the_old_rects`, `the_hole_and_the_fill_share_the_corner_ramp` (fill + hole = 1 on every pixel of a 24px corner), `an_image_under_a_round_clip_loses_its_corners_and_keeps_its_texels` (every piece's UVs are the quad's at that place; area within 2% of the disc), `a_split_glyph_stays_inside_its_atlas_cell`, `a_glyph_between_the_arcs_of_a_pill_is_not_split`, `a_textured_quad_under_a_square_clip_is_the_one_piece_it_was`, `images_are_blended_not_copied`. The four existing outer-shadow tests pass unchanged apart from the new arguments.

Suites at head __HEAD__: __SUITES__

## Campaign receipt

Arms: develop **473a047** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first (the shared target judges freshness by mtime).

```
__SUMMARY__
```

__RATCHET__

**One case is worse on `diff_pct`:** `card-grid` 1.3068 -> 1.3074, about six pixels of 1,024,000. Its cards have rounded corners and a blurred shadow; the halo's corners are now rounded, and the layered blur approximation lands a few pixels on the other side of the threshold there. Gate B's paint fraction for the same case rises (0.82521 -> 0.82911), as it does on four other cases, and falls on none. Every other case has the same `diff_pct` on both arms.

<details><summary>Per-case diff_pct (all 26), develop 473a047 vs fix __HEAD__</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct (builtins 5 and micro 13)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Frames

__FRAMES__

## Real sites

__REALSITE__

large-diff: about 1,100 lines, of which about 500 are tests; the rest is one geometry function, one clip function, and four call sites re-indented into a loop.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
