# E0 night-16 — fossil-lock CONFIRMED; 09:00Z land window MISSED (2026-08-16)

> **Status:** measurement receipt (Prometheus design only). No PR opened this seat.  
> **Companion:** `docs/MACOS_E0_NIGHTLY_FOSSIL_LOCK_IMPLEMENT_2026-08-13.md` (patch A+B **STANDS**).  
> **Prior receipts:** night-14 **STANDS** · night-15 **STANDS** · #147/#148/#149 CLEAR **STAND** (unmerged).  
> **Exists in service of:** proving the lock against the first scheduled night *after* Pete-go was requested, and recording that the 2026-08-16 09:00Z unlock window was missed.  
> **Does not:** re-rank E0→E0a-provenance→#149→freeze · rewrite A+B · seed · quote shelf +2.06 as a `34ec5b4` paint regression · re-pin S0(a) · divert Tank / WPT / suite · re-open an ink/1px/CT engine PR.

---

## 0. Live re-measure (this tick · 2026-08-16 morning)

No new tip. Banked CLEARs stay banked. Tips **UNCHANGED**.

| Surface | Live truth |
|---------|------------|
| macOS **#147** | tip **`bf55a53`** · OPEN · MERGEABLE · CLEAR banked · vs master |
| macOS **#148** | tip **`8fb9792`** · OPEN · MERGEABLE · CLEAR banked · base #147 branch (not yet retargeted) |
| macOS **#149** | tip **`8706566`** · OPEN · MERGEABLE · CLEAR banked · vs master |
| macOS master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED) |
| Scheduled Parity Gate | **FAIL** [31939340261](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31939340261) · `schedule` · head `34ec5b4` · 2026-08-16T09:33Z |
| Prior scheduled (same SHA) | FAIL [31877216287](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31877216287) night-15 · [31791150917](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31791150917) night-14 · [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) night-13 |
| Swarm 0–3 / script-guards / selector-key | **SUCCESS** |
| nightly-aggregate | **FAILURE** — only red job (Regression check) |
| Ratchet on master | **ABSENT** · **RATCHET OFF** |
| Win | open **#33 HOLD only** @ `d12321d` · keyboard FALLEN |
| Linux / umbrella / tank | open **zero** |
| community **#6** | OPEN · CLEAR @ **`f6b7891`** · GitHub tip **UNCHANGED** |
| Atlas last real | 2026-08-15T20:09Z seq **376** (Pollux ask; answered seq 555) |
| Argos last real | 2026-08-15T17:41Z seq **523** |

**Process fact (new):** Pete-go was requested so land could happen *before* 2026-08-16 09:00Z. Night-16 fired at 09:33Z on unpatched master. The unlock-and-stamp night is now **night-17+**, not tonight.

---

## 1. The prediction (Aug-13 brief + night-14/15 confirms)

Downloader still asks dawidd6 for the last **successful** workflow's `nightly-aggregate`. Night-15 uploaded zip `9245120202` `if: always()` but the *workflow* was red; night-16 skips every red zip and hits the Aug-3 fossil again.

---

## 2. Independent ground (new this tick — artifacts + job log)

Pulled `nightly-aggregate` zips night-14 (`9215681171`), night-15 (`9245120202`), night-16 (`9261639825`). Read `nightly-aggregate` job log on 31939340261 (job 95146602977).

### 2.1 Download step (the lock, observed)

```
uses: dawidd6/action-download-artifact@v3
workflow_conclusion: success
==> (found) Run ID: 30813903898       # 2026-08-03T12:31Z @ 5aa912d
==> Artifact: 8856038965
```

| Candidate zip | Run | Workflow | Downloaded tonight? |
|---------------|-----|----------|---------------------|
| Fossil `8856038965` | 30813903898 @ `5aa912d` | SUCCESS (Aug 3) | **YES** |
| Night-13 `9177280916` | 31690222033 @ `34ec5b4` | FAILURE | **NO** |
| Night-14 `9215681171` | 31791150917 @ `34ec5b4` | FAILURE | **NO** |
| Night-15 `9245120202` | 31877216287 @ `34ec5b4` | FAILURE | **NO** |
| Night-16 `9261639825` | 31939340261 @ `34ec5b4` | FAILURE | uploaded `if: always()`; will be skipped tomorrow |

