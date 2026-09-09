# E0 night-18 — fossil-lock CONFIRMED (2026-08-18)

> **Status:** measurement receipt (Prometheus design only). No PR opened this seat.  
> **Companion:** `docs/MACOS_E0_NIGHTLY_FOSSIL_LOCK_IMPLEMENT_2026-08-13.md` (patch A+B **STANDS**).  
> **Prior receipts:** night-14 **STANDS** · night-15 **STANDS** · night-16 **STANDS** · night-17 **STANDS** · #147/#148/#149/#150 CLEAR **STAND** (unmerged).  
> **Exists in service of:** proving the lock against the first scheduled night *after* night-17. Night-18 is still pre-land baseline.  
> **Does not:** re-rank E0→E0a-provenance→#149→freeze · rewrite A+B · seed · quote shelf +2.06 as a `34ec5b4` paint regression · re-pin S0(a) · divert Tank / WPT / suite · re-open an ink/1px/CT engine PR · land #150 to master before E0.

---

## 0. Live re-measure (this tick · 2026-08-18 ~15:20Z)

No new tip. Banked CLEARs stay banked. Tips **UNCHANGED**. New measurement: scheduled night-18.

| Surface | Live truth |
|---------|------------|
| macOS **#147** | tip **`bf55a53`** · OPEN · MERGEABLE · CLEAR banked · vs master · audit+guards+swarm×4+aggregate **SUCCESS** · nightly-aggregate **SKIPPED** |
| macOS **#148** | tip **`8fb9792`** · OPEN · MERGEABLE · CLEAR banked · base #147 branch (not yet retargeted) · PR CI SUCCESS · nightly-aggregate **SKIPPED** |
| macOS **#149** | tip **`8706566`** · OPEN · MERGEABLE · CLEAR banked · vs master · PR CI SUCCESS |
| macOS **#150** | tip **`87c058a`** · OPEN · MERGEABLE · CLEAR banked · vs master · PR CI SUCCESS |
| macOS master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED) |
| Scheduled Parity Gate | **FAIL** [32122723365](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/32122723365) · `schedule` · head `34ec5b4` · 2026-08-18T09:39Z |
| Prior scheduled (same SHA) | FAIL [32017191626](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/32017191626) night-17 · [31939340261](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31939340261) night-16 · [31877216287](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31877216287) night-15 · [31791150917](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31791150917) night-14 · [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) night-13 |
| Swarm 0–3 / script-guards / selector-key | **SUCCESS** |
| nightly-aggregate | **FAILURE** — only red job (Regression check) · job **95667478131** |
| Ratchet on master | **ABSENT** · **RATCHET OFF** |
| Win | open **#33 HOLD only** @ `d12321d` · keyboard FALLEN |
| Linux / umbrella / tank | open **zero** |
| community **#6** | OPEN · CLEAR @ **`f6b7891`** · GitHub tip **UNCHANGED** |
| Atlas last real | 2026-08-16T16:08Z seq **377** (noon digest n27; #150) |
| Argos last real | 2026-08-16T16:23Z seq **531** CLEAR @ `87c058a` (later seqs are tank-gauges only) |

**Process fact:** Pete-go 09:00Z 2026-08-16 was already **MISSED**. Night-18 fired at 09:39Z on unpatched master. First stampable night is still the schedule *after* #147+#148 land.

---

## 1. The prediction (Aug-13 brief + night-14/15/16/17 confirms)

Downloader still asks dawidd6 for the last **successful** workflow's `nightly-aggregate`. Night-17 uploaded zip `9284144856` `if: always()` but the *workflow* was red; night-18 skips every red zip and hits the Aug-3 fossil again.

---

## 2. Independent ground (new this tick — artifacts + job log)

Pulled `nightly-aggregate` zips night-17 (`9284144856`) and night-18 (`9319310209`). Read `nightly-aggregate` job log on 32122723365 (job 95667478131).

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
| Night-16 `9261639825` | 31939340261 @ `34ec5b4` | FAILURE | **NO** |
| Night-17 `9284144856` | 32017191626 @ `34ec5b4` | FAILURE | **NO** (uploaded last night; skipped as predicted) |
| Night-18 `9319310209` | 32122723365 @ `34ec5b4` | FAILURE | uploaded `if: always()`; will be skipped tomorrow |

Prediction **CONFIRMED**. Not a new mechanism. Sixth consecutive same-SHA fossil hit.

### 2.2 Regression report (night-18 ≡ night-17 ≡ night-16 ≡ night-15 ≡ night-14 minus timestamp)

| Field | Night-14 / 15 / 16 / 17 / 18 |
|-------|------------------------------|
| `shelf@1280x120` | 3.6204 → 5.6836 (+2.06) |
| improvements | gradient-backgrounds −2.31 · gradient-no-radius −1.41 |
| `net_delta` | −1.652265625 |
| `pass` | false |
| `engine_sha` / `git_sha` / `provenance` | **absent** |

`regression_report.json` minus `timestamp` is **byte-equal** n17 ≡ n18 (sha prefix `1770c8c4d48f79a2` — same prefix as the night-16/17 receipts vs n14/n15). Only field diff vs night-17 is `timestamp` (`2026-08-17T09:56:33` → `2026-08-18T09:44:20`).

### 2.3 Same-SHA paint (seed-law relevant)

Night-18 vs night-17 `nightly_aggregate.json` **results** are bit-identical (`json.dumps` sha `fc0b56ca6fddab6c`). Zero per-case `diff_pct` deltas. `total_global_diff_pixels` = 16,002,264. `avg_diff_pct` = 6.553192488338484. Summary minus timestamp **equal**.

Non-paint diffs vs night-17 are only `timestamp` and runner `attribution_path` / `overlay_path` (`nightly-452-*` vs `nightly-453-*`). Night-17 already established n14 ≡ n15 ≡ n16 ≡ n17 paint.

N on current master SHA `34ec5b4` scheduled nights is now **6** (31690222033 · 31791150917 · 31877216287 · 31939340261 · 32017191626 · 32122723365). Paint is stable across all six. Provenance fields still missing. Lock still closed. **Seed still HARD NO.**

The N≥3 *count* precondition was already **MET** on night-15. Night-18 does not move seed law. Remaining blockers: (1) fossil-lock still closed, (2) no `engine_sha`/`git_sha`, (3) these six nights are red workflows — they are not a seedable master baseline. After land, first-green is N=1 of a *new* stamped window, not N=7 of this red series.

---

## 3. Rulings (confirm, do not re-argue)

| Item | Ruling |
|------|--------|
| Aug-13 lock mechanism | **CONFIRMED** on night-18 (sixth consecutive same-SHA fossil hit) |
| 09:00Z 2026-08-16 unlock window | **MISSED** (already recorded) — night-18 is still pre-land baseline |
| E0 #147 A+B vs **master** | **STANDS** · CLEAR @ `bf55a53` |
| E0a-provenance #148 · guard #149 | **STAND** · CLEAR @ `8fb9792` / `8706566` |
| #150 softwrap slice-0 | **STANDS** · CLEAR @ `87c058a` · do **not** land to master before E0 |
| Land order | **#147 → retarget #148 → #148 → #149 (if CI green) → freeze** |
| Merge without Pete | **HARD NO** (prior retraction stands) |
| Raise `--regression-budget` / land develop-only / treat PR CI as proof | **HARD NO** |
| Quote shelf +2.06 as a `34ec5b4` paint regression | **HARD NO** (still cross-engine vs `5aa912d`) |
| Treat scheduled N=6 as seed GO | **HARD NO** |
| Re-rank / re-open ink / divert Tank·WPT·suite | **HARD NO** |
| Merge | **Pete then Atlas** — not Prometheus |

### Smoke-criterion amend (checklist only)

First green scheduled night must download **not** `8856038965` / run **30813903898**. Acceptable: `9319310209` (night-18) or `9284144856` (night-17) or `9261639825` (night-16) or `9245120202` (night-15) or `9215681171` (night-14) or `9177280916` (night-13) or later. Workflow conclusion **SUCCESS**. `nightly_aggregate.json` should carry `provenance.engine_sha` only *after* #148 is on master.

---

## 4. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Hold Pete-gate. After go: 147 → retarget 148 → 148 → 149 → freeze. Prefer retarget #150 → develop unless Pete reorders. | Seed. Cite 146.6 / 91 / 95.7 / 14.7. Open an ink/1px/CT PR. Treat N=6 as go. Auto-merge #150 to master before E0. |
| **Argos** | Smoke the *first scheduled after #147+#148 on master*. This night is still the pre-land baseline. | Treat 6 red `34ec5b4` nights as 6 engine regressions. Download 8856038965 / 30813903898 as "previous". |
| **Athena** | Hold Win keyboard FALLEN. Review-lane ACK on #150 is not merge auth. | Restart Pollux from this pin. |
| **Pete** | Master go still required. No re-ping from this seat. | — |
| **Prometheus** | Outside-eye first *new* tip, or first-green scheduled after land. | Re-pin this confirm · night-17 · night-16 · night-15 · night-14 · #147/#148/#149/#150 CLEAR · SHARE 3.45% · S0(a) PARK unless measurement changes. |

— Prometheus / Grok seat · grind tick · 2026-08-18 · design only · no merge / attend / seed
