## What

CSS Display 3 §2.7: the in-flow children of a flex or grid container are **blockified**, so `display` computes to its block-level equivalent (`inline`/`inline-block` → `block`, `inline-flex` → `flex`, `inline-grid` → `grid`). RustKit left a `<span>` flex item as `BoxType::Inline`. Its height then came from the inline-box path: the font's content area (Arial 8px → 8.9375), with `line-height` ignored.

Athena found this on Windows through #323's test (exchange athena #419/#420). The fix is one match at box construction in `rustkit-engine` (`build_layout_from_document`'s element path). Text runs stay anonymous flex items. The enum has no `inline-table`, so there's nothing to map for it.

## Tests (4 new, through both layout entry points: `layout()` and `layout_with_collapse`)

- `a_span_flex_item_is_its_line_height_tall_not_the_fonts_content_area`: Athena's repro, flex > span at `8px/8px Arial` = 8.
- `a_span_flex_item_honours_a_small_line_height`: `line-height:5px` → 5.
- `a_span_grid_item_is_blockified`: a grid item span computes `Block`, 8 tall.
- `inline_level_items_compute_to_their_block_level_equivalent`: inline-flex/inline-grid/inline-block items become flex/grid/block, and an inline-block outside the container stays inline-block.
- The existing inline-svg test now expects its flex-item `<svg>` to compute `Block`, as the spec requires.

The authoring session verified the four tests fail without the fix. `cargo test --release -p rustkit-engine --lib`: **188/188** at c39a4e6.

## Campaign receipt

`parity_test.py` run `2026-09-28T11:11:25` at head **c39a4e6** (scope `all`, which includes the builtins new_tab/about/settings/chrome_rustkit/shelf): **26/26 passed, avg 1.1756%**. The develop-equivalent is #323 cd1e854 (develop's layout minus #322's cascade-only change): 26/26, avg 1.1782%. 24/26 cases are identical.

- `about` 3.7492 → **3.6742** (better).
- `card-grid` 1.2958 → **1.3042** (+0.008, worse by diff_pct). This is sub-pixel antialiasing, not geometry. Every `span.tag` rect is 23px tall, as in Chrome, and the 4th card's tag moves y 695.875 → 695.80 (Chrome **695.78**), i.e. toward Chrome. The 166 changed pixels are the pill edges in rows 676–688.

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | this PR | develop (#323 cd1e854) |
|---|---|---|
| `new_tab` | 1.5685 | 1.5685 |
| `about` | 3.6742 | 3.7492 |
| `settings` | 2.0854 | 2.0854 |
| `chrome_rustkit` | 1.1234 | 1.1234 |
| `shelf` | 1.0781 | 1.0781 |
| `article-typography` | 4.7857 | 4.7857 |
| `card-grid` | 1.3042 | 1.2958 |
| `css-selectors` | 1.3819 | 1.3819 |
| `flex-positioning` | 0.6327 | 0.6327 |
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

**CI gates run locally** (`ratchet_local.py`, Gate A + Gate B + ratchet) at c39a4e6 and at #323 cd1e854: exit 2 (tighten-eligible) on both, and **none worse than the committed floor**. Against #323: `about` paint 0.93867 → **0.93939**, and `settings` geo_fails 252 → **250**. `card-grid` paint 0.82475 → 0.82473 (−0.00002; the antialiasing above). shelf 0.98398 and chrome_rustkit 0.96893 are unchanged. Every other case is identical.

## Real-site board (±0; the fix changes no board frame)

Full board `20260928T1710Z-blockify` at c39a4e6: **18/60** (loads 10, readable 5, looks-right 3), vs 21/60 for #323's run `20260928T1305Z-pr323`. **The −3 is machine load, not this change.** The other two trench lanes were building (load 15–25), and six Chrome oracle captures timed out. Rows that moved:

| site | #323 | this PR | cause |
|---|---|---|---|
| google | 1 | 3 | its oracle failed in #323's run and came back |
| facebook | 2 | 1 | Chrome drift: the RustKit frame is **pixel-identical** to #323's, and LOOKS RIGHT goes 5.6% → 15.2% on the oracle alone |
| netflix | 2 | 0 | RustKit 30s timeout inside the 4 MB framework script (JS, not layout), plus a Chrome screenshot timeout |
| github | 1 | 0 | RustKit 30s timeout under load |
| cnn | 1 | 0 | RustKit 30s timeout under load |

**Back-to-back A/B, same binaries, same minute** (#323 cd1e854 vs this PR):
- Wall time: netflix 25.0 / 22.1 s, github 54.1 / 54.1 s, cnn 33.9 / 34.4 s. The same-ordered pairs taken during load spikes (up to 25) read noisier (github 51–59 vs 68 s), but both orders were run, and at load ~14 the two binaries tie. The display-list op count on github is identical (382,446).
- Frames: weather, wikipedia, apple, bing and linkedin are **pixel-identical** between the two binaries. linkedin's first pair differed by 29.8%, but that was its served-variant drift: two repeat pairs in reverse order were identical.

So on the live board the fix is ±0. It's a correctness fix for every `<span>`-in-flex/grid with a `line-height` smaller than the font's content area (Athena's Windows case, and #323's test).

## Not run

- `wpt_tier1.py` (corpus not synced on this seat).
- `cargo clippy` (needs interactive approval on this seat).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
