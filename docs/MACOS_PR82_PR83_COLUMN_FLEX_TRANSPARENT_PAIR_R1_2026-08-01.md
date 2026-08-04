# Outside-eye R1: hiwave-macos #82 + #83 (PAIR) — DESIGN CLEAR / APPROVE pair-merge

**Seat:** Prometheus (design only)  
**Date:** 2026-08-01  
**Request:** Atlas `2a22e5465b4e` — review #82 and #83 **together**; do not reject #82 on solo parity.  
**Merge:** Atlas lane only — Prometheus does not merge / force-push / master-write.

## Tips measured

| PR | Branch | HEAD |
|----|--------|------|
| #82 | `atlas/column-flex-cross-stretch` | `32b27848f145053fd12fef3dd0f9ac20c2fc1880` |
| #83 | `atlas/input-transparent-background` | `fb43a620988751ac06a2e8b7ba0d6bff2a91a2e1` |
| master (this tick) | | `5aa912d` (#79) / `472bfab` (#80) already MERGED |

Worktrees: `/tmp/hiwave-pr82-r1`, `/tmp/hiwave-pr83-r1`, `/tmp/hiwave-pr82-master`.

`git merge-tree` of #82 + #83: **clean** (no content conflict). Both bases sit on pre-#79/#80 `3db1783`; rebase onto tip master is Atlas process residual, not design.

---

## Why a pair (not two solo reviews)

Atlas stated, and this R1 **confirms**:

1. **#82 alone** makes column flex layout correct → search field expands ~61px → ~1200px.
2. While expanded, a paint bug paints author-`transparent` form controls as **white**.
3. Parity **shelf** then fails the per-case gate (CI pr-aggregate: `✗ FAIL: 1/26 … shelf`).
4. **#83** moves the white default into the UA cascade layer so `background: transparent` survives paint.
5. Together (Atlas temp-merge receipt, not re-run here): **26/26**, shelf **33.87% → 6.10%**.

Rejecting #82 because of solo parity would reward a **broken layout that hid a broken paint**. That is instrument-optimization, not product.

**Hard rule for this pair:** merge **as a stack** (#82 then #83, or equivalent same-land). Landing #82 alone is DESIGN REJECT even though the layout fix itself is CLEAR. Landing #83 alone is safe but product-incomplete (white slab only fully visible after #82).

---

## #82 — column flex cross size on wrong axis

### Independent ground

**Master (`5aa912d` lineage / worktree master tip):**

```rust
fn get_content_cross_size(layout_box: &LayoutBox) -> f32 {
    // always height: content.height, line_height, sum of children heights, style.height
}
// has_definite_cross_size Horizontal: !matches!(width, Auto)  // auto ⇒ indefinite
```

Confirmed: **no axis parameter**. Column containers (cross = horizontal) therefore fed **heights as widths** → square / height-tracking boxes. Matches Pete “looks awful” and Atlas MCP layout-vs-paint diagnosis.

**Tip #82:**

1. `get_content_cross_size(box, cross_axis)` → height vs width helpers.
2. **`get_content_cross_width` never falls back to line-height** (returns `0.0` so stretch can fill). That is the load-bearing anti-square rule.
3. `has_definite_cross_size` for Horizontal: style width not Auto **OR** `container_cross_size > 0.0` (used width from containing block). Height arm left asymmetric on purpose (`test_auto_height_stretch`).
4. Axis threaded into `calculate_cross_sizes`.

**Local tests (this seat):**

```
test_column_stretch_fills_cross_axis_width                 ... ok
test_column_non_stretch_item_uses_content_width_not_height ... ok
```

**Test quality (Atlas honesty note — re-checked):**

| Test | What it bites | Mask risk |
|------|---------------|-----------|
| stretch fills width | cause 2 (definite used width) | stretch **masks** wrong content-cross measurement |
| non-stretch + width:auto + preseeded 200×400 | cause 1 (axis split) | **load-bearing** for wrong-axis height read |

Atlas’s claim that the first tests were worthless until rewritten is **credible and matches the code structure**. The second test is the one that would fail if `get_content_cross_size` went height-only again.

### CI

| Check | #82 |
|-------|-----|
| audit + pr-swarm ×4 | SUCCESS |
| pr-aggregate | **FAIL** — shelf only (`1/26` gate) |

Aggregate fail is **expected and correctly documented** on the PR. Not a hidden red.

### Rulings — #82

| Item | Ruling |
|------|--------|
| Product layout fix (axis split + used-width definite) | **DESIGN CLEAR** |
| Line-height ban on horizontal path | **CLEAR** |
| Height asymmetry for auto-height row | **CLEAR** |
| Solo land with red aggregate | **REJECT** — pair with #83 |
| Pair land after #83 closes shelf gate | **APPROVE merge** (Atlas) |
| DIVERGENCE: NONE vs prior broken reference | **ACCEPT** (spec-correct; Linux/Windows already clear per fleet measure) |

### Soft residuals (non-blocking)

- **FormControl width arm absent** in `get_content_cross_width` (falls to 0.0). Stretch path still supplies container width for typical app UIs. Non-stretch auto-width form controls may under-measure — separate unit if seen.
- **Intrinsic % / rem / em width** on horizontal path only handles Px/Em explicitly (same class as many layout helpers).
- Rebase onto post-#79/#80 master before merge if process requires linear history.

---

## #83 — form controls ignored `background: transparent`

### Independent ground

**Paint (layout DisplayList) before:** three form paths (TextInput / TextArea / Select):

```rust
background_color: if bg_color.a > 0.0 { bg_color } else { Color::WHITE }
```

Alpha-0 cannot distinguish **unset** from **authored transparent**. Product CSS pattern `.command-input { background: transparent }` over a dark wrapper → white slab once layout width is correct.

**Tip #83:**

1. **UA layer** in `compute_style_for_element`: `input` / `select` / `textarea` set `background_color = WHITE` **before** stylesheet + inline cascade.
2. Paint arms for those three: pass `bg_color` through **without** alpha-0 white substitute.
3. Cascade order verified on tip:

   ```
   ComputedStyle::new()  →  inheritance seeds  →  UA tag match (WHITE)
     →  stylesheet rules (override)  →  inline style (highest)
   ```

   `parse_color("transparent")` → `Color::TRANSPARENT` (a=0). Author rule wins over UA white. **Mechanism holds.**

**Checkbox / Radio:** still alpha-0 → white face fill. **ACCEPT as stated** (control chrome, not CSS background fallback for text fields).

**Button residual (soft):** still paints `if bg_color.a > 0.0 { bg } else { Color::new(239,239,239,1.0) }` and button UA arm does **not** set white. Out of #83 stated scope (search-field class). Document only — not a hold.

### CI

| Check | #83 |
|-------|-----|
| audit + pr-swarm ×4 + pr-aggregate | **SUCCESS** |

### Rulings — #83

| Item | Ruling |
|------|--------|
| Product paint fix (UA default + drop paint substitute) | **DESIGN CLEAR / APPROVE** |
| Cascade layer choice (same shape as min-width:auto) | **CLEAR** |
| Checkbox/Radio face white kept | **ACCEPT** |
| Button gray alpha fallback | **SOFT residual** (follow-up if transparent buttons appear) |
| Shelf residual ~6.10% (magnifier SVG + close button) | **ACCEPT stated** — not a gate failure after pair |
| Solo land | **safe** but incomplete vs Pete “looks awful” |
| Pair land with #82 | **required for honest mainline** |

---

## Fleet design pin (Atlas pattern ask)

Three defects same root shape this week:

| Case | Wrong layer | Fix |
|------|-------------|-----|
| #81 min_width Zero vs Auto | computed default collides with authored 0 | Auto as unset; content floor at flex |
| #82 line-height as horizontal fallback | measurement guesses axis | axis-split; never LH as width |
| #83 alpha-0 → white at paint | paint guesses authoredness | UA default; cascade overrides |

**PIN (Prometheus):**  
When a fallback must **guess whether a value was authored**, the fallback is at the wrong layer. Put the default in the cascade (or give the unset state its own variant). Do not invent authoredness at paint/layout from alpha or zero alone.

This is fleet advice for Athena/Talos ports of the same classes — not a merge gate on this pair.

---

## Merge instruction (Atlas)

1. Rebase/stack onto current master if needed (#79/#80 already in).
2. Land **#82 then #83** (or one stacked PR that is the pair).
3. Do **not** land #82 alone while aggregate red.
4. Expect post-pair shelf ~6% residual (honest layout); magnifier/close are separate units.
5. **#81** remains open independent product residual — next Prometheus R1 candidate after this pair clears (or in parallel if capacity).

## Explicit non-actions (this seat)

no merge · no force-push · no master write · no null attend · no CI redispatch · no spend · no Windows/Linux code ports

## Athena / Talos notes

- #82 shape is **macOS-broken reference**; Talos Linux-clear / Athena Windows-clear already claimed — do not cargo-cult the bug *into* clear trees. Re-measure before porting the fix as a “port.”
- #83 paint substitute may still exist on Windows/Linux form DL paths — **worth a same-class greps** for `bg_color.a > 0.0` → white on form controls.

---

**Verdict:** **DESIGN CLEAR / APPROVE pair-merge** for #82 + #83.  
**Do not** reject #82 on solo parity.
