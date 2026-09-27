## What

An inline `<svg>` with a `viewBox` but no `width=`/`height=` has an aspect ratio and no natural size. RustKit gave it the 300×150 replaced-element fallback anyway. Chrome (and CSS 2.1 §10.3.2 / §10.6.2) handles it like this:

- both axes auto: the svg takes the containing block's width, and its height follows across the ratio;
- one axis given (attribute or CSS): the other follows across the ratio;
- neither a size nor a viewBox: 300×150, as before.

The fix is in the inline-svg arm of `build_layout_from_parent_style_and_path`. With a ratio, a missing axis stays `auto` (or becomes `100%` when both are missing), and the Image box's natural size is the viewBox, so `layout_image` derives the other axis. Without a viewBox, nothing changes.

## Why now

#297 made `S`/`A` path segments draw. google's apps button is `<svg viewbox="0 0 24 24">` with no size, in a 24px content box. Its nine dots were invisible before #297; after it, they painted across 150px in the header (develop board run `20260927T0420Z-dev`). Every sizeless icon on the board had the same problem.

## Test

`windows_a_leg_pins::an_inline_svg_with_only_a_viewbox_is_sized_by_its_ratio` lays out six svgs and checks each size against Chrome 148 (pinned CfT) for the same markup:

| case | Chrome 148 | develop | this PR |
|---|---|---|---|
| viewBox 24×24 in a 24px div | 24×24 | 150×150 | 24×24 |
| viewBox 48×24 in a 100px div | 100×50 | 300×150 | 100×50 |
| `width=96`, viewBox 48×24 | 96×48 | 96×48 | 96×48 |
| CSS `height:12px`, viewBox 48×24 | 24×12 | 24×12 | 24×12 |
| no size, no viewBox | 300×150 | 300×150 | 300×150 |
| google's button (40px border-box, 8px padding, lowercase `viewbox`) | 24×24 | 150×150 | 24×24 |

The test fails on develop (with the ratio disabled, the first case is 150×150). rustkit-engine 165/165, rustkit-svg 21/21, rustkit-layout 522/523. The one layout failure is the known parallel-runner flake `a_new_web_font_set_invalidates_the_cache` (noted on #290); it passes alone.

## Real-site board (headed CfT 148 oracle), full 20 sites

develop c2772b1 `20260927T0420Z-dev` **18/60** → this PR `20260927T0515Z-svgratio` **18/60**. Rows that moved:

| site | develop | this PR | why |
|---|---|---|---|
| google | 1 (READABLE 69%, LOOKS RIGHT 19.0%) | **2** (READABLE 86%, LOOKS RIGHT 17.3%) | apps dots shrink back into the button, and the search box's `+` icon (`viewBox="0 -960 960 960"`) paints. Today's page is the 28th-birthday doodle variant, which RustKit lays out badly for other reasons. |
| x | **2** (LOOKS RIGHT 13.4%) | 1 (LOOKS RIGHT 19.6%) | the X logo (`class="h-auto w-full max-w-[480px]"`, viewBox 480×490) is now ~470px wide, as in Chrome (~455), instead of 140px. But RustKit puts it at the top of its column, where Chrome centres it vertically (`items-center` in a flex column). A small logo in the wrong place scored better than a correct-size one in the wrong place. The centring is a separate, pre-existing bug. |

Other moves are oracle drift (github 71.7 → 83.6% with a byte-identical RustKit frame) or improvements under the threshold (weather LOOKS RIGHT 57.4 → 48.5%).

**Campaign receipt (gate 5):** `scripts/parity_test.py` (all scopes), run 2026-09-27T01:45 local at head 57bce7c. **26/26 passed, avg diff_pct 1.2534%**, the same as develop's 1.2534%. All 26 cases are identical to #297's receipt.

<details><summary>Per-case diff_pct (26 cases)</summary>

| case | diff_pct |
|---|---|
| `new_tab` | 1.5685 |
| `about` | 3.7492 |
| `settings` | 2.0854 |
| `chrome_rustkit` | 1.1234 |
| `shelf` | 2.8711 |
| `article-typography` | 4.7857 |
| `card-grid` | 1.2958 |
| `css-selectors` | 1.4975 |
| `flex-positioning` | 0.6793 |
| `form-elements` | 0.9875 |
| `gradient-backgrounds` | 1.0133 |
| `image-gallery` | 0.5217 |
| `sticky-scroll` | 0.6575 |
| `backgrounds` | 1.2163 |
| `bg-solid` | 0.2106 |
| `bg-pure` | 0.0000 |
| `combinators` | 0.6547 |
| `form-controls` | 3.2442 |
| `gradients` | 0.1412 |
| `gradient-no-radius` | 0.5204 |
| `gradient-radius-only` | 0.6892 |
| `gpu-gradient-regression` | 0.3903 |
| `images-intrinsic` | 0.3267 |
| `pseudo-classes` | 0.4341 |
| `rounded-corners` | 1.3221 |
| `specificity` | 0.6015 |

</details>

WPT tier 1 not run (no `third_party/wpt` on this seat).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
