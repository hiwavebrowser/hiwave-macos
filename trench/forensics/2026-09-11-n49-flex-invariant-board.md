# The flex invariant board — a font-independent reading of P3

**Seat:** Linux / SwiftShader / **no font backend at all**. MECHANICS, NOT A RECEIPT.
**Basis:** `develop da8f413`, 26 gating cases, one iteration.
**Tool:** `trench/tools/n49_flex_invariants.py` (diagnostic only, not in CI).

## Why

P3 (flex residual) has been recorded as unworkable from this seat since
2026-08-20: `flex-positioning`'s failures are 115 roots and **0** of them are
font-independent under `scripts/geometry_attribution.py`'s strict column. That
column asks *can a text measurement reach this box*, and on this corpus the
answer is almost always yes.

This board asks a question that does not depend on the answer:

> Given **RustKit's own** item sizes, does RustKit place those items where
> `justify-content` and `align-items` say it must?

It never reads Chrome's rects. Chrome's `computed-styles.json` is read only for
what the author asked for. A box whose width is wrong because a glyph was
measured with no font still has to sit flush against the content edge under
`flex-start`, and still has to leave equal space on both sides under `center`.

## The board

```
26 cases · flex containers measured 156 · skipped 159 · violations 13
```

Skips, itemised rather than folded into the pass column:

| count | reason |
|---:|---|
| 110 | no element children in the RustKit box (the items are text runs) |
| 23 | an anonymous box with area — the two engines' item sets disagree |
| 19 | multi-line container (needs `align-content`; out of scope) |
| 7 | a child Chrome does not report |

**`flex-positioning` itself: 15 containers measured, 0 violations.** Every
justify-content variant and every align-items variant in P3's own case places
its items exactly right given the sizes it has. Tests 5 and 6 of that fixture —
the parts that are literally about flex alignment — are clean. Its 174 failing
axes are text advances, `normal` line heights, and their propagation.

## The 13, and what happened to the first one

| case | rule | Δ | box |
|---|---|---:|---|
| chrome_rustkit | align:center symmetry | −57.00 | `.nav-bar > .sidebar-toggle` |
| settings | justify:flex-end trailing | −18.98 | `.setting-control.decay-control` |
| form-elements | align:center symmetry | +9.89 | `.toggle-label > .toggle-switch` |
| image-gallery ×4 | justify:center symmetry | +3.69 | `.aspect-box > .content` |
| about ×6 | align:center symmetry | +0.60 | `.card-title > span.icon` |

The first was diagnosed and fixed tonight (`atlas/n49-p3-flex`): `height: 100%`
on a flex item was not resolved at all, so the item fell to a content measure
that had already picked up the 100px chrome viewport — 100 tall against Chrome's
43. After the fix the board reads **12**, and the cleared row is exactly that
one.

Two of the remaining twelve look like the same family from the outside and are
not the same defect — `.toggle-switch` has an explicit `height: 26px` and comes
out 16.11, which is not a percentage at all. That one is unread.

## What a clean row does not mean

- The invariants are **necessary, not sufficient**. This board is deliberately
  blind to item *size*; an item at the right offset can still be the wrong box.
  Gate A owns size, and Gate A still fails 2645 axes.
- Where an item's cross size is text-derived on this seat, only the **boolean**
  transfers to macOS, not the magnitude. `about`'s 0.60 says the centring
  arithmetic disagrees with itself; it does not predict 0.60px on macOS.
- 159 skips is not 159 passes.
