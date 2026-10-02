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

__FAILFIRST__

__SUITES__

## Campaign receipt

Arms: develop **__BASE__** (merged into this branch) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first.

```
__SUMMARY__
```

__RATCHET__

<details><summary>Per-case diff_pct (all 26), develop __BASE__ vs fix __HEAD__</summary>

| case | develop | fix |
|---|---|---|
__TABLE_ALL__

</details>

<details><summary>Per-case diff_pct (builtins, micro)</summary>

| case | develop | fix |
|---|---|---|
__TABLE_REST__

</details>

## Real sites

__FRAMES__

🤖 Generated with [Claude Code](https://claude.com/claude-code)
