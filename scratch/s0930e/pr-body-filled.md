## What

`height: fit-content` is sized by content, exactly as `height: auto` is. Three places in `rustkit-layout/src/flex.rs` asked "is the height anything but `auto`?" and so treated it as a definite, already-resolved height. `main_size_is_auto` in the same file already puts the two together for the flex basis; these were the places that did not. Two commits, one per side of the flex relationship.

**1. A `fit-content` flex ITEM in a column (7a7300a).** `content_border_height` computes the vertical automatic minimum (css-flexbox-1 §4.5, `min-height: auto`) from an item's laid-out content. `fit-content` fell to its "already resolved" arm, which returns the box's own height. For a `flex: 1` item that is the 0 it was just squeezed to, so the minimum was 0 and the item stayed 0 tall, with its content painted over by the next sibling.

**2. A `fit-content` flex CONTAINER (06926e5).** `has_definite_cross_size` and step 12's "leave an explicit height alone" both counted it as definite. The height they then used was whatever the block pre-pass had stacked the items to, so a row's line stretched over it: six 32px pills on one line made a 192px container with every pill 192 tall.

## Where it shows

linkedin's layered-CSS variant (served on roughly 1 fetch in 10) uses both shapes. The page body is `flex: 1; display: grid; height: fit-content` inside an auto-height column (0 tall around 5697px of content, so the footer painted over the first section), and each topic-pill row is `display: flex; flex-wrap: wrap; height: fit-content` (each pill 570 tall instead of 32).

Probes are on the hub branch under `scratch/s0930e/probe/`; rects are border boxes, Chrome's from pinned Chrome for Testing 148.

| probe | measured | Chrome 148 | develop 570e25d | this PR |
|---|---|---|---|---|
| `a-all`: column > `flex:1; display:grid; height:fit-content` | item height / next sibling y | 80 / 80 | 0 / 0 | 80 / 80 |
| `d-nogrid`: column > `flex:1; height:fit-content` | item height / next sibling y | 80 / 80 | 0 / 0 | 80 / 80 |
| `p-fit-block`: wrapping row, `height:fit-content` | container / pill height | 32 / 32 | 192 / 192 | 32 / 32 |
| `p-fit-narrow`: the same at 200px (two lines) | container / pill height | 72 / 32 | 192 / 192 | 72 / 32 |
| `p-fit-in-col`: the same as a column's item | container / pill height | 72 / 32 | 18 / 192 | 72 / 32 |
| `p-fit-in-row`: the same as a row's item | container / pill height | 72 / 32 | 392 / 192 | 72 / 32 |
| 10 controls (each property alone, `auto`, plain blocks) | | | match Chrome | unchanged |

**linkedin's saved layered page, offline (honest).** Hub `scratch/li0930/li-new.html` (sheet inlined, scripts dropped), first-viewport diff against Chrome 148: develop **82.9%**, this PR **78.2%**. The section now has Chrome's structure (heading left, pill rows right, footer no longer on top of it), but the number barely moves because the hero above it is still missing, which shifts everything by about 530px. That is a different cause, found this session and not in this PR: the hero is shown by `display: revert-layer`, which the cascade does not implement. Resolving that one declaration by hand in the saved page gives 52.7% with this PR.

With commit 1 alone the offline number is 62.7%, lower than the final 78.2%. That is not a better render: without commit 2 the section is 2639px of flat grey (it now encloses the 570px pills), and flat grey happens to match more of Chrome's hero than the correct section does. Stated so the 62.7 is not read as a regression from commit 2.

**Not fixed here (same on develop, disclosed):**
- `g-all-parent300`: when the column is definite and the grid item grows to 280, Chrome stretches the grid's own item to 280 and RustKit leaves it at 80. A grid container resized by its parent flex is not laid out again at its used height (only nested flex containers are).
- `p-auto-grid1fr` / `p-fit-grid1fr`: a `grid-template-rows: 1fr` grid around a wrapping row sizes its row from the pre-pass stack (next sibling at 192, Chrome 32). `height: auto` does the same on develop.
- `p-fit-in-row`: the outer row container's own height is 0 (next sibling at 0, Chrome 72), on develop too.

## Pins

Both in `flex_resolve_tests`, each through both entry points (`layout` and `layout_with_collapse`), each **fails first**:
- `a_fit_content_height_column_item_keeps_its_content` (item as a block and as a grid container): 80 tall, next box at 80. Without commit 1: 0.
- `a_fit_content_height_flex_container_is_sized_by_its_lines` (wrapping and not): pills and container 32. Without commit 2: 192.

`rustkit-layout` lib: 577/578 in the parallel run. The one failure is `a_new_web_font_set_invalidates_the_cache`, a shared-cache test that passes when run alone (checked) and has flaked the same way in earlier parallel runs; it does not touch flex.

## Campaign receipt

- `parity_test.py --scope all`: develop run 2026-09-30T20:55:54, fix run **2026-09-30T21:37:08**. 26/26 and 26/26 pass, avg 1.1622% vs 1.1622%. **26/26 identical case for case.**
- `--scope builtins`: develop 2026-09-30T20:56:40, fix 2026-09-30T21:37:46: **5/5 identical** (avg 1.9052% vs 1.9052%).
- `scratch/shelf302/ratchet_local.py`: output **identical** on both arms (`settings: geo_fails=243 paint=0.95071 discrete=0`). Both exit 2, which is develop's existing state.

<details><summary>Per-case diff_pct (all 26), develop 570e25d vs fix 06926e5</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3068 | 1.3068 |
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2442 | 3.2442 |
| form-elements | 0.9875 | 0.9875 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5685 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 1.3221 | 1.3221 |
| settings | 2.0819 | 2.0819 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

WPT tier-1 not run: `third_party/wpt` is not synced in this worktree.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
