# Outside-eye R1 — hiwave-macos PR #134 (instrument-only / crates≡master)

**Seat:** Prometheus (design) · **Date:** 2026-08-12 · **Tip:** `4f38109`  
**Base:** `master` @ `d6f054a` (#110 promote **MERGED**)  
**Verdict:** **DESIGN CLEAR / APPROVE** merge of instrument packaging to master  
**Merge authority:** Atlas (+ Pete master gate) — not Prometheus

---

## 1. Board context (live re-measure this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#134** | tip **`4f38109`** · OPEN · MERGEABLE · **NEW** · supersedes #130 instrument half |
| macOS **#130** | tip moved · OPEN · **CONFLICTING** · close-as-superseded after #134 lands |
| macOS **#135** | **MERGED** → develop (P1 rounded overflow clip) — prior product CLEAR @ `6b09f9d` packaging fulfilled |
| macOS **#136** | **MERGED** → develop (grid margin-box) — **no pre-land Prometheus R1** (see §6) |
| macOS **#110** | **MERGED** → master @ `d6f054a` — prior CLEAR banked |
| macOS master / develop | **`d6f054a`** / **`bb6ccd9`** |
| Win | open **#33 HOLD only** @ `d12321d` · develop **`36c3b75`** · keyboard FALLEN stands |
| Linux | open **zero** (#59 body prior CLEAR banked · MERGED per Argos) |
| community **#6** | OPEN · CLEAR banked @ `f6b7891` |
| tank / umbrella | open **zero** / #11 HARD AMEND banked |

Atlas executed the SPLIT pin (exchange seq 358): #134 instrument→master, #135 clip→develop, #136 grid→develop. This R1 covers **#134 only** (first new master-path tip).

---

## 2. Independent ground (worktree `/tmp/hiwave-pr134-r1` @ `4f38109`)

| Unit | Result |
|------|--------|
| Residual scope | 23 files · **+8404 / −248** · scripts + tools + trench + docs + `.github/workflows/parity.yml` |
| **crates/ tree SHA vs master** | **`38dedf61` ≡ `38dedf61`** — **BYTE-IDENTICAL** |
| **Cargo.toml tree SHA vs master** | **`70dcfd0c` ≡ `70dcfd0c`** — **BYTE-IDENTICAL** |
| `.rs` files in `origin/master...HEAD` | **NONE** |
| Commits in range that touch `crates/` | **ZERO** (`git log origin/master..HEAD -- crates/` empty) |
| Gate A | `GEOMETRY_TOLERANCE_PX = 0.5` · selector join · registry viewport · present |
| Gate B geometry precondition | `attributable_selectors` · imports Gate A tol · **both** `detect_wrong_solid_color` + `detect_missing_clip` on attributable scope · withheld counted · `no_rustkit_layout` → UNMEASURED |
| Gate C / stability / N/26 | forensic board · `STABILITY_MIN_RUNS` · `conjoin` · unmeasured never green — present |
| Branch law | `trench/BASELINE-parity-finish-line.md` § "Branch law (added 2026-08-12)" — engine never on instrument branch · N/26 attributable to master engine |
| merge-tree | **CLEAN** write-tree vs master (`9fa6a1d…`) |
| CI (live this tick) | **audit PASS** · **script-guards PASS** (47s) · **selector-key PASS** · pr-swarm **pending** (not a design blocker) |
| Local pytest | host anaconda collection hung this seat — **not** scored; CI script-guards is ground |
| Packaging claim vs #130 | #130 still mixed/conflicting; #134 restores the banked property that nights 7–9 voided |

### What is *not* re-litigated

Banked CLEARs that this tip **restores the packaging property of** (not re-argued body):

- Instrument CLEAR @ `e2dba9c` (P0a gates + P0b 1/26 as master-engine receipt)
- Gate B geometry precondition CLEAR @ `5d98c4e`
- Product CLEAR on overflow rounded clip (landed as **#135** on develop)
- SPLIT packaging pin (executed)

---

## 3. Rulings

| Item | Ruling |
|------|--------|
| #134 instrument packaging | **DESIGN CLEAR / APPROVE** @ `4f38109` |
| crates ≡ master as merge invariant | **HARD KEEP** — tree SHAs prove it at tip |
| Branch law in BASELINE | **HARD KEEP** — process fix for the night-7/9 failure mode |
| Quote 1/26 as engine win | **HARD NO** (unchanged) — receipt is master's engine under honest instrument |
| Quote night-7 discrete 51→35 / 62→0 as paint win | **HARD NO / RETRACTED** stands |
| Land #130 tip as-one | **HARD NO** — #130 CONFLICTING; close superseded after #134 |
| Merge #134 | **Atlas + Pete** — Prometheus does not merge |
| pr-swarm still pending | **SOFT** — runtime receipt; does not void instrument design CLEAR |

---

## 4. Seat actions

| Seat | Action |
|------|--------|
| **Pete** | Master go on #134 when ready (instrument only; no engine surprise) |
| **Atlas** | Land #134 → master; close #130 as superseded; do not force-push trench history |
| **Argos** | Re-smoke per #465 checklist once tip stable; optional swarm wait |
| **Athena** | S0b FontLoader Q2 (Atlas tasking stands) |
| **Talos** | S1 abs-pos containing-block (Atlas tasking stands) |
| **Prometheus next** | Outside-eye first *new* tip only. Do not re-pin #134 CLEAR @ `4f38109` · #110 MERGED · #135 MERGED product · banked Gate B · flip HARD NO · #33 HOLD · community #6 · #11 HARD AMEND unless measurement changes. Optional thin post-hoc on #136 grid margin only if develop tip opens a design residual |

---

## 5. Ranked residual (post-SPLIT execution — supersedes 2026-08-11 idle ranking)

| Rank | Slice | Owner |
|------|-------|-------|
| **E0** | Land #134 @ `4f38109` → master · close #130 superseded | Atlas + Pete |
| E0done | #110 promote · #135 clip · #136 grid | **MERGED** this tick |
| **S0** | Post-flip text residual research (wavy/vertical/gamma/coverage/CT↔Skia) | Atlas research when capacity |
| **S0b** | FontLoader Q2 wire | Athena |
| **S1** | Abs-pos containing-block | Talos |
| S2 | WPT residual | Atlas |
| S3 | Win margin SAME_DEFECT + chrome bridge after fresh key receipt | Athena |
| S4 | Tank weight-fit C1 | **DEFER** |
| S5 | Suite positioning | thin only if public drift |

Process pin reaffirmed: **a gate covers the irreversible act, not preparation** (Atlas diagnosis — fleet-wide).

---

## 6. Note on #136 (grid margin) — process only

#136 landed develop without a Prometheus pre-land R1. Not a merge veto (already develop). Product body not re-audited this tick (one solid unit = #134). If Argos or a tip move surfaces a design residual on develop post-#136, that is the next outside-eye unit — not a re-pin of this CLEAR.

---

## 7. Irreversible acts this seat

**None.** No merge, force-push, master write, spend, or `null attend`. Durable doc + exchange note + WORK_QUEUE only.

— prometheus (Grok / design seat, scheduled grind 2026-08-12)
