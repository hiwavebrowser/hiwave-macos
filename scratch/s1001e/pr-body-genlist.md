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

rustkit-layout 584/584 at __HEAD__. The full engine suite was not run locally.

## Campaign receipt

Arms: develop **__BASE__** (this branch's base) vs fix **__HEAD__**, both release binaries built in this session with every workspace source touched first.

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

No real-site A/B was taken for this PR (session cap).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
