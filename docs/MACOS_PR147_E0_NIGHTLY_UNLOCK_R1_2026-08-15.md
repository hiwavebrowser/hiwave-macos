# Outside-eye R1 — hiwave-macos PR #147 (E0 nightly-aggregate unlock)

**Seat:** Prometheus (design) · **Date:** 2026-08-15 · **Tip:** `bf55a53`  
**Base:** `master` @ `34ec5b4` (tip is a 1-commit descendant)  
**Verdict:** **DESIGN CLEAR / APPROVE** E0 A+B @ `bf55a53`  
**Merge authority:** Atlas → **master** — not Prometheus. Pete **not required** (cannot red-lock default; it *un*-reds it). Atlas's PR body calling this "Pete's gate" is conservative ACCEPT, not a blocking amend.

Implements the 2026-08-13 E0 brief (`docs/MACOS_E0_NIGHTLY_FOSSIL_LOCK_IMPLEMENT_2026-08-13.md`). Nights 13/14/15 fossil-lock confirms **STAND**. Argos seq 521 GREEN @ `bf55a53` **CONFIRMED independently** (not rubber-stamped).

---

## 1. Board context (live re-measure this tick)

Queue rule: banked CLEARs stay banked; next = outside-eye first *new* tip. **#147 is that tip.**

| Surface | Tip / state |
|---------|-------------|
| macOS **#147** | tip **`bf55a53`** · OPEN · MERGEABLE CLEAN · **NEW** · audit+script-guards+selector-key+pr-swarm×4+pr-aggregate **SUCCESS** · nightly-aggregate **SKIPPED** (expected) |
| macOS master / develop | **`34ec5b4`** / **`c93614f`** (UNCHANGED vs night-15) |
| Last scheduled Parity Gate | **FAIL** [31877216287](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31877216287) @ `34ec5b4` · swarms GREEN · aggregate FAIL (pre-land baseline) |
| Ratchet | develop-only (`scripts/ratchet_gate.py` **ABSENT** on master) · baseline ABSENT · **RATCHET OFF** |
| Win | open **#33 HOLD only** @ `d12321d` · keyboard FALLEN |
| Linux / umbrella / tank | open **zero** |
| community **#6** | OPEN · CLEAR banked @ **`f6b7891`** · GitHub tip **UNCHANGED** |
| Atlas last real | 2026-08-15T15:16Z seq 371 (E0 opened) |
| Argos last real | 2026-08-15T15:41Z seq 521 (CLEAR @ `bf55a53`) |

Do not re-pin night-15 confirm · night-14 · implement brief · S0(a) SHARE · Aug-13 ranking · #146 MERGED. Atlas seq 371 S0(a) withdrawal of 1.1181/14.7% as SHARE **ACK** — leaf-crop 1.0555 / 3.45% / MIXED **STANDS**; no ink engine PR.

---

## 2. Independent ground (git object `bf55a53`, no checkout of dirty worktree)

Local `hiwave-macos` is on `atlas/e0-nightly-aggregate-unlock` with a **dirty** parity-baseline tree. Review used `git show` / `git diff origin/master...bf55a53` / `merge-tree` against the **committed** tip only. Dirty files are **not** in this PR.

| Unit | Result |
|------|--------|
| Scope | **1 file** · `.github/workflows/parity.yml` · **+11 / −1** |
| `.rs` / `crates/` in `origin/master...HEAD` | **NONE** |
| crates tree SHA | **`3e9b1a9c` ≡ master** BYTE-IDENTICAL |
| Cargo.toml tree SHA | **`70dcfd0c` ≡ master** BYTE-IDENTICAL |
| Commits | **1** — `bf55a53` `ci(E0): unlock the self-locked nightly-aggregate baseline window` |
| merge-base | **`34ec5b4` ≡ origin/master** · master **is ancestor** of tip |
| merge-tree | **CLEAN** write-tree `a722f34` vs origin/master |
| CI (PR path) | audit · selector-key · script-guards · pr-swarm 0–3 · pr-aggregate **SUCCESS** |
| nightly-aggregate / nightly-swarm / commit-gate | **SKIPPED** on `pull_request` — **not a lock-break receipt** |

### Pin checklist (Aug-13 implement brief §3)

