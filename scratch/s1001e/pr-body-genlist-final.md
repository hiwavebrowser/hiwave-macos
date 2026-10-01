## What

`font-family: Consolas, monospace` on a Mac without Consolas was **measured in the system font** (proportional) and painted in Menlo at those advances. The same for `X, serif`: measured in the system font, painted in Times New Roman.

`FontFamilyChain::from_css_value` expands `sans-serif` and the system keywords when they follow the first family, but pushed `serif` and `monospace` into the chain as if they were font names. No font has those names, so the walk went past them to `.AppleSystemUIFont`, which is appended to every list.

Found by the real-site A/B on #411 (its finding 1). This is the smallest piece of it and stands alone; it does not depend on #411.

| 28 characters at 16px | Chrome 148 | develop (layout) | this PR (layout) |
|---|---|---|---|
| `"No Such Family", monospace` | 268.84 | 220.78 | 269.72 |
| `"No Such Family", serif` | 199.52 | 220.78 | 199.51 |
| `monospace` | 268.84 | 269.72 | 269.72 |
| `serif` | 199.52 | 199.51 | 199.51 |

Fixture and Chrome numbers: `scratch/s1001e/generic.html`, `fx/generic-chrome.json` on the hub branch.

## Change

One arm in `from_css_value`'s fallback loop: `serif` and `monospace` expand to `FontFamilyChain::serif()` / `monospace()`, as `sans-serif` already did. A list whose earlier family is installed is unaffected (`Georgia, serif` still resolves to Georgia).

## Not in this PR

- The 0.88px left on `monospace`: Chrome's default fixed font is Courier (9.6016 per character), not Menlo. Layout's chain and paint's `map_generic` both say Menlo.
- `sans-serif` and unstyled text resolve to the system font in layout (220.78); Chrome uses Helvetica (217.02) and Times (199.52). That moves measurements on every page that relies on a generic and about twenty layout tests build on the current chain, so it is the next PR, with its own receipt.
- `cursive` and `fantasy` after a missing family.

## Tests

`a_generic_after_missing_families_resolves_as_the_generic`: **fails with the new arm disabled** (`left: 220.78125, right: 269.71875`) and passes with it. It checks that `X, monospace` and `X, serif` measure exactly as the keyword alone, that the result is not the system font's width, that `X, monospace` is fixed pitch, and that `Georgia, serif` is still Georgia.

rustkit-layout 584/584 at d251d02. The full engine suite was not run locally.

## Campaign receipt

Arms: develop **6eb6f5f** (this branch's base) vs fix **d251d02**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T13:24:47 26/26 avg 1.1271 | fix 2026-10-01T14:16:03 26/26 avg 1.1271 | 26/26 identical
builtins: develop 2026-10-01T13:20:50  5/5  avg 1.9072 | fix 2026-10-01T14:12:33  5/5  avg 1.9072 | 5/5 identical
micro:    develop 2026-10-01T13:20:10 13/13 avg 0.6550 | fix 2026-10-01T14:11:52 13/13 avg 0.6550 | 13/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

<details><summary>Per-case diff_pct (all 26), develop 6eb6f5f vs fix d251d02</summary>

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
| css-selectors | 1.3819 | 1.3819 |
| flex-positioning | 0.6327 | 0.6327 |
| form-controls | 3.2405 | 3.2405 |
| form-elements | 0.9535 | 0.9535 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-backgrounds | 1.0133 | 1.0133 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| image-gallery | 0.5217 | 0.5217 |
| images-intrinsic | 0.3267 | 0.3267 |
| new_tab | 1.5685 | 1.5685 |
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
| new_tab | 1.5685 | 1.5685 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| combinators | 0.6547 | 0.6547 |
| form-controls | 3.2405 | 3.2405 |
| gpu-gradient-regression | 0.3903 | 0.3903 |
| gradient-no-radius | 0.5204 | 0.5204 |
| gradient-radius-only | 0.3429 | 0.3429 |
| gradients | 0.1412 | 0.1412 |
| images-intrinsic | 0.3267 | 0.3267 |
| pseudo-classes | 0.4341 | 0.4341 |
| rounded-corners | 0.4354 | 0.4354 |
| specificity | 0.6015 | 0.6015 |

</details>

No real-site A/B was taken for this PR (session cap).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
