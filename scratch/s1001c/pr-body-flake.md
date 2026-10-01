## What

Two tests in `rustkit-layout`'s `text::font_resolve_tests` fail when the suite runs in parallel on a loaded machine. Found while taking the suite numbers for #401: develop **f16ad4e** gave **579/580 on 4 parallel runs out of 4** at machine load 14, the failure being one of these two; each passes when run alone.

The web-font generation is process-wide and several tests in the crate install a web-font set on their own threads.

- `a_new_web_font_set_invalidates_the_cache` shaped twice and asserted the second was a cache hit, unconditionally. An install on another test's thread between the two shapes made it a miss (`left: 2, right: 1`).
- `a_font_is_resolved_once_not_once_per_shape` allowed for one generation bump during its 200 shapes; there can be more than one.

## Change

Test code only, 27 lines.

- The first test judges the hit only when the generation held still across the pair, retrying up to 20 times: the pattern `a_new_web_font_set_invalidates_shaped_runs` in the same module already uses.
- The second reads the generation before and after its loop and allows two resolutions per bump, instead of a fixed allowance of one bump. With no bump it still requires exactly the cached count (2 for 200 shapes).

Neither test got weaker in the case it exists for: with the generation still, both assert what they asserted before.

## Tests

`cargo test -p rustkit-layout --lib`, parallel, 8 consecutive runs on this branch at machine load 14 to 15: **580/580 eight times.** Before the change, same machine and load: 579/580 four times out of four (two runs on develop f16ad4e, two on #401's branch).

## Campaign receipt

No binary changes: the diff is inside one `#[cfg(test)]` module, so a release build of this branch is develop f16ad4e's. The campaign was not re-run for it. Develop's own numbers, from the run taken this session for #401 (release binary built from f16ad4e with every workspace source touched first):

```
all:      2026-10-01T07:19:09  26/26  avg 1.1612
builtins: 2026-10-01T07:14:36  5/5   avg 1.9072
```

<details><summary>Per-case diff_pct (all 26), develop f16ad4e = this branch</summary>

| case | diff_pct |
|---|---|
| about | 3.6742 |
| article-typography | 4.7857 |
| backgrounds | 1.2163 |
| bg-pure | 0.0000 |
| bg-solid | 0.2106 |
| card-grid | 1.3068 |
| chrome_rustkit | 1.1234 |
| combinators | 0.6547 |
| css-selectors | 1.3819 |
| flex-positioning | 0.6327 |
| form-controls | 3.2405 |
| form-elements | 0.9535 |
| gpu-gradient-regression | 0.3903 |
| gradient-backgrounds | 1.0133 |
| gradient-no-radius | 0.5204 |
| gradient-radius-only | 0.3429 |
| gradients | 0.1412 |
| image-gallery | 0.5217 |
| images-intrinsic | 0.3267 |
| new_tab | 1.5685 |
| pseudo-classes | 0.4341 |
| rounded-corners | 1.3221 |
| settings | 2.0919 |
| shelf | 1.0781 |
| specificity | 0.6015 |
| sticky-scroll | 0.6575 |

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
