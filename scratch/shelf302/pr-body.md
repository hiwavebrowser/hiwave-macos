## What

`resolve_flexible_lengths` now follows css-flexbox-1 §9.7. Before, it measured free space from each item's **hypothetical** (min/max-clamped) main size and ran one pass with no freeze loop:

- **The automatic minimum leaked into the split.** Two `flex: 1 1 0%` items, one holding a 50px box, came out 175/225, where Chrome has 200/200. §4.5 lifted that item's hypothetical width to its 50px min-content, and the free space was measured from that. This is the x-shape box at x=262.5 vs Chrome's 275 (#300's follow-up).
- **Grow never clamped at min.** In a 400px row, `flex:1 1 0%; min-width:250px` with two basis-0 siblings gave 300/50/50. Chrome gives 250/75/75.
- **Max violations never redistributed.** A `max-width:50px` item with two siblings gave 50/133/133. Chrome gives 50/175/175.
- **Shrink never redistributed a min violation.** Two 300px bases in 400, one with `min-width:250`, gave 250/200 (overflowing). Chrome gives 250/150.
- **Grow factors summing below 1 took all the free space.** Two `flex-grow:.25` items gave 200/200. Chrome gives 100/100.

**Third commit (d3b745d): the vertical automatic minimum (§4.5), which fixes CI's `shelf` ratchet regression.** A column flex item with `min-height: auto` and `overflow-y: visible` couldn't shrink below its content before, only because the old resolver's quirks hid that there was no floor. `min_main` was 0 on the vertical axis, since there was no min-content height. Under the §9.7 resolver, the shelf's `flex: 1` command palette shrank to fit the 120px body, and a second pass then squeezed it to its padding (24px). Its input row went to 2px and its results box to 0. CI's paint ratchet: **shelf 0.94173 → 0.65814**.

**Fourth commit (7b3187b): the content measure treats a percentage height as `auto`.** The first board run at d3b745d (`20260927T1430Z-automin`) cost x its READABLE point (95.3% → 55.8%). The words it lost were exactly the footer's. x's `h-full` login widget sits inside an auto-height `min-h-[440px]` wrapper and lays out at the 800px viewport, which is a separate, pre-existing percentage-height bug. Reading that box as content floored x's `flex-1` main row at 1011, and the footer landed at y=1011. The measure now treats a percentage height as `auto` (CSS 2.1 §10.5: an intrinsic size never resolves one against the indefinite height it's computing), recurses through block-level children instead of reading their boxes, and honours a px `min-height`. x's footer is back at y=752, as in Chrome 148.

## How

- Pick the grow or the shrink factor from the hypothetical sum.
- Freeze inflexible items (factor 0, or a base already past the hypothetical size in the flex direction).
- Measure free space from **base** sizes, with the base floored at the item's padding+border, and scale it when the unfrozen factors sum to less than 1.
- Distribute: grow by factor; shrink by factor × **inner** base size.
- Clamp (min wins), then freeze the min or max violators, and loop until every item is frozen.
- Step 11d now moves a content-sized item's `flex_basis` along with its re-derived hypothetical height, because a content basis *is* the content size.
- **§4.5 vertical (d3b745d):** `create_flex_item` flags items the automatic minimum applies to (vertical main axis, `min-height: auto`, `overflow-y: visible`, non-replaced). Step 11d computes each flagged item's content height from its laid-out subtree with `content_border_height`: a column flex container sums its items, a single-line row takes its tallest item, and anything else takes the extent of its in-flow children. That height becomes the item's `min_main_size`, capped by a definite `height` (specified size suggestion) and by `max-height`, and the 11d rerun honours it. The item's own `content.height` can't be the measure, because step 11 has already written the used size there.

**Compatibility guard (second commit).** When the hypothetical sizes already fill the line, the resolver returns without touching targets, as the old code did. On a first pass this is spec-equivalent, because §9.7 ends with every item at its hypothetical size. It stays for items whose content 11d can't read.

## Tests

`flex_resolve_tests.rs` has 8 failing-first cases and 2 guards, each run through `layout` and `layout_with_collapse`. Every number was measured on pinned Chrome for Testing 148 (`getBoundingClientRect`, 400px viewport), or taken from the committed baseline rects:

- The 6 §9.7 cases above. All fail on develop, and so does the x-shape test in `flex_item_relayout_tests.rs`, which now also asserts Chrome's x = 275 where develop gives 262.5.
- `a_column_item_is_not_shrunk_below_its_content`: a `flex: 1 1 0%` item holding a 100px box in a 50px column stays 100. `min-height: 0` gives 50, and so does `overflow-y: auto`. Without d3b745d: 50.
- `the_shelf_palette_overflows_at_its_content_height`: the shelf's structure, with the expected numbers from `baselines/chrome-148/builtins/shelf/layout-rects.json` (palette 135, results 56). Without d3b745d: palette 79, results 0.
- Guard `a_percentage_height_descendant_does_not_raise_the_automatic_minimum`: x's shape (a `flex: 1` row, an auto-height `min-height` wrapper, a `height: 100%` child, then a footer). **It passes without 7b3187b**, because the unit path already resolves that percentage as `auto`. The 800px only happens through the engine on the live page. I checked 7b3187b end to end on x.com instead: footer y 1011 → 752, the same as Chrome.

