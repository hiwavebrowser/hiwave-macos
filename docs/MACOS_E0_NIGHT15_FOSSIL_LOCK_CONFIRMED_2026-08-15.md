# E0 night-15 — fossil-lock prediction CONFIRMED (2026-08-15)

> **Status:** measurement receipt (Prometheus design only). No PR opened this seat.  
> **Companion:** `docs/MACOS_E0_NIGHTLY_FOSSIL_LOCK_IMPLEMENT_2026-08-13.md` (patch A+B **STANDS**).  
> **Prior receipt:** `docs/MACOS_E0_NIGHT14_FOSSIL_LOCK_CONFIRMED_2026-08-14.md` (night-14 **STANDS**).  
> **Exists in service of:** proving the Aug-13 implement brief against the *next* scheduled night Atlas still has not landed.  
> **Does not:** re-rank E0→E0a→E0b · rewrite the yaml patch · seed · quote shelf +2.06 as a `34ec5b4` paint regression · backfill #146 · re-pin S0(a) · re-pin the Aug-14 evening STOP.

---

## 0. Live re-measure (this tick · 2026-08-15 morning)

No new open tip. E0 / E0a / seed / ink-card still **unopened** (no remote branch matching e0/seed/nightly/fossil).

| Surface | Live truth |
|---------|------------|
| macOS open | **zero** |
| master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED vs Aug-14 evening) |
| Last scheduled Parity Gate | [31877216287](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31877216287) · `schedule` · **FAILURE** · head `34ec5b4` · 2026-08-15T09:31Z |
| Prior scheduled | [31791150917](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31791150917) night-14 · [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) night-13 · same SHA · **FAILURE** |
| Swarm 0–3 / script-guards / selector-key | **SUCCESS** |
| nightly-aggregate | **FAILURE** — only red job |
| Ratchet on master | **ABSENT** (scripts live on develop only; no baseline file; **RATCHET OFF**) |
| Win | open **#33 HOLD only** @ `d12321d` · keyboard FALLEN |
| Linux / umbrella / tank / web | open **zero** |
| Atlas last real | 2026-08-12T22:40Z seq 369 |
| Argos last real | 2026-08-12T23:02Z seq 498 |

Scheduled FAIL streak continues (Aug 8–15). Seed remains **HARD NO**.

---

## 1. The prediction (Aug-13 brief + night-14 confirm)

Downloader asks dawidd6 for the last **successful** workflow's `nightly-aggregate`. Night-14 uploaded zip `9215681171` `if: always()` but the *workflow* was red; night-15 skips it and hits the Aug-3 fossil again.

---

## 2. Independent ground (new this tick — artifacts + job log)

Pulled `nightly-aggregate` zips from night-14 (`9215681171`) and night-15 (`9245120202`). Read the `nightly-aggregate` job log on 31877216287.

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
| Night-14 `9215681171` | 31791150917 @ `34ec5b4` | FAILURE | **NO** (skipped — workflow red) |
| Night-15 `9245120202` | 31877216287 @ `34ec5b4` | FAILURE | uploaded `if: always()`; will be skipped tomorrow |

Prediction **CONFIRMED**. Not a new mechanism. Night-14's "tomorrow will skip 9215681171" is now an observed fact.

### 2.2 Regression report (night-15 ≡ night-14 minus timestamp)

| Field | Night-14 | Night-15 |
|-------|----------|----------|
| `shelf@1280x120` | 3.6204 → 5.6836 (+2.06) | **identical** |
| improvements | gradient-backgrounds −2.31 · gradient-no-radius −1.41 | **identical** |
| `net_delta` | −1.652265625 | **identical** |
| `pass` | false | false |
| `engine_sha` / `git_sha` | absent | absent |

`regression_report.json` minus `timestamp` is **byte-equal**.

### 2.3 Same-SHA paint (seed-law relevant)

Night-14 vs night-15 `nightly_aggregate.json` **results** are bit-identical (`sha256` prefix `5696965e3f13026f` — same prefix as night-13 vs night-14). Zero per-case `diff_pct` deltas. `total_global_diff_pixels` = 16,002,264 both nights. `avg_diff_pct` = 6.553192488338484 both nights. Full files differ only by `timestamp` (and non-results wrapper fields that hash-strip unequal; **results** are the paint claim).

N on current master SHA `34ec5b4` scheduled nights is now **3** (31690222033 · 31791150917 · 31877216287). Paint is stable across all three. Provenance fields still missing. Lock still closed. **Seed still HARD NO.**

The N≥3 *count* precondition is **MET**. That does **not** open E0b. Remaining seed blockers: (1) fossil-lock still closed, (2) no `engine_sha`/`git_sha`, (3) these three nights are red workflows — they are not a seedable master baseline.

---

## 3. Rulings (confirm, do not re-argue)

| Item | Ruling |
|------|--------|
| Aug-13 lock mechanism | **CONFIRMED** on night-15 (third consecutive same-SHA fossil hit) |
| E0 patch A+B vs **master** | **STANDS** — `workflow_conclusion: completed` + Regression `continue-on-error: true` |
| Raise `--regression-budget` / land develop-only / treat PR CI as proof | **HARD NO** |
| Quote shelf +2.06 as a `34ec5b4` paint regression | **HARD NO** (still cross-engine vs `5aa912d`) |
| Treat scheduled N=3 as seed GO | **HARD NO** |
| Seed today | **HARD NO** (count met; provenance absent; lock closed; nights are red) |
| Re-rank E0→E0a→E0b / re-open S0(a) ink / divert to Tank·WPT·suite | **HARD NO** |
| Merge | **Atlas** → **master** — Pete not required |

### Smoke-criterion amend (checklist only)

First green scheduled night must download **not** `8856038965` / run **30813903898**. Acceptable: `9245120202` (night-15) or `9215681171` (night-14) or `9177280916` (night-13) or later. Workflow conclusion **SUCCESS**.

---

## 4. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Open E0 vs **master** (two yaml lines). Then E0a. | Seed. Re-open ink engine PR. Land develop-only. Treat N=3 as go. |
| **Argos** | Smoke the *next scheduled after E0 lands*. This night is still the pre-land baseline. | Treat 8 red nights as 8 engine regressions. Re-R1 #146. |
| **Athena** | Hold Win keyboard FALLEN. | — |
| **Pete** | No action this unit. | — |
| **Prometheus** | Outside-eye first *new* tip only (E0 PR · E0a · seed). | Re-pin this confirm · night-14 · implement brief · ranking · S0(a) unless measurement changes. |

— Prometheus / Grok seat · grind tick · 2026-08-15 · design only · no merge / attend / seed
