## The pair, first

| Oracle | develop `afd73ab` (clean-tree capture tonight) | this branch `87e7255` |
|---|---:|---:|
| Campaign pixel board, 26/26 avg | 2.6944 | **2.6258** |
| shelf | 4.6185 | **2.9264** (−1.69pp) |
| new_tab | 2.6835 | 2.5924 (−0.09) |
| form-controls | 5.0167 | 5.0168 (+0.0001) |
| other 23 cases | — | byte-flat |
| WPT Tier-1 | 24/26 | 24/26, same two fails (`trench/wpt/last-run.json` pinned on the engine commit) |

Geometry: unchanged (no layout code touched; `about` frame byte-identical). Paint: the three changes below.

## Three defects, one lane

Lane was the n43 digest's option (a), **currentColor through the Image command**, taken because the whole review queue landed (#174–#188, develop → master #190). The first colored pixels the shelf's search icon ever painted exposed the other two.

**1. `currentColor` resolved to black** (`rustkit-svg`, `rustkit-layout`, `rustkit-engine`). `Paint::CurrentColor` existed; `as_color()` returned `Color::BLACK` "would need context". Now `SvgStyle.current_color` is a render-time input inherited unconditionally, `fill_color()`/`stroke_color()` resolve through `Paint::resolve`, `SvgDocument::render_with_color` carries it, and `DisplayCommand::Image` gains `current_color` filled from the box's computed CSS color. `render()` (the `<img src=*.svg>` lane) keeps black — a standalone document's initial `color`.

**2. Pseudo-classes on an ancestor compound were never evaluated** (`rustkit-engine`). `simple_selector_matches_ancestor` parsed tag/class/id and `break`-ed at the first `:` — `.card:hover .title` and `.wrapper:focus-within .icon` matched with the state off on every page. The subject matcher's static-false list also lacked `focus-within`/`focus-visible`/`target`, which fell to `_ => true`. One `pseudo_class_is_static_false()` now serves both matchers. The shelf's icon had painted in `--accent-hover` (`#22d3ee`) and its whole search bar carried the `:focus-within` border + glow.

**3. `StrokeCircle` was a colored disc plus an opaque white disc** (`rustkit-renderer`). Every `fill="none"` circle icon on a dark surface carried a white interior, and the stroke sat inside the geometry rather than centred (SVG 2 §13.4, r ± w/2). Replaced with `draw_ring`, a triangle strip between the two radii.

## Receipts

- Repro `parity-tests/repro/inline-svg.html` (`.icon-wrap { color: #6b7280 }`) vs pinned Chrome 148 (`scratch_n44/icon_color.py`): icon-gray pixels **0 → 64** at bbox (241,25)-(252,36) vs Chrome (242,25)-(251,34); near-black pixels **149 → 80** = Chrome's 80.
- Shelf frame row 71 after: `334155 94a3b8 94a3b8 334155 … 94a3b8 94a3b8 334155` — ring in Chrome's `rgb(148,163,184)`, interior the toolbar background. Before: `22d3ee` ring around `ffffff`.
- **T-RED:** the engine test fails on the subject-only fix with the board's own number (`left: (34, 211, 238)`); the currentColor test fails on develop (stroke black); the Image-command test does not compile on develop (no field).
- **Measurement honesty:** the currentColor fix alone reads **byte-flat 26/26** on the meter while the shelf frame changed (black → accent) — t15 counts a wrong pixel the same whichever wrong color it is. Banked frames (`scratch_n44/captures_*`) were pixel-diffed; that is how bugs 2 and 3 were found.

Tests: rustkit-svg 16/16 (+1), rustkit-layout 369/369 (+1), rustkit-renderer 64/64, rustkit-engine 82/82 (+1). rustfmt clean on my hunks (`scratch_n43/fmt_check.py`; both big files carry pre-existing hunks).

## Ledgered, not chased
- Structural pseudo-classes and attribute selectors on ancestor/sibling compounds still match unconditionally.
- `match_pseudo_class` `_ => true`: an unknown pseudo-class should invalidate the selector, not match. Wide blast; its own board.
- `<g>` nesting in rustkit-svg stays flat; a `color` set inside the svg does not override the inherited CSS color.
- Ellipse/polyline stroke widths not audited for the two-disc pattern.
- new_tab reads 2.68 on this basis vs 2.26 on n43's with #182 + #184 both merged; the n42 prediction that the pair would take it below basis did not hold — re-table.

Forensics: hub `trench/forensics/2026-09-09-n44-currentcolor-focus-within-stroke-ring.md`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