Prediction **CONFIRMED**. Not a new mechanism. Fourth consecutive same-SHA fossil hit.

### 2.2 Regression report (night-16 ≡ night-15 ≡ night-14 minus timestamp)

| Field | Night-14 / 15 / 16 |
|-------|---------------------|
| `shelf@1280x120` | 3.6204 → 5.6836 (+2.06) |
| improvements | gradient-backgrounds −2.31 · gradient-no-radius −1.41 |
| `net_delta` | −1.652265625 |
| `pass` | false |
| `engine_sha` / `git_sha` / `provenance` | **absent** |

`regression_report.json` minus `timestamp` is **byte-equal** across all three (sha prefix `1770c8c4d48f79a2`).

### 2.3 Same-SHA paint (seed-law relevant)

Night-14 vs night-15 vs night-16 `nightly_aggregate.json` **results** are bit-identical (`sha256` prefix `5696965e3f13026f`). Zero per-case `diff_pct` deltas. `total_global_diff_pixels` = 16,002,264. `avg_diff_pct` = 6.553192488338484.

N on current master SHA `34ec5b4` scheduled nights is now **4** (31690222033 · 31791150917 · 31877216287 · 31939340261). Paint is stable across all four. Provenance fields still missing. Lock still closed. **Seed still HARD NO.**

The N≥3 *count* precondition was already **MET** on night-15. Night-16 does not move seed law. Remaining blockers: (1) fossil-lock still closed, (2) no `engine_sha`/`git_sha`, (3) these four nights are red workflows — they are not a seedable master baseline. After land, first-green is N=1 of a *new* stamped window, not N=5 of this red series.

---

## 3. Rulings (confirm, do not re-argue)

| Item | Ruling |
|------|--------|
| Aug-13 lock mechanism | **CONFIRMED** on night-16 (fourth consecutive same-SHA fossil hit) |
| 09:00Z unlock window | **MISSED** — night-16 is still pre-land baseline |
| E0 #147 A+B vs **master** | **STANDS** · CLEAR @ `bf55a53` |
| E0a-provenance #148 · guard #149 | **STAND** · CLEAR @ `8fb9792` / `8706566` |
| Land order | **#147 → retarget #148 → #148 → #149 (if CI green) → freeze** |
| Merge without Pete | **HARD NO** (prior retraction stands) |
| Raise `--regression-budget` / land develop-only / treat PR CI as proof | **HARD NO** |
| Quote shelf +2.06 as a `34ec5b4` paint regression | **HARD NO** (still cross-engine vs `5aa912d`) |
| Treat scheduled N=4 as seed GO | **HARD NO** |
| Re-rank / re-open ink / divert Tank·WPT·suite | **HARD NO** |
| Merge | **Pete then Atlas** — not Prometheus |

### Smoke-criterion amend (checklist only)

First green scheduled night must download **not** `8856038965` / run **30813903898**. Acceptable: `9261639825` (night-16) or `9245120202` (night-15) or `9215681171` (night-14) or `9177280916` (night-13) or later. Workflow conclusion **SUCCESS**. `nightly_aggregate.json` should carry `provenance.engine_sha` only *after* #148 is on master.

---

## 4. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Hold Pete-gate. After go: 147 → retarget 148 → 148 → 149 → freeze. | Seed. Cite 146.6 / 91 / 95.7 / 14.7. Open an ink/1px/CT PR. Treat N=4 as go. |
| **Argos** | Smoke the *first scheduled after #147+#148 on master*. This night is still the pre-land baseline. | Treat 4 red `34ec5b4` nights as 4 engine regressions. Download 8856038965 / 30813903898 as "previous". |
| **Athena** | Hold Win keyboard FALLEN. | Restart Pollux from this pin. |
| **Pete** | Master go still required. Window for *tonight* is gone; next stampable night is the following schedule. | — |
| **Prometheus** | Outside-eye first *new* tip, or first-green scheduled after land. | Re-pin this confirm · night-15 · night-14 · implement brief · S0(a) · #147/#148/#149 CLEAR unless measurement changes. |

— Prometheus / Grok seat · grind tick · 2026-08-16 · design only · no merge / attend / seed
