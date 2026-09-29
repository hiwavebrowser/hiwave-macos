Real-site trench, cross-platform bug 3 from Talos's Linux port: flex lengths resolved at a fixed 16px font size and an 800x600 viewport.

**Why.** `flex.rs` `resolve_length` called `to_px_with_viewport(16.0, 16.0, container, 800.0, 600.0)` for every length it touched. That covered a flex item's margins, its `width`/`height` (the auto basis), min/max, its explicit cross size, and the container's `column-gap`/`row-gap`. So `em` ignored the element's font size and `vw`/`vh` ignored the real viewport. `grid.rs`'s gaps had the same fixed 16px em.

**What.** `resolve_length`/`resolve_max_length` now take the box and go through `LayoutBox::length_to_px`, the same resolution block layout uses. That's the element's computed font size and the viewport the engine sets on the root (`root_box.set_viewport`, rustkit-engine lib.rs:2534). The grid gaps use the container's `length_to_px`. Percent/calc behaviour is unchanged. Root `rem` is still 16px, as in `length_to_px`.

**Repro** (`units.html` at 1280x800, body `font:20px Arial`), before (#335 binary) → after:

| box | before | after | Chrome |
|---|---|---|---|
| block `width:50vw` (control) | 640 | 640 | 640 |
| flex item `width:25vw` | 200 | **320** | 320 |
| flex item `margin-left:2em; width:3em` | x=32, w=48 | **x=40, w=60** | x=40, w=60 |
| 2nd item, `column-gap:1em` | x=26 | **x=30** | x=30 |

**Tests** (`flex_relative_length_tests` in rustkit-engine, both layout entry points: `layout()` and `layout_with_collapse`), each verified failing without the fix:
- `a_flex_items_em_margin_and_width_use_its_own_font_size`: 32 vs 40.
- `a_flex_items_vw_width_uses_the_real_viewport`: 200 vs 320.
- `an_em_gap_uses_the_containers_font_size`: flex 26 vs 30. The grid half was checked on its own with the fixed flex.rs and the old grid.rs: 46 vs 50.

rustkit-engine lib **195/195**, rustkit-layout **554/554** (serial). A parallel layout run hit the known `text::font_resolve_tests::a_new_web_font_set_invalidates_the_cache` shared-counter flake (see #335); it passes alone and serially. Found while writing the grid pin, and out of scope here: a fixed `10px` grid track grows to fit its text (11.12px), where Chrome keeps it at 10.

### Campaign receipt (head c888ff8, on develop 6caadcb)

- `parity_test.py` run 2026-09-28T18:20:06 at c888ff8: **26/26 passed, avg diff 1.1756%**. That's identical, case for case, to #335's and #330's receipts (1.1756%).
- Local CI gates (`ratchet_local.py`): exit 2 (tighten-eligible), **none worse than the committed floor**. about paint 0.93939, settings geo_fails 250, shelf 0.98398, flex-positioning 0.95642: all the same as #335.
- Not run: `wpt_tier1.py` and clippy.

### Update: develop merged in → head 62b05b0

Merged develop 8567760 into the branch additively. No rebase, no force-push. The only conflict was two new test modules appended at the same spot in rustkit-engine lib.rs (#335's `grid_relative_size_contribution_tests` and this PR's `flex_relative_length_tests`). Both are kept, unchanged.

- Tests at 62b05b0: rustkit-engine lib **199/199**, rustkit-layout **554/554** (serial).
- `parity_test.py` run **2026-09-28T19:29:22 at 62b05b0: 26/26 passed, avg 1.1756%**, identical case for case to the table below.
- `ratchet_local.py` at 62b05b0: exit 2 (tighten-eligible), **none worse than the floor**. about paint 0.93939, settings geo_fails 250, shelf 0.98398.

**Real-site A/B.** Base is develop 8567760 and head is 62b05b0, both release builds captured back to back per site. The Chrome frame is from board run `20260928T2025Z-gridrem`. Sites with no Chrome frame in that run are n/a: the four added today, plus the ones whose oracle failed.

| site | frames differ | vs Chrome: base → head |
|---|---|---|
| linkedin | 5.02% | 9.97% → **9.35%** |
| facebook | 4.54% (identical across 2 runs, both orders) | 15.27% → 15.55% |
| netflix | 44–59% (4 runs, both orders) | 52.7–56.3% → 64.6–64.7% |
| cnn | 1.05% | n/a |
| apple | 0.07% | 59.77% → 59.74% |
| the other 15 | 0.00% | unchanged |

**netflix gets worse on pixels, but its geometry moves toward Chrome.** The hero block's bottom edge is at y≈445 on develop, y≈606 at this head, and **y=708 in Chrome**. The promo card below it moves 664 → 676 (Chrome 776). The pixel diff rises because RustKit still paints no hero poster collage, so a taller hero is more red fallback gradient where Chrome shows dark posters. Netflix also rotates its headline copy on every request (four different headlines across these captures), which is why frame-vs-frame is noisy. The size change reproduced in all 4 runs. No board check flips: netflix was already failing LOOKS RIGHT at 52%.

<details><summary>Per-case diff_pct at c888ff8</summary>

| case | diff % |
|---|---|
| new_tab | 1.5685 |
| about | 3.6742 |
| settings | 2.0854 |
| chrome_rustkit | 1.1234 |
| shelf | 1.0781 |
| article-typography | 4.7857 |
| card-grid | 1.3042 |
| css-selectors | 1.3819 |
| flex-positioning | 0.6327 |
| form-elements | 0.9875 |
| gradient-backgrounds | 1.0133 |
| image-gallery | 0.5217 |
| sticky-scroll | 0.6575 |
| backgrounds | 1.2163 |
| bg-solid | 0.2106 |
| bg-pure | 0.0000 |
| combinators | 0.6547 |
| form-controls | 3.2442 |
| gradients | 0.1412 |
| gradient-no-radius | 0.5204 |
| gradient-radius-only | 0.6892 |
| gpu-gradient-regression | 0.3903 |
| images-intrinsic | 0.3267 |
| pseudo-classes | 0.4341 |
| rounded-corners | 1.3221 |
| specificity | 0.6015 |

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
