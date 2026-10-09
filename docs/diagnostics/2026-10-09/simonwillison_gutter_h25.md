# H25: simonwillison.net has no left gutter (flex-column body + `margin: 0 auto` item)

Docs only, no engine code. Site: `https://simonwillison.net/tags/browser-challenge/` (live HTML and `all.css` saved 2026-10-09 and measured as a local snapshot).

- **Measured:** develop `f54103d9`, RustKit `parity-capture --profile parity`, viewport 1280x800, DPR 1.
- **Oracle:** pinned Chrome for Testing 148.0.7778.216, headless, page served over local http.
- **Reduced page:** `simonwillison_gutter_h25.html` (8 lines of CSS and markup).

## Root cause

The site's `body` is `display: flex; flex-direction: column; min-height: 100vh`, and the page wrapper is
`div#wrapper { width: 940px; margin: 0 auto; padding: 0 10px; overflow: hidden; flex: 1 }`
(`body.smallhead div#wrapper { padding: 15px 0 }`). In a column flex container `margin: 0 auto` on an item
auto-centers it on the cross (horizontal) axis. Chromium centers `#wrapper` at x=170 (a 170px gutter each side at 1280).
RustKit resolves the item's auto margins to 0 and puts it at x=0, so the content is flush against the left edge.
The same item in a block `body`, or on the main axis of a row flex container, matches Chromium
(see controls), so the defect is cross-axis `auto` margins on a flex item in a column container.
It is not `padding`, `margin` on `body`, or `max-width`; none of those are involved.

## Reduced page, 1280x800

| | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `body` | display flex, 0,0 1280x800 | 0,0 1280x800 |
| `#bar` (stretch item) | 0,0 1280x18 | 0,0 1280x18 |
| `#wrapper` (300px, `margin: 0 auto`, `flex: 1`) | computed margin-left/right `490px`; **rect 490,18 300x764** | **rect 0,18 300x764** |
| `#ctl` (300px, `margin: 0 auto`) | **490,782 300x18** | **0,782 300x18** |
| `#p` inside wrapper | 490,49 300x18 | 0,49 300x18 |
| **pixel (200,200)** (left of where the wrapper should be) | **255,255,255** | **153,204,153** (wrapper green) |
| **pixel (600,200)** (inside the centered wrapper) | **153,204,153** | **255,255,255** |

Controls (same file with one change, `#wrapper` rect):

| Variant | Chromium | RustKit |
|---|---|---|
| `body` as `display: block` (remove `display: flex; flex-direction: column`) | 490,18 300x80 | 490,18 300x80 (matches) |
| `flex-direction: row` (main-axis auto margins) | 91.06,0 888.94x800 | 91.06,0 888.94x80 (x and width match; height differs, not this finding) |

## Live page snapshot, 1280x800

| Box | Chromium | RustKit |
|---|---|---|
| `div#wrapper` (940px, `margin: 0 auto`) | **170,72.22 940x4798.89** | **0,72.24 940x3568.8** |
| `div#primary` | **170,87.22 560x4768.89** | **0,87.24 560x3538.8** |
| `div#secondary` | 765,87.22 280x310.34 | 595,87.24 280x305.96 |
| `div#smallhead-inner` (a block with `margin: 0 auto`, not a flex item) | 170,2 940x31.19 | 170,2 940x31.2 (centered correctly) |
| `div#ft` | 0,4871.11 **1280**x59.41 | 0,3641.04 **1513.95**x39.72 |
| `body` height | 4935.95 | 3686.2 |

So the header bar is centered in both engines, and only the `#wrapper` that is a flex item loses its gutter: the
whole content column and the sidebar shift 170px left. Related, not the cause: RustKit's `#ft` is 1513.95px wide
(wider than the 1280 viewport) and the page is 1250px shorter than Chromium's, which fits Atlas's note that the live page
does not scroll and the pinned footer bar misbehaves. Both are unmeasured further here and probably a second finding.
