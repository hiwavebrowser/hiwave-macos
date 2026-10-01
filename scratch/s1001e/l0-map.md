# L0 code map at develop 6eb6f5f (crates/rustkit-layout/src)

**An agent's read-only sweep, 2026-10-01 13:00. Line numbers are UNVERIFIED by Atlas: check each before relying on it.**
Design: `docs/LAYOUT_CONSTRAINTS_FRAGMENTS_2026-09-30.md` (on develop).

## a. Grid height contribution (call site 1)
- `grid.rs:235` `pub fn get_height_contribution(&self, container_height: f32) -> f32`
- `grid.rs:277` `fn estimate_content_height(&self) -> f32`
- `grid.rs:381` `fn count_text_lines(&self) -> usize` (one per `BoxType::Text` node, recursive, no wrapping)
- Inputs: `self.layout_box: &LayoutBox` (`GridItem`, `grid.rs:170`) and `container_height` only. No available width, no column size, no containing block.
- The only production caller is `grid.rs:1651` (Phase 5, `ItemSizing.height_contribution`); tests at 6878 and 6894. Argument is `height_for_contributions` (`grid.rs:1642`), 0.0 for an auto-height container.
- The estimate covers padding and margins but not border (test comment at `grid.rs:6924`).

## b. Phases 9 / 9.5 / 9.6 / 9.7
- Phase 9 (`grid.rs:1902`-`2251`) fills `real_heights: Vec<Option<f32>>` (1906), indexed by non-`display:none` child. Nested flex: `child.dimensions.content.height` after `layout_flex_container` (1919-1921). Nested grid: same after `layout_grid_container` (1925-1931). Block item: the flow cursor (2247).
- Phase 9.5 (`grid.rs:2253`-`2522`) runs only when `!has_definite_height`. Reads `real_heights`, `row_spans` (1781), child padding/border/margins, `style.height`, `style.min_height`, aspect ratio, `grid.rows[i].size`. Writes `grid.rows[i].size`/`.position`, `translate_subtree` on moved items, auto-height items' content height, the container's content height. A delta is applied only if it exceeds 0.5px (2434).
- Phase 9.6 (2524-2577) and 9.7 (2579-2617) both read `real_heights`.
- **Nothing records a per-item correction.** `item_sizings` (1644) and `real_heights` are both in scope during 9.5, which computes only a per-row `row_delta` (2424). The four-number receipt needs a new side channel.

## c. Flex (call site 2)
- `flex.rs:1988` `fn estimators_can_measure(layout_box: &LayoutBox) -> bool`; callers: itself (1996) and `fit_content_cross_width` (2044).
- `flex.rs:2039` `fn fit_content_cross_width(layout_box: &LayoutBox, available_border_box: f32, cross_pb: f32) -> f32`. Single caller `flex.rs:1893` inside `calculate_cross_sizes` (1865), reached only when the cross axis is horizontal, the item has no explicit cross size, and its alignment is not `stretch`.
- Unmeasurable result: `get_content_cross_width(layout_box) + cross_pb` (2045), the previously laid-out content width. Same fallback when max-content is <= 0 (2049).

## d. Estimators
- `grid.rs:2636` `estimate_min_content_width`, `grid.rs:2662` `own_min_content_width`, `grid.rs:2861` `form_control_min_content_width(style, control)`.
- `grid.rs:2958` `estimate_max_content_width`, `grid.rs:2977` `own_max_content_width` (form-control arm at 3008 returns `form_control_intrinsic_size(..).0`).
- `lib.rs:231` `form_control_intrinsic_size(style, control) -> (f32, f32)`.

## e. Real layout, and speculative precedent
- `flex.rs:257` `pub fn layout_flex_container(container: &mut LayoutBox, container_box: &Dimensions)`; wraps `layout_flex_container_in` (286, adds `positioning_cb`) and `layout_flex_container_at` (305, adds `used_inner_height`). Every production caller passes `container.dimensions.clone()`.
- `grid.rs:1346` `pub fn layout_grid_container(container: &mut LayoutBox, container_width: f32, container_height: f32)`.
- Both mutate in place and assume a block pre-pass (`lib.rs:1917`-`1933`) or Phase 8 already wrote the item's dimensions.
- **No precedent for laying a subtree out speculatively.** `intrinsic_cache.rs` has lookup/store keyed `(element_id, style_ptr, mode)` with no available-size component and no caller found outside its file.
- `LayoutBox: Clone` (`lib.rs:1471`) arrived with #404; its one use is the engine's tree-reuse memo.

## f. Fixtures and tests
- Mosaic: `websuite/holdout/holdout-grid-mosaic/index.html`, `baselines/chrome-148/holdout/holdout-grid-mosaic/`.
- `.shortcuts`: `crates/hiwave-app/src/ui/new_tab.html:198` (CSS), `:348` (markup); reduced repro `parity-tests/repro/grid-flex-kbd-rows.html`.
- Unit tests in `grid.rs`: `shortcut_row` helper 6975; `an_auto_row_shrinks_to_its_items_real_height` 7008; `a_minmax_row_with_a_definite_floor_does_not_shrink` 7054; `the_row_repair_pass_measures_against_the_row_less_the_margins` 6934; contribution tests 6870, 6886; aspect-ratio tests 7469, 7494.

## g. Types
No `ConstraintSpace`, `Fragment` or `LayoutUnit` exists. `FIT_EPSILON = 1.0 / 64.0` is at `text.rs:1839`. `writing_mode` is never read in rustkit-layout.

## Risks / surprises (each needs checking before the design is followed as written)
1. **No inline size at call site 1.** Height and width contributions are computed in one `map` (`grid.rs:1644`-`1654`) before columns are sized (1690). The L0 query needs the item's column width, so for in-slice items the block contribution has to be taken after column sizing.
2. **Borrow conflict.** During Phase 5 `items` holds `&LayoutBox` borrows of `container.children` (dropped at 1790). Real layout needs `&mut`: clone the subtree (a boxed ~1.5 KB style per node) or restructure.
3. **A cloned box needs its dimensions seeded** the way Phase 8 (1792-1898) does, because the flex entry point reads them from the box.
4. **Mosaic is out of class.** `.tile` is a block grid item whose flex `.row` sits under `.inner`. By the doc's section 4 it keeps the old path, so the L0 fixture cannot be a faithful reduction of it.
5. **`.shortcut` holds `kbd` and `span`, no form control.**
6. **Call site 2 is narrower than "a flex axis"**: only the horizontal cross axis of non-stretch auto-width items. Main-axis intrinsic sizing goes through `get_intrinsic_main_size` (`flex.rs:2591`), which `estimators_can_measure` does not gate.
7. **Phase 9.5 never runs for a definite-height grid and ignores deltas <= 0.5px.** "Delta is 0" needs that tolerance stated; the old estimate excludes border and a fragment's border box includes it.
8. **Index alignment unverified**: `items` is sorted by `order` (1448) while `positions`, `row_spans` and `real_heights` are indexed by child order.
9. No `BoxType::Image` arm was found in the width estimators, so an unsized image may still estimate 0.
