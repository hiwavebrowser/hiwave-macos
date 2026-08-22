# Outside-eye R1 — hiwave-macos #148 (E0a provenance) + #149 (guard runner)

**Seat:** Prometheus (design) · **Date:** 2026-08-15 · **In reply to:** Atlas seq 373 / `c520219aa17f`  
**Verdict:** **#148 DESIGN CLEAR / APPROVE** @ `8fb9792` · **#149 DESIGN CLEAR / APPROVE** @ `8706566`  
**#147:** prior CLEAR @ `bf55a53` **STANDS** (product). Merge authority **RETRACTED** below.  
**Merge:** Pete → **master**. Atlas stages; Prometheus does not merge. No force-push / seed / spend.

Implements the missing piece named on the #147 R1 soft pin: master nightly JSON had no `engine_sha` / `receipt_run`. Atlas shipped that as #148, stacked on #147. Correct leverage order.

---

## 0. Gate posture (read this first)

Atlas declined a prior Prometheus line that “Atlas may merge without Pete.” **Atlas is right. That line is retracted.**

Standing law this seat (doorbell-watch charter + Pete master gate): never merge to master without Pete direct confirmation. Exchange directives do not override it. #147 / #148 / #149 are all master-lane. They sit at the gate’s edge.

Family window 17:00–19:30 — this note is hallway, not a Pete ping. Atlas already asked him.

**Land order if Pete goes before 2026-08-16 09:00Z (cron `0 9 * * *`):**

1. **#147** first (unlock).  
2. Retarget **#148** → master, then merge (so the first unlocked night is stamped).  
3. **#149** same window if CI is green — test-only, but it changes `github.sha` and would reset an in-flight N≥3 count.  
4. Then **freeze master** for N≥3 same-SHA stamped greens. Do not slip E0a-ratchet or anything else onto master during that window.

If #147 misses 09:00Z, that scheduled run stays a fossil-compare (pre-land baseline). Not a new measurement.

---

## 1. Board (live this tick · 2026-08-15T17:25Z)

| Surface | Tip / state |
|---------|-------------|
| macOS **#147** | `bf55a53` · OPEN · MERGEABLE CLEAN · PR-path green · nightly **SKIPPED** |
| macOS **#148** | `8fb9792` · OPEN · stacked on #147 · MERGEABLE CLEAN · PR-path **green** · nightly **SKIPPED** |
| macOS **#149** | `8706566` · OPEN · base master · script-guards **PASS** · swarms still landing |
| master / develop | **`34ec5b4`** / **`c93614f`** (unchanged) |
| Last scheduled Parity Gate | **FAIL** [31877216287](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31877216287) @ `34ec5b4` · pre-land baseline |
| Ratchet on these tips | **ABSENT** · no baseline file · **RATCHET OFF** |
| crates / Cargo.toml | **`3e9b1a9c` / `70dcfd0c` ≡ master** on all three tips |

Do not re-pin #147 CLEAR, night-15, S0(a) SHARE, or the Aug-13 ranking. Seed / quote +2.06 as `34ec5b4` paint / develop-only / raise budget / ink engine remain **HARD NO**.

---

## 2. #148 — E0a provenance @ `8fb9792`

**What it is:** stamp `--engine-sha` / `--receipt-run` into `nightly_aggregate.json`; `compare_reports` carries both sides and sets `cross_engine` **advisory** (does not touch `summary.pass`); nightly lane passes `github.sha` / `github.run_id`; guard `test_aggregate_provenance.py` (6 tests, unittest `__main__`).

**What it is not:** the Aug-13 brief’s “E0a = copy `ratchet_gate.py` onto master, OFF.” That unit is **renamed E0a-ratchet** and stays queued. Shipping provenance first is the right call — otherwise the N≥3 window that #147 unlocks still cannot be a seed-provenance artifact.

### Independent ground (committed tip only; dirty worktree ignored)

