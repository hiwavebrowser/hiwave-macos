# H24: ebay.com large black icons painted over the header and page (CSS-sized `<svg><use>`)

Docs only, no engine code. Site: `https://www.ebay.com/` (home), plus `https://www.ebay.com/n/all-categories` for the Chromium side.

- **Measured:** develop `f54103d9`, RustKit `parity-capture --profile parity`, 800x600 (reduced page) and 1280x800 (live), DPR 1.
- **Oracle:** pinned Chrome for Testing 148.0.7778.216, headless. **Headless Chrome is bot-walled on the eBay home page** (HTTP 403 "Error Page | eBay", also with a Chrome user agent and from curl), so the Chromium column for live icon sizes comes from `/n/all-categories`, which loads and uses the same `gh-` header and sprite. RustKit loads the real home page, so the RustKit live numbers are from the home.
- **Reduced page:** `ebay_icon_bg_h24.html` (10 lines).

## It is not a background

The icons are not CSS backgrounds (the only `background-image` on the Chromium page is the 12px category-select chevron, `background-attachment: scroll`, `background-size: auto`). They are inline `<svg class="icon icon--24"><use href="#icon-...">` elements from a zero-size sprite
(`<svg style="position:absolute;width:0;height:0"><defs></defs><symbol viewBox="0 0 16 16" id="icon-camera-16">...`). They "scroll with the page" only because they are painted in page coordinates, as every other box is.

## Root cause: a `<use>` svg sized only by CSS paints into a 300x150 viewport

The eBay icon `svg` has no `width`, `height` or `viewBox` attributes; its size comes from CSS (`.icon--16` etc., computed 16x16, 20x20, 24x24). RustKit's layout box is the right size, but the paint maps the symbol's `viewBox` into the default 300x150 replaced-element viewport, so a 16-unit symbol is drawn at 150/16 = 9.4x, centered in a 300x150 box at the icon's origin. Giving the outer `<svg>` its own `viewBox` (or `width`/`height` attributes) makes RustKit match Chromium.

## Reduced page, 800x600

| | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `#css` (`.icon { width: 24px; height: 24px }`, no attributes) layout rect | 161.25,50 24x24 | 161.25,50 24x24 (the same) |
| painted black area of `#css` | 161,50 to 185,74 (24x24) | **236,50 to 386,200 (150x150)** |
| **pixel (300,100)**, outside the 24px icon | **255,255,255** | **0,0,0** |
| **pixel (245,60)** | **255,255,255** | **0,0,0** |
| pixel (180,60), inside the icon | 0,0,0 | 255,255,255 (the icon moved 75px right) |
| `#attr` (same icon with `width="24" height="24"` attributes), control | 170.59,224 24x24; pixel (180,240) 0,0,0 | 24x24; pixel (180,240) 0,0,0 (matches) |

Variants (RustKit, pixels at (300,100) / (180,60)):

| Variant of `#css` | Chromium | RustKit |
|---|---|---|
| `style="width:24px;height:24px"` on the svg | 255,255,255 / 0,0,0 | **0,0,0 / 255,255,255** (still wrong) |
| `viewBox="0 0 16 16"` on the svg | 255,255,255 / 0,0,0 | 255,255,255 / 0,0,0 (matches) |

## Live

| | Chromium 148 (`/n/all-categories`) | RustKit (home) |
|---|---|---|
| header icons (`svg.icon` with `<use>`) | chevron 12x12 at (915,18), notification 20x20 at (1172,14), cart 20x20 at (1220,13), search 16x16 at (279,71), camera 16x16 at (801,71); all with no width/height/viewBox attributes, computed size from CSS | 65 `svg` layout boxes, only 3 of them 40px or larger (the logo 117x48, the QR code 71x71, and the `body > svg` sprite at 300x150); the icon boxes are the right small size |
| painted icon extents | the layout sizes above | 64 groups of `fill_polygon` ops, about 60 of them **75-150px wide and 110-135px tall**: magnifier (330,115)-(440,178), camera (929,89)-(1079,220), bell/cart (1201,24)-(1369,159), play button (1033,392)-(1146,523), and a row of 113x131 icons at y=1561 spaced 261px apart |

In the RustKit frame the black icons overlap the header nav labels ("Electronics", "Toys", "Advanced") and extend below the header.
