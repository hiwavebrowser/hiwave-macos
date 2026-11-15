# E54b: srcset / sizes / <picture> selection, measured in Chrome

Page: `e54b_srcset_matrix.html` (16 `<img>` cases, 4 inside `<picture>`), images `i{50..800}.png` made by
`e54b_srcset_matrix_make.py` (a PNG N px wide, N/2 px tall). `e54b_srcset_matrix_run.py` drives headless Chrome
(`--headless=new --force-device-scale-factor=<1|2> --window-size=<w>,900 --dump-dom`) and prints `currentSrc`,
`naturalWidth`, `naturalHeight`, `width`, `height` per image. Raw answers: `e54b_srcset_matrix_chrome157.json`.

**Oracle: Google Chrome 157.0.8103.38 (installed stable, Windows).** Chrome for Testing 148 was NOT available on
the box that measured this. Source selection is defined by the HTML spec ("select an image source") and has not
changed between 148 and 157, but these are 157 numbers; re-run the script against the pinned 148 to confirm.
Viewport widths Chrome reported: 500 and 784 at DPR 1, 492 and 784 at DPR 2 (headless chrome insets).

What the numbers say (the rules the unit tests pin):
- `x` candidates: the smallest density >= DPR wins, else the largest. `naturalWidth` is density-corrected: a 200 px image
  picked as 2x has `naturalWidth` 100.
- No descriptor means 1x. `src` is added as a 1x candidate when no candidate is 1x (case c4: DPR 1 picks `src`,
  not the 1.5x candidate).
- `w` candidates: density = w / slot width, where the slot is the evaluated `sizes` (default `100vw`); the same
  smallest-covering rule applies at `DPR x slot`, and `naturalWidth` equals the slot width in CSS px.
- `<picture>`: the first `<source>` whose `media` matches, whose `type` is supported and whose `srcset` is non-empty
  decides, with its own `sizes`; otherwise the `<img>`'s own `srcset`/`src`.
- A `width` attribute or CSS width does not change the pick (c8, c16).
