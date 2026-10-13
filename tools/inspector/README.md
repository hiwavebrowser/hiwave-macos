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
- **Padding and content boxes** are the bounding rect less computed border
  and padding, so they are null under a scaling, rotating or skewing
  transform on the element or an ancestor.
- **Privacy** (`record_scrub.mjs`, applied in Node before writing):
  - the page URL loses user name and password, and its query and fragment
    unless `--keep-query`
  - text is a length only
  - every `url()` in `background-image` (including inside `image-set()`)
    loses user name, password, query and fragment; `data:` URLs become
    `data:…`; `file:` URLs become a path relative to the page
  - nothing reads cookies, storage or form values
  - a local page is named by a relative path: as given when relative,
    otherwise relative to the working directory, otherwise its file name
  - `nav_error` and `error` name the target as the record does: a URL
    quoted in an error message loses its query and fragment
- **Exit status:** 1 when navigation failed (threw, or got no response for
  an http(s) URL; nothing is recorded, since what loaded is Chromium's own
  error page) or the page answered HTTP 400 or above (the records of the
  site's error page are still written).

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
goes under `matching.ambiguous` and is never paired. Before matching, the
class part of a key is normalised to the first two classes joined by dots.
The engine mirrors the `getSelector` that captured the committed baselines
(`div.card featured`); today's `getSelector` writes `div.card.featured`.

### Shifted siblings

The oracle key numbers same-tag siblings (`:nth-of-type`), so one removed or
added sibling shifts the keys of those after it. Wherever a paired parent has
an unpaired child, its same-tag children are re-aligned in order on (tag, id,
classes, text length within the DOM tolerance). A pair is kept only when the
leftmost and the rightmost longest alignments agree on it. An element they
disagree on (one of several identical siblings went missing, and which one
cannot be told) is counted in `matching.unverified` and not classified: the
parent's `child_count` still reports the difference. Descendants of re-paired
elements pair by their path below them (`match_key: "aligned"`).

### Classification

Classification walks Chrome's elements in document order and stops at the
first stage that differs:

- **DOM:**
  - a missing element (`chrome_only`)
  - an extra element (`ours_only`)
  - a different `tag` or `child_count`
  - a different `text_length` beyond the tolerance, not counting the text of
    `<style>` and `<script>` descendants
- **STYLE** compares only properties that can matter for this element:
  - **Never compared**, because they are used values that come out of layout:
    `width`, `height`, `margin-*`, `padding-*`, `top/right/bottom/left/inset`.
    Their effect shows up as GEOMETRY.
  - **Flex/grid container** properties only on flex/grid containers.
  - **Item** properties (`flex-*`, `align-self`, `justify-self`, `order`,
    `grid-row/column-*`) only on items.
  - **`z-index`** only on positioned boxes and items.
  - **A `display:none` element** is compared on `display` only.
  - **Size limits and overflow are ignored on inline boxes.**
  - **No effect, not compared:** `float`/`clear` on flex and grid items,
    `vertical-align` outside inline-level boxes and table cells, `object-fit`
    outside replaced elements.
  - **Normalisation:**
    - colours become `rgba(r, g, b, a)` (names `black`/`white`/`transparent`
      and hex too); every fully transparent colour is `rgba(0, 0, 0, 0)`
    - a bare `0` becomes `0px`
    - numbers are rounded to 0.01
    - font family lists are compared unquoted and in lower case
  - **Equivalent keywords** are treated as the same value:
    - `justify-content: normal ≡ flex-start`, and `start`/`end` ≡
      `flex-start`/`flex-end` for `justify-content`, `align-items`,
      `align-self` (the engine maps them on parse)
    - `align-items: normal ≡ stretch`
    - `text-align: start ≡ left`, `end ≡ right` (LTR), `-webkit-center ≡ center`
    - `font-weight: normal ≡ 400`
    - gaps: `normal ≡ 0px`, per side (`gap: normal 5px ≡ 0px 5px`)
    - `min-*: auto ≡ 0px`, except on flex and grid items
    - `z-index: auto ≡ 0` (the engine stores both as 0)
    - `display: list-item ≡ block`
  - **`grid-template-*`** is compared only when both sides are plain px
    lists. `none` on a grid container (Chromium reports the implicit tracks)
    and lists with line names are not compared.
  - **`transform`**: the identity matrix is `none`, an affine `matrix3d` is a
    `matrix`, and matrices are compared to within 0.01.
  - **`background-image`**: a relative URL (the engine keeps it as
    specified) equals Chromium's absolute one when the absolute URL ends with
    it; a rooted or `../` path is not guessed at. `-webkit-` gradients are
    named like the unprefixed ones.
- **GEOMETRY:** `has_box` differs, or the border box (else the content box)
  is off by more than the threshold.

A **divergence root** is a divergent element that is not a consequence of
another divergence. These are consequences, not roots:

- anything under a parent that diverges at an *earlier* stage (GEOMETRY
  under a STYLE-divergent parent), and anything inside a missing or extra
  element;
- a GEOMETRY difference that its neighbours' deltas explain. Each axis of the
  border box moved with the parent's content box (or an abspos box's
  containing block), anywhere within the room that box gained or lost, or
  with the previous sibling's far edge; the width grew or shrank with the
  parent's content box; the height equals the change in its in-flow
  children's extent, with its own padding and border unchanged. A
  difference in the content box alone is always the element's own;
- a STYLE difference in inherited properties whose values equal the
  parent's on both sides (a root keeps only its own properties in `fields`).

Known over-count: items stretched to a flex line's height all change with
the tallest item, and each stays a root, because the records cannot say
which item set the height.

### Output (`schema: hiwave-inspector-diff/1`)

The JSON is written with sorted keys and holds no timestamps, so the same two
inputs always give the same bytes.

| key | contents |
|---|---|
| `first` | the first root in document order |
| `first_innermost` | the first root with no root of the same stage below it. The literal first is often `html` or a wrapper whose height is off because of something inside it; **start here.** |
| `roots` | the top 20 roots by `affected_area`. For GEOMETRY that is the area covered by exactly one of the two boxes (element_diff's `mismatch_area`); for DOM and STYLE it is the element's larger box area. |
| `summary.divergent_elements` | how many elements diverge, roots or not |
| `summary.roots` | how many roots |
| `summary.stage_counts` | `{DOM, STYLE, GEOMETRY}`: roots per stage. **This is the per-site "failure cause" the board publishes.** |
| `summary.style_properties` | STYLE roots counted by property name (one root can count under several) |
| `summary.geometry_contexts` | GEOMETRY roots by formatting context: `block`, `inline`, `float`, `flex item`, `grid item`, `table cell`, `abspos`, `fixed`, `no box` |
| `matching` | counts, the `ambiguous` list, `unverified` (elements left unclassified by sibling re-alignment, both sides), and `truncated` (either input hit its element cap) |
| `page`, `settings` | the two page names (each as its record gives it) and engines, the viewport, our `text_metrics_font_derived`, the thresholds |

Each root in `first`, `first_innermost` and `roots` carries: `stage`,
`selector` (Chrome's structural selector), `oracle_key`, `tag`, `match_key`
(`id`, `path`, `path_noclass`, or `aligned`), `formatting_context`,
`affected_area`, `order` (the index of the element in Chrome's record, or
null for an extra element of ours), `root` (always true in these lists), and
`fields`. A field is one of:

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
serialisations and irrelevant properties produce no difference, that a
consequence is not counted as a root, and (through node, skipped without it)
that `record_scrub.mjs` scrubs background-image URLs.
