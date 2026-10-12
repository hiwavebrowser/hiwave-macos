# inspector — which element diverges from Chrome, and at which stage

`element_diff` says which element's box is wrong. The inspector goes one step
further: it compares a full **element diagnostics record** from each engine
(identity, tree state, layout-deciding computed style, boxes, paint facts) and
names the first element that diverges and the **stage** where it diverges:

| stage | the records differ in | look at |
|---|---|---|
| `DOM` | a missing or extra element, the tag, the child count, or the text length (beyond a tolerance) | parsing, scripts, component construction |
| `STYLE` | a computed property that can matter for this element | stylesheet loading, selectors, cascade, inheritance |
| `GEOMETRY` | a box, by more than the threshold (default 2px) | layout and sizing |
| none | nothing either side knows about | paint, clipping, transforms, fonts |

```
ours    <-- parity-capture --dump-diagnostics ours.json                       (RustKit)
chrome  <-- node tools/inspector/chrome_records.mjs <page> chrome.json         (Chromium)
            python3 tools/inspector/inspector_diff.py ours.json chrome.json
```

Both files use one record shape, `hiwave-element-diagnostics/1`, defined in
`crates/rustkit-engine/src/diagnostics.rs`. The Chrome side fills it from
`getComputedStyle`, `getBoundingClientRect` and what script can work out about
containing blocks, clipping and opacity. A field one side cannot know is
`null`, with a reason under `unavailable`. The diff never counts an unknown as
a difference.

## Run it on a fixture

```bash
cargo build --release -p parity-capture
(cd tools/parity_oracle && npm ci)        # playwright; see "Chromium" below
OUT=tools/inspector/out; mkdir -p $OUT    # out/ is git-ignored
CASE=websuite/cases/card-grid/index.html

./target/release/parity-capture --html-file $CASE --width 1280 --height 800 \
  --dump-diagnostics $OUT/ours.json --dump-frame $OUT/ours.ppm
node tools/inspector/chrome_records.mjs $CASE $OUT/chrome.json --width 1280 --height 800
python3 tools/inspector/inspector_diff.py $OUT/ours.json $OUT/chrome.json \
  --out $OUT/diff.json --frame $OUT/ours.ppm --overlay $OUT/overlay.png
```

For a URL, use `parity-capture --url URL` and `chrome_records.mjs URL ...`
(add `--settle MS`; the default is 1500). A live page changes between the two
loads, so content differences show up as DOM roots and are not engine defects.

## chrome_records.mjs

`node tools/inspector/chrome_records.mjs <url|html-file> <out.json> [--width 1280] [--height 800] [--scale 1] [--settle 1500] [--max-elements 5000] [--keep-query]`

- **Page setup** is `element_rects.mjs`'s. An html file gets
  `capture_baseline.mjs`'s deterministic context (and the parity reset for
  micro cases), so its keys match the committed baselines. A URL gets the
  real-site context.
- **`oracle_key`** is `getSelector`, read verbatim out of
  `capture_baseline.mjs` (the extractor `verify_selector_key.mjs` pins).
- **Element order** is tree order, with open shadow trees after their host.
  The harness's own `style[data-parity-freeze]` is skipped and removed from
  its ancestors' child counts and text lengths.
- **Fields script cannot know** are null with a reason:
  - `identity.node_id` (no node ids in script)
  - `paint.display_commands` and `paint.command_kinds`
  - `box.grid_lines` (Chromium does not expose resolved placement)
- **Privacy:**
  - the URL loses its query and fragment unless `--keep-query`
  - text is a length only
  - every `url(...)` in `background-image` (inside `image-set()` too) loses
    its credentials, query and fragment, and `data:` URLs lose their
    contents
  - a `file:` URL becomes a path relative to the page
  - nothing reads cookies, storage or form values
  - a local file is named by the path as given (pass it repo-relative)

## inspector_diff.py

