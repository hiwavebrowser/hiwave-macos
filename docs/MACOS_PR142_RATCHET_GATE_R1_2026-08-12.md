# Outside-eye R1 — hiwave-macos PR #142 (ratchet regression gate)

**Seat:** Prometheus (design) · **Date:** 2026-08-12 · **Tip:** `9cc83b5`  
**Base:** `develop` @ merge-base `44a878b` (live develop tip `12b8885` = #140 F1)  
**Verdict:** **DESIGN CLEAR / APPROVE** ratchet instrument @ `9cc83b5`  
**Merge authority:** Atlas (develop self-merge-on-green) — not Prometheus. Pete gate only if/when this posture first lands on **master** *and* a committed baseline could red-lock the default branch. This tip cannot: there is no baseline file; **RATCHET OFF**.

Implements the 2026-08-12 blessed pin (exchange seq 542): blocking teeth without pretending absolute green.

---

## 1. Board context (live re-measure this tick)

| Surface | Tip / state |
|---------|-------------|
| macOS **#142** | tip **`9cc83b5`** · OPEN · MERGEABLE · **NEW** · audit+script-guards+selector-key+f1 **SUCCESS** · swarm pending |
| macOS **#141** | **MERGED** → develop (WebP test-hooks; F1's second never-compiled test) |
| macOS **#140** | **MERGED** → develop (F1 `cargo test --workspace --no-run`) |
| macOS **#139** | **MERGED** → master (Engine test-literal hotfix) |
| macOS **#138** | **MERGED** → develop (same literal on develop) |
| macOS **#137** | **MERGED** → develop (S0(b) unit 1 flex/grid sibling collapse) — Argos GREEN; **no pre-land Prometheus R1** (process note, same class as #136) |
| macOS **#134** | **MERGED** → master (prior CLEAR @ `4f38109`) |
| macOS **#130** | **CLOSED** superseded (not merged) |
| macOS master / develop | **`34ec5b4`** / **`12b8885`** |
| Win | open **#33 HOLD only** @ `d12321d` · keyboard FALLEN stands |
| Linux | open **zero** |
| community **#6** | OPEN · CLEAR banked @ `f6b7891` |
| tank / umbrella | open **zero** / #11 HARD AMEND banked |

Queue rule: banked CLEARs stay banked; this tick's new tip is **#142 only**. Do not re-pin #134 / #137–#141 / HOLD / FALLEN.

---

## 2. Independent ground (worktree `/tmp/hiwave-pr142-r1` @ `9cc83b5`)

| Unit | Result |
|------|--------|
| Residual scope | 7 files · **+1134 / −0** · `scripts/ratchet_gate.py` + tests + excerpts + `parity.yml` + **ride-along** `livesuite_verify.py` |
| **crates/ vs merge-base** | **`b0ee80fb` ≡ `b0ee80fb`** — **BYTE-IDENTICAL** |
| **Cargo.toml vs merge-base** | **`70dcfd0c` ≡ `70dcfd0c`** — **BYTE-IDENTICAL** |
| `.rs` / `crates/` in `origin/develop...HEAD` | **NONE** |
| Commits | `d74c85b` script · `9b53ebd` wire both lanes (+ livesuite) · `dbcc6a6` real schema · `9cc83b5` pipefail |
| merge-tree | **CLEAN** write-tree vs live develop (`2ac4a6f…`) |
| Baseline file `trench/ratchet-gates-baseline.json` | **ABSENT** — RATCHET OFF by design |
| Local | `python3 scripts/tests/test_ratchet_gate.py` **12/12 PASS** |
| CI (live this tick) | audit · script-guards · selector-key · **f1-test-compile PASS** · swarm pending (soft) |

### Pin checklist (seq 542 §2)

| Blessed rule | Tip |
|--------------|-----|
| Fail only on **regression** vs committed floor | **CLEAR** — `compare()`: geo count ↑ · join count ↑ · new discrete id · paint below variance band · green→red · measured→UNMEASURED |
| Absolute red matching floor is **not** a fail | **CLEAR** — exit **2**, workflow `::warning::`, no `exit 1` |
| Exit split 1 / 2 / 0 | **CLEAR** in script + **both** PR and nightly steps |
| No baseline → **RATCHET OFF**, loud, exit 0 | **CLEAR** — this PR therefore **changes no gate posture** |
| Missing gate report fails loud (exit 1) | **CLEAR** — instrument honesty; runs *before* OFF/compare |
| Manual tighten only; no silent write-back | **CLEAR** — `--write-seed` writes then **returns 0 before compare**; gated path never writes |
| PR + nightly flip **together** | **CLEAR** — identical step, both lanes |
| A/B stay advisory until teeth exist | **CLEAR / HARD KEEP** — A/B still `continue-on-error: true`. Correct. Dropping that without giving A/B an exit-2 would red-lock at 4/26. **Ratchet is the only blocking layer.** Do not "complete" the pin by stripping A/B continue-on-error. |
| N/26 stays non-gating receipt | **CLEAR** — finish-line step unchanged |
| Variance band reused, not invented | **CLEAR** — default `max_variance=0.1` matches `parity_gate` / nightly `--max-variance 0.10` |
| Production schema (assassin 1) | **CLEAR** — counts not lists; paint is `within_fraction`; discrete lives in `failures[]` with `discrete: true`; non-discrete `paint_below_bar` row in synthetics so tests cannot pass by treating every entry as an id; `test_production_schema_excerpt` pins verbatim master run **31624231006** |
| Tee cannot swallow fail (assassin 2) | **CLEAR** — `set +e -o pipefail` + echoed `rc` in **both** lanes (run 31624496939 was the proof) |
| Seed from mixed engine+instrument tip | **NOT DONE here** — correct. Atlas parked 1-iter commit-lane as mechanics smoke. Seed waits for **3-iter nightly on master** (`34ec5b4`+) |
| One file, master-seeded | **STANDS as next-PR pin** — develop engine wins (#135/#136/#137) must read as tighten-eligible against a **master** floor, not be silently dual-seeded |

### Assassin trail (bank, do not re-argue)

Atlas's two pre-land catches are the load-bearing honesty of this tip:

1. Fixtures encoded the author's schema, not production's — 11 tests green against an invented shape. Fixed + pinned.
2. `rc=$?` after `python | tee` read **tee**. The blocking layer could not fail. Fixed.

Both are the same family as F1 / Linux-#59 silent coverage: the thing that claims to bite was structurally unable to. The fixes are in the tip. Argos: the workflow STEP is a second implementation layer the Python suite cannot reach — a deliberate crash probe (rename the script in a scratch branch) still belongs on the smoke list.

---

## 3. Rulings

| Item | Ruling |
|------|--------|
| #142 ratchet instrument | **DESIGN CLEAR / APPROVE** @ `9cc83b5` |
| Blessed pin (seq 542) implementation | **MATCHES** on every load-bearing tooth |
| Zero engine / crates≡merge-base | **HARD KEEP** |
| A/B remain advisory; ratchet is the job-fail condition | **HARD KEEP** |
| Silent nightly write-back / auto-mutate baseline | **HARD NO** (unchanged) |
| Raw A/B blocking flip (absolute green) | **HARD NO** (unchanged) |
| Quote 4/26 or 1/26 as product victory | **HARD NO** (unchanged) |
| Seed from 1-iter or develop-mixed receipt | **HARD NO** — wait for master nightly N≥3 |
| Discrete identity `kind::selector` (stricter than detector-id) | **ACCEPT / HARD KEEP** — a new selector is a new fail |
| Merge | **Atlas** → develop. Not Prometheus. Pete not required for this land (cannot red-lock). |

### Soft residuals (do not block develop land)

| Residual | Why soft |
|----------|----------|
| **`snapshot()` omits pin-minimum provenance** (`receipt_sha` / `engine_sha` / `captured_at` / `stability_runs`) | Seed PR is the first write of the baseline. Add these fields in `snapshot()` **before or with** the seed, or the committed floor will have no provenance to audit. Not a posture bug today (no file is written). |
| **Identity is `case_id` only**, not `(case_id, viewport)` | Both lanes are `--primary-viewport-only`. Fine while that flag holds. If multi-viewport returns, the key must become composite or the ratchet will smash viewports together. Document in the seed PR. |
| **`livesuite_verify.py` ride-along** (commit `9b53ebd`, not in PR body, **not** in `parity.yml`) | Separate S4 unit. Tests are pytest-style with `tmp_path` and **no** `unittest.main()`. `python3 scripts/tests/test_livesuite_verify.py` — the script-guards invocation — **exits 0 without running a single assertion**. A new guard file that no-ops is decoration, the class this campaign exists to kill. **Convert to unittest or drop from this PR.** Do not banner "CI guards livesuite." |
| Workflow `rc ∉ {0,1,2}` fallthrough → step green | Python Traceback is exit 1 (now fails). Segfault 139 would still pass. Add an `else` fail-loud. Cheap. |

### What this is not

- Not permission to flip A/B blocking.
- Not a finish-line declaration.
- Not a seed. Teeth do not exist until the baseline-only follow-up lands.
- Not a re-open of #137 product (already on develop; Argos GREEN stands).
- Not a rewrite of Gate A 0.5px or Gate B attributable-selector law.

---

## 4. Seat actions

| Seat | Action |
|------|--------|
| **Atlas** | Land #142 → develop when you are ready (swarm soft). Convert or split livesuite. Next: **seed PR** from master nightly N≥3 — baseline file only + provenance fields + case_id/viewport note. Then S0(b) unit 2 (horizontal-scroll +6.8) with SHARE. |
| **Argos** | Optional: mutation list still maps 1:1; add crash-probe on the **workflow step**; note livesuite no-op. Do not treat absolute red as CI red once the seed is on. |
| **Athena** | Hold Win keyboard FALLEN. F1 is live on develop — next PRs compile all test targets. |
| **Talos** | Same F1 note. No Linux tip this tick. |
| **Pete** | No action this unit. Master go later if/when ratchet+seed promote and you want teeth on the default branch. |
| **Prometheus** | Outside-eye first *new* tip (seed PR · S0(b) unit 2 · ink SHARE card). Else STOP. |

---

## 5. Process notes (not re-litigated)

- **#137** landed without Prometheus R1. Same class as #136. Argos GREEN + Atlas SHARE receipt stand. Do not backfill a product CLEAR unless the tip moves.
- **#139/#140/#141** are F1 hygiene. Argos already owned them (GREEN / HARD AMEND then land). Banked.
- Subpixel flip HARD NO @ `7863e81` stands. S0 order **(b)→(a)→(c)** stands.
- Configs are receipts: the first baseline commit is a product decision, not a CI side effect.
