# Outside-eye R1 — hiwave-macos PR #88 (trench parity finish-line baseline)

**Seat:** Prometheus (design only)  
**Date:** 2026-08-05  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/88  
**Tip measured:** `3272e35`  
**Master measured:** `9103f7b` (#89 P0a-0 MERGED · #86 gradient clip MERGED)

## Verdict

| Item | Ruling |
|------|--------|
| #88 as-is | **REJECT / SUPERSEDE** — do not merge |
| Master `trench/BASELINE-parity-finish-line.md` (post-#89) | **AUTHORITATIVE** |
| Class-6 fleet rule (blank instrument ≠ pass) | **DESIGN CLEAR content** — salvage onto master |
| Forensics probe `2026-08-04-geometry-join-probe.py` | **SOFT HOLD land** — absolute machine paths |
| Next product residual | **P0a** (gates A/B/C + stability) — not this PR |
| Merge authority | Atlas / Pete — **not Prometheus** |

## Why this unit now

Queue rule after #2 / #89 / #86 CLEARs: outside-eye first *new* tip; #88 only if residual after product land. Live re-measure: #89 and #86 **MERGED** to master; #88 still OPEN on pre-P0a-0 base `d5df733` and still claims the join-key hole that #89 closed. That is a **measurement change**, not a re-pin.

## Independent ground

### Scope (tip `3272e35`)

| Path | On #88 | On master `9103f7b` |
|------|--------|---------------------|
| `trench/BASELINE-parity-finish-line.md` | pre-P0a-0 UNMEASURABLE story + Class-6 bank | post-P0a-0 baseline (P0a-0 **CLEARED**; gates A/B/C still open) |
| `trench/forensics/2026-08-04-geometry-join-probe.py` | present | **absent** |
| `trench/digest-parity-finish-line.md` | not in PR | present (night-1 P0a-0 receipt) |

Diff vs master is only those two paths (+154/−0 vs its own base). Against **current** master the baseline file is **both-modified**.

### Merge-tree

`git merge-tree` vs `origin/master`: **conflict markers on `BASELINE-parity-finish-line.md`** (count ≥2 hunks). Clean auto-merge is impossible without choosing a side.

### Join-key claim on #88 — STALE

#88 body and baseline still state:

> There is no element identity in the RustKit dump to join on.

Master after #89:

- `export_layout_json` emits `element_id` / `tag` / `selector` for element boxes
- corpus: **1593/1593** Chrome baseline selectors across 26 cases (banked in prior Prometheus R1 + master digest)
- master baseline table: join-key blocker marked **CLEARED** night 1

Landing #88's baseline prose would **rewind** that truth.

### Remaining blockers (master baseline — still open; not re-designed here)

| Blocker | State |
|---------|-------|
| Gate A geometry (`scripts/layout_oracle_gate.py`) | **stub** — `extract_layout_from_rustkit` **returns `None`** (confirmed on `origin/master`) |
| Gate B paint tolerance + discrete auto-fail | open — P0a |
| Gate C forensic board | open — P0a |
| Stability at `pr_merge` | open — P0a |

So UNMEASURABLE **still** holds for the conjunction metric — but the *reason* is "gates unbuilt", not "no join key". Master already says this; #88 does not.

### Class-6 salvage (unique value on #88 tip)

Tip-only bank (Talos, 2026-08-04):

> **A blank Class-6 row is not a pass.** … Linux reads **NOT APPLICABLE — NO INSTRUMENT**, never blank and never green.

Master baseline has **no** Class-6 / blank-instrument language. Content is sound and campaign-aligned. It must not die with a supersede close — Atlas should paste it into master's baseline (or a one-line digest note) in a thin docs commit.

### Forensics probe

Read-only positional geometry probe. Useful as a *historical* receipt of the 0.6% non-measurement.

**Block land as-is:**

- Hardcodes `ROOT="/Users/petecopeland/Repos/hiwave-macos"`
- Hardcodes a Claude session scratch `BOARD=/tmp/claude-501/.../board/parity-commit`

Not portable; not a gate. If kept: scrub to repo-relative + env override, or leave closed as local scratch.

### CI

audit + pr-swarm×4 + pr-aggregate **SUCCESS** at tip (docs-only; expected green). Green CI does not clear a supersede conflict.

## Rulings (frozen)

1. **Do not merge #88 onto master as-is.** It conflicts and would stale the post-P0a-0 baseline.
2. **Master baseline is SoT** for the finish-line metric file after #89.
3. **Salvage Class-6** onto master (thin docs amend or tiny follow-up PR). Content CLEAR.
4. **Probe:** optional scrub-and-land under `trench/forensics/`; not blocking; not a gate.
5. **Close or HARD AMEND #88** — Atlas process choice:
   - preferred: close with comment pointing at this R1 + open thin Class-6 salvage if not amended on master
   - acceptable: force-rebuild #88 tip onto master that *only* adds Class-6 (+ optional scrubbed probe) and drops the superseded baseline body
6. **Next design residual for Atlas trench:** P0a gate implementation (A geometry join using shipped selectors · B paint · C forensic · stability enforce). Zero engine behavior change per plan. Prometheus does not open that PR.
7. **No Prometheus merge / force-push / master write.**

## What this seat did / did not

**DID:** live re-measure board; independent tip vs master; merge-tree conflict confirm; layout_oracle stub confirm; durable R1; WORK_QUEUE + exchange note.

**DID NOT:** merge #88, force-push, rewrite master baseline, open P0a, re-pin #89/#86/#2/#3 CLEARs, null attend.

## Cross-links

- Prior P0a-0 CLEAR: `docs/MACOS_PR89_P0A0_ELEMENT_IDENTITY_R1_2026-08-04.md`
- Plan: `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md`
- Master baseline: `trench/BASELINE-parity-finish-line.md` @ `9103f7b`
- Master night-1 digest: `trench/digest-parity-finish-line.md`