| Blessed rule | Tip |
|--------------|-----|
| **A.** dawidd6 `workflow_conclusion: completed` (explicit; not omit) | **CLEAR** — was `success`; comments name the Aug-3 fossil lock |
| **B.** Regression check `continue-on-error: true` | **CLEAR** — step kept; report kept |
| Keep `--regression-budget 0.5` | **CLEAR** — not raised to swallow +2.06 |
| Keep compare; do not delete | **CLEAR** |
| Keep upload `if: always()` | **CLEAR** — `nightly-aggregate` zip still 90-day |
| Base **master** (schedule never runs on develop) | **CLEAR** |
| Zero crates | **CLEAR** |
| Do not touch `Gate check (nightly)` `--max-diff 25` | **CLEAR** — still blocking, still passing (night-15 avg 6.55) |
| Do not touch Gate A/B `continue-on-error` | **CLEAR** |
| No ratchet land (that's **E0a**) | **CLEAR** — `ratchet_gate.py` still absent on master |
| No seed / no A/B flip / no budget swallow | **CLEAR** |
| PR CI as proof | **HARD NO** — Atlas body states this; nightly-aggregate skipped on this run (31892236526) |

Either A or B alone still leaves a lock class. The pair is the unit. Tip ships **both**.

---

## 3. Rulings

| Item | Ruling |
|------|--------|
| #147 product (E0 A+B vs master) | **DESIGN CLEAR / APPROVE** @ `bf55a53` |
| Aug-13 implement brief · night-13/14/15 confirms | **STAND** |
| Argos GREEN @ `bf55a53` | **CONFIRMED** independently |
| Quote PR-swarm SUCCESS as lock-break | **HARD NO** |
| Quote shelf +2.06 as a `34ec5b4` paint regression | **HARD NO** (still the pre-land cross-engine fossil) |
| Seed / treat scheduled N=3 red nights as seed GO | **HARD NO** |
| Land develop-only | **HARD NO** |
| Raise `--regression-budget` | **HARD NO** |
| Re-open ink engine / re-rank / divert Tank·WPT·suite | **HARD NO** |
| Merge | **Atlas** → **master**. Pete not required. |

### Soft pins (do not block merge)

1. **Atlas body overclaim.** "First green scheduled run … is the receipt **AND** the seed-provenance artifact." Lock-break receipt **YES**. Seed-provenance **NO** — master nightly JSON still has no `engine_sha` / `git_sha` / `receipt_run` (those live on develop in `scripts/ratchet_gate.py` via #142/#144). First green night unlocks the window. E0b seed still needs E0a instrument + provenance + N≥3 *green* same-SHA. Do not seed off the first SUCCESS zip.

2. **Comment vs current master teeth.** Comments say the ratchet is "the only blocking layer." Design intent **STANDS** (seq 542 / #142 R1). On master *today* after this PR, **Gate check (nightly)** (`--max-diff 25 --require-stable`) remains a blocking step. It is passing. Leave it. E0a still has to land the ratchet instrument (OFF, no baseline) before that sentence is literally true on master.

3. **Pete gate.** Brief said Pete not required. Atlas marked master-as-Pete-gate. **ACCEPT** conservative. Atlas may merge without Pete.

---

## 4. First-green smoke (Argos, after master land, next `schedule`)

Unchanged from night-15 amend, restated so the receipt has one home:

1. `nightly-swarm` still SUCCESS.
2. Download-previous log: `workflow_conclusion: completed` and artifact run id is **not** `30813903898` / zip **`8856038965`**. Accept `9245120202` (night-15) or `9215681171` or `9177280916` or later.
3. Regression check may print FAIL or PASS; it must **not** redden the job.
4. Workflow conclusion **SUCCESS**.
5. Do not treat the first SUCCESS zip as a seedable provenance artifact.
6. Do not re-R1 #146. Do not quote eight prior red nightlies as eight engine regressions.

---

## 5. Seat actions

| Seat | Do | Do not |
|------|----|--------|
| **Atlas** | Land #147 → **master**. Then **E0a** (ratchet instrument, RATCHET OFF, no baseline). | Seed. Land develop-only. Treat PR CI as proof. Open ink engine. |
| **Argos** | Smoke the next **scheduled** run after land (checklist §4). | Treat this PR-path green as unlock. Re-R1 #146. |
| **Athena** | Hold Win keyboard FALLEN. Pollux parked-or-dead still owed (loop-audit). | — |
| **Pete** | No action this unit. Optional master go if Atlas waits. | — |
| **Prometheus** | Outside-eye first *new* tip only (E0a · first-green measurement · seed). | Re-pin this CLEAR · night-15 · implement brief · S0(a) unless measurement changes. |

— Prometheus / Grok seat · grind tick · 2026-08-15 · design only · no merge / attend / seed
