# Outside-eye R1: hiwave-macos PR #112 — WPT Tier-1 honesty gates

**Date:** 2026-08-06 (Prometheus grind tick)  
**PR:** https://github.com/hiwavebrowser/hiwave-macos/pull/112  
**Tip:** `1fb18fc` · branch `atlas/wpt-slice0` · base **master**  
**Master:** `427390c` · develop `bbf4f8e` (#113 MERGED)  
**Verdict:** **DESIGN CLEAR / APPROVE merge** @ `1fb18fc`  
**Merge lane:** Atlas / Pete — **not Prometheus**

---

## Queue context

Banked CLEARs stay banked. Prior tick named **macOS #112** as first *new* outside-eye residual. Community #5 Orphan adjacent CLEAR then **MERGED**. Win #84+#85 **MERGED** → develop. This unit does not re-pin those.

## Live board (measured this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#112** | tip **`1fb18fc`** · OPEN · MERGEABLE CLEAN · **NEW** · audit+swarm×4+pr-aggregate **SUCCESS** (ran ~14:33–14:37Z, before Actions quiet) |
| macOS **#110** | promote OPEN · tip ≡ develop `bbf4f8e` · separate unit |
| macOS **#99** | SUPERSEDE banked @ `69a5ac2` · CONFLICTING · still OPEN |
| macOS master / develop | **`427390c`** / **`bbf4f8e`** |
| Win | open **#33 HOLD only** · develop `67ec265` (#85 MERGED) · master `f0c2f5a` |
| Linux **#59** / **#58** | OPEN · CLEAR banked @ `b662494` / `387a8ee` |
| umbrella **#11** | OPEN · HARD AMEND banked · tip moved `d141f26` |
| community | open **zero** · **#5 MERGED** |
| tank | open **zero** |
| CI-void | Pete/Atlas: CI-green-before-merge **VOID** until Actions returns; substitute gate ratified (local numbers + mutation + honest scope). develop→master R1+preflight **unchanged**. |

---

## Independent ground

Worktrees: `/tmp/hiwave-pr112-r1` @ `1fb18fc` · `/tmp/hiwave-pr112-master` @ `427390c`  
Merge-base ≡ master tip. merge-tree conflict markers: **0**.  
**Engine paths in diff: none.** Scope is instrument-only (5 files, +252/−26).

| Path | Role |
|------|------|
| `scripts/wpt_tier1.py` | unrunnable gate · Ahem staging · webfont attribution · suspect_passes |
| `scripts/wpt_sync.sh` | materialise `MANIFEST.support_paths` with tests/refs |
| `scripts/wpt_fetch_support.py` | **NEW** — same paths without `git clone` allowlist |
| `trench/wpt/MANIFEST.json` | `support_paths`: `fonts/ahem.css`, `fonts/Ahem.ttf` · seed_n **still 14** |
| `trench/wpt/last-run.json` | post-gate receipt |

### Parent defects (master runner @ #74/#77)

Confirmed from master `last-run.json` (pass 6 / fail 6 / skip 0 / error 2 / rate 0.5):

| Case (master) | Status | Tip reclass |
|---------------|--------|-------------|
| empty-span-height | FAIL 2.94% | **SKIP** reftest-wait |
| empty-span-size-001 | FAIL 16.53% | **SKIP** reftest-wait |
| empty-text-node-001 | FAIL 1.41% | **SKIP** needs JS |
| empty-span-scroll | ERROR blank | **SKIP** reftest-wait |
| overflow-wrap-001/002 | FAIL 0.7054 / 2.495 | **FAIL unchanged** + `blocked_by` |
| empty-span-size-002 | FAIL 0.6658% | **FAIL unchanged**, unattributed |
| align-items-baseline… | ERROR blank | ERROR + blocked_by webfont |

### WPT pin sources (pin `a6f29b0b…`, independent fetch)

| Case | reftest-wait | script | /fonts or ahem |
|------|--------------|--------|----------------|
| empty-span-height | YES | YES | no |
| empty-span-scroll | YES | YES | no |
| empty-span-size-001 | YES | YES | no |
| empty-span-size-002 | **NO** | **NO** | no ← honest residual head |
| empty-text-node-001 | no | YES | no |
| overflow-wrap-001 | no | no | **YES** |
| overflow-wrap-004 | no | no | **YES** |
| br-font-size | no | no | **YES** |

Gate ↔ source: **CLEAR**.

### Harness invariants (must keep; #99 lacked some)

| Check | Result |
|-------|--------|
| Blank frame → ERROR | **PRESERVED** |
| Negative control (must FAIL) | **PRESERVED** · run aborts otherwise |
| `WPT_MAX_DIFF_PCT` | **0.0** unchanged |
| rel=match authority | **PRESERVED** |
| Seed expand | **NO** (still 14; #99's 30 is separate SUPERSEDE) |

### Rate arithmetic (packaging honesty)

```
master: scored = PASS+FAIL = 6+6 = 12 · rate 0.50
tip:    scored = PASS+FAIL = 6+3 =  9 · rate 0.6667
```

SKIP and ERROR leave the scored denominator. **Pass count unchanged (6).** Rate rise is denominator cleanup, not engine improvement. PR body states this; last-run `honesty` still requires red on the seed.

### @font-face capability (engine tree on tip, no product change)

| Check | Result |
|-------|--------|
| `FontLoader::load_font` | hollow — `data: Vec::new()` + comment-only body |
| `queue_font_face` callers | **only** `tests::test_font_loader` |
| `FontFaceRule` in rustkit-css | **zero** hits — unproduced |
| Taxonomy | **orphan / hollow** (same class as community Orphan Law #5, now MERGED) |
| Attribution policy | FAIL stays FAIL + `blocked_by`; not excused from score |

Ahem staging receipt: tip last-run keeps overflow-wrap-001/002 at **0.7054 / 2.495** identical to master — staging fonts on disk did not change pixels. That is evidence the gap is load/register, not path rewrite. (Prometheus did not re-run parity-capture; numbers are tip-committed receipt vs master last-run.)

### suspect_passes

Three PASSes tagged webfont-dependent: `overflow-wrap-004`, `br-font-size`, `br-line-height`. Dangerous direction is green that never loaded the declared font — **CLEAR** that they are reported, not celebrated.

### #99 relationship

| Item | Ruling |
|------|--------|
| #99 as merge of parallel 30-case runner | **SUPERSEDE stands** (blank gate absent, negctl absent, CONFLICTING) |
| This PR | ports **unrunnable / attribution** ideas onto **master harness** which keeps blank+negctl |
| Seed reconcile 14→30 | **separate decision** — not this PR |
| Close or HARD AMEND #99 | Atlas — after or with this land |

### CI / merge process under void

Historical Actions SUCCESS on tip is real (pre-outage). While CI-void stands: Atlas still lands with **substitute receipt** if re-run is needed; do not wait on empty statusCheckRollup. This PR already has numbers in body + committed last-run.

---

## Rulings

| Item | Ruling |
|------|--------|
| #112 instrument | **DESIGN CLEAR / APPROVE merge** @ `1fb18fc` |
| Unrunnable → SKIP (not FAIL) | **CLEAR** (upstream TIMEOUT semantics; harness honesty) |
| Webfont FAIL stays scored + attributed | **HARD KEEP** |
| suspect_passes field | **CLEAR** |
| Ahem support_paths + staging | **CLEAR** instrument completeness |
| Threshold / seed / engine expand | **NO** this PR |
| Quote 6/9 as engine progress | **HARD NO** — packaging only after attribution |
| #99 SUPERSEDE | **unchanged** |
| empty-span-size-002 | **honest product residual** — next engine/queue head after instrument honesty |
| @font-face implement | **separate product unit** (Orphan Law lane) — not expand this PR |
| Coarse any-`<script` SKIP | **SOFT ACCEPT** on 14-seed; revisit if seed adds incidental scripts |
| Merge | **Atlas** — not Prometheus |
| Master promote of unrelated develop stack | **#110 separate** — not this unit |

---

## Seat tasking

### Atlas
1. Land #112 → master when process allows (substitute receipt if Actions still void).
2. Do **not** quote 6/9 as product win in banners without "denominator only" gloss.
3. HARD AMEND or close #99 (salvage seed only onto master harness post-#112).
4. Next product residual from this seed: `empty-span-size-002` (0.6658%, script-free, font-free) and/or real `@font-face` implement unit.

### Argos
Optional tip re-R1: greps for blank gate + negative_control + `WPT_MAX_DIFF_PCT == 0` + no crates/ in diff.

### Athena / Talos
No Windows/Linux product change required from this PR. When WPT runners land elsewhere: same three honesty classes (unrunnable, blank≠match, webfont attribute-not-excuse).

### Pete
No new product call. CI-void re-arm remains yours. #110 promote still needs external R1 + preflight when you want master.

### Prometheus next
Outside-eye first *new* tip after #112 lands or tip moves (#110 promote residual if asked · Win only if tip moves past HOLD · #59 only if tip moves past CLEAR). Else **STOP**. Do not re-pin #112 CLEAR · #99 SUPERSEDE · #59/#58 CLEAR · #33 HOLD · #11 HARD AMEND · community zero unless measurement changes.

---

## No irreversible from this seat

No merge · no force-push · no spend · no master write · no null attend · no Actions reconfigure.
