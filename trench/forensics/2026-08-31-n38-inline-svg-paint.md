# n38 — inline SVG paints: one serializer, three parser bugs the first pixels exposed

**Night:** 2026-08-31 (Atlas, macOS seat)
**Branch:** `atlas/n38-inline-svg-paint` (stacked on `atlas/n37-ws-line-box`, PR #171)
**Boards:** campaign 26/26 avg 3.9318 → 3.9332 (shelf +0.04pp sole mover, 25/26 byte-flat) | WPT Tier-1 24/26 unchanged

## The lane

n37 gave inline `<svg>` a correctly sized replaced box and painted nothing into
it. Only `<img src=*.svg>` reached `svg_cache` (rustkit-svg's `SvgDocument`,
whose `render()` emits display commands directly — vector, no raster). The fix
rides that lane end to end:

1. **relayout pre-pass** (`cache_inline_svgs`): serialize every inline `<svg>`
   DOM subtree back to markup (sorted attributes — HashMap iteration order is
   not a contract between the two serializations of one layout pass), key it
   `inline-svg:<fnv1a64>` (content-addressed: identical icons share one parsed
   document), parse into `svg_cache`. The pre-pass exists because the box
   build runs with `&self` and cannot insert.
2. **box build**: the `<svg>` branch computes the same key; on a cache hit the
   box becomes `BoxType::Image { url: key, natural via get_size }` — geometry
   identical to n37 (style width/height already forced to Px) — and the
   existing display-list splice (`svg_cache.get(url) → svg.render(dest_rect)`)
   paints it. Cache miss (parse failure) keeps the unpainted n37 block.
   The `inline-svg:` scheme survives the display-list URL normalization
   because joining an absolute URL against any base returns it unchanged.

## What the first painted pixels exposed (repro A/B vs pinned Chrome)

`parity-tests/repro/inline-svg.html` — rect+text placeholder, stroke-only
currentColor icon, circle, path triangle, polygon+line — captured in both
engines, shapes located by color bbox (`scratch_n38/bbox.py`):

- **`viewBox` died in the case fold.** HTML's tree builder lowercases
  attribute names, so the serialized root reads `viewbox=` and rustkit-svg's
  case-sensitive `extract_attr` never found it → no scale transform. Every
  shape under `viewBox != box-size` painted UNSCALED: the path triangle
  measured 20px where Chrome draws 38; circle/polygon "matched" only because
  their viewBox equaled the box. The spec-correct fix is the HTML parser's
  "adjust SVG attributes" step (rustkit-html has no foreign-content handling
  at all — ledgered); the shipped fix accepts both spellings at parse.
  Post-fix: RustKit red bbox (346,25)–(385,63) vs Chrome (347,26)–(384,63).
- **`SvgText` was dead code.** `parse_element` only ever sees the open tag, so
  `<text>` content between the tags never parsed — the type existed, nothing
  constructed it. `parse_svg_content` now captures up to `</text>`, strips
  nested markup, decodes the named basics. SVG `y` is the BASELINE; the
  renderer computes `baseline = y + ascent`, so the command carries
  `ascent: Some(0.0)` to hand the baseline over directly. `text-anchor`
  middle/end offset in LOCAL units via rustkit-layout's shaper (real
  advances, not a per-char constant), then transform; font-size scales by
  the transform's uniform scale.
- **Root presentation attributes never reached shapes.** The parser is flat —
  `<svg fill="none" stroke="currentColor">` (the standard icon idiom) lost its
  root style and the shelf search icon painted a default-black DISC where
  Chrome draws an outline. `SvgDocument::parse` now seeds every shape's style
  from the root tag's presentation attributes. currentColor still resolves
  black (`Paint::as_color` has no CSS color context) — the shelf icon is now
  the right SHAPE in the wrong gray (+0.04pp, the board's only mover).

## Board read

The campaign board carries exactly one inline svg (shelf). The board case
`images-intrinsic` — which I briefly believed was seven inline svg
placeholders — is 14 `<img>` elements; the seven-svg file is
`websuite/micro/image-intrinsic-size/`, which is NOT in the registry. Boards
are honest: byte-flat everywhere the lane doesn't reach. The lane's value is
real-page: every nav/button/icon `<svg>` (eBay-class UIs, the browser's own
shelf/settings chrome) now paints its vector content instead of an empty slot.

## Ledgered, not chased

- **currentColor needs the CSS color** — a color channel through
  `BoxType::Image`/`DisplayCommand::Image` (rustkit-layout ripple), resolved
  at box build where the computed style is in hand.
- **Group/nesting inheritance in rustkit-svg** — the parser is still flat;
  `<g fill=...>` styles are dropped. Root seeding covers the icon idiom only.
- **SVG-in-HTML foreign content** in rustkit-html (attribute case adjustment,
  CDATA, namespaces) — the parser has none of it.
- **svg width:Npx with height:auto** should derive height from the intrinsic
  ratio (Chrome: 20×20 for `width=40 height=40 style="width:20px"`; we keep
  n37's 20×40) — geometry lane.
- The layout export still carries no selector identity for early-return
  replaced boxes (n37 ledger item, unchanged).
