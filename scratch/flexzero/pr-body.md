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

RECEIPT_PLACEHOLDER

🤖 Generated with [Claude Code](https://claude.com/claude-code)
