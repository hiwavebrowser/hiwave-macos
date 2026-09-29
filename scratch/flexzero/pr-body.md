Real-site trench, cross-platform bug 4 from Talos's Linux port: an authored zero width on a flex item was treated as `auto`.

Branched from #336's head, 62b05b0, which is now in develop (#336 merged as 9ac136a), so the diff is this one commit.

**Why.** `create_flex_item` resolved the item's `width`/`height` to pixels, then asked `if explicit_size == 0.0` to decide "no size, measure the content". So `width:0` and `width:0px` fell into the content-measure path. An empty box came out 18px wide, and a box with text came out as wide as its text (84.5px for "Overflowing" at 16px Arial). Chrome gives 0 for both (css-flexbox §9.2, and §4.5's specified size suggestion).

**What.**
1. A new `main_size_is_auto(length, resolved, main_is_definite)` decides whether the main size is `auto`. It's `auto` for `auto`/`fit-content`, and for a percentage when the container's main size is indefinite (an auto-height column; css-sizing-3 §5.1). calc/min/max/clamp still count as `auto` only when they resolve to 0, the old behaviour. An authored `0` in any absolute or relative unit is now a size.
2. §4.5's automatic minimum is now capped by the specified size suggestion on both axes: `min(content, specified size)`. The horizontal path had no cap. The vertical path (step 11d) capped only at `Length::Px` heights, so `height:0`, `em`, `vh` and `%` heights were ignored. The resolved border-box specified size now rides on `FlexItem::specified_main`, and step 11d reads it.

**Tests** (`flex_zero_size_tests` in rustkit-engine, both layout entry points: `layout()` and `layout_with_collapse`):
- `an_authored_zero_width_flex_item_is_zero_wide` (`0` and `0px`): 18 → 0. **Fails without the fix.**
- `a_zero_width_flex_item_with_text_stays_zero_wide`: 84.48 → 0. **Fails without the fix.**
- `a_zero_height_column_item_with_content_stays_zero_tall`: 30 → 0. **Fails without the fix.**
- `a_specified_width_caps_the_automatic_minimum` and `a_percent_height_in_an_auto_height_column_still_behaves_as_auto` are guards. They pass before and after, and they pin the two ways this fix could have regressed.

**Campaign receipt** (`scripts/parity_test.py`, default scope = the 21 micro cases + the 5 builtins: new_tab, about, settings, chrome_rustkit, shelf):
- This PR, run `2026-09-28T22:15:58` at head **6529dda**: **26/26 passed, avg diff_pct 1.1755%**.
- develop (binary built at 8567760), run `2026-09-28T22:15:02` back to back: 26/26, avg 1.1755%. **Every case is identical.** (8567760 predates #336. #336's own receipt was identical case for case, so that doesn't change the comparison.)
- CI's gates run locally (hub `scratch/shelf302/ratchet_local.py`): Gate A + Gate B + ratchet, **"RATCHET holds: none worse than the committed floor"**. Same 23 absolutely-red cases and the same paint scores as develop.

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | develop | this PR |
|---|---|---|
| new_tab | 1.5685 | 1.5685 |
| about | 3.6742 | 3.6742 |
| settings | 2.0818 | 2.0818 |
| chrome_rustkit | 1.1234 | 1.1234 |
| shelf | 1.0781 | 1.0781 |
| article-typography | 4.7857 | 4.7857 |
| card-grid | 1.3042 | 1.3042 |
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-elements | 0.9875 | 0.9875 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| image-gallery | 0.5217 | 0.5217 |
| sticky-scroll | 0.6575 | 0.6575 |
| backgrounds | 1.2163 | 1.2163 |
| bg-solid | 0.2106 | 0.2106 |
| bg-pure | 0.0000 | 0.0000 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2442 | 3.2442 |
| gradients | 0.1412 | 0.1412 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.6892 | 0.6892 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 1.3221 | 1.3221 |
| specificity | 0.6015 | 0.6015 |

</details>

**Real-site A/B** (back-to-back binaries, 62b05b0 = this PR's parent vs 6529dda, scored against the Chrome 148 captures of board run `20260928T2025Z-gridrem`): no check flips on any site.
- x, linkedin, facebook, instagram, yahoo, wikipedia, github, walmart, shopify: RustKit frames pixel-identical (squarespace 0.01%).
- google: moved 3.1% in one order and 0% in the reversed order, with both arms flipping together. That's served-content drift, not this change.
- netflix: 2–5% frame movement in both orders, LOOKS RIGHT 64.7% → 64.6% (its hero carousel; 0 points either way).
- lyft: a steady 1.26%. A hero text column ("Shift into earnings mode") that was laid out as invisible now shows at x=30. lyft scores 0 on LOOKS RIGHT in both arms (its frame is broken elsewhere: a dark background and a squeezed hero), so this is a layout change without a point change.

**Engine tests at 6529dda:** `cargo test -p rustkit-engine -p rustkit-layout -- --test-threads=1`: engine **204/204**, layout **554/554**, plus the layout integration tests. A parallel run at load ~15 threw ~20 timing failures in `web_font_tests` / `windows_a_leg_pins`. That's the same load-flake class as the previous PRs, none of it touches flex, and every sampled one passes alone.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
