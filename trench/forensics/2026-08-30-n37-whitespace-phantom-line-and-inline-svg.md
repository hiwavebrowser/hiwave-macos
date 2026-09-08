# n37 — the phantom line box was cross-node whitespace, and the census that named it was 98% same-line spaces

Night block 37, macOS trench seat (Atlas). Lane: PLAN §n37 scope — "whitespace-only text between block siblings must not produce a line box". Branch `atlas/n37-ws-line-box` → develop.

## Board
| | develop `2be7d37` (basis, n36 byte-flat receipts) | this PR `4b1c2f8` |
|---|---|---|
| campaign pixel board | 26/26, avg **4.0546%** | 26/26, avg **3.9318%** — images-intrinsic 8.4501 → 5.2510; shelf +9 px; settings +1 px; 23 byte-flat |
| WPT Tier-1 | 24/26 (92.3%) | 24/26 (92.3%) — unchanged, same two fails |

Suites: rustkit-engine 69 green (67 + 2 new). Fresh release `parity-capture` at the exact tip for both boards.

## 1. The census was mis-sized: 129 boxes, 2 phantom lines
The n36 reply counted whitespace-only text boxes with `height > 0` across the 26 develop captures (14/26 cases, pseudo-classes 32 …) and read them as line boxes. `scratch_n37/ws_rows.py` classifies each by geometry — a phantom line has `ws.y >= prev.bottom` and `next.y >= ws.bottom`; a same-line space shares its y with a neighbour:

| case | boxes | phantom lines | what the rest are |
|---|---|---|---|
| pseudo-classes / backgrounds / gradients / rounded-corners / bg-solid / sticky-scroll | 32 / 17 / 18 / 17 / 3 / 5 | 0 | spaces between `display: inline-block` swatches on one row — Chrome renders these too (the ~4px gap between swatches) |
| form-controls / css-selectors / form-elements | 19 / 3 / 1 | 0 | spaces beside inputs/labels on one row |
| article-typography / holdout-cascade-depth | 4 / 2 | 0 | spaces between inline spans |
| settings | 6 | 0 | three adjacent `' '` boxes on ONE row at y=2067 (below the 768px fold) — the same bug as below, on a line |
| **images-intrinsic** | 1 | **1** | `' '` at y=68 h=24 between `<h1>` and `<h2>` |
| **shelf** | 1 | **1** | `' '` inside the search-icon `<svg>` |

So the unit was real but small on the meter: one phantom line on the board (images-intrinsic, −3.2pp) and a hidden one that was holding up a different bug (shelf).

## 2. Root cause: the boundary strip reads only immediate siblings
images-intrinsic's markup between the heading and the first section:

```html
</h1>
  <!-- Use a data URI for a simple 100x100 red square -->
  <!-- This ensures the test works without external resources -->
<h2>
```

That is THREE whitespace-only DOM text nodes separated by comments. The engine's Text arm turns each into `Text(" ")`; comments become empty Inline boxes that `should_include` drops. The box-build post-pass (css-text §4.2 phase 2) then strips a text run's leading space unless the PREVIOUS sibling is inline-level and its trailing space unless the NEXT is — reading `children[i-1]` / `children[i+1]` only. Run 1 sees `<h1>` on its left → stripped to nothing → removed. Run 3 sees `<h2>` on its right → removed. Run 2 sees a TEXT box on both sides, `is_inline_level_box` says yes, and it survives as a 24px line box (`line-height` 1.5 × 16px). Chrome: h1 bottom 68 → h2 at 88; RustKit: h2 at 112, every later block +24.

css-text §4.1.1 is explicit that the collapse crosses node boundaries: "any collapsible space immediately following another collapsible space — even one outside the boundary of the inline containing that space, provided both spaces are within the same inline formatting context — is collapsed to have zero advance width."

**Fix (`f243f16`, rustkit-engine `build_layout_from_node_with_parent_style`):** before the strip, join adjacent `Text` boxes into one run — dropping one space where the left ends and the right begins with one under collapsible white-space, verbatim concatenation under the pre family. Bare text siblings of one element always share the parent's computed style (pseudo-element text lives inside its own Inline wrapper), so the join loses nothing. The strip then sees one `" "` between two blocks and removes it; `"alpha <!-- --> beta"` becomes one run `"alpha beta"` (it was `"alpha "` + `" beta"`); `<span>x</span> <!-- --> <span>y</span>` keeps ONE space (it had three).

T-RED: with the join disabled the new test reports exactly those three shapes: `[(" ", true, true)]` next to blocks, `"alpha " " beta"`, and `"x" " " " " "y"`.

## 3. The shelf regression that named the second gap
First board run after the fix: shelf 5.1992 → 5.2422 (+66 px). The new layout had no `<svg>` box at all. The engine has no inline-`<svg>` handling (only `<img src="*.svg">` through `svg_cache`): the UA match's `_ => {}` fallback leaves `ComputedStyle::new()`'s default display (Block), `<circle>`/`<path>` build empty boxes that are dropped, and a childless block with no visible styling is dropped too. The icon had a box on develop ONLY because the whitespace between `<circle>` and `<path>` was the same comment-less triple (`"\n  "`, `"\n  "`, `"\n"`) and its middle run survived as an 18px text line inside the svg — 18×18 at y=58.5 where Chrome has 14×14 at y=67.5. Collapsing the whitespace correctly emptied the svg, the box vanished, and the command input moved 28px left (18 + `margin-right: 10px`).

**Fix (`4b1c2f8`):** `<svg>` is an inline-level replaced element — UA display inline (Chrome's sheet has no display rule for it), sized by its own `width=`/`height=` presentational hints with author CSS winning (CSS replaced fallback 300×150 without them), atomic inline placement, and its SVG children generate no CSS boxes. The graphics are still not painted (inline SVG paint is its own lane). Second board run: shelf +9 px with the input at Chrome's x=53.

## Real-page reach
- Every comment between blocks on the web (`<!-- header -->` … `<!-- /header -->`, build-tool markers, template comments) left a 1-line gap after the preceding block. Common; now gone.
- Every text run interrupted by a comment, `display: none` element or `<script>` rendered its two halves with a doubled or missing space.
- Every inline `<svg>` icon (nav bars, buttons, search fields) now occupies its declared size in flow instead of nothing (or a phantom line). Its paint is still blank — a visible next lane.

## Ledgered, not chased
- Inline SVG PAINT: the box exists, the vector content does not render. rustkit-svg + `svg_cache` already rasterise `<img src=*.svg>`; an inline-svg path would serialise the subtree and reuse it.
- The engine's `_ => {}` UA fallback makes every unknown/custom element a BLOCK; Chrome's is inline. Wide blast radius, own lane.
- Early-return replaced boxes (`img`, now `svg`) carry no selector identity in the layout export, so the geometry oracle cannot join them to Chrome rects (pre-existing for `img`).
- settings' three same-row spaces at y=2067 (below the fold) are collapsed to one now; +1 px on the board.
- shelf residual +9 px: `.command-input-wrapper` sits at y=47 h=41 vs Chrome 53/43 — the flex row's height, not this lane.

## Scratch
`scratch_n37/`: `ws_probe.py` (census), `ws_rows.py` (phantom-vs-same-line classifier), `compare.py` (per-case delta + attribution), `peek.py`, `basis_results.json`, `basis_lastrun.json`, `ws_rows.out`.
