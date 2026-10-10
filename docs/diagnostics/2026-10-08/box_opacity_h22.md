# H22: `opacity` on boxes that are not images

- **Page:** [`box_opacity_h22.html`](box_opacity_h22.html), viewed at **800x600**, DPR 1. Seventeen cells, each 100x60; row 2 sits on a black band.
- **Oracle:** pinned Chromium through `tools/parity_oracle/capture_url.mjs` (the same launch as every oracle capture).
- **Ours:** `parity-capture --profile parity --html-file`, develop `2616ec28` and the fix.
- **Companion:** [`google_searchbox_conic_h22.md`](google_searchbox_conic_h22.md), the live shape this came from.

Pixels at a point in each cell (r,g,b):

| cell | what it is | point | Chromium | develop `2616ec28` | per-command fade | group layer |
|---|---|---|---|---|---|---|
| b | red at .5 on white | 170,40 | 255,128,128 | 255,0,0 | 255,128,128 | 255,128,128 |
| c | red at 0 | 280,40 | 255,255,255 | 255,0,0 | 255,255,255 | 255,255,255 |
| d | red at .25 | 350,40 | 255,191,191 | 255,0,0 | 255,191,191 | 255,191,191 |
| d | blue at .5 inside red at .25 | 390,40 | 223,191,223 | 0,0,255 | 223,167,199 | 223,191,223 |
| e | 10px blue border at .5, corner | 452,12 | 128,128,255 | 0,0,255 | 64,64,255 | 128,128,255 |
| e | the same, side | 455,40 | 128,128,255 | 0,0,255 | 128,128,255 | 128,128,255 |
| f | blue child of a box at 0 | 610,40 | 255,255,255 | 0,0,255 | 255,255,255 | 255,255,255 |
| g | gradient background at .5 | 720,40 | 255,128,128 | 255,0,0 | 255,128,128 | 255,128,128 |
| h | red at .5 on black | 60,140 | 128,0,0 | 255,0,0 | 128,0,0 | 128,0,0 |
| i | white, rounded, at .5 on black | 170,140 | 128,128,128 | 255,255,255 | 128,128,128 | 128,128,128 |
| j | rgba white .5 at opacity .5 | 280,140 | 64,64,64 | 128,128,128 | 64,64,64 | 64,64,64 |
| k | blue child over red, clipped, at .5 | 420,140 | 0,0,128 | 0,0,255 | 64,0,128 | 0,0,128 |
| l | white at .5, translated 20px | 520,140 | 128,128,128 | 255,255,255 | 128,128,128 | 128,128,128 |
| o | inline box, red background, at .5 | 250,240 | 255,128,128 | 255,0,0 | 255,128,128 | 255,128,128 |
| p | blue child covering red, group at .5 | 500,240 | 128,128,255 | 0,0,255 | 128,64,191 | 128,128,255 |
| q | blue child inside red, group at .5 | 610,240 | 128,128,255 | 0,0,255 | 128,64,191 | 128,128,255 |

Whole frame, pixels more than 8 apart from Chromium on any channel: develop **15.36%**, per-command fade 2.59%, group layer **0.32%**.

The "per-command fade" column is the first commit of the fix (each paint command faded by itself). It is kept as the fallback for a display list with more than 256 opacity scopes and for the backdrop-blur path. It is wrong exactly where a group's paint overlaps itself: rows d, e (corner), k, p, q.

What is left in the 0.32% (1,533 px), by cell: the rounded corners of cell i (252 px), the glyphs of cell m (303 px) and the glyphs and inline box of cell o (978 px). These were not separated from the edge and glyph raster differences the same shapes have at `opacity: 1`.

Not covered by this page: `opacity` below 1 makes a stacking context (paint order against positioned siblings is unchanged by the fix); `opacity` on an inline that wraps over two lines; an animated or transitioned opacity.