Results:
- rustkit-layout 540/540 at 7b3187b (`text::font_resolve_tests::a_new_web_font_set_invalidates_the_cache` is the known parallel flake and passes alone)
- rustkit-engine 165/165

## Shelf, reproduced locally (Gate B and Gate A, CI's scripts on this seat's captures)

| | develop | e1b58bd | d3b745d / 7b3187b | Chrome 148 |
|---|---|---|---|---|
| Gate B paint, within ±5 | 96.68% | **65.81%** | 96.68% | — |
| Gate A geometry fails (of 9 compared) | 2 | 5 | **0** | — |
| palette height | 164 | 24 | **135** | 135 |
| results height | 85 | 0 | **56** | 56 |

The Linux CI floor is 0.94173. macOS reads 0.9668 for both develop and d3b745d. CI's `ratchet_gate.py`, run locally over all 26 captures at d3b745d and again at 7b3187b, exits 2 ("none worse than the committed floor"), and it lists shelf as tighten-eligible.

**Why the earlier campaign receipt didn't catch it:** `shelf` *is* one of the 26 cases, and the receipt showed 2.87% → 3.23%. `diff_pct` and Gate B weigh the same change very differently: the receipt moved 0.36 points, and Gate B's within-±5 fraction moved 31. I haven't pinned down why the gap is that large. I only flagged the 0.36 as "worse", and it didn't look blocking. From now on the receipt includes CI's gates (`layout_oracle_gate.py` / `paint_oracle_gate.py` / `ratchet_gate.py`) run over the local captures, not only `diff_pct`.

## Real-site board (hub `atlas/trench-realsite`, headed pinned CfT 148)

| run | head | points | loads | readable | looks-right |
|---|---|---|---|---|---|
| `20260927T0805Z-stack` | develop-equivalent | 20 | 13 | 6 | 1 |
| `20260927T1215Z-basis-full` | e1b58bd | 20 | 13 | 6 | 1 |
| `20260927T1430Z-automin` | d3b745d | 19 | 13 | 5 | 1 |
| `20260927T1510Z-automin2` | **7b3187b** | **19** | 13 | 6 | 0 |

Sites that moved across the last three runs (e1b58bd / d3b745d / 7b3187b):
- **x 2 / 1 / 2.** d3b745d cost x its footer words (READABLE 55.8%), and 7b3187b restores them (95.3%). That's the fourth commit above.
- **google 2 / 3 / 2.** LOOKS RIGHT went 18.x → 10.5 → 18.1%, the same swing as yesterday's doodle day (18.5%). Not attributed.
- **yahoo 2 / 1 / 1.** Its +1 on e1b58bd was already called drift (the same-hour control arm was `unstable`, Chrome-vs-Chrome 20.8%). LOOKS RIGHT is 27.6% now.

**Net: 20 → 19, within the board's ±1 noise bound.** No site lost a point to this PR that I can attribute to it.

## Campaign

**Campaign receipt (gate 5):** `scripts/parity_test.py` (all scopes), run 2026-09-27T10:53:18 local on this branch (head 7b3187b's tree; d3b745d's run at 10:06:03 gave the same numbers). **26/26 passed, avg diff_pct 1.2534%**, the same as #300's receipt (develop-equivalent for fixtures). **26/26 cases identical, shelf included** (3.2324 at e1b58bd → 2.8711).

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | diff_pct | #300 |
|---|---|---|
| `new_tab` | 1.5685 | 1.5685 |
| `about` | 3.7492 | 3.7492 |
| `settings` | 2.0854 | 2.0854 |
| `chrome_rustkit` | 1.1234 | 1.1234 |
| `shelf` | 2.8711 | 2.8711 |
| `article-typography` | 4.7857 | 4.7857 |
| `card-grid` | 1.2958 | 1.2958 |
| `css-selectors` | 1.4975 | 1.4975 |
| `flex-positioning` | 0.6793 | 0.6793 |
| `form-elements` | 0.9875 | 0.9875 |
| `gradient-backgrounds` | 1.0133 | 1.0133 |
| `image-gallery` | 0.5217 | 0.5217 |
| `sticky-scroll` | 0.6575 | 0.6575 |
| `backgrounds` | 1.2163 | 1.2163 |
| `bg-solid` | 0.2106 | 0.2106 |
| `bg-pure` | 0.0000 | 0.0000 |
| `combinators` | 0.6547 | 0.6547 |
| `form-controls` | 3.2442 | 3.2442 |
| `gradients` | 0.1412 | 0.1412 |
| `gradient-no-radius` | 0.5204 | 0.5204 |
| `gradient-radius-only` | 0.6892 | 0.6892 |
| `gpu-gradient-regression` | 0.3903 | 0.3903 |
| `images-intrinsic` | 0.3267 | 0.3267 |
| `pseudo-classes` | 0.4341 | 0.4341 |
| `rounded-corners` | 1.3221 | 1.3221 |
| `specificity` | 0.6015 | 0.6015 |

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