| flag | default | meaning |
|---|---|---|
| `--out PATH` | stdout | write the JSON result there; the text report then goes to stdout |
| `--threshold PX` | 2 | a border-box (then content-box) delta above this is `GEOMETRY` |
| `--text-tolerance N` | 2 | text-length difference tolerated; the tolerance is `max(N, 1% of Chrome's length)` |
| `--include-head` | off | classify elements inside `<head>` too (they are always matched) |
| `--frame`, `--overlay` | none | draw the GEOMETRY roots on our frame, with element_diff's overlay (red ours, blue Chrome); needs Pillow |

### Matching

Matching is element_diff's `match()` with its keys, in order: `id`, then
`path` (the oracle key), then `path_noclass`. A key duplicated on either side
goes under `matching.ambiguous` and is never paired by key.

Structural keys pair children by position (`:nth-of-type`), so one removed
or added sibling would shift every later sibling onto its neighbour's
partner. Under any parent whose children did not pair one-to-one, the
children are therefore **realigned**:
- the two child lists are sequence-aligned on (tag, id, classes, text length)
- within a gap of equal length, same-tag children pair in order (the same
  element with changed text)
- each new pair's subtree is realigned the same way

`matching.realigned` counts the pairs this changed, and their `match_key` is
`aligned`. Before matching, the
class part of a key is normalised to the first two classes joined by dots.
The engine mirrors the `getSelector` that captured the committed baselines
(`div.card featured`); today's `getSelector` writes `div.card.featured`.

### Classification

Classification walks Chrome's elements in document order and stops at the
first stage that differs:

- **DOM:**
  - a missing element (`chrome_only`)
  - an extra element (`ours_only`; always a root)
  - a different `tag` or `child_count`
  - a different `text_length` beyond the tolerance
- **STYLE** compares only properties that can matter for this element (also:
  `float`/`clear` are ignored on flex and grid items, `vertical-align` is
  compared only on inline-level boxes and table cells, and `object-fit` only
  on replaced elements):
  - **Never compared**, because they are used values that come out of layout:
    `width`, `height`, `margin-*`, `padding-*`, `top/right/bottom/left/inset`.
    Their effect shows up as GEOMETRY.
  - **Flex/grid container** properties only on flex/grid containers.
  - **Item** properties (`flex-*`, `align-self`, `justify-self`, `order`,
    `grid-row/column-*`) only on items.
  - **`z-index`** only on positioned boxes and items.
  - **A `display:none` element** is compared on `display` only.
  - **Size limits and overflow are ignored on inline boxes.**
  - **Normalisation:**
    - colours become `rgba(r, g, b, a)`
    - a bare `0` becomes `0px`
    - numbers are rounded to 0.01
    - font family lists are compared unquoted and in lower case
  - **Equivalent keywords** are treated as the same value:
    - `justify-content: normal ≡ flex-start` (not compared on a grid
      container when either side is `normal`, which behaves as stretch there)
    - `justify-content`/`align-items`: `start ≡ flex-start`, `end ≡ flex-end`
      (the engine maps them when it parses)
    - `align-items: normal ≡ stretch`
    - `z-index: 0 ≡ auto` (the engine stores auto as 0)
    - `text-align: start ≡ left`
    - `font-weight: normal ≡ 400`
    - gaps: `normal ≡ 0px`, per value (`normal 5px ≡ 0px 5px`)
    - `min-*: auto ≡ 0px`
    - `display: list-item ≡ block`
    - `transparent ≡ rgba(0, 0, 0, 0)`
  - **`grid-template-*`** on a grid container is compared only when both
    sides are plain px lists. The engine has no resolved tracks: it reports
    the specified list, or `none` for implicit tracks. Chromium reports the
    resolved px tracks.
  - **`transform`** matrices are compared to within 0.01. An identity matrix
    is equal to `none`, and a 2D `matrix3d` to its `matrix`.
  - **`background-image`** compares a URL's last path segment (Chromium
    resolves URLs, the engine keeps the specified text) and gradients by
    kind (`-webkit-` prefix dropped).
- **GEOMETRY:** `has_box` differs, or the border box (else the content box)
  is off by more than the threshold.

