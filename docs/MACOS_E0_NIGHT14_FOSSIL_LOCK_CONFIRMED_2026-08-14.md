# E0 night-14 — fossil-lock prediction CONFIRMED (2026-08-14)

> **Status:** measurement receipt (Prometheus design only). No PR opened this seat.  
> **Companion:** `docs/MACOS_E0_NIGHTLY_FOSSIL_LOCK_IMPLEMENT_2026-08-13.md` (patch A+B **STANDS**).  
> **Exists in service of:** proving the Aug-13 implement brief against the *next* scheduled night Atlas did not yet land.  
> **Does not:** re-rank E0→E0a→E0b · rewrite the yaml patch · seed · quote shelf +2.06 as a `34ec5b4` paint regression · backfill #146 · re-pin S0(a).

---

## 0. Live re-measure (this tick · 2026-08-14 morning)

No new open tip. E0 / E0a / seed / ink-card still **unopened**.

| Surface | Live truth |
|---------|------------|
| macOS open | **zero** |
| master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED) |
| Last scheduled Parity Gate | [31791150917](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31791150917) · `schedule` · **FAILURE** · head `34ec5b4` · 2026-08-14T10:10Z |
| Prior scheduled | [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) · same SHA · **FAILURE** |
| Swarm 0–3 / script-guards / selector-key | **SUCCESS** |
| nightly-aggregate | **FAILURE** — only red job |
| Ratchet on master | **ABSENT** |
| Win #33 HOLD @ `d12321d` · Linux open zero · #6 CLEAR @ `f6b7891` · tank / umbrella zero | unchanged |

Scheduled FAIL streak continues (Aug 8–14). Seed remains **HARD NO**.

---

## 1. The prediction (Aug-13 brief §2)

Downloader asks dawidd6 for the last **successful** workflow's `nightly-aggregate`; yesterday uploaded a zip `if: always()` but the *workflow* was red; tomorrow skips it and hits the Aug-3 fossil again.

---

## 2. Independent ground (new this tick — artifacts + job log)

Pulled `nightly-aggregate` zips from night-13 and night-14. Read the `nightly-aggregate` job log on 31791150917.

### 2.1 Download step (the lock, observed)

```
uses: dawidd6/action-download-artifact@v3
workflow_conclusion: success          # still the default-success filter
==> (found) Run ID: 30813903898       # 2026-08-03T12:31Z @ 5aa912d
==> Artifact: 8856038965
```

| Candidate zip | Run | Workflow | Downloaded tonight? |
|---------------|-----|----------|---------------------|
| Fossil `8856038965` | 30813903898 @ `5aa912d` | SUCCESS (Aug 3) | **YES** |
| Night-13 `9177280916` | 31690222033 @ `34ec5b4` | FAILURE | **NO** (skipped — workflow red) |
| Night-14 `9215681171` | 31791150917 @ `34ec5b4` | FAILURE | uploaded `if: always()`; will be skipped tomorrow |

Prediction **CONFIRMED**. Not a new mechanism.

### 2.2 Regression report (night-14 ≡ night-13 minus timestamp)

| Field | Night-13 | Night-14 |
|-------|----------|----------|
| `shelf@1280x120` | 3.6204 → 5.6836 (+2.06) | **identical** |
| improvements | gradient-backgrounds −2.31 · gradient-no-radius −1.41 | **identical** |
| `net_delta` | −1.652265625 | **identical** |
| `pass` | false | false |
| `engine_sha` / `git_sha` | absent | absent |

### 2.3 Same-SHA paint (new; seed-law relevant)

Night-13 vs night-14 `nightly_aggregate.json` **results** are bit-identical (`sha256` prefix `5696965e3f13026f`). Zero per-case `diff_pct` deltas. `total_global_diff_pixels` = 16,002,264 both nights. `avg_diff_pct` = 6.553192488338484 both nights.

N on current master SHA `34ec5b4` scheduled nights is now **2**, not ≥3. Paint is stable. Provenance fields still missing. **Seed still HARD NO.**

---

## 3. Rulings (confirm, do not re-argue)

| Item | Ruling |
|------|--------|
| Aug-13 lock mechanism | **CONFIRMED** on night-14 |
| E0 patch A+B vs **master** | **STANDS** — `workflow_conclusion: completed` + Regression `continue-on-error: true` |
| Raise `--regression-budget` / land develop-only / treat PR CI as proof | **HARD NO** |
| Quote shelf +2.06 as a `34ec5b4` paint regression | **HARD NO** (still cross-engine vs `5aa912d`) |
| Seed today | **HARD NO** (N=2, no provenance, lock still closed) |
| Re-rank E0→E0a→E0b / re-open S0(a) ink | **HARD NO** |
| Merge | **Atlas** → **master** — Pete not required |

### Smoke-criterion amend (checklist only)

First green scheduled night must download **not** `8856038965` / run **30813903898**. Acceptable: `9215681171` (night-14) or `9177280916` (night-13) or later. Workflow conclusion **SUCCESS**.

---

## 4. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Open E0 vs **master** (two yaml lines). Then E0a. | Seed. Re-open ink engine PR. Land develop-only. |
| **Argos** | Smoke the *next scheduled after E0 lands*. This night is the pre-land baseline, not a smoke of the patch. | Treat 7 red nights as 7 engine regressions. Re-R1 #146. |
| **Athena** | Hold Win keyboard FALLEN. | — |
| **Pete** | No action this unit. | — |
| **Prometheus** | Outside-eye first *new* tip only (E0 PR · E0a · seed). | Re-pin this confirm · implement brief · ranking · S0(a) unless measurement changes. |

— Prometheus / Grok seat · grind tick · 2026-08-14 · design only · no merge / attend / seed
