## What

The intrinsic width of text left out `letter-spacing` and `word-spacing`. Line layout (`shape_line`) includes them, so a shrink-to-fit box around spaced text was narrower than the line laid out in it.

`new_tab` shows it. Its logo is "HIWAVE" at 48px with `letter-spacing: 0.5rem` inside an inline-block:

```
h1.logo on new_tab     Chrome    develop   fix
width                  214.80    165.66    213.66
x                      532.59    557.17    533.17
```

Six letters at 8px is the 48px that was missing, and the too-narrow box sat 24.6px right of Chrome's.

## Change

`crates/rustkit-layout/src/grid.rs` only:

- `spaced_text_width(text, style)` measures one line with `measure_text_with_spacing`, resolving `letter-spacing` and `word-spacing` the way `shape_line` does (px, em against the element's font size, rem against 16px).
- `text_min_content_width`, `text_max_content_width` and `collapsed_space_width` call it. They called `measure_text_advanced`, which takes no spacing.

With both spacings zero the result is the same shaping call as before, so text without spacing does not move (the campaign below is the check).

**Not closed by this:** the logo is still 1.14px narrower than Chrome's (213.66 against 214.80), so its four rows stay over the 0.5px geometry gate and `new_tab` keeps 17 geometry failures on both arms. The 1.14px was there before the spacing (165.66 against Chrome's 214.80 − 48 = 166.80), so it is the measure of the six letters themselves in that face, not the spacing. `new_tab`'s pixel diff goes 1.50% -> 1.27%.

## Test

`a_shrink_to_fit_box_is_as_wide_as_its_spaced_text` (rustkit-layout): a box around "HIWAVE" at 48px is 48px wider with `letter-spacing: 8px` and with `0.5rem`, and a box around "HI WAVE" is 10px wider with `word-spacing: 10px`. It runs four ways: the box as an inline-block and as a flex item, each through `layout()` and through `layout_with_collapse` (the path the engine's page layout takes).

Fail-first, with develop's `grid.rs` under the new test:

```
test tests::a_shrink_to_fit_box_is_as_wide_as_its_spaced_text ... FAILED
thread 'tests::a_shrink_to_fit_box_is_as_wide_as_its_spaced_text' panicked at crates/rustkit-layout/src/lib.rs:12137:17:
flex_item=false collapse_path=false: px: 184.05469 vs 184.05469 + 48
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 594 filtered out; finished in 0.25s
restored: True
```

The assertion stops at the first of its four cases, so this shows the inline-block through `layout()` failing on develop's code. The other three cases (flex item; both through `layout_with_collapse`) pass on the branch; whether each fails separately on develop was not run.

Suites at 848c872: rustkit-layout 595/595 (plus its integration tests, 5/5). Only rustkit-layout changed; the other crates' suites were not run locally, CI runs them.

## Campaign receipt

Arms: develop **f657cf2** (merged into this branch) vs fix **848c872**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T22:05:31 26/26 avg 1.1224 | fix 2026-10-01T22:33:29 26/26 avg 1.1136 | 25/26 identical
builtins: develop 2026-10-01T22:01:31  5/5  avg 1.8936 | fix 2026-10-01T22:12:34  5/5  avg 1.8478 | 4/5 identical
micro:    develop 2026-10-01T22:00:46 13/13 avg 0.6548 | fix 2026-10-01T22:11:20 13/13 avg 0.6548 | 13/13 identical
```

The one case that moves is `new_tab` (1.5003 -> 1.2713). The fix arm's `all` scope was run twice: in the first run `css-selectors` and `flex-positioning` failed to capture while the machine was at load 20 (both capture in 5 to 6 s when run directly, on both binaries), so the scope and the ratchet were re-run; the numbers here are from the second run. Develop has moved to b35c82e since (JavaScript bindings only); the arms were not rebuilt for it.

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ between the arms, develop then fix:

```
dev: new_tab: geo_fails=17 paint=0.94520 discrete=0
fix: new_tab: geo_fails=17 paint=0.94686 discrete=0
```

<details><summary>Per-case diff_pct (all 26), develop f657cf2 vs fix 848c872</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3074 | 1.3074 |
| chrome_rustkit | 1.1234 | 1.1234 |
| combinators | 0.6547 | 0.6547 |
| css-selectors | 1.3299 | 1.3299 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2388 | 3.2388 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5003 | 1.2713 | moved
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

<details><summary>Per-case diff_pct (builtins, micro)</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| chrome_rustkit | 1.1234 | 1.1234 |
| new_tab | 1.5003 | 1.2713 | moved
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2388 | 3.2388 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

## Real sites

RustKit frames, develop / fix / develop / fix on each live URL at 1280x800 (`scratch/s1001g/ab.py` on the hub branch). "Across arms" is the share of pixels where any channel differs by more than 8. The two sites that differ were then scored with the board's own diff against the Chrome frames stored by today's 14:54 quiet board.

```
site        within develop  within fix  across arms             against Chrome: develop -> fix
microsoft        0.00%        0.00%     0.74% (all four pairs)   54.38% -> 54.37%
shopify          0.00%        0.00%     1.04% (all four pairs)   19.91% -> 19.91% (203923 -> 203839 pixels)
facebook, wikipedia, lyft, x, yahoo, walmart, apple, weather, youtube, reddit: 0.00% across arms (pixel-identical)
google           8.36%        4.09%     0.00% to 8.82%  (its own variance; one develop/fix pair is identical)
linkedin         2.64%        0.00%     1.55% to 2.96%  (its own variance)
bing             0.24%        0.00%     0.00% or 0.24%  (its own variance)
netflix          4.61%        5.01%     2.08% to 4.81%  (its own variance: the hero rotates between loads)
instagram: one develop/fix pair captured, identical; the other two captures timed out
```

- **microsoft:** the left column of centred links (rows 259 to 798) sits about 6px further left. Those links carry `letter-spacing`, and their shrink-to-fit boxes are now as wide as the text in them.
- **shopify:** the hero's "all-star" headline (negative `letter-spacing`) is a few pixels narrower, the same reason with the opposite sign.
- Both fail LOOKS RIGHT on both arms, and no board check changes on any site.
- github, squarespace and cnn did not finish within the binary's 30 s on either arm (machine load was 22 during that part of the run), so they are unverified on both arms alike.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
