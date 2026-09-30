## What

CSS Color 5 `light-dark(<light>, <dark>)` was not understood anywhere, so every declaration that used it was dropped.

linkedin serves two CSS bundles at random. The newer one (`assets/*.css`, 1.3 MB, all `@layer`) turns up about 1 fetch in 10. It defines **411 theme colours** as `--x: light-dark(var(--a), var(--b))`, and uses them through `color`, `background` and `border` shorthands. On that variant, RustKit painted links UA blue and buttons UA grey, and painted no section backgrounds.

The fix is in `resolve_css_variables`, the one path that sheet, inline-style and recorded declarations all go through. After `var()` substitution, each `light-dark(L, D)` call is replaced by `L`.
- **Why on the substituted value:** the function almost always arrives through a custom property inside a shorthand (`border: 1px solid var(--c)`), where the colour parser never sees it on its own.
- **Why the light argument:** RustKit renders the light scheme and does not track `color-scheme`, so on a light page the used scheme picks the light arm. That is what pinned Chrome 148 shows. A subtree that opts into `color-scheme: dark` is out of scope. linkedin has a few (`[data-color-scheme="dark"]`); a real `color-scheme` would be its own change.
- A call without exactly two arguments is left alone, so the declaration stays invalid and is dropped as before. A name that only ends in `light-dark(` (`my-light-dark(`) is not touched. Nested calls resolve.

**Also in this PR:** the same two-line #379 compile follow-up as #381/#382 (`&layout_box.style` in the two pseudo calls). #382 has merged it, so this merges cleanly.

## Pins

`light_dark_tests`:
1. `the_light_argument_is_used`: plain, inside a shorthand, nested, twice in a shadow list, plus invalid arities and `my-light-dark(` left alone. **Fails first.**
2. `a_light_dark_custom_property_colours_text_background_and_border`: linkedin's shape (`--fg: light-dark(var(--l), var(--d))` used in `color`, `background` and the `border` shorthand). **Fails first** (black).
3. `a_literal_light_dark_applies_in_sheets_and_inline_styles`. **Fails first.**

rustkit-engine 314/314 (headless, serial).

## linkedin's layered variant, offline

The page is saved with its sheet inlined and scripts dropped (hub `scratch/li0930/li-new.html`), then rendered by the release build and compared with pinned Chrome 148 on the same file. The colours now match: nav links `#0A66C2`, the Sign-in pill, and the `#F3F2EE` section band.

**Honest, pixel-wise it's worse on this variant for now:** 50.7% → 74.0% of first-viewport pixels differ. The variant has a separate layout problem: the hero doesn't render, and the footer row is drawn near the top. Now that the band has its colour, it paints over the area where Chrome has the white hero. That layout cause is the next item. The old variant (the one linkedin serves ~9 fetches in 10) contains no `light-dark()`, and it is unaffected. Live A/B below.

## Campaign receipt

Head `eef161e` on develop `1a016c4`. **The develop arm is `315fbb3` + the compile fix** (the arm built for #382 this session). A second develop build didn't fit (this one took 34 min at load 12–24). Between the two bases, only #380 (cascade layers) differs, and its own receipt was identical to develop.

- `parity_test.py --scope all`: develop run 2026-09-30T12:47:25, fix run **2026-09-30T13:52:49**. 26/26 and 26/26 pass, avg 1.176% vs 1.176%. **26/26 identical case for case.**
- `--scope builtins`: develop 2026-09-30T12:47:38, fix 2026-09-30T13:53:33: **5/5 identical** (avg 1.905%).
- `scratch/shelf302/ratchet_local.py`: output **identical** on both arms. Both exit 2 (tighten-eligible only), which is develop's existing state.

<details><summary>all: per-case diff_pct (dev-315fbb3 vs ld-eef161e)</summary>

| case | dev-315fbb3 | ld-eef161e |
|---|---|---|
| new_tab | 1.57% | 1.57% |
| about | 3.67% | 3.67% |
| settings | 2.08% | 2.08% |
| chrome_rustkit | 1.12% | 1.12% |
| shelf | 1.08% | 1.08% |
| article-typography | 4.79% | 4.79% |
| card-grid | 1.31% | 1.31% |
| css-selectors | 1.38% | 1.38% |
| flex-positioning | 0.63% | 0.63% |
| form-elements | 0.99% | 0.99% |
| gradient-backgrounds | 1.01% | 1.01% |
| image-gallery | 0.52% | 0.52% |
| sticky-scroll | 0.66% | 0.66% |
| backgrounds | 1.22% | 1.22% |
| bg-solid | 0.21% | 0.21% |
| bg-pure | 0.00% | 0.00% |
| combinators | 0.65% | 0.65% |
| form-controls | 3.24% | 3.24% |
| gradients | 0.14% | 0.14% |
| gradient-no-radius | 0.52% | 0.52% |
| gradient-radius-only | 0.69% | 0.69% |
| gpu-gradient-regression | 0.39% | 0.39% |
| images-intrinsic | 0.33% | 0.33% |
| pseudo-classes | 0.43% | 0.43% |
| rounded-corners | 1.32% | 1.32% |
| specificity | 0.60% | 0.60% |

</details>

<details><summary>builtins: per-case diff_pct (dev-315fbb3 vs ld-eef161e)</summary>

| case | dev-315fbb3 | ld-eef161e |
|---|---|---|
| new_tab | 1.57% | 1.57% |
| about | 3.67% | 3.67% |
| settings | 2.08% | 2.08% |
| chrome_rustkit | 1.12% | 1.12% |
| shelf | 1.08% | 1.08% |

</details>

WPT tier-1 not run: `third_party/wpt` is not synced in this worktree.

## Real-site board

linkedin, one interleaved round (develop = A, `eef161e` = B), load 13, runs `trench/realsite/runs/20260930T1800Z-abld-{1-A,2-B}`: both arms got the old variant, and both score **3/3** (LOOKS RIGHT 8.0% / 7.6%). **±0 points.** This fix matters only when linkedin serves the layered variant.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
