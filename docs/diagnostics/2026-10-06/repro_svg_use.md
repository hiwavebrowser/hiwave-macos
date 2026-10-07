# Reduced repro: svg `<use>` paints nothing (#565 extend)

- **Finding:** Census #572 icon technique 7, "svg use local" (Pollux draft `repro_svg_use.html` from #565, extended here).
- **Source sites:** youtube.com (player buttons, B) and the Census sprite pattern (A). cnn.com also uses local `<use>`, inside `<mask>` in the logo (`use < mask#prefix__b < g < g < svg.cnn-logo-dark`), not reduced here.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216** (mac_arm64, `scripts/realsite_board.py` `PINNED_CHROME_VERSION`), headless, 1280x800 for the live probe, 800x600 and DPR 1 for the reduced page, `--force-color-profile=srgb`.
- **Ours:** `parity-capture --html-file` built from `0c3b3cde` (develop `50e77c83` plus 2 navigation-only commits; the engine is the same as develop `3c1f48ee`).
- **Page:** [`repro_svg_use.html`](repro_svg_use.html), 12 lines.

## What the live page has (youtube.com, pinned Chrome on macOS)

15 `<use>` elements, all local (`#…`). Every one is a player-button "shadow":

```
use.ytp-svg-shadow < svg < div.ytp-copylink-icon < button.ytp-button.ytp-copylink-button
  < div.ytp-chrome-top-buttons < div.ytp-overlay-top-right < div.ytp-overlays-container
<svg viewBox="0 0 36 36" width="100%" height="100%">
  <use class="ytp-svg-shadow" xlink:href="#ytp-id-25"></use>
  <path class="ytp-svg-fill" id="ytp-id-25" d="M21.9,8.3H11.3…"></path>
</svg>
```

Computed on the `use`: `fill: none; stroke: rgb(0,0,0); stroke-opacity: 0.15; stroke-width: 2px`. The target is a **sibling `<path>`** reached by **`xlink:href`**, not a symbol in hidden `<defs>`. The path paints the white glyph and the `use` paints a 15% black outline around it. (The reduced page uses opaque black and a 4-unit stroke so the probe pixel is clear.)

## Pixels (reduced page, 800x600, DPR 1)

| Probe | What is there in Chrome | Chrome RGBA | Ours RGBA |
|---|---|---|---|
| **(25,25)** case A, centre of the `<use href="#icon">` circle | green circle | `0,128,0,255` | `255,255,255,255` |
| **(18,86)** case B, stroke ring outside the path | black stroke painted by the `use` | `0,0,0,255` | `255,255,255,255` |
| (36,86) case B, inside the path (control) | white path fill | `255,255,255,255` | `255,255,255,255` |

## First difference

Ours paints nothing for `<use>`, whichever form is used (`href` or `xlink:href`, a symbol in hidden `<defs>` or a sibling). The sibling `<path>` itself paints at the right place: the display list has one `FillPolygon (20,70)-(52,102)`, the same box Chrome reports for `path` and `use`. So the svg viewport and viewBox transform are fine. The engine says so too: a test in `rustkit-engine` reads "`<use>` is unresolved in the renderer". The fix needs `<use>` to clone its target into the use's coordinate space, with the use's `fill`/`stroke` inherited by the clone.

## Side observation (not this finding)

Ours treats `svg { display: block }` as inline for whitespace. Two block svgs with a newline between them get an 18px anonymous line (`text " "` at y=50), so the second one lands at y=68 where Chrome puts it at y=50 (Chrome `0,0,255,255` vs ours `255,255,255,255` at (25,60) on a two-block-svg page). The reduced page positions its svgs absolutely so this doesn't affect the probes.
