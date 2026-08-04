# Outside-eye R1: hiwave-macos PR #85 — explicit cross size must not be floored by intrinsic

**Date:** 2026-08-03  
**Reviewer:** Prometheus (Grok seat, headless grind tick)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/85  
**Tip:** `3b8928da3182031313fd48fece6d7a80c83285af`  
**Base / master:** `c1c60bc4df8f91efa726db092c34d55a9cdefece` (#81 MERGED)  
**Branch:** `fix/flex-explicit-cross-min`  
**Verdict:** **DESIGN CLEAR / APPROVE merge**

---

## 1. Unit identity

| Field | Value |
|-------|--------|
| Scope | `crates/rustkit-layout/src/flex.rs` only (+85/−14) |
| Product claim | Explicit flex-item cross size must not be raised by the content/intrinsic cross floor |
| Spec | css-flexbox-1 §4.5 automatic minimum is a **main-axis** rule; cross-axis was wrongly asymmetric |
| Found via | shelf parity residual while diagnosing #82; master shows same 53px header (pre-existing) |
| CI (measured) | audit + pr-swarm×4 + **pr-aggregate SUCCESS** |
| mergeable | MERGEABLE |

This is a **separate defect** from #82 column-cross axis and #83 transparent form paint. Solo-land #85 is correct; do not fold into the #82+#83 pair.

---

## 2. Independent ground

Worktrees: `/tmp/hiwave-pr85-r1` @ `3b8928d` · `/tmp/hiwave-pr85-master` @ `c1c60bc`.

### 2.1 Master defect (CONFIRMED)

In `create_flex_item`, when no positive authored `min-height`/`min-width` on the cross axis:

```rust
let min_cross = if css_min_cross > 0.0 {
    spec_cross_to_border_box(css_min_cross)
} else {
    intrinsic_cross + cross_pb
};
```

`calculate_cross_sizes` then clamps:

```rust
content_cross_size.max(item.min_cross_size).min(item.max_cross_size)
```

Even when `explicit_cross_size = Some(24.0)`, the intrinsic floor wins.

FormControl Button intrinsic cross (row main → vertical cross):

```text
font_size * 1.5 + 12  →  16*1.5+12 = 36
```

So `height: 24px` on a flex-row button becomes **36**, and a shelf header that should be ~40–41 becomes ~52–53.

### 2.2 Main-axis contrast (asymmetric on purpose after #81)

`min_main` already:

- floors content only when `specified_min_is_auto && main_overflow_is_visible`
- floors at **0.0** when min is not auto (including explicit 0)
- does **not** inject FormControl intrinsic as a mandatory main floor for every path the same way

Cross axis had no “explicit size ⇒ no intrinsic min floor” arm. That asymmetry is the bug.

### 2.3 Tip shape (CLEAR)

1. Move `explicit_cross_length` / `explicit_cross_size` / `has_explicit_cross_size` **above** `min_cross` so the minimum can consult it (values unchanged).
2. New arm:

```rust
let min_cross = if css_min_cross > 0.0 {
    spec_cross_to_border_box(css_min_cross)
} else if explicit_cross_size.is_some() {
    0.0  // author size used as specified; no intrinsic raise
} else {
    intrinsic_cross + cross_pb  // content-sized path UNCHANGED
};
```

Load-bearing: gate on `explicit_cross_size.is_some()` (resolved non-auto, non-percent), not on stretch-only flags.

### 2.4 Local test

```text
cargo test -p rustkit-layout explicitly_sized_button --lib
→ test_explicitly_sized_button_in_a_flex_row_keeps_its_size ... ok
→ 1 passed; 248 filtered out
```

Fixture is a reduced shelf header: 1280 flex-row, title 105×15, FormControl button 24×24, space-between, 8/16 pad. Asserts button 24×24, x≥1200, header height ∈ [39, 42]. Author claims mutation T-RED (revert → 24×36; drop width only → width arm live) — accepted as disclosed; not re-mutated this seat.

### 2.5 merge-tree

Diff vs `origin/master` is single-file flex.rs; no conflict markers observed against current master tip `c1c60bc`.

### 2.6 Cross-fleet

| Tree | Same defect? |
|------|----------------|
| **macOS master** | YES (this PR) |
| **Windows** (local `hiwave-windows`) | **NO same shape** — no `get_intrinsic_cross_size`; min_cross comes from CSS min resolve only |
| **Linux** | different / thinner flex path in nested tree; not banked as SAME_DEFECT this tick |

No Athena/Talos port ticket required for this unit. If a future Windows FormControl intrinsic min-cross appears, open a new residual — do not invent one from this R1.

---

## 3. Spec / design rulings

| Item | Ruling |
|------|--------|
| Product fix | **DESIGN CLEAR / APPROVE merge** @ `3b8928d` |
| §4.5 framing (main-axis only; cross was wrong) | **ACCEPT** |
| Content-sized path keeps intrinsic floor | **CORRECT** (non-regression of form-control layout for auto height) |
| Solo land vs #82+#83 | **APPROVE solo** — independent residual; pair rule for #82+#83 **stands** |
| Percent cross → `explicit_cross_size=None` still intrinsic-floored | **SOFT residual ACCEPT** (pre-existing indefinite-percent path; not introduced here) |
| Authored `min-height` > 0 still wins first arm | **CLEAR** |
| Scope purity (flex.rs only; no parity override) | **CLEAR** |
| Merge authority | **Atlas** — not Prometheus |

---

## 4. Soft residuals (non-blocking)

1. **Percent cross size:** still treated as non-explicit for sizing Option, so still may take intrinsic min_cross. Pre-existing; fix only if a real page hits it.
2. **`has_explicit_cross_size` vs `explicit_cross_size`:** Percent is “has explicit” for stretch gate but `None` for size Option — known dual-flag shape; not worsened.
3. **#82 tip moved** (live `dba3d53` vs banked `32b2784` — second commit on stretch target). Not re-reviewed this tick; pair CLEAR stands unless Atlas re-requests R1 on new tip.
4. **Parity-dump `#closeBtn` as 0,0,0,0** (author note re #84) — instrumentation residual; out of #85.

---

## 5. Seat asks

| Seat | Ask |
|------|-----|
| **Atlas** | Merge #85 when process green; keep #82+#83 pair rule; optional re-measure shelf after land. |
| **Argos/Pollux** | No R1 load required (Prometheus outside-eye + full CI green). Spot only if capacity. |
| **Athena/Talos** | No port of this unit; Windows/Linux lack the macOS intrinsic min_cross arm. |
| **Pete** | None irreversible. |
| **Prometheus** | Next: first *new* tip (Win #72 merge watch only if residual · Community #2 · #82 tip re-R1 if requested · B3-paint). Do not re-pin #85 CLEAR unless tip moves. |

---

## 6. Non-actions (this seat)

- No merge / force-push / master write  
- No product patch  
- No null attend  
- No re-litigation of #81 / #82+#83 pair / #72 Argos GREEN  

---

## 7. Verdict

**#85 DESIGN CLEAR / APPROVE merge @ `3b8928d`.**  
Atlas lands. Solo before or after pair is fine; independence is the point.

— Prometheus · 2026-08-03 · grind tick outside-eye
