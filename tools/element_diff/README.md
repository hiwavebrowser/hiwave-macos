# element_diff — which element is wrong

A pixel score says a page differs from Chrome. `element_diff.py` says where:
it joins RustKit's layout dump to Chrome's element rects element by element,
reports both boxes and the delta for each, ranks the worst, points at the
first divergence in document order, and can draw the mismatches over our
frame.

It is a diagnostic, not a gate. Gate A (`scripts/layout_oracle_gate.py`) owns
pass/fail at 0.5px; this tool imports Gate A's rect choice (`border_box`,
which prefers `visual_border_box` and `fragment_union_border_box`) and its
skip list, so the two never disagree about what an element's box is.

Requirements: Python 3, Pillow for `--overlay` (`pip install pillow`), pytest
for the tests. The Chrome side needs `tools/parity_oracle` set up
(`npm ci` there).

## Run it on a fixture

The committed baseline already holds Chrome's rects:

```bash
cargo build --release -p parity-capture
OUT=tools/element_diff/out; mkdir -p $OUT        # out/ is git-ignored

./target/release/parity-capture \
  --html-file websuite/cases/card-grid/index.html --width 1280 --height 800 \
  --dump-layout $OUT/card-grid.layout.json --dump-frame $OUT/card-grid.ppm

python3 tools/element_diff/element_diff.py \
  $OUT/card-grid.layout.json \
  baselines/chrome-148/websuite/card-grid/layout-rects.json \
  --frame $OUT/card-grid.ppm --overlay $OUT/card-grid.overlay.png \
  --out $OUT/card-grid.diff.json
```

Width and height come from the case's entry in `cases/registry.json`.

## Run it on a URL

Chrome's rects for a URL come from `tools/parity_oracle/element_rects.mjs`,
which writes the same shape as `layout-rects.json` using the exact
`getSelector` from `capture_baseline.mjs`:

```bash
URL=https://example.com/
node tools/parity_oracle/element_rects.mjs "$URL" $OUT/site.chrome.json 1280 800 1500 \
  --png $OUT/site.chrome.png        # optional Chrome screenshot

./target/release/parity-capture --url "$URL" --width 1280 --height 800 \
  --dump-layout $OUT/site.layout.json --dump-frame $OUT/site.ppm

python3 tools/element_diff/element_diff.py $OUT/site.layout.json $OUT/site.chrome.json \
  --frame $OUT/site.ppm --overlay $OUT/site.overlay.png --only-worst 25 --out $OUT/site.diff.json
```

Use `PARITY_CHROME_PATH` for the pinned Chrome, as with every oracle script.
A live page changes between the two loads; differences in content (an ad, a
rotating hero) show up as unmatched or shifted elements and are not engine
defects.

`element_rects.mjs` also accepts an html file path instead of a URL; it then
uses the fixture context from `capture_baseline.mjs`, so its selectors match
the committed baseline exactly.

## Options

| flag | default | meaning |
|---|---|---|
| `--frame PNG/PPM` | none | our frame to draw on; without it the overlay is on white at the viewport size |
| `--overlay PATH` | none | write the overlay PNG |
| `--out PATH` | stdout | write the full JSON; a text summary then goes to stdout |
| `--tolerance PX` | 2 | an element whose every delta is within this counts as matching (green) |
| `--threshold PX` | 4 | an element with any delta above this is "wrong": ranked, drawn, and eligible as first divergence |
| `--only-worst N` | all | overlay draws only the N worst wrong elements |
| `--no-green` | off | overlay skips the green within-tolerance boxes |

## How elements are matched

In order of preference:

1. **id** — Chrome's `getSelector` emits `#id` for an element with an id, and
   the engine mirrors it.
2. **path** — the structural selector `body > div.card:nth-of-type(2) > p`
   (tag, up to two classes, nth-of-type among same-tag siblings). This is
   the same join key Gate A uses.
3. **path_noclass** — the same chain with classes removed, tried only for
   elements still unmatched and only when unique on both sides. It catches
   live pages where a script set a class on one side only.

A key that occurs more than once on either side is never used to pair; those
go in `ambiguous`. Boxes without an element (anonymous, text) are ignored, and
our boxes Chrome's capture would have dropped (a `script`/`style`/... tag, or
zero width and height) are counted as `ours_skipped`, not `ours_only`.

## What the numbers mean

All deltas are **ours minus Chrome**, in CSS px, on the border box
(Chrome's `getBoundingClientRect`). `dx`/`dy` are position, `dw`/`dh` size.

Per element (`elements[]`, in Chrome document order): `path`, `match_key`,
`tag`, `id`, `class`, `ours{x,y,w,h}`, `chrome{x,y,w,h}`, `dx dy dw dh`,
`max_error` (largest absolute delta) and `area_error`.

`area_error` is the area covered by exactly one of the two boxes (symmetric
difference), i.e. the pixels one engine gives the element and the other does
not. Zero iff the boxes are identical. It ranks a 1px error on a full-width
banner above a 1px error on an icon, and a big shift of anything above both.

`summary`:

- `chrome_elements`, `ours_elements`, `matched`, `matched_by` (count per key),
  `chrome_only`, `ours_only`, `ambiguous`
- `within_tolerance`, `within_tolerance_share` — matched elements whose
  every delta is ≤ tolerance, and their share of matched
- `over_threshold` — matched elements with any delta > threshold
- `first_over_threshold` — the first such element in document order. Later
  errors are often consequences of it.
- `first_over_threshold_non_ancestor` — the first wrong element none of whose
  descendants are wrong. The literal first is usually `body` or a wrapper
  whose height is off because something inside it is; this one is usually
  where the error enters. Start here.
- `worst` — the 20 worst by `area_error`
- `text_backend`, `text_metrics_font_derived`, and `warning` when our capture's
  text advances came from the non-font stub (Linux builds): every
  text-sized element is then measured against a stub, so its numbers say
  nothing about the engine on macOS.

`unmatched.chrome_only` / `unmatched.ours_only` list elements with no partner.
A Chrome-only element we did not lay out at all is often the actual bug.

## Overlay

Drawn on our frame: for each wrong element (over threshold), **red** is our
box and **blue** is Chrome's, labelled `tag.class dx,dy dw,dh`. Matched
elements within tolerance get a thin **green** outline. Elements between
tolerance and threshold are not drawn.

## Tests

```bash
python3 -m pytest tools/element_diff/tests -q
```

Synthetic JSON only; no capture, browser or network needed.
