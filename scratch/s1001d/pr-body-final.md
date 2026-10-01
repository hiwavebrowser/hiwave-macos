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

Suites at head fc64b0e: **rustkit-renderer 110/110. rustkit-layout 583/583** (and its 3 + 1 integration tests). **rustkit-engine** with `--features headless`: `windows_engine_pins`, `rounded_paint_frame_tests` and `web_font_format_tests`, 22/22. The first commit's tree alone: rustkit-renderer 101/101. The full engine suite was not run locally (three lanes share this machine, load 13 to 30 all session), so CI is the full-suite evidence.

## Campaign receipt

Arms: develop **473a047** (this branch's base) vs fix **fc64b0e**, both release binaries built in this session with every workspace source touched first (the shared target judges freshness by mtime).

```
all:      develop 2026-10-01T09:23:08 26/26 avg 1.1271 | fix 2026-10-01T10:09:53 26/26 avg 1.1271 | 25/26 identical
builtins: develop 2026-10-01T09:22:05  5/5  avg 1.9072 | fix 2026-10-01T10:05:26  5/5  avg 1.9072 | 5/5 identical
micro:    develop 2026-10-01T09:21:52 13/13 avg 0.6550 | fix 2026-10-01T10:04:35 13/13 avg 0.6550 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: card-grid: geo_fails=0 paint=0.82521 discrete=0
fix: card-grid: geo_fails=0 paint=0.82911 discrete=0
dev: css-selectors: geo_fails=3 paint=0.94178 discrete=0
fix: css-selectors: geo_fails=3 paint=0.94281 discrete=0
dev: flex-positioning: geo_fails=0 paint=0.95651 discrete=0
fix: flex-positioning: geo_fails=0 paint=0.95756 discrete=0
dev: form-elements: geo_fails=47 paint=0.96421 discrete=0
fix: form-elements: geo_fails=47 paint=0.96461 discrete=0
dev: sticky-scroll: geo_fails=16 paint=0.98150 discrete=0
fix: sticky-scroll: geo_fails=16 paint=0.98265 discrete=0
```

**One case is worse on `diff_pct`:** `card-grid` 1.3068 -> 1.3074, about six pixels of 1,024,000. Its cards have rounded corners and a blurred shadow; the halo's corners are now rounded, and the layered blur approximation lands a few pixels on the other side of the threshold there. Gate B's paint fraction for the same case rises (0.82521 -> 0.82911), as it does on four other cases, and falls on none. Every other case has the same `diff_pct` on both arms.

<details><summary>Per-case diff_pct (all 26), develop 473a047 vs fix fc64b0e</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3068 | 1.3074 | moved
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2405 | 3.2405 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5685 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

<details><summary>Per-case diff_pct (builtins 5 and micro 13)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| chrome_rustkit | 1.1234 | 1.1234 |
| new_tab | 1.5685 | 1.5685 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2405 | 3.2405 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

## Frames

One fixture, served over local HTTP, captured by both release binaries and by pinned Chrome 148 at 1280x800. Row 1: six 140x90 boxes with outer shadows (a 6px ring on 20px corners; a 10px offset shadow; a blurred shadow on a pill; an 8px ring on a transparent `60px / 30px` ellipse; a 12px ring on 4px corners; a ring on a square box). Row 2: an image in a `50%` clip, in a 24px card, with `border-radius` on the `<img>` itself, as a background tile under a `40px 0` clip, a transparent PNG over blue, and text in a pill.

Pixels of the fixture's 1280x340 region that differ from Chrome (any channel by more than 8/255):

| arm | pixels | share |
|---|---|---|
| develop 473a047 | 29,910 | 6.873% |
| fix fc64b0e | 9,661 | 2.220% |

Per box, pixels that differ from Chrome:

| box | develop | fix |
|---|---|---|
| 6px ring, 20px corners | 1,016 | 412 |
| 10px offset shadow, 20px corners | 460 | 140 |
| blurred shadow on a pill | 8,839 | 5,594 |
| 8px ring on a transparent ellipse | 3,343 | 950 |
| 12px ring, 4px corners | 188 | 140 |
| ring on a square box | 0 | 0 |
| image in a `50%` clip | 3,256 | 204 |
| image in a 24px card | 556 | 68 |
| `border-radius` on the `<img>` itself | 852 | 852 |
| background tile under `40px 0` | 748 | 70 |
| transparent PNG over blue | 9,384 | 0 |
| text in a pill | 1,270 | 1,231 |

What is left on the fix arm: the blurred halo (the layered approximation is wider and flatter than Chrome's Gaussian; its shape is right now, its falloff is not), the corner antialiasing ramp (two pixels here, one in Chrome), `border-radius` on the `<img>` itself (not in this PR), and glyph rasterisation in the pill. The square box is pixel-identical on all three.

Fixture and the three frames: hub branch `atlas/trench-realsite`, `scratch/s1001d/` (`shadow-clip.html`, `dev.png`, `fix.png`, `chrome.png`, `dev-fix-chrome.png`).

## Real sites

RustKit frames only, develop/fix/develop/fix on 14 board sites (`frames_ab.py`; no Chrome, machine load 19 to 30, so no scoring board and **no points claimed**). "Across" is develop vs fix; "within" is the same binary twice.

| site | within develop | within fix | across | what changed |
|---|---|---|---|---|
| microsoft | 0.00% | 0.00% | 0.17% | the header logo: develop paints the wordmark's transparent PNG as a dark block, the fix shows "Microsoft" |
| lyft | 0.00% | 0.00% | 0.08% | one 784x1 line at y=241 (a transparent image that painted opaque) |
| bing | 0.00% | 0.00% | 0.04% | a black square beside "Copilot" is gone (transparent icon); the search box's shadow is rounded |
| wikipedia | 0.00% | 0.00% | 0.02% | the padlock icon loses its dark backing square |
| walmart | 0.00% | 11.43% | 0.02% | one 32px icon; the 11% is walmart's own rotation on one capture |
| google | 0.00% | 0.00% | 3.76% | **not the change:** Google serves two variants of the page per request. Six more captures (order fix, develop, develop, fix, fix, develop) gave each binary both variants; within one variant the arms differ by under 0.01% (the search box's shadow corners) |
| x, yahoo | 0.00% | 0.00% | 0.00% | nothing |
| linkedin | 2.60% | 1.55% | 0.14% to 3.04% | inside linkedin's own variance |
| shopify | 0.00% | n/a | 0.00% | one fix capture failed to load |
| facebook, apple, github, netflix | n/a | n/a | n/a | captures failed on both arms (load) |

Every change found is a transparent image now showing what is behind it, or a shadow corner. Nothing moved. Crops: hub `scratch/s1001d/ab-<site>.png`.

large-diff: about 1,100 lines, of which about 500 are tests; the rest is one geometry function, one clip function, and four call sites re-indented into a loop.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
