## What

The flow-relative border properties had no arms in the engine, so any declaration written with them was dropped: `border-inline-start`, `border-block-end-width`, `border-inline-color`, `border-inline`, and the rest of the family. StyleX and Tailwind v4 emit them.

Found on facebook while taking #401's live-site frames: its login inputs painted a top and a bottom border and no left or right one.

## Change

One file, `rustkit-engine`. `logical_to_physical` (which already maps the margin, padding and inset names) gains:

- the per-side shorthands: `border-inline-start|end`, `border-block-start|end` -> `border-left|right|top|bottom`;
- the per-side longhands for width, style and color (12 names);
- the two-value forms `border-inline-width|style|color` and `border-block-width|style|color` (`start end`; one value sets both);
- `border-inline` and `border-block`, whose whole value goes to both sides (a new `Both` mapping).

A two-value logical shorthand is now split at top-level whitespace instead of every space, so `rgb(1, 2, 3)` is one value. That also fixes the existing `margin-inline: calc(1px + 2px)`, which was dropped as three values.

Horizontal-tb, ltr only, as for the names already mapped.

## Tests

`logical_property_tests::logical_border_properties_map_to_physical_sides`: laid-out border widths for `border-inline-width: 2px 5px`, `border-block-end-width`, `border-inline-start: 3px solid red`, `border-block: 4px solid rgb(1, 2, 3)` and `border-inline: 6px solid blue`; computed styles and colors for the style and color forms, including two colors that each contain spaces. **Fails on develop f16ad4e's code** (run with only the test added: borders `(0, 0, 0, 0)` where `(0, 5, 7, 2)` is expected) and passes with the change. The existing `logical_margin_padding_and_inset_map_to_physical_sides` still passes. The full engine suite was not run locally (machine load 15 to 17); CI is that evidence.

## facebook

The email box on facebook.com (536x60 at x 692), display list from `parity-capture --url`, both binaries in this session:

| | develop f16ad4e | this PR |
|---|---|---|
| border strips painted | top and bottom (536x1 each) | top, bottom, left (1x60 at x 692) and right (1x60 at x 1227) |
| overflow clip (padding box) | 536x58 at x 692 | 534x58 at x 693 |

No board points are claimed; a scoring run was not taken at this machine load.

## Campaign receipt

Arms: develop **f16ad4e** (this branch's base) vs fix **04ebb59**, both release binaries built in this session with every workspace source touched first.

```
all:      develop 2026-10-01T07:19:09 26/26 avg 1.1612 | fix 2026-10-01T08:26:57 26/26 avg 1.1612 | 26/26 identical
builtins: develop 2026-10-01T07:14:36  5/5  avg 1.9072 | fix 2026-10-01T08:27:35  5/5  avg 1.9072 | 5/5 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): output identical on both arms, line for line.

<details><summary>Per-case diff_pct (all 26), develop f16ad4e vs fix 04ebb59</summary>

| case | develop | fix |
|---|---|---|
| about | 3.6742 | 3.6742 |
| article-typography | 4.7857 | 4.7857 |
| backgrounds | 1.2163 | 1.2163 |
| bg-pure | 0.0000 | 0.0000 |
| bg-solid | 0.2106 | 0.2106 |
| card-grid | 1.3068 | 1.3068 |
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
| rounded-corners | 1.3221 | 1.3221 |
| settings | 2.0919 | 2.0919 |
| shelf | 1.0781 | 1.0781 |
| specificity | 0.6015 | 0.6015 |
| sticky-scroll | 0.6575 | 0.6575 |

</details>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
