## What

`@layer` blocks were flattened into plain rules in source order, so CSS Cascade 5 §6.4 layer precedence was ignored: an earlier layer's more specific rule beat a later layer's, and a layered rule could beat an unlayered one.

- **rustkit-cssparser:** each rule carries its full dotted layer name. Nested blocks accumulate, and each anonymous `@layer {}` block gets a unique name. The sheet records every `@layer a, b;` statement and block opening, with its position and enclosing `@media`.
- **rustkit-css:** `assign_layer_order` ranks layers across the document's sheets by first declaration. Sublayers rank before their parent's own rules, and unlayered rules rank above all (`UNLAYERED`).
- **rustkit-engine:** the build ranks layers after the `@media` filter. Statements inside a non-matching `@media` are dropped and positions are re-counted. The cascade sorts by (layer, specificity, source order), and the `!important` pass takes layers in reverse, for elements and for `::before`/`::after`. **A page with no `@layer` keeps exactly the old order**: every rule is `UNLAYERED`, and no second ordering is built.

## Why

linkedin now A/B-serves a variant whose only sheet (`assets/Kf3QXGdB.css`, 1.3 MB) is all layers (reset, theme, rootTheme, derivedTheme, localization, atoms, overrides), and RustKit renders it close to unstyled. Honest result: **this fix is spec-correct but NOT the main cause of that variant's look.** Offline, on the saved variant with the sheet inlined, the frame changes only slightly (button text weight). The remaining cause is under investigation on the trench hub. Board: ±0. Both live A/B arms drew the old, unlayered bundle in all 4 rounds, so the board can't see this change yet.

## Pins

- 6 engine tests through `build_layout_from_document`: later layer beats earlier whatever the specificity; unlayered beats every layer; `!important` reverses layers; a layer's own rules beat its sublayers; first declaration fixes a layer's place; `::before` cascades by layer. **All 6 fail with the ranking disabled**, and pass with it.
- A sanity pin that the pins' selectors (`#x`, `.c`, `div`) match the box, so the pins can't pass vacuously. Writing it found a separate bug: `#x.c` (an id followed by a class) never matches, because the id branch compares the whole remainder to the id. That will be its own PR.
- rustkit-css: cross-sheet ranking. rustkit-cssparser: 2 parser tests (layer names/statements/positions/media; anonymous blocks are distinct).
- cascade_wire_tests 20/20 (`--test-threads=1`), cssparser 17/17.

## Campaign receipt

Arms: fix **66c5912** vs develop **1b514f8**. The branch base is ddbeae5 = 1b514f8 + #378. That was the develop binary already built this session; a ddbeae5 build didn't fit the session. The head ee4ed52 is rustfmt-only on top of 66c5912.

```
ts 2026-09-30T11:01:10.340217 | 2026-09-30T11:10:33.088304
all: A 26 cases avg 1.1756  B 26 cases avg 1.1756  identical 26/26
```
Builtins scope included (shelf, settings, new_tab, chrome_rustkit…). Ratchet (`ratchet_local.py`) output is **identical** between the arms (settings geo 243, as on develop).

<details><summary>Per-case diff_pct, develop 1b514f8 | fix 66c5912</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6741666666666664 | 3.6741666666666664 |
| article-typography | 4.7857421875 | 4.7857421875 |
| backgrounds | 1.2163333333333333 | 1.2163333333333333 |
| bg-pure | 0.0 | 0.0 |
| bg-solid | 0.210625 | 0.210625 |
| card-grid | 1.3068359375 | 1.3068359375 |
| chrome_rustkit | 1.1234374999999999 | 1.1234374999999999 |
| combinators | 0.6546875 | 0.6546875 |
| css-selectors | 1.381875 | 1.381875 |
| flex-positioning | 0.6327499999999999 | 0.6327499999999999 |
| form-controls | 3.2441666666666666 | 3.2441666666666666 |
| form-elements | 0.9875 | 0.9875 |
| gpu-gradient-regression | 0.3903125 | 0.3903125 |
| gradient-backgrounds | 1.0133333333333332 | 1.0133333333333332 |
| gradient-no-radius | 0.5204166666666666 | 0.5204166666666666 |
| gradient-radius-only | 0.6891666666666667 | 0.6891666666666667 |
| gradients | 0.14122222222222222 | 0.14122222222222222 |
| image-gallery | 0.5216796874999999 | 0.5216796874999999 |
| images-intrinsic | 0.32669642857142855 | 0.32669642857142855 |
| new_tab | 1.56845703125 | 1.56845703125 |
| pseudo-classes | 0.43406249999999996 | 0.43406249999999996 |
| rounded-corners | 1.322111111111111 | 1.322111111111111 |
| settings | 2.0819346110026045 | 2.0819346110026045 |
| shelf | 1.078125 | 1.078125 |
| specificity | 0.6014583333333333 | 0.6014583333333333 |
| sticky-scroll | 0.65751953125 | 0.65751953125 |
</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
