# Outside-eye: hiwave-macos PR #72 — grad/rad longest-first

**Seat:** Prometheus (design / R1 only)  
**Date:** 2026-07-30 (grind tick)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/72  
**HEAD:** `3e573c21098348e4c032f556932a19d5ffac5ce0` · branch `atlas/grad-suffix-order`  
**Verdict:** **DESIGN CLEAR / APPROVE merge**  
**Exists in service of:** HiWave transform wire honesty — `parse_angle` must not silently drop gradian angles on the reference tree.

**Not this unit:** Linux #19 (box-shadow CLEAR already; Argos R1) · Windows #33 C2 HOLD · W0b implementation · third-gate constitution · force-push carve-out (Pete queue) · ms/s + fleet ends_with sweep (CLOSED).

---

## 0. Why this unit

| Fact | Measurement |
|------|-------------|
| Queue residual | Standing open list after ms/s closed: **grad/rad fix on macOS reference** (Atlas execute) |
| Live board | Open design-relevant: macOS **#72**, Linux #19 (design closed), Win #33 HOLD, macOS #68 GPU ACCEPTED |
| Pattern pin | `overlapping-suffix-longest-first` already banked (rem/em, grad/rad); this is the product fix |
| PR self-labels | Body: **R1: Prometheus** |

---

## 1. Independent ground (this tick)

### 1.1 Master still has the defect

`origin/master` `parse_angle` order:

1. `deg` → `rad` → `turn` → `grad` → bare number

`grad` ends with `rad`. Example: `"200grad".ends_with("rad")` is true → strip 3 chars → `"200g".parse()` fails → **`None`**. The `grad` arm is unreachable dead code that looked like support.

Silent failure mode (not wrong number): `transform: rotate(200grad)` drops the whole transform op.

### 1.2 Tip fix

Order becomes: **`grad` → `turn` → `deg` → `rad`** → bare number.

| Unit | Conversion on tip | Unchanged from master? |
|------|-------------------|------------------------|
| grad | `* 0.9` → degrees | yes |
| turn | `* 360.0` | yes |
| deg | identity | yes |
| rad | `to_degrees()` | yes |
| bare | degrees | yes |

Hazard comment names the class and points at the round-trip test as the real guard.

### 1.3 Tests — local execute (Prometheus)

```
cargo test -p rustkit-engine grad_suffix --lib
→ 4 passed / 0 failed
```

| Test | Why it matters |
|------|----------------|
| `grad_is_not_swallowed_by_the_rad_arm` | Direct T-RED of the bug (100/200/400 grad) |
| `rad_still_works_after_the_reorder` | Regression guard on the shorter suffix |
| `the_other_angle_units_are_unaffected` | deg / turn / bare |
| `every_unit_round_trips_to_the_same_quarter_turn` | **Class detector** — same 90° in five spellings; per-unit tests cannot see suffix-eating |

### 1.4 CI

All required checks **SUCCESS** on HEAD `3e573c2` (audit, pr-swarm ×4, pr-aggregate, collect-metrics).

### 1.5 Scope mix (non-blocking nit)

Second file: `docs/WPT_W0B_IMPLEMENT_PIN_2026-07-29.md` (229 lines) — banked W0b IMPLEMENT pin landing into tree. Orthogonal to the engine fix; content already ruled IMPLEMENT_NOW. Prefer separate docs commit next time; **not a hold** — pin was missing from master and is load-bearing for the next W0b PR.

---

## 2. Rulings

| Item | Ruling |
|------|--------|
| Product fix (longest-first + conversions) | **DESIGN CLEAR / APPROVE** |
| Hazard comment + ordered chain | **APPROVE** — matches standing rule |
| Cross-unit round-trip test | **APPROVE** — keep as the class guard; do not thin to per-unit only |
| W0b pin docs ride-along | **Non-blocking accept** (nit: prefer separate PR/commit) |
| Sweep claim (only live instance on macOS) | **ACCEPTED** — dual independent negatives already closed ms/s + ends_with fleet list |
| Standing rule `overlapping-suffix-longest-first` | **UNCHANGED** — this is the product land of the pin |
| Renderer still ignores transform | **Out of scope** — wire honesty, not paint |

---

## 3. Seat actions

| Seat | Action |
|------|--------|
| **Atlas** | Merge #72 when process green (your lane). Optional follow-up: split pure-docs landings from engine fixes. |
| **Athena** | No re-open on Windows if #48 already fixed parse_angle there; confirm no regress. |
| **Talos / Argos** | Linux #18 already receipted longest-first; no re-work from this ruling. |
| **Pollux** | Optional execute-count if process wants dual R1; Prometheus R1 is the design half. |
| **Pete** | None on this PR. |
| **Prometheus** | No re-review unless tip changes semantics or drops the round-trip test. |

---

## 4. What this does not unlock

- W0b runner (still IMPLEMENT_NOW, separate PR)
- Windows #33 net-cache wire (C2 HOLD)
- Third-gate constitution / force-push carve-out (Pete queue)
- Paint path for transform / box-shadow

---

## 5. One-line summary

**#72 fixes the only confirmed live overlapping-suffix bug on macOS `parse_angle` with the correct longest-first order, hazard comment, and a class-level round-trip test — DESIGN CLEAR / APPROVE merge.**
