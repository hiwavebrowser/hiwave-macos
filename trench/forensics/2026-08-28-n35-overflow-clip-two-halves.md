# n35 — square overflow clipping was two halves, and the second one was already leaking through the rounded clip
2026-08-28, macOS trench seat (Atlas). Branch `atlas/n35-overflow-clip` → develop.

## Board
| | develop `6e5d944` (fresh build, reproduced) | `atlas/n35-overflow-clip` @ 558bfb0 |
|---|---|---|
| WPT Tier-1 scored | 22/26 (84.6%), 0 ERROR | **24/26 (92.3%), 0 ERROR** |
| campaign pixel board | 26/26, avg 4.0586% | **26/26, avg 4.0546%** — new_tab 2.3646 → 2.2600, 25 cases byte-flat |

Every number on a fresh-built `parity-capture` with the n24 freshness guard on.
Suites: rustkit-layout 298, rustkit-renderer 63, rustkit-engine 67, `cargo check --workspace` green.

## The lane as authorised (n34 decision 2)
`overflow: hidden|clip|auto|scroll` clips descendants to the padding box. The
display-list builder's `overflow_clip()` said in its own words that the square
half was "deliberately NOT implemented" — it returned `None` for a zero radius
(the Gate B rounded-corner unit drew that scope line on purpose). owa002/003
were the exact-match probes: a `height: 1em; overflow: hidden` div whose second
line must not paint.

## Half 1 — the builder (rustkit-layout, `DisplayList`)
`overflow_clip()` now returns the padding box with a zero radius; the builder
emits `PushClip(rect)` for it (`PushClipRounded` when round). Two things the
spec requires that the naive version would have got wrong on real pages:

- **The root never clips, and body does not clip while the root's overflow is
  visible** (CSS 2.1 §11.1.1 / css-overflow-3 §3.3 — the value propagates to the
  viewport). new_tab and every chrome page carry `body { overflow: hidden }`;
  clipping body to its own padding box would have cut them at the first screen.
  Tracked as `depth` on the builder (root box = depth 0; the root LayoutBox has
  no identity tag) plus `root_overflow_visible`.
- **An absolutely positioned box whose containing block is above a static
  clipper escapes it** (css-overflow-3 §3.1 — the dropdown-escapes-its-wrapper
  case). The builder keeps `escapable_clips`: the clips pushed by non-positioned
  boxes since the nearest positioned ancestor. An abspos/fixed box pops them
  before it paints and re-pushes them after; a positioned box scopes them out
  for its subtree. Positioned clippers clip everything inside (image-gallery's
  overlay test keeps its assertion, with the parent now `relative`).

## Half 2 — the renderer never clipped textured quads
This is the finding. `draw_text_with_metrics` (grayscale AND color glyphs),
`draw_image` and `draw_background_image_tile` pushed vertices straight past
`clip_stack`; only color quads went through `collect_clipped_pieces`. So the
rounded clip that shipped in Gate B clipped a child's BACKGROUND and never its
TEXT or IMAGES — invisible on the board only because no campaign case had text
crossing a rounded clipper's edge. A square clip that clips backgrounds but
not glyphs would have flipped nothing (owa002/003 are text). New pure fn
`clip_textured_rect(clip, rect, tex)`: intersects with the clip's rect and
rescales the texel window so the atlas bitmap does not stretch (4 units). The
advance is charged before the clip check — a clipped-away glyph still occupies
its run.

## The regression that found the real containing-block rule
First board run: image-gallery +74 px, all of them short diagonal runs at the
gallery items' bottom corners (rows 337–345 and 552–561, radius-sized), RustKit
painting (8,8,15) — the overlay's 0.7-black over page background — where Chrome
shows the page. The `.image-overlay` (absolute) had ESCAPED
`.gallery-item { position: relative; overflow: hidden }`. Cause: the escape
rule read `LayoutBox::position`, and the engine's `transfer_positioning`
deliberately maps `position: relative` → `Position::Static` (its paint-side
stacking path is not ready for relative boxes). A relative box is still the
containing block of its absolute children, so the rule now also reads
`style.position`. Test pins the engine-real shape (style Relative, layout
Static). Second run: image-gallery byte-flat.

## Real-page reach
Every `overflow: hidden` on the web now does what it says. The two visible
changes on real sites: text-overflow containers stop leaking their text past
the box (the shelf/chrome `.tab-title`, `.shelf-item-url` family — the
ellipsis itself is still unimplemented, so text is cut hard), and fixed-height
cards clip their content. Dropdowns/tooltips positioned out of a static
`overflow: hidden` wrapper still show (the escape rule).

## Ledgered, not chased
- The rounded part of a clip is not applied to textured quads: a glyph
  straddling an arc keeps its square corner. Needs per-glyph rounded pieces or
  a stencil; not measured on any current case.
- `position: fixed` escapes only the static clips up to its nearest positioned
  ancestor, like absolute; per spec it escapes everything except transformed
  ancestors. No current case.
- An abspos box inside a `position: relative` box that is itself inside a
  static clipper: correctly clipped (the relative box is inside the clipper).
- `overflow: clip` honours no `overflow-clip-margin` (treated as 0).
- Remaining Tier-1 fails: line-break-anywhere-001 0.0173 (AA column,
  tolerance decision with Pete since n33), empty-span-size-002 0.0121 (outline
  paint, n21).

## Receipts
Commits on `atlas/n35-overflow-clip`: `729ae5c` (both halves + tests),
`eabe507` (containing block from computed style + test), `558bfb0` (boards).
Scratch: `scratch_n35/` (`compare.py` per-case delta, `diffdelta.py` diff-image
localiser, basis diff images).
