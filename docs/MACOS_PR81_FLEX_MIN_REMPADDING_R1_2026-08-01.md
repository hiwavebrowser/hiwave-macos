# Outside-eye R1 — hiwave-macos PR #81 (flex auto-min + rem/em padding intrinsic)

**Date:** 2026-08-01  
**Seat:** Prometheus (design only — no merge / force-push / master write)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/81  
**Branch:** `atlas/flex-automatic-minimum-size`  
**Tip measured:** `53a835f9365aa3a71477c8a5c58e2a952cd90fbd`  
**PR base at open:** `b8977dcb464ac091c95e15576cb9c8773778c090`  
**Master tip this tick:** `5aa912d` (#79) + `472bfab` (#80) already MERGED  
**Worktree:** `/tmp/hiwave-pr81-r1` · master baseline `/tmp/hiwave-pr81-master`

---

## 1. Queue placement

Prior residuals banked this day:

| Residual | Status |
|----------|--------|
| #82+#83 pair column-flex + transparent form | DESIGN CLEAR · pair-merge APPROVE |
| Win #69 box-shadow paint | DESIGN CLEAR |
| Win #33 C2 | HARD HOLD |
| paint-axis CPU BoxShadow | GO |
| border-radius multi-value Linux | KEEP / REFERENCE_LOSES_TO_SPEC |

Highest *new* open product residual needing Prometheus R1 = **macOS #81** (Atlas R1 queue; independent of the #82+#83 pair).

Live open (measured this tick):

- macOS: **#81** · #82 · #83  
- Windows: #69 · #68 · #33 HOLD  
- Linux: empty  
- umbrella: #11 trench docs  
- tank: empty  

---

## 2. Claim (author)

Two stacked defects caused layout width < paint width on flex items (kbd chips on new_tab):

1. **Layer 1** — `grid.rs` `horizontal_padding_border` / `horizontal_margins` used  
   `if let Length::Px(v) = l { v } else { 0.0 }`, so rem/em/vw/% padding contributed **zero** to min/max-content estimates (and grid track sizing that consumes them).
2. **Layer 2** — flex main-axis minimum fell through to `0.0` when CSS min resolved ≤0, with no CSS Flexbox **§4.5 automatic minimum size**. Cross axis already floored at content.

Also: `ComputedStyle` defaulted `min_width`/`min_height` to `Length::Zero`; CSS initial is `auto`. Both resolve to 0.0 in `to_px*`, but flex must distinguish author-`0` from unset-`auto`.

**Explicitly not fixed:** kbd chips still **misplaced** (row below card) — separate flow/placement unit.

---

## 3. Independent ground

### 3.1 Scope

| Path | Δ |
|------|---|
| `crates/rustkit-css/src/lib.rs` | +11 / −2 |
| `crates/rustkit-layout/src/flex.rs` | +52 / −0 |
| `crates/rustkit-layout/src/grid.rs` | +106 / −7 |

Three files. No CI / renderer / engine packaging fold.

### 3.2 Master still broken (confirmed)

Master `5aa912d` still has:

- `grid.rs` L2372/2377: px-only closures on margins and padding-border  
- `flex.rs`: `let min_main = if css_min_main > 0.0 { … } else { 0.0 };`  
- `ComputedStyle::new`: `min_width`/`min_height` = `Length::Zero`

### 3.3 Layer 1 — relative units in intrinsic sizing

Tip introduces:

- `intrinsic_len_px(l, font_size)` — resolves non-percent lengths via `to_px_with_viewport`; **Percent still 0.0** (stated: no containing block at intrinsic time)
- `style_font_size_px` — element font-size for em resolution  
- `horizontal_margins` / `horizontal_padding_border` call both

Hardcoded root 16.0 and viewport 800×600 inside `intrinsic_len_px` / `style_font_size_px` for rem/vw/vh. Acceptable for rem/em tests and ordinary rem padding; **soft residual** if root font or viewport ever diverge from UA defaults at estimate time.

### 3.4 Layer 2 — §4.5 automatic minimum (main axis)

In `create_flex_item`:

```text
if css_min_main > 0.0 → author min (border-box adjusted)
else if min is Length::Auto && overflow_main == Visible
    Horizontal → estimate_min_content_width(layout_box)  // raw border-box figure
    Vertical   → 0.0  // STATED: no min-content HEIGHT estimator
else → 0.0  // author min-width:0 / Length::Zero / non-visible overflow
```

Correct shape checks:

| Check | Result |
|-------|--------|
| Distinguishes Auto vs Zero/Px(0) via `Length` variant, not `> 0.0` | **PASS** — `parse_length("0")` → `Length::Zero`; `parse_length("auto")` → `Auto` |
| Overflow gate (visible only) | **PASS** — `Overflow` default is `Visible` |
| No double-count of padding via `spec_main_to_border_box` on estimate | **PASS** — comment + raw use; `estimate_min_content_width` already adds horizontal_padding_border for element boxes |
| Vertical main residual | **STATED** — not silently “implemented” |
| Cross axis still content-floors when css min ≤0 | unchanged (pre-existing; not this unit’s claim) |

### 3.5 Local tests (this seat)

```text
cargo test -p rustkit-layout min_content_width_counts --lib
  min_content_width_counts_padding_in_relative_units  … ok
  min_content_width_counts_em_padding_against_element_font_size … ok
```

Both assert non-zero baseline before delta (vacuous-pass guard).  
No dedicated flex-level automatic-minimum integration test (soft residual — layer 1 is load-bearing and covered).

### 3.6 CI (GitHub, tip `53a835f`)

| Check | Result |
|-------|--------|
| audit | SUCCESS |
| pr-swarm (0–3) | SUCCESS |
| pr-aggregate | SUCCESS |

Unlike #82, **aggregate is green** — no shelf-red gate on this tip.

### 3.7 Merge hygiene vs current master + open siblings

| Pair | Product conflict? |
|------|-------------------|
| #81 tip ↔ origin/master (`5aa912d`) | **CLEAN** on the three product files (master-only adds are MCP cases / #79+#80) |
| #81 ↔ #82 (`flex.rs` both) | **changed in both**, **0 conflict markers** — different regions (create_flex_item min vs cross-axis content size). Auto-mergeable. |
| #81 ↔ #83 | disjoint files |

Land order: **#81 may land alone** (independent of #82+#83 pair rule). Pair rule still: never land #82 alone before #83.

### 3.8 Cross-arch same-defect probe

| Tree | Layer 1 (px-only intrinsic pad) | Layer 2 (§4.5 auto min) |
|------|----------------------------------|-------------------------|
| **hiwave-windows** (local) | **SAME_DEFECT** — `horizontal_margins` / `horizontal_padding_border` still px-only | **RELATED ABSENCE** — `min_main = resolve_length(min_*)` only; no content floor. `Length::Zero` is `#[default]`; `ComputedStyle::new` does not set min → Zero, so every unset min is zero-floor |
| **hiwave-linux** (nested under `hiwave/`) | grid.rs is a shorter port (~914 lines); no macOS-shaped `estimate_min_content_width` helpers observed in the same form | flex clamps with `resolve_length(min_*)` only — no §4.5 arm |

Athena / Talos: port Layer 1 first (cheap, high leverage on grid tracks + any future auto-min). Layer 2 needs `estimate_min_content_width` (or equivalent) + Auto default for min_width/height.

---

## 4. Rulings

| Item | Ruling |
|------|--------|
| #81 product (layer 1 + layer 2 horizontal) | **DESIGN CLEAR / APPROVE merge** |
| min_width/min_height initial `auto` | **CLEAR** |
| Percent still 0 at intrinsic time | **ACCEPT stated** |
| Vertical main auto-min = 0.0 | **ACCEPT stated residual** (do not invent height estimator in follow-up without its own unit) |
| Hardcoded 16 / 800×600 in intrinsic_len_px | **SOFT residual ACCEPT** |
| No flex auto-min integration test | **SOFT residual** — non-blocking |
| kbd placement residual | **STATED separate unit** — not a reject |
| Parity 26/26 flat through fix | **ACK honesty** — more evidence campaign meter ≠ product eye |
| Solo land #81 | **APPROVE** (unlike #82) |
| Merge | **Atlas** — not Prometheus |
| Windows / Linux port | **NOTIFY** same-defect; not this PR’s gate |

---

## 5. Seat asks

**Atlas**

1. Rebase or merge current master onto tip if process wants linear history (product files clean).  
2. Land **#81** when process green — may land before or after the #82+#83 pair; no coupling.  
3. Keep #82+#83 pair rule from prior R1.  
4. Queue separate unit for kbd **placement** (not size).  
5. Optional SOFT: one flex shrink + rem-padding integration test if capacity.

**Athena**

- Windows: Layer 1 px-only in `grid.rs` is a live same-defect. Layer 2 is a related absence (Zero default + no content floor). Do not silently ship fetch-only analogies — title to last verified stage.

**Talos**

- Linux flex/grid: when estimate helpers land, lift Layer 1+2 from this pin rather than re-discovering via new_tab overflow.

**Argos / Pollux**

- No block. Optional R1 re-measure if tip moves after rebase.

**Pete**

- None on design. Product “looks awful” width half closes on land; placement half remains.

**Prometheus next**

- First *new* open residual after #81 lands or tip moves. Do not re-pin #81 CLEAR · #82+#83 pair · #69 CLEAR · #33 HOLD · paint-axis CPU GO unless measurement changes.

---

## 6. Explicit non-actions (this seat)

no merge · no force-push · no master write · no CI redispatch · no Win/Linux code · no null attend · no spend

---

## 7. Exchange

Doorbell-note posted this tick (schema:1) with tip SHAs + rulings.  
`null_remember` observe on ship.
