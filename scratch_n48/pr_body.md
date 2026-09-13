## What was wrong

`Renderer::current_transform` (rustkit-renderer) folded every `PushTransform { matrix, origin }` as `result * T(-origin) * M * T(+origin)`. `multiply_matrices_2d(a, b)` is `a · b` in the column-vector convention (b applies first), so the origin was moved the wrong way: a point went `M·(p + o) − o` instead of css-transforms-1 §6's `M·(p − o) + o`.

Translations commute with each other, so every `translate()` on every board case was unaffected and the bug hid under the whole campaign. Every `scale()` / `rotate()` / `skew()` landed its box `M·o − o` away from where it belonged: a 60×20 card at (110, 20) with `scale(2); transform-origin: 0 0` painted at (330, 60). n47's repro section E — a scaled `overflow: hidden; border-radius` card — was ledgered as "paints nothing at all"; its box went to y = 900, off the 480px frame. The engine's geometry oracle (`own_transform_affine`, the layout-rects export) had the right order all along, so the exported rect and the painted pixels disagreed for every scaled or rotated box on every page — hover-zoom cards, thumbnails, rotated chevrons, `scale()` badges.

n47's screen-space clip path (`clip_entry_under` / `clip_quad_under`) is correct; it takes the composed matrix as given. Its scale test fed a raw matrix with no origin, which is why it passed.

## The fix (rustkit-renderer only)

- `affine_about_origin(matrix, origin)` = `T(origin) · matrix · T(−origin)`, a free function so it is testable without a GPU.
- `current_transform` folds the stack as `outer · inner` through it. No other line changes; the identity path, the clip path and every emitter are untouched.
- rustkit-renderer 75 → 80 tests: scale about the top-left keeps the corner fixed and doubles the far corner (the repro's numbers); scale about the centre grows evenly; rotate(90deg) turns about its origin; translate ignores its origin (the campaign's translate pixels stay identical, pinned); nested translate-over-scale composes inner first. T-RED: with the old order restored, four of the five fail with the board's numbers.

## Receipts

`parity-tests/repro/transform-origin.html` (400×500) vs pinned Chrome 148, seven rows: A scale(2) origin 0 0 · B scale(2) centre · C the n47 section-E shape · D rotate(90deg) · E scaleX(2) origin 100% 50% · F translate control with a non-default origin · G a translated shine child inside a scaled clipper.

Ink by color (`scratch_n48/ink.py`):

| color | Chrome | before | after |
|---|---|---|---|
| `#aa33aa` fill | 11592 | 3285 (bbox x 150..399, y 60..499) | 11612 (bbox x 50..229, y 20..399 = Chrome) |
| `#33aa33` box | 9600 | 1000 (x 180..399) | 9600 (x 110..229 = Chrome) |
| `#ffcc00` shine (G) | 2400 (x 110..169, y 440..479) | 0 | 2400 (same bbox) |

Pixels differing from Chrome per 70px row (`vs_chrome.py`): total **30850 → 5437**; row F (control) 785 → 785 flat; the residual on every row is the 12px label's antialiasing (the band F carries) plus corner/edge antialiasing on C and D.

n47's `clip-transform-order.html` re-rendered on this branch: section E band **5209 → 897**; every other band byte-identical to n47's after-frame.

**Campaign board** (develop `da8f413` + this PR): 26/26 **byte-flat**, avg 2.6245. By construction: a census of every `scale`/`rotate`/`skew`/`matrix` declaration in the 26 case sources (`scratch_n48/census.py`) finds them only in `@keyframes`, `:hover`, `:active`, or as `scaleY(0)` at rest (singular — paints nothing either way): new_tab (ripple keyframes, `.logo:hover`, `.shortcut:hover/:active`, `.shortcut::before { scaleY(0) }`), about (`.logo:hover`, `.feature::before { scaleY(0) }`), shelf (ripple keyframes, `.shelf-close:hover { rotate(90deg) }`). The meter cannot see this lane; the receipt is the repro.

**WPT Tier-1** 24/26 (same two fails), last-run pinned on `faf1270`.

## Ledgered, not chased

- A rotated / skewed `overflow: hidden` clipper still keeps only its bounding box (n47 fallback) — now at the right place.
- `apply_backdrop_filter` intersects its document-space rect with the screen-space clip (n47 ledger, unchanged).
- The engine's whole-page scroll `PushTransform` uses origin (0, 0), so it was never affected.

Forensics: `trench/forensics/2026-09-13-n48-transform-origin-composed-backwards.md` (hub repo).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
