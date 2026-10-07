# Reduced repro: PUA icon font, `::before { content: "\E721" }` paints nothing (#565 extend)

- **Finding:** Census #572 icon technique 5, "icon font PUA" (Pollux draft `repro_icon_font.html` from #565, extended here). The draft pointed at a missing `icons.woff2`, so it had no glyph to show. It now carries a real font inline.
- **Source sites:** microsoft.com (header search, cart and nav chevrons, B) and amazon.com (VideoJS play buttons).
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64), headless, 1280x800 for the live probe, 800x600 and DPR 1 for the reduced page.
- **Ours:** `parity-capture --html-file` at the develop engine (`50e77c83` plus 2 navigation-only commits).
- **Page:** [`repro_icon_font.html`](repro_icon_font.html), 14 lines.

## What the live pages have (pinned Chrome on macOS)

microsoft.com/en-us: 6 PUA glyphs, 3 in the first viewport, all in `FabricMDL2Icons` (one face, `unicode-range: U+0-10FFFF`, status `loaded`), all drawn from `::before`:

| Element chain | Codepoint | Box | font-size / line-height | color |
|---|---|---|---|---|
| `i.ms-Icon.ms-Icon--Search::before < uhf-icon < button.uhf-nav-item.uhf-nav-button < uhf-search < uhf-actions < div.uhf-header__container < header.uhf-header` | U+E721 | 1102,20 16x16 | 16px / normal | `rgb(38,38,38)` |
| `i.ms-Icon.ms-Icon--ShoppingCart::before < … uhf-cart#uhf-shopping-cart` | U+E7BF | 1146,20 16x16 | 16px / normal | `rgb(38,38,38)` |
| `i.ms-Icon.ms-Icon--ChevronDown::before < … nav.uhf-global-nav` | U+E70D | 1066,25 8x8 | 8px / normal | `rgb(38,38,38)` |

All three have `font-style: normal`, `font-weight: 400` and `speak: none`, with `display: inline` on the `::before`.

amazon.com: 7 PUA glyphs in `VideoJS`, e.g. `span.vjs-icon-placeholder::before` (U+F101/U+F103) inside `button.vjs-play-control`, 20px/20px, white, `display: block`.

## The font

`ReduceIcon` is a 644-byte TrueType generated for this page (CC0, no third-party outlines). It has one glyph, U+E721, a solid square from descent −200 to ascent 800 in a 1000-unit em, so at `16px/1` it fills its 16x16 box exactly. It's inlined as `data:font/ttf;base64` because `parity-capture --html-file` doesn't fetch file subresources. Ours does load it (`Loaded local web fonts count=1`). To regenerate it (fontTools 4.25):

```python
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
p = TTGlyphPen(None); p.moveTo((0,-200)); p.lineTo((0,800)); p.lineTo((1000,800)); p.lineTo((1000,-200)); p.closePath()
fb = FontBuilder(1000, isTTF=True); fb.setupGlyphOrder(['.notdef','uniE721']); fb.setupCharacterMap({0xE721:'uniE721'})
fb.setupGlyf({'.notdef': TTGlyphPen(None).glyph(), 'uniE721': p.glyph()}); fb.setupHorizontalMetrics({'.notdef':(1000,0),'uniE721':(1000,0)})
fb.setupHorizontalHeader(ascent=800, descent=-200); fb.setupNameTable({'familyName':'ReduceIcon','styleName':'Regular'})
fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200); fb.setupPost(); fb.save('reduce_icon.ttf')
```

## Pixels (reduced page, 800x600, DPR 1)

| Probe | What is there in Chrome | Chrome RGBA | Ours RGBA |
|---|---|---|---|
| **(48,8)** case B, `::before { content: "\E721" }` | glyph | `38,38,38,255` | `255,255,255,255` |
| (8,8) case A, `&#xE721;` as text (control) | glyph | `38,38,38,255` | `38,38,38,255` |
| (17,8) between the icons (control) | background | `255,255,255,255` | `255,255,255,255` |

## First difference

The `@font-face`, the PUA mapping and the glyph paint all work in ours: case A matches Chrome. Case B fails because **ours never decodes the CSS escape in the `content` string**. The display list has a text run whose text is the six characters `\E721` (no glyph run) where Chrome has the single character U+E721. Two checks:

- Writing the literal U+E721 character in the stylesheet makes (48,8) match Chrome (`38,38,38,255`).
- `content: "\41 B"` comes out as the text `\41 B` in ours. It should be `AB`.

The likely cause is `crates/rustkit-engine/src/lib.rs` around line 9316 (`"content" =>`), which strips the quotes and keeps the inside verbatim. CSS string escapes (`\` followed by 1–6 hex digits and an optional space, or `\` followed by any other character) are never processed. Icon-font CSS nearly always writes its codepoints this way, so every `::before` icon font on the board breaks.
