# Reduced repro: CSS `mask-image: url()` is ignored (#565 extend)

- **Finding:** Census #572 icon technique 6, "mask-image url()" (Pollux draft `repro_mask_image.html` from #565, extended here).
- **Source sites:** en.wikipedia.org (Vector 2022 header icons, B) and bing.com (camera icon). Case A is the draft's circle.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64), headless, 1280x800 for the live probe, 800x600 and DPR 1 for the reduced page.
- **Ours:** `parity-capture --html-file` at the develop engine (`50e77c83` plus 2 navigation-only commits).
- **Page:** [`repro_mask_image.html`](repro_mask_image.html), 10 lines.

## What the live pages have (pinned Chrome on macOS)

en.wikipedia.org/wiki/Main_Page has 28 elements with a `url()` mask, and 3 of them are in the first viewport:

| Element chain | Box | background-color | mask-image | mask-size / repeat / position |
|---|---|---|---|---|
| `span.vector-icon.mw-ui-icon-menu < label#vector-main-menu-dropdown-label < div#vector-main-menu-dropdown < nav.vector-main-menu-landmark < div.vector-header-start < header.vector-header` | 44,23 20x20, `display:block` | `rgb(64,66,68)` | `url(load.php?…image=menu…)` (an SVG: `<path d="M1 3h18v2H1zm0 6h18v2H1zm0 6h18v2H1z"/>`) | `20px` / `no-repeat` / `50% 50%` |
| `span.cdx-text-input__icon.cdx-text-input__start-icon < div.cdx-text-input < div#simpleSearch < form#searchform` | 275,24 18x18 | `rgb(32,33,34)` | `url("data:image/svg+xml;utf8,<svg …><path d="M8 1a7 7 0 015.605 11.191…"/></svg>")` | `18px` / `no-repeat` / `50% 50%` |
| `span.vector-icon.mw-ui-icon-verticalEllipsis` (page tools) | 998,243 20x20 | `rgb(64,66,68)` | `url(load.php?…image=verticalEllipsis…)` | `20px` / `no-repeat` / `50% 50%` |

bing.com: one, `div.sbi_hpicn.b_icon` (camera, inside `form#sb_form`), 24x24, `display:flex`, `background-color: rgb(23,74,228)`, a `data:image/svg+xml` mask, `mask-size: contain`.

The pattern is the same everywhere: a small box whose `background-color` is the icon colour, with an SVG `mask-image` cutting out the glyph. Without the mask, every one of these icons draws as a solid square.

## Pixels (reduced page, 800x600, DPR 1)

| Probe | What is there in Chrome | Chrome RGBA | Ours RGBA |
|---|---|---|---|
| **(70,7)** case B, gap between hamburger bars | masked out, page background | `255,255,255,255` | `64,66,68,255` |
| (1,1) case A, corner outside the circle | masked out | `255,255,255,255` | `255,0,0,255` |
| (70,4) case B, on a bar (control) | bar | `64,66,68,255` | `64,66,68,255` |
| (25,25) case A, circle centre (control) | red | `255,0,0,255` | `255,0,0,255` |

## First difference

Ours paints the full `background-color` rectangle and applies no mask: the display list is just `solid_color 50x50 at (0,0)` and `solid_color 20x20 at (60,0)`. `mask-image`, `-webkit-mask-image`, `mask-size`, `mask-repeat` and `mask-position` don't appear anywhere under `crates/` (grep), so the properties are dropped at parse. The data: SVG is never decoded. A fix needs those properties in the cascade plus a paint-time alpha mask from the decoded image, with mask-size/position/repeat applied the same way as background layers.
