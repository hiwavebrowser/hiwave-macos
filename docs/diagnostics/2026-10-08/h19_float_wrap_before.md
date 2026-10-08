# H19 float wrap: BEFORE evidence, 5 float shapes

- **Finding:** Atlas H19 FLOAT WRAP (Null `fe92c2103d55`). On en.wikipedia.org/wiki/Web_browser the right-floated thumbnail sits at the right edge, but the paragraph text runs full width underneath it. Line boxes beside a float are not shortened.
- **Measured:** develop at **`6f6496cd`** (Merge PR #614), before any fix. The fix is being written on `cloud/w6-a-float-wrap`; its PR title starts `fix(layout): line boxes beside a float are shortened (H19`.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64), headless, through Playwright. Viewport 800x600, DPR 1, `--force-color-profile=srgb --use-angle=swiftshader --disable-lcd-text --disable-font-subpixel-positioning`.
- **Ours:** RustKit `parity-capture --html-file <page> --width 800 --height 600 --dump-layout --dump-display-list`, built from `6f6496cd` with `cargo build --profile parity -p parity-capture`.
- **Pages** (10 to 12 lines each, `font: 16px/20px Menlo, monospace`, `body { margin: 8px; width: 784px }`, `p { margin: 0 0 10px }`):
  - [`float_shape_a_figure_right.html`](float_shape_a_figure_right.html): `div > figure[float:right] + p + p`, the Wikipedia thumbnail shape. The figure is 220x180 with `margin: 0 0 8px 16px`.
  - [`float_shape_b_infobox_table.html`](float_shape_b_infobox_table.html): `table.infobox`, `float: right; width: 260px`, 4 rows, with two paragraphs beside it.
  - [`float_shape_c_image_left.html`](float_shape_c_image_left.html): `<img width=150 height=120 style="float:left">` (a data-URI SVG) with one paragraph to its right.
  - [`float_shape_d_clear_both.html`](float_shape_d_clear_both.html): a 200x160 `float: left` div, a short paragraph beside it, then a `clear: both` block.
  - [`float_shape_e_overflow_hidden.html`](float_shape_e_overflow_hidden.html): a 200x160 `float: left` div, then an `overflow: hidden` block (a new BFC that should sit beside the float, narrowed).

## How each side was measured

**Chromium.** For each `p[id]`: `Range.selectNodeContents(p).getClientRects()`, rects grouped by `top`. Per line, x = the leftmost rect's left, and width = the rightmost right minus that x. This is the text extent of the line. Chromium leaves the hanging trailing space out. The float, the cleared block and the BFC use `getBoundingClientRect()` (border box).

**RustKit.** `--dump-layout` gives one text node per paragraph (the whole block rect), not one per line. So per-line positions come from `--dump-display-list`: each `op: "text"` run is one line, with x = `x` and width = the sum of `advances`, trailing spaces trimmed to match Chromium. A run belongs to the `<p>` whose layout box contains its `y`. Only 16px runs are counted, which drops the infobox's 12px cell text. The float, cleared-block and BFC rects are `border_box` from `--dump-layout`.

**Overlap** = any line rect (x, y, width, 19px tall) of any measured paragraph intersects the float's border box.

Menlo advances are 9.633px in both engines, so where the two sides break a line at the same word the widths agree to 0.01px (shape e). Coordinates are CSS px, origin top-left of the viewport.

## Results

| Shape | Chromium line 1/2/3 x / width | RustKit line 1/2/3 x / width | Float rect Chromium | Float rect RustKit | Overlaps text Chromium / RustKit | Extra (Chromium vs RustKit) | Verdict |
|---|---|---|---|---|---|---|---|
| (a) figure float:right + p + p | 8 / 529.81<br>8 / 472.02<br>8 / 491.28 | 8 / 751.36<br>8 / 751.36<br>8 / 712.83 | 572,8 220x180 | 572,8 220x180 | no / yes | - | **mismatch** |
| (b) table.infobox float:right | 8 / 433.48<br>8 / 462.38<br>8 / 491.28 | 8 / 751.36<br>8 / 751.36<br>8 / 712.83 | 532,8 260x145 | 530,8 262x122 | no / yes | - | **mismatch** |
| (c) img float:left | 174 / 597.23<br>174 / 606.88<br>174 / 616.5 | 8 / 751.36<br>8 / 751.36<br>8 / 712.83 | 8,8 150x120 | 8,8 150x120 | no / no | p1 line 1 y: 8 vs 141.15 | **mismatch** |
| (d) float + clear:both block | 224 / 529.81<br>224 / 558.7<br>224 / 558.7 | 8 / 751.36<br>8 / 751.36<br>8 / 712.83 | 8,8 200x160 | 8,8 200x160 | no / yes | cleared block y: 176 vs 176 | **mismatch** |
| (e) float + overflow:hidden BFC | 224 / 529.81<br>224 / 558.7<br>224 / 558.7 | 224 / 529.8<br>224 / 558.7<br>224 / 558.7 | 8,8 200x160 | 8,8 200x160 | no / no | BFC x / width: 224 / 568 vs 224 / 568 | **match** |

In Chromium, line boxes beside the float have room for 548px in (a) (572 − 16 − 8), 508px in (b), 618px in (c), and 568px in (d) and (e). In RustKit, every paragraph line in (a) to (d) gets 784px, the full containing-block width.

## What the numbers say

- **(a) and (b), the H19 bug.** RustKit lays out every line at x=8 across the full 784px, so lines run 751px wide under the 220px figure and the 262px infobox. p1 wraps in 8 lines, where Chromium needs 11 (a) or 10 (b). In the RustKit frame the float is painted over the text it overlaps. In (a), RustKit's p2 starts at y=178, still beside the float, which ends at y=188.
- **(c) is a different failure.** A floated `<img>` is not floated at all. RustKit gives it in-flow space: p1 starts at **y=141, below the image**, full width. Chromium starts p1 at y=8, beside the image, at x=174. The text doesn't overlap the image, but only because it sits below it. A fix that only shortens line boxes won't move this shape; the img first has to become a float.
- **(d)** Lines beside the float are full width and overlap it (x=8, where Chromium gives 224). **Clearance is right:** the `clear: both` block starts at y=176 in both engines (float bottom 168 + 8px margin).
- **(e) already matches.** The `overflow: hidden` block is placed beside the float at x=224, 568px wide, and all 10 lines match Chromium (x and width). So BFC placement beside a float works on develop. Only line boxes inside the float's own block formatting context aren't shortened.

Side notes, not part of H19:
- In (b), the infobox is 262x122 in RustKit and 260x145 in Chromium. RustKit adds the 1px borders outside `width: 260px` and doesn't apply `border-spacing: 3px`.
- In RustKit the float paints after (over) the paragraph text. Chromium paints floats before inline content. That doesn't show on these pages, because Chromium never puts text under the float.

## Reuse

Handtest and Iris will reuse these five pages. The fix PR (`cloud/w6-a-float-wrap`, title starting `fix(layout): line boxes beside a float are shortened (H19`) should be re-measured the same way on the same pages, and its AFTER numbers compared against the table above. Expected after the fix: (a), (b) and (d) match Chromium's x and widths with no overlap; (e) stays matched; (c) may still differ until a floated `<img>` is handled as a float. The cloud session's own `float_wrap_wikipedia_figure.html` and `float_wrap_infobox_table.html` are separate pages and are not measured here. No live-site screenshots are committed.
