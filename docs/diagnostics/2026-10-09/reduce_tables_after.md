# Tables, AFTER evidence: cell rects on develop with #624, #619 and #630 landed

Docs only, no engine code. Complements `docs/diagnostics/2026-10-08/table_2x2.html`, `table_spans.html` and `table_infobox.html` (#630), which state expected sizes in comments but record no cell rects. These pages add ids, borders, border-spacing, `caption-side: bottom`, and a float-width table; each cell's `getBoundingClientRect` is recorded below.

- **Measured:** develop `f54103d9` (includes #630 `ca45f458`, #624, #619), RustKit `parity-capture --profile parity`, 800x600, DPR 1.
- **Oracle:** pinned Chrome for Testing 148.0.7778.216, headless, `getBoundingClientRect()` per id.
- **Pages:** `reduce_table_2x2_bordered.html`, `reduce_table_float_infobox.html`, `reduce_table_spans_cells.html` (all 10 lines or fewer).

## Result: tables now lay out. Two small differences remain

### (a) 2x2 bordered table, `border: 3px`, `border-spacing: 4px`, `caption-side: bottom`

Every column width, row height and cell size matches. The one difference is the caption: **RustKit ignores `caption-side: bottom`** and puts the caption above the rows.

| id | Chromium 148 x,y,w,h | RustKit `f54103d9` |
|---|---|---|
| `#t` | 8,8 95.59x**90** | 8,**28** 95.57x**70** (the table box excludes the caption) |
| `#cap` | 8,**78** 95.59x20 | 8,**8** 95.57x20 |
| `#a` | 15,15 30.23x26 | 15,**35** 30.22x26 |
| `#b` | 49.23,15 47.36x26 | 49.22,**35** 47.35x26 |
| `#c` | 15,45 30.23x26 | 15,**65** 30.22x26 |
| `#d` | 49.23,45 47.36x26 | 49.22,**65** 47.35x26 |
| pixel (20,20) | 255,238,153 (cell A) | **221,221,221** (caption) |
| pixel (20,85) | 221,221,221 (caption) | **255,238,153** (cell C) |

Widths and heights of the cells (30.23/47.36 by 26) match to 0.01px, so borders, padding and border-spacing are right; only the caption position is wrong (every cell is 20px lower, the caption's height).

### (b) `float: right` table, `width: 22em` (14px font), 3px spacing, 1px border, `colspan` header

Table and cell widths match. Everything sits **2px lower** in RustKit, the paragraph included, which is not a table problem.

| id | Chromium 148 | RustKit `f54103d9` |
|---|---|---|
| `#t` | 484,**14** 308x74 | 484,**16** 308x74 |
| `#h` (colspan 2) | 488,18 300x20 | 488,20 300x20 |
| `#k1` / `#v1` | 488,41 100.36x20 / 591.36,41 196.64x20 | 488,43 100.36x20 / 591.36,43 196.64x20 |
| `#k2` / `#v2` | 488,64 100.36x20 / 591.36,64 196.64x20 | 488,66 100.36x20 / 591.36,66 196.64x20 |
| `#p` (text wraps left of the float) | 8,14 784x48 | 8,16 784x48 |
| pixel (600,14) / (600,16) | 162,169,177 (table border) / 248,249,250 | 255,255,255 / 162,169,177 |

Cause of the 2px: RustKit's UA `p` margin is a fixed 16px, not `1em`. With `body { font: 14px }` Chromium's `p` margin is 14px (collapsed with the body margin, so y=14), RustKit's is 16. Check: a bare `<p>` under the same body is at y=14 in the Chromium oracle and y=**16** in RustKit; `<p style="margin:1em 0">` is at y=14 in RustKit (matches).

### (c) colspan + rowspan, `border-spacing: 0`, 1px cell and table borders

All six rects match exactly.

| id | Chromium 148 = RustKit `f54103d9` |
|---|---|
| `#t` | 8,8 180x98 |
| `#a` (rowspan 2) | 9,9 56x64 |
| `#b` (colspan 2) | 65,9 122x32 |
| `#c` / `#d` | 65,41 76x32 / 141,41 46x32 |
| `#e` (colspan 3) | 9,73 178x32 |

pixels (20,20) and (40,28): 204,255,204 in both.

## Not covered here

The 3px stray black frame around a thumb image and the figure holding a video collapsing to a ~100px caption need a real image/video resource and are not reproduced by these pages; they remain open for the Wikipedia thumb pages.
