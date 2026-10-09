# H22: google.com search box, rainbow conic-gradient block and black rectangle

- **Finding:** Atlas H22. RustKit draws an opaque rainbow conic-gradient block over google.com's search box, with a black rectangle at the box's right end. Atlas also saw the box 688px wide in RustKit against 584px in Chrome, and the footer at y=520 instead of the bottom of the viewport.
- **Live page:** www.google.com on 2026-10-08 (World Space Week doodle), loaded at **1280x800**, DPR 1, with a Chrome 148 desktop User-Agent and en-US, as for H13. Google changed the page today, so the board drop is the new markup, not a regression.
- **Oracle:** pinned Chrome for Testing **148.0.7778.216**, headless.
- **Ours:** RustKit `parity-capture --profile parity` built at develop **`6c9265db`** (Merge PR #628). The live page was loaded with `--url`, and the reduced page with `--html-file`, both with `--dump-layout --dump-display-list --dump-frame`.
- **Page:** [`google_searchbox_conic_h22.html`](google_searchbox_conic_h22.html) (14 lines, view at **800x600**). It keeps today's class names, the rules that matter and the resolved custom property values.

## Live page: what the layers are (Chromium computed style)

Two copies of the same "glow" component sit in the search box. Each has two `.eruMcc` layers (the second is `.eruMcc.eOuHwe`, which uses another mask), and each holds `.Tdahud > .tgPjse`:

1. **Box glow:** `div.A8SBwf > div.BznTFe > div.fZhNMe > div.WkMYIb > div.eruMcc > div.Tdahud > div.tgPjse`, the full 688px width of the box.
2. **AI Mode button glow:** `button.plR5qb > div.CcxW7b.BznTFe > div.fZhNMe > …` at the right end of the box. A sibling `div.bvUkz` is the button face, and its `::before` is a hover overlay.

| Layer | Chromium rect (offset box) | background | mask-image / mask-composite | opacity | position / inset / size | border-radius | z-index | filter / transform | animation |
|---|---|---|---|---|---|---|---|---|---|
| `.BznTFe` (box glow root) | 296,326 688x124 | none | none | 1 | absolute, inset 0 | 24px (`var(--BgHDjb)`) | **0** | none; `transition: height .15s` | none |
| **`.fZhNMe`** | 296,326 688x124 | none | none | **0** | absolute, inset 0 | inherit | auto | `will-change: --q9niGe, --cqcjz, opacity` | none |
| `.eruMcc` | 0,0 688x124 | none | none | 1 | absolute, inset `var(--HM63Tc,0)` = 0, **overflow hidden** | inherit | auto | `filter: blur(var(--bFlrOb,5px))` = blur(5px); `.eOuHwe`: blur(1px); `transform: translateZ(0)` | none |
| `.Tdahud` | 0,0 688x124 | none | `conic-gradient(from var(--q9niGe), transparent 0, transparent 50%, black 68%, black 75%, transparent 89%), conic-gradient(from var(--q9niGe), black 30%, transparent 50%, black 70%)`; `.eOuHwe .Tdahud`: first stops `transparent 62%, black 82%, transparent 89%`; **mask-composite: intersect** (`-webkit-mask-*` the same) | 1 | absolute, inset 0, `scale: var(--sjdAce,7) var(--mWus4b,1.5)` = 7 1.5 | inherit (24px) | auto | none | none |
| `.tgPjse` (the rainbow) | 0,62 688x**688** | `conic-gradient(rgb(49,134,255) 34%, rgb(147,120,255) 37%, rgb(249,107,214) 39%, rgb(252,65,61) 41%, rgb(252,65,61) 48%, rgb(255,107,43) 50%, rgb(254,199,0) 52%, rgb(255,219,15) 56%, rgb(136,222,66) 58%, rgb(14,188,95) 61%, rgb(14,188,95) 65%, rgb(46,170,178) 70%, rgb(0,169,187) 72%, rgb(49,134,255) 73%, rgb(49,134,255) 83%, rgb(49,134,255) 100%)` | none | 1 | absolute, inset 0, `inset-block-start: 50%`, `translate: 0 -50%`, `aspect-ratio: 1/1` | 50% | auto | `rotate: var(--cqcjz)` = 0deg; `transform: translateZ(0)`; `backface-visibility: hidden` | none |
| `.CcxW7b.BznTFe` (AI Mode glow root) | 879,334 96x36 | none | none | **0.6** | absolute, inset 0 | 100px | 0 | none | none |
| AI Mode `.fZhNMe` / `.Tdahud` | 96x36 | none | as above, `from 200deg` (`--q9niGe: 200deg`) | `.fZhNMe` **0** | `scale: 4 1.5` (`--sjdAce: 4`) | 100px | auto | as above | none |
| `.bvUkz` (AI Mode button face) | 879,334 96x36 | `background-color: rgb(243,245,246)`; computed `background-image: none` | none | 1 | relative | 100px | auto | none | none |
| **`.bvUkz::before`** (hover overlay) | 879,334 96x36 | `background-color: #000` | none | **0** (`.plR5qb.VzUPFe:not(.Sw4CSc):hover .bvUkz::before` sets 0.0824) | absolute, inset 0, `content: ""` | inherit | auto | `transition: opacity 50ms` | none |

**Custom properties used, as resolved by Chromium:**
- On `.Tdahud`: `--q9niGe: 0deg` (set on `.fZhNMe`; `200deg` in the AI Mode glow), `--sjdAce: 7` (on `.BznTFe`; `4` in the AI Mode glow) and `--mWus4b: 1.5`.
- On `.tgPjse`: `--cqcjz: 0deg`.
- On `.eruMcc`: `--HM63Tc: 0` and `--bFlrOb: 5px`.
- On `.BznTFe`: `--BgHDjb: 24px`.
- On `.bvUkz`: `--aim-button-gradient-start` and `--aim-button-gradient-end` both `rgb(243,245,246)`, from `var(--XKMDxc)` = `#f3f5f6`.

**`@property` rules (inline sheet):** there are two, `--aim-button-gradient-start` and `--aim-button-gradient-end`, each `syntax: "<color>"`, `inherits: false`, `initial-value: transparent`. They exist so `.bvUkz`'s `radial-gradient(85.92% 80.66% at 34.54% 11.11%, var(--aim-button-gradient-start) 0, var(--aim-button-gradient-end) 81.89%)` can transition over 50ms. The glow variables (`--q9niGe`, `--cqcjz`) are **not** registered. No keyframes or animations are running on any of these layers at load.

**Box and footer, live, 1280x800:**

| Box | Chromium | RustKit |
|---|---|---|
| `div.RNNXgb` (search box) | 296,326 **688x52** | 296,236 **688x98** |
| `div.A8SBwf` | 296,326 688x124 | 296,236 688x186 |
| box glow `.tgPjse` | offset 688x688 (rotated, scaled 7x1.5 by the parent, clipped) | 296,329 688x**93** |
| footer `div.g55egf.xQQEXb` | 0,**754** 1280x46 (viewport bottom) | 0,**490** 1280x46 |

With today's markup Chromium's box is also **688px** wide, so the 584 vs 688 width gap Atlas saw doesn't reproduce today. The box is 98px tall in RustKit against 52 in Chromium. The footer sits 264px high, the same symptom the H13 note traced to percentage heights in the `.plsC5e` column flex (`docs/diagnostics/2026-10-06/h13_h15_search_boxes.md`). Neither of these causes the gradient, and neither is reduced here.

## Reduced page

The page reproduces the live symptom exactly: in RustKit, a rainbow block over the box and a black rectangle over the AI Mode button. In Chromium nothing is drawn.

`#box` is the `.RNNXgb` search box, `#aim` is the `.bvUkz` button face, `#glow` is `.BznTFe`, `#fade` is `.fZhNMe` (opacity 0), `#masked` is `.Tdahud`, and `#conic` is `.tgPjse`.

| | Chromium 148 | RustKit `6c9265db` |
|---|---|---|
| `#box` rect | 56,100 688x52 | 56,100 688x52 |
| `#aim` rect | 639,108 96x36 | 639,108 96x36 |
| `#glow` / `#fade` rect | 56,100 688x124, `#fade` opacity 0 | 56,100 688x124 |
| `#masked` rect (offset box) | 56,100 688x124 (transformed bounds -2008,69 4816x186) | 56,100 688x124 |
| `#conic` rect (offset box) | 0,62 **688x688** (transformed bounds -2008,-354 4816x1032) | 56,162 **688x62** |
| **pixel (687,110)**, in the AI Mode button | **243,245,246** (button face; the `::before` is opacity 0) | **0,0,0** (the `::before` painted opaque) |
| pixel (687,126), in the button | 243,245,246 | 26,182,125 (the glow, over the button) |
| **pixel (300,126)**, inside the box | **255,255,255** | **252,65,61** (the glow, over the box) |
| pixel (400,200), the glow area below the box | 255,255,255 | 49,134,255 |
| RustKit display list | | `rounded_rect` box, `rounded_rect` #f3f5f6 button, **`solid_color` #000 96x36** (`::before`), `push_clip` + `push_transform` (7, 1.5) + **`conic_gradient` 688x62**, with no opacity or mask op anywhere |

### Which feature causes it: `opacity` on a box is not painted

Two scratch variants of the page settle it. Neither is committed.

| Variant | Chromium pixels (687,110) / (300,126) / (400,200) / (100,180) | RustKit pixels |
|---|---|---|
| as committed (`opacity: 0` on `.fZhNMe` and `.bvUkz::before`) | 243,245,246 / 255,255,255 / 255,255,255 / 255,255,255 | **0,0,0 / 252,65,61 / 49,134,255 / 49,134,255** |
| `opacity: 0` → `visibility: hidden` | 243,245,246 / 255,255,255 / 255,255,255 / 255,255,255 | **243,245,246 / 255,255,255 / 255,255,255 / 255,255,255**: matches Chromium |
| `opacity: 0` removed | 0,0,0 / 255,255,255 / 255,255,255 / 33,170,182 | 0,0,0 / 252,65,61 / 49,134,255 / 49,134,255: **identical to the committed page** |

RustKit renders the committed page exactly as if `opacity` weren't there. With `visibility: hidden` instead, it matches Chromium pixel for pixel. On the same build, a plain `<div style="opacity: 0; background: #000">`, and the same at `opacity: .5`, also paint solid black: `opacity` is parsed into the style but only images use it at paint, so no box background or subtree is faded. **Root cause: `opacity` (0 here) on non-image boxes is ignored at paint.** That one gap produces both the rainbow block (`.fZhNMe { opacity: 0 }` around the conic glow) and the black rectangle (`.bvUkz::before { opacity: 0; background: #000 }`). It isn't `@property`: the glow variables aren't registered, and the registered `<color>` pair resolves to the same `#f3f5f6` in both engines. It isn't the conic gradient either: RustKit paints `conic-gradient` correctly, and Chromium's own layer is the same rainbow.

### Problems that will show once opacity works (not the cause today)

The "opacity removed" row shows what Chromium draws when the glow is visible on interaction: the glow under the box, masked down to a thin rim, here (100,180) = 33,170,182. RustKit would still differ there in three ways:
1. **`mask-image` / `mask-composite: intersect` aren't applied.** No mask op appears in the display list, so (400,200) is solid blue where Chromium's mask leaves white.
2. **Stacking:** `.BznTFe { z-index: 0 }` paints over `.RNNXgb { position: relative; z-index: 3 }`, its earlier sibling. At (300,126) inside the box, Chromium shows white and RustKit shows the gradient.
3. **`aspect-ratio: 1` on an absolutely positioned box with `inset: 0`:** `.tgPjse` is 688x62 in RustKit and 688x688 in Chromium.

`filter: blur(5px)` shows up as an `unknown` display-list op and was not checked.

## Reuse

The fix for H22 is box opacity. After it, re-measure this page: RustKit should give (687,110) = 243,245,246, and white at (300,126), (400,200) and (100,180). The live page should lose the rainbow block and the black rectangle. Items 1–3 above are follow-ups for when Google shows the glow (focus or hover on AI Mode). No live-site screenshots are committed.