A **divergence root** is a divergent element that no other divergence
explains. The others are **consequences**: they get `root: false` and
`attributed_to` (the selector of the element that explains them), and they
are left out of `roots` and the counts. A divergence is a consequence when:

| rule | example |
|---|---|
| its parent diverges at an earlier stage | a box that moved under a STYLE-divergent flex container |
| GEOMETRY: it only follows its parent. Its shift equals the parent's content-box shift, or that plus the earlier siblings' size changes (or half or all of the parent's width change, for centred or end-aligned content). Its width change is no larger than the parent's, in the same direction, and its height is unchanged. Heights flow up, not down, so a height change is never explained from above. | the flex cells of a narrower row; everything under a body with more padding |
| STYLE: every differing property is inherited, and equal on both sides to the parent's value | the cards of a section with a different `color` |
| DOM: it is an ancestor whose only differences are child count and text length, and a DOM root lies below it | the parent of a missing element |
| GEOMETRY: it is an ancestor whose only difference is its height, and a GEOMETRY root lies below it | `html` and `body`, taller because a paragraph is |

### Output (`schema: hiwave-inspector-diff/1`)

The JSON is written with sorted keys and holds no timestamps, so the same two
inputs always give the same bytes.

| key | contents |
|---|---|
| `first` | the first root in document order |
| `first_innermost` | the first root with no root of the same stage below it; **start here.** Ancestors that only grow with their content are already consequences, so this is usually the same as `first`. |
| `roots` | the top 20 roots by `affected_area`. For GEOMETRY that is the area covered by exactly one of the two boxes (element_diff's `mismatch_area`); for DOM and STYLE it is the element's larger box area. |
| `summary.divergent_elements` | how many elements diverge, roots or consequences |
| `summary.roots` | how many roots |
| `summary.consequences` | how many divergences are explained by another (`divergent_elements - roots`) |
| `summary.stage_counts` | `{DOM, STYLE, GEOMETRY}`: roots per stage. **This is the per-site "failure cause" the board publishes.** |
| `summary.style_properties` | STYLE roots counted by property name (one root can count under several) |
| `summary.geometry_contexts` | GEOMETRY roots by formatting context: `block`, `inline`, `float`, `flex item`, `grid item`, `table cell`, `abspos`, `fixed`, `no box` |
| `matching` | counts, the `ambiguous` list, and `truncated` (either input hit its element cap) |
| `page`, `settings` | the two URLs and engines, the viewport, our `text_metrics_font_derived`, the thresholds |

Each root in `first`, `first_innermost` and `roots` carries:
- `stage`
- `selector` (Chrome's structural selector) and `oracle_key`
- `tag`
- `match_key`: `id`, `path`, `path_noclass` or `aligned`; `null` for an extra element
- `formatting_context`
- `affected_area`
- `order`: the position in Chrome's records; `null` for an extra element, which only ours has
- `root` (always `true` in these lists) and `attributed_to` (`null` for a root)
- `fields` A field is one of:

- `{property, ours, chrome}` for STYLE
- `{field, ours, chrome, dx, dy, dw, dh}` for a box
- `{field, ours, chrome}` otherwise

A Linux capture uses the stub text shaper (`page.text_metrics_font_derived:
false`). Every text-sized GEOMETRY root on such a capture is measured against
a ruler, not a font, and the text report warns about it.

## Chromium

Install Playwright's Chromium with
`cd tools/parity_oracle && npm ci && npx playwright install --with-deps chromium`,
or point `PARITY_CHROME_PATH` at a Chrome binary, as with every oracle script.
Do not commit browsers.

## Tests

```bash
python3 -m pytest tools/inspector/tests -q
```

`tests/pages/planted.html` covers block, flex, grid, abspos and repeated
classes. Its Chromium record is committed as
`tests/fixtures/planted.chrome.json`. Each test copies that record as "ours",
plants one difference (a removed or added element, a tag, a text length, a
colour, a flex-grow, a box delta, a duplicated key), and checks that the tool
names the right element, stage and fields. Other tests check that equivalent
serialisations and irrelevant properties produce no difference.