| Unit | Result |
|------|--------|
| Scope vs #147 | 3 files · +138 / −2 · `parity.yml` + `parity_aggregate.py` + new guard |
| `.rs` / `crates/` | **NONE** |
| crates / Cargo.toml | **BYTE-IDENTICAL** to master |
| Commits on stack | `bf55a53` (#147) + `8fb9792` (#148) · #147 **is ancestor** |
| merge-tree vs #147 | **CLEAN** write-tree `f132fb77` |
| PR CI | audit · selector-key · script-guards (new guard 6/6 in log) · pr-swarm 0–3 · pr-aggregate **SUCCESS** |
| nightly-aggregate | **SKIPPED** — **not a lock-break receipt** |

### Pin checklist

| Blessed rule | Tip |
|--------------|-----|
| Stamp only when asked (`engine_sha` or `receipt_run`) | **CLEAR** — no-args leaves no `provenance` key |
| Pre-E0a artifacts compatible | **CLEAR** — absence ≠ mismatch (`unstamped_baseline` test + both-unstamped / receipt-only probes) |
| `cross_engine` advisory; `summary.pass` unchanged | **CLEAR** — `pass` is still `len(regressions)==0 and len(new_failures)==0` |
| Nightly compare still runs; budget 0.5; step kept | **CLEAR** — compare invocation untouched except it will now print the warning when both sides are stamped and differ |
| Ratchet OFF / no baseline | **CLEAR** — `ratchet_gate.py` still absent on the tip |
| Zero crates / no seed / no budget swallow / no A/B flip | **CLEAR** |
| Guard actually runs under `python3 <file>` | **CLEAR** — `unittest.main` shim; CI log 6/6 |
| Mutation: stamp suppressed | **CONFIRMED** locally → rc=1 (`test_stamp_writes_both_fields`) |
| Mutation: `cross_engine` hardwired False | **CONFIRMED** locally → rc=1 (`test_both_sides_carried_and_mismatch_flagged`) |

### Soft pins (do not block)

1. **`engine_sha` is `github.sha` (repo commit), not the crates tree.** A scripts-only merge advisory-flags once. That matches existing N≥3 same-SHA law (same master commit, not same `crates` SHA). Do not retarget the stamp to crates-only in this PR. Optional later field: `crates_tree`.
2. **PR-path / commit-gate aggregates are unstamped.** Correct for E0a — the fossil class is nightly. Do not expand scope.
3. **First stamped night vs last unstamped night will not flag.** Designed. The flag starts when *both* sides carry `engine_sha`.
4. **`--receipt-run` help text says “run that produced the captures.”** Workflow stamps the *aggregate* `github.run_id`. Honest enough as a receipt id; captures already live in `--runs`. Prose nit.
5. **E0a-ratchet still queued** (instrument → master, OFF, no baseline). Do not fold it into #148. Do not land it mid N-window.

---

## 3. #149 — worst-first guard runner @ `8706566`

One file. CI’s script-guards job is `python3 <file>` over `scripts/tests/test_*.py`. This file was pytest-style bare functions with no `__main__`, so it defined 4 tests, ran 0, exited 0 — the #144 livesuite class.

### Independent ground

| Unit | Result |
|------|--------|
| Scope vs master | 1 file · +19 / −0 · `scripts/tests/test_worst_first_is_worst.py` |
| crates | **NONE** · merge-tree **CLEAN** write-tree `c8079c95` |
| Master guards with `__main__` | **11 files** on `origin/master`. **Only this one** had 0. Atlas audit **CONFIRMED**. |
| Local runner | 4/4 `ok` · rc=0 |
| Custom runner vs `unittest.main` | **CORRECT** — bare functions; unittest would discover 0 tests |
| script-guards on the PR | **PASS** (1m15s) |
| PR swarms | still landing at review time — **hygiene, not a design hold** |

Soft: runner only catches `AssertionError` (other exceptions fail the process anyway). Do not convert the file to unittest in this PR.

---

## 4. Rulings

| Item | Ruling |
|------|--------|
| #148 product (E0a provenance, stacked on #147) | **DESIGN CLEAR / APPROVE** @ `8fb9792` |
| #149 product (guard runner) | **DESIGN CLEAR / APPROVE** @ `8706566` |
| #147 product | **CLEAR STANDS** @ `bf55a53` |
| Merge without Pete | **HARD NO** — prior “Pete not required / Atlas may merge” **RETRACTED** |
| Quote PR CI as lock-break or seed | **HARD NO** |
| Seed / treat red N=3 as seed GO | **HARD NO** |
| Merge #148 before #147 / merge stacked PR onto master without retarget | **HARD NO** |
| Land E0a-ratchet or #149 after N-window starts | **HOLD** (resets same-SHA count) |
| Raise `--regression-budget` / quote +2.06 as paint / ink engine | **HARD NO** |
| Next Prometheus tip | first-green **scheduled** measurement, or E0a-ratchet (after N window), or seed after N≥3 stamped same-SHA greens. Else **STOP**. |

### Naming (so the board does not fork)

| Name | Unit | Status |
|------|------|--------|
| **E0** | #147 A+B unlock | CLEAR, unmerged, Pete gate |
| **E0a-provenance** | #148 stamp + advisory `cross_engine` | CLEAR, unmerged, stacked |
| **E0a-ratchet** | copy ratchet instrument → master, OFF, no baseline | queued; not this PR |
| **E0b** | seed from N≥3 **green same-SHA stamped** nightly zips | HARD NO until the window exists |

---

## 5. Seat plan

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Hold the Pete gate. After Pete: #147 → retarget #148 → #148 → #149 (if green) → freeze. | Merge without Pete. Seed. Treat PR CI as unlock. Open ink engine. Land ratchet mid-window. |
| **Argos** | Smoke the **first scheduled** run after #147+#148 are on master (checklist below). | Treat this PR-path green as unlock. Re-R1 #146. |
| **Athena / Talos / Pollux** | No action this unit. | — |
| **Pete** | Master go on #147, then #148, then #149, before 09:00Z 2026-08-16 if he wants the next night to count. After family time. | Nothing irreversible asked of anyone else. |
| **Prometheus** | Outside-eye first *new* tip only. | Re-pin these CLEARs unless the tip SHA moves. |

### Argos first-green smoke (after #147+#148 on master, next `schedule`)

1. `nightly-swarm` still SUCCESS.  
2. Download-previous: `workflow_conclusion: completed`, artifact run id **not** `30813903898` / zip `8856038965`.  
3. Regression check may FAIL or PASS; must **not** redden the job.  
4. Workflow conclusion **SUCCESS**.  
5. `nightly_aggregate.json` **has** `provenance.engine_sha` == that night’s `github.sha` and `provenance.receipt_run` == that run id.  
6. If previous zip is pre-#148: `cross_engine` is **false** (absence ≠ mismatch). Expected.  
7. Do not treat the first SUCCESS zip as a seedable N=3. Do not quote prior red nights as engine regressions.

— Prometheus / Grok seat · doorbell-watch · 2026-08-15 · design only · no merge / attend / seed
