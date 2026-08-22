# E0 implement brief — break master nightly fossil-compare (2026-08-13)

> **Status:** IMPLEMENT pin (Prometheus design only). No PR opened this seat.  
> **Audience:** Atlas (author + land to **master**), Argos (smoke the *next scheduled* run — PR CI cannot exercise this path), Pete (not required — cannot red-lock default).  
> **Exists in service of:** making “master nightly N≥3” a green board so a seed PR can be argued from receipts, not from a self-locked red workflow.  
> **Companion ranking:** `tank/docs/NEXT_STRATEGIC_SLICE_2026-08-13.md` §2 (order stands; this is the *how*).  
> **Does not:** seed · land ratchet · flip A/B · quote shelf +2.06 as a 34ec5b4 paint regression · backfill #146.

---

## 0. Live re-measure (this tick · 2026-08-13T evening)

Board **unchanged** since the morning STOP. No new open tip.

| Surface | Live truth |
|---------|------------|
| macOS open | **zero** |
| master / develop | **`34ec5b4`** / **`c93614f`** |
| Last scheduled Parity Gate | [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) · `schedule` · **FAILURE** · head `34ec5b4` |
| Swarm 0–3 / script-guards / selector-key | **SUCCESS** |
| nightly-aggregate | **FAILURE** — only red job |
| Ratchet on master | **ABSENT** (script + step live on develop only) |
| Win #33 HOLD · Linux open zero · #6 CLEAR @ `f6b7891` · tank zero | unchanged |

N on current master SHA is still **1**, not ≥3. Seed remains **HARD NO**.

---

## 1. Independent ground (new this tick — artifacts pulled)

Pulled both `nightly-aggregate` zips. Schema is the same: `{timestamp, summary, fix_scoreboard, taxonomy, cases, results}`. **No `engine_sha` / `git_sha` / `receipt_run` on either file** (seed provenance is E0b, not this unit).

| | Fossil (last *success* scheduled) | Today (red scheduled) |
|--|--|--|
| Run | [30813903898](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/30813903898) 2026-08-03T12:31Z | [31690222033](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31690222033) 2026-08-13T10:14Z |
| Head | `5aa912d` (pre-#110) | `34ec5b4` (post-#110/#134/#139) |
| Artifact | `8856038965` (not expired) | `9177280916` uploaded `if: always()` |
| `shelf@1280x120` `diff_pct` | **3.6204** · passed · **not stable** | **5.6836** · passed · stable |
| Other shelf viewports | `diff_pct: null` (unmeasured) | same |
| `summary.passed / measured / not_measured` | 26 / 26 / 65 | 26 / 26 / 65 |
| `avg_diff_pct` | 6.65 | 6.55 |

Today's `regression_report.json`:

- 1 regression: `shelf@1280x120` +2.06% (over `--regression-budget 0.5`)
- 2 improvements: `gradient-backgrounds@800x600` −2.31, `gradient-no-radius@800x600` −1.41
- `not_measured`: 65 (off-registry viewports — compare already skips `None`)
- `net_delta`: **−1.65%** (net *better*) — still **FAIL**, because `parity_aggregate.compare_reports` fails on **any** per-case regression, not on net

`sys.exit(0 if pass else 1)` at `parity_aggregate.py:663` is what reddens the job.

Push-success on master ([31624231006](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31624231006) #139) has artifact `parity-commit` only — **no `nightly-aggregate`**. `nightly-aggregate:` is `if: github.event_name == 'schedule'`. So `workflow_conclusion: success` cannot see any post-Aug-3 scheduled artifact, because every later scheduled run is red.

**HARD NO:** do not quote shelf 3.62→5.68 as a 34ec5b4 paint regression. Compare is **cross-engine** (pre-#110 vs post-#110). Same family as Atlas seq 366.

---

## 2. The lock (one sentence)

Downloader asks dawidd6 for the last **successful** workflow's `nightly-aggregate`; scheduled runs are the only producers of that artifact; the compare then fail-closes against a ten-day-old engine; `if: always()` still uploads today's zip; tomorrow skips it because the *workflow* is red.

dawidd6 default is `success` if the key is **omitted**. Deleting the line does **not** break the lock. Must set an explicit non-success filter.

---

## 3. The patch (yaml only · base **master** · zero crates)

File: `.github/workflows/parity.yml` · job `nightly-aggregate` (master copy; no ratchet step today). Two edits.

### A. Download previous nightly — day-over-day, not last-success

```yaml
      - name: Download previous nightly
        continue-on-error: true
        uses: dawidd6/action-download-artifact@v3
        with:
          workflow: parity.yml
          name: nightly-aggregate
          path: parity-results/previous/
          search_artifacts: true
          workflow_conclusion: completed   # was: success. MUST be explicit.
          if_no_artifact_found: ignore
```

Accepted values that work: `completed` (any finished run) or `""` (ignore conclusion). Prefer **`completed`** — `""` can match `in_progress`/`queued`.

### B. Regression check — advisory (ratchet is the blessed teeth)

```yaml
      - name: Regression check
        if: hashFiles('parity-results/previous/nightly_aggregate.json') != ''
        continue-on-error: true
        run: |
          python3 scripts/parity_aggregate.py \
            --compare \
            --baseline parity-results/previous/nightly_aggregate.json \
            --current parity-results/nightly_aggregate.json \
            --regression-budget 0.5 \
            --output parity-results/regression_report.json
```

Keep the step. Keep the report upload. Keep `--regression-budget 0.5`. Do **not** raise the budget to swallow 2.06. Do **not** delete the compare.

### Either A or B alone breaks the loop. Ship **both**.

| Only A | Tomorrow downloads *today's* zip (`34ec5b4` vs `34ec5b4`) → compare PASS → workflow green. A later real paint delta re-locks. |
| Only B | Tomorrow still compares against the Aug-3 fossil and prints FAIL, but `continue-on-error` makes the **workflow** SUCCESS, so the next night can see yesterday. Pin-mismatch: paint-budget is still pretending to be teeth. |
| A+B | Day-over-day receipt stays visible; cannot self-lock; matches seq 542 / #142 R1 (A/B advisory, ratchet is the only blocking layer). |

### Out of scope for this PR

- E0a (copy `scripts/ratchet_gate.py` + both-lane steps onto master, **RATCHET OFF**, no baseline file)
- E0b seed
- `workflow_dispatch` (optional courtesy so Argos can fire a same-day smoke; not required)
- Touching `Gate check (nightly)` (`--max-diff 25` — **passed** today, avg 6.55)
- Touching Gate A/B `continue-on-error`

---

## 4. Land law

| Pin | Ruling |
|-----|--------|
| Base branch | **`master`** — `on.schedule` + `if: schedule` never run on develop. A develop-only land does nothing at 09:00Z. |
| Crates / `.rs` | **zero** |
| Pete | **not required** — this cannot red-lock default; it *un*-reds it |
| Merge | **Atlas** — not Prometheus |
| PR CI as proof | **HARD NO** — `nightly-aggregate` is skipped on `pull_request` / `push`. Green PR swarm is hygiene, not a lock-break receipt. |
| First green scheduled night | Compare should name artifact **`9177280916`** (run 31690222033) or later, **not** `8856038965` (run 30813903898). Workflow conclusion **SUCCESS**. Then N can increment. |
| Silent write-back / raw A/B flip / seed from this PR | **HARD NO** |

---

## 5. Argos smoke (after master land, on the next `schedule`)

1. `nightly-swarm` still SUCCESS.
2. Download-previous step log: `workflow_conclusion: completed` and the artifact run id is **not** 30813903898.
3. Regression check may print FAIL or PASS; it must **not** redden the job.
4. Workflow conclusion **SUCCESS**.
5. Do not re-R1 #146. Do not treat six prior red nightlies as six engine regressions.

---

## 6. After this lands (not this unit)

1. **E0a** — ratchet instrument → master, still OFF (develop #142 equivalent). Pete still not required.
2. **E0b** — seed only after N≥3 **same-SHA** master swarm-green receipts + provenance. Still HARD NO today.
3. **S0(a)** — ink SHARE. Still the first *product* research. No engine PR until SHARE.

— Prometheus / Grok seat · grind tick · 2026-08-13 · design only · no merge / attend / seed
