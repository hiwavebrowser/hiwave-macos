# Trench baseline — macOS Chrome-parity finish line

**Started:** 2026-08-04 · **Authorized by:** Pete (plan ratified 2026-08-04)
**Plan:** `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md` §4 (queue) · §5 (config)
**Branch:** `atlas/trench-parity-finish-line`

> This file was referenced by the campaign's night order before it existed.
> Created on night 1 to stop the reference dangling. It carries the metric and
> the reason the metric is not yet a number.

---

## The one metric

**`N/26 finish-line-green`** — cases passing the FULL conjunction, not a mean:

1. Geometry within **0.5px per box** vs `baselines/chrome-148/**/layout-rects.json`
2. Paint **≥ 99%** within `aa_tolerance: 5` (the single pinned constant, in
   `docs/VISUAL_DIFF_POLICY.md` — no second number may be introduced)
3. **Stable** across 3 iterations
4. **Zero** discrete structural failures (paint outside box, missing clip,
   wrong solid color) — these auto-fail regardless of percentage

```
BASELINE (2026-08-04):  UNMEASURABLE
P0b      (2026-08-09):  1/26 finish-line-green   (master 44389f1)
master   (2026-08-28):  1/26 finish-line-green   (master f58950c, nightly 33209750736)
develop  (2026-08-30):  2/26 finish-line-green   (develop 2be7d37, run 33294082148)
```

**`master` and `develop` are two different numbers and both are live.** The
campaign's headline for five weeks was master's. `develop` carries 93 commits
master does not, and until 2026-08-30 nobody had run the conjunction on it.
Both figures above are `macos-14` — CoreText and Metal — and the comparison is
instrument-constant: `layout_oracle_gate.py`, `paint_oracle_gate.py`,
`finish_line_receipt.py`, `forensic_board.py`, `parity_gate.py`,
`docs/VISUAL_DIFF_POLICY.md` and `baselines/` are **byte-identical between the
two branches**, so the delta is the engine and nothing else.

| column | master `f58950c` | develop `2be7d37` |
|---|---|---|
| **metric** | **1/26** | **2/26** |
| geometry green | 4/26 | 4/26 |
| paint green | 1/26 | **3/26** |
| stability | 26/26 | 26/26 |
| discrete green | 25/26 | 25/26 |
| measured on all four | 26/26 | 26/26 |
| discrete failure sits on | `image-gallery` (13 ids) | **`gradient-backgrounds`** (3 ids, 1 unique) |

Green on master: `bg-pure`. Green on develop: `bg-pure`, **`bg-solid`**.
develop's third paint-green case is `gradients` (99.2982%), which geometry
still fails, so it does not reach the conjunction.

**The whole delta is paint.** Geometry's green count is 4 on both.

**UNMEASURABLE was the honest reading for five nights, not a placeholder for a
bad number.** Any figure produced before the oracle existed would have been a
mean-pixel-diff wearing a conjunction's clothes — the exact substitution §1 of
the plan documents: master's collapsed-shelf layout scored 3.71% (pass) while
the geometrically correct tree scored 33.87% (fail). The metric preferred the
broken layout.

### The P0b receipt

**`1/26`** — only `bg-pure` passes all four conditions simultaneously.

| condition | green | measured |
|---|---|---|
| geometry (≤0.5px/box) | 4/26 | 26/26 |
| paint (≥99% within ±5) | 1/26 | 26/26 |
| stability (3 measured iterations) | 26/26 | 26/26 |
| discrete (zero structural) | 18/26 | 26/26 |

**The columns are 4, 1, 26, 18 and the metric is 1.** They are not meant to
add up: a case is green only where every column is. Reporting the best column,
or the mean of the columns, is the Goodhart substitution this campaign exists
to end, and `scripts/finish_line_receipt.py` has a mutation-checked guard
against the metric ever becoming `min()` of them.

**Zero cases were unmeasured.** All 26 scored on all four conditions, so `1/26`
is a measurement and not a coverage artefact. Three cases — `bg-solid`,
`pseudo-classes`, `specificity` — are geometry-green, discrete-green and stable
and are blocked by paint alone.

Provenance, stated precisely because the plan asks for a receipt *on master*:
run [31296359482](https://github.com/hiwavebrowser/hiwave-macos/actions/runs/31296359482),
`macos-14` (CoreText and Metal, not SwiftShader), commit `c9b2b5e` on
`atlas/trench-parity-finish-line`. That commit's `crates/`, `Cargo.toml` and
`Cargo.lock` are **byte-identical to master at `44389f1`** — P0a and P0b carry
no engine changes, so the number is attributable to master's engine. It could
not be taken on master literally: the gates that compute it do not exist there
until #130 merges.

**The honest headline.** The old board read mean 6.64% and "~93% raw pixel
agreement" on this same engine. The conjunction reads 1/26. Nothing regressed —
the engine was never touched. The gap between those two readings *is* the
campaign's thesis, and it is now a measurement rather than an argument.

### What blocked measurement

**Every row below is now CLEARED, and the metric has a number.** The table is
kept as the record of what had to exist before `1/26` could be honest.

One thing the gates did not have until night 6 and is worth naming: nothing
computed the conjunction. Gates A, B and C published three independent
verdicts and the AND was left to whoever read them. `scripts/finish_line_receipt.py`
closes that, and it is the last piece of the instrument rather than a fifth
gate — it measures nothing itself and refuses to fill in a blank.

| Blocker | State |
|---|---|
| Nothing computed the four-way conjunction — three gates, three separate numbers, the metric ANDed by eye in prose | **CLEARED** night 6, 26/26 mutation-checked. `scripts/finish_line_receipt.py`, run on both the PR and nightly lanes. Unmeasured is never green; paint and discrete stay separate columns; a receipt that measured nothing exits 1. |
| RustKit `layout.json` had no join key — only `type`/`text`/`control_type`, while Chrome's rects are keyed by selector | **CLEARED night 1 for element boxes; NOT cleared for replaced elements and form controls until 2026-08-19.** Night 1 proved the selectors are *reproducible* (1593/1593) and stamped them on the generic construction path — which sits below the `img`/`input`/`button`/`textarea`/`select` early returns, so none of those elements ever got one. Gate A filed them as `missing_box` **join** failures, so the receipt read "26 measured, 0 unmeasured" while 115 boxes Chrome measures were scored on zero axes (settings 31, form-controls 30, form-elements 17, images-intrinsic 14, flex-positioning 7, tail across five more). Closed by `9fcfbdf`, 10/10 mutation-checked: join 115 → 20, boxes compared 1478 → 1581, all 32 frames byte-identical. **Every `N/26` taken before that date, P0b's `1/26` included, was taken with those boxes unscored.** |
| Gate A (geometry) not implemented — `scripts/layout_oracle_gate.py` is a stub whose `extract_layout_from_rustkit` returns `None` | **CLEARED** night 2. Gate built, joined on the P0a-0 key, 14/14 mutation-checked. It has never seen a real RustKit capture — see below. |
| Gate A has no real RustKit input yet — every capture path needs a GPU adapter, and this trench seat is Linux with none | **Half cleared** night 4. The seat *does* render: SwiftShader ships with the bundled Playwright Chromium and wgpu takes it via `VK_ICD_FILENAMES`. 32/32 registry cases captured, and Gate A ran end-to-end on real engine output for the first time (26 measured, 2 green, 24 red, 2703 geometry failures, 115 join failures). Its **code path** is now observed; its **numbers** are still not macOS numbers — **no text backend at all** (see below), not CoreText; SwiftShader, not Metal. Nothing from this seat can be the receipt. |
| Gate B (paint tolerance + discrete-structural auto-fail) not implemented | **CLEARED** night 3, with one gap: 2 of 3 discrete kinds. `paint_outside_box` is unbuilt because the obvious form was measured to be decoration (0.00% of the viewport lies outside Chrome's rects on all 26 cases) and the attributable form needs Gate A's per-element verdict as a precondition. |
| Gate B's two SHIPPED discrete detectors had that same precondition and did not enforce it — both read RustKit's pixels at **Chrome's** rect, so a displaced box makes every pixel they read belong to something else | **CLEARED** night 8, 9/9 mutation-checked. Measured: **62 of 62** `missing_clip` auto-fails were firing on elements Gate A already fails, displaced 8px–384px; **zero** fired on a geometrically exact element. `attributable_selectors` now joins the layout dump and admits an element only where its border box matches Chrome's rect within Gate A's tolerance (constant and join imported from `layout_oracle_gate`, not restated). A capture with no `layout.json` is UNMEASURED. Discrete 62 → 0 on this seat; the percentage half is bit-identical on all 26 cases. |
| Gate C (non-gating forensic board) not published | **CLEARED** night 5, 17/17 mutation-checked. `scripts/forensic_board.py`: raw heatmap, a tolerance sweep at 0/1x/2x/4x the pinned constant, 32px tiles ranked by above-tolerance pixels and attributed to the most specific Chrome element. Non-gating is enforced as *the numbers never fail a PR*, not *always exits 0* — a board that measured nothing exits 1. Validated end to end on real SwiftShader frames (26/26 measured, 21s); those numbers are mechanics, never a receipt. |
| Gates A, B and C never wired into `parity.yml` at all | **CLEARED** night 5. All three run on the PR and nightly lanes against the shard artifacts' own captures. A and B are **advisory for one cycle** per ratified decision 2; C is non-gating forever. Advisory means visible, not ignored: every receipt, including did-not-run text, goes to the job summary. Flipping A and B to blocking is a follow-up that changes only `continue-on-error`. |
| The join key was never guarded, only assumed | **CLEARED** night 5, 5/6 mutation-checked. `tools/parity_oracle/verify_selector_key.mjs` extracts `getSelector` from `capture_baseline.mjs` and asserts it reproduces all 1757 committed selectors. Blocking in CI from its first run. |
| Stability never enforced at `pr_merge` (`stable:false` does not gate in `parity_gate.py`) | **CLEARED** night 4, 19/19 mutation-checked. The ≥2-run waiver is gone: a row that cannot show 3 **measured** iterations now fails as `stability_unmeasured`, a reason distinct from `unstable`, and unknown counts as zero. Measured ≠ attempted — three captures of which two errored is one measurement. The PR and nightly scout phases run `--iterations 3` in the same commit, because tightening the gate without producing the evidence is a permanent red lock rather than a stricter check. Like Gates A and B, it has never run against a real macOS capture. |

---

## What the trench seat can and cannot read (measured 2026-08-17)

Nights 4 through 13 labelled this seat's divergence "the Linux font stack, not
CoreText". **That understated it by a category. There is no font stack here.**

`rustkit-text` ships DirectWrite (Windows) and CoreText (macOS) and, for
everything else, a `nowin` stub whose every method returns `NotImplemented`.
`TextShaper::shape` under `#[cfg(all(not(windows), not(target_os = "macos")))]`
hands back `font_size * 0.5` per ASCII character. No font file is opened; the 59
fonts installed on the box are never consulted.

Consequence, measured across all 26 gating cases:

```
Gate A failures on boxes carrying text anywhere beneath them:  2187  (88%)
Gate A failures on boxes with no text beneath them:             298  (12%)
card-grid:                                          150 TEXT,    0 CLEAN
sticky-scroll:                                      104 TEXT,    9 CLEAN
```

`TEXT` is a **necessary condition for unreadability, not proof of it** — night
13's `fit-content` sidebars were text-bearing and their defect was a 1400px
stretch. So 2187 bounds what this seat cannot score from above, and 298 bounds
what it can from below.

**Rule for this seat, going forward: a Gate A count is not a receipt and is
barely a signal.** Per-box magnitudes on CLEAN boxes are the readable
instrument; anything on a TEXT box needs the macOS lane to arbitrate.

---

## Decisions RATIFIED by Pete (2026-08-07 evening) — stop asking, start executing

The three questions carried in digests since nights 1–4 are settled. Full text
in `docs/RENDERING_GAP_PLAN_2026-08-07.md` §5 (on develop/master).

1. ~~**Selector drift: PIN `capture_baseline.mjs` back to the committed form**~~
   **The premise was false — measured night 5.** The script has never drifted:
   it reproduces 1757/1757 committed selectors. The claim came from reading
   `split(/\s+/).join('.')` as intent; the source says `/\\s+/`, which matches
   a literal backslash and not whitespace, so the split is a no-op and the raw
   className survives as `div.card featured`. The pin half is a no-op; the
   test half shipped and is what actually holds the form.
2. **Gates A + B + the stability bar enter `parity.yml` ADVISORY-FIRST for
   one cycle** (print receipts, never block), then flip blocking. Wiring
   them in is in scope for the trench.
3. **SwiftShader: approved for developing/validating instrument mechanics —
   Gate C may be built and validated against SwiftShader frames on this
   seat. NOTHING SwiftShader-derived is ever a receipt**; receipts are macOS
   numbers only, and every SwiftShader figure carries the label.

Also in scope per the ratified plan: the **livesuite freezer + harness**
(frozen real-page snapshots, Chrome baseline, same gates — plan §3) once P0a
completes. P0b's first N/26 still runs on macOS, and remains the campaign
metric.

## Order, and why

Per plan §4, worked strictly in sequence, one item per night, no skipping:

**P0a-0** export element identity → **P0a** build the four gates → **P0b** first
real `N/26` receipt → **P1** gradient/clip → **P2** grid/sticky → **P3** flex
residual → **P4** text advance widths → **P5** images + 10-site holdout board →
**P6** forms and paint-order/stacking.

P0a and P0b carry **zero engine behavior changes** so the metric delta is
attributable. A re-instrument PR that also "fixes something small" makes the
first real number unattributable and has to be redone.

---

## Fleet rule banked from this campaign's tasking (Talos, 2026-08-04)

**A blank Class-6 row is not a pass.** Asked to audit instrument integrity,
Talos reported *"I have no parity harness on this seat, so there is no
scoreboard here to catch lying"* — and returned NOT APPLICABLE instead of
green. A seat with no instrument reports the same "nothing wrong" as a seat
whose instrument is honest. In any cross-seat table, a seat without the
instrument reads **NOT APPLICABLE — NO INSTRUMENT**, never blank, never green.

He also withdrew his own advice to port the old parity harness to Linux, in
his words: *"I wanted a number so much that I proposed adopting one already
known to be false."* The old harness is not portable because it is the thing
being replaced.

## Stop rule (hard)

Any change that improves the metric while **any** oracle regresses on **any**
case is auto-reverted and logged in the digest as a mistake, with reasoning.
Improving a number while breaking correctness is the failure this campaign
exists to end.

## Banned from this loop

Mean-pixel-diff-only wins in PR prose (report geometry and paint as a pair, or
report UNMEASURABLE and why) · `dead_code` cleanups · MCP work · cross-platform
ports · corpus expansion · merges to master · force-pushes.

---

## Per-night receipt

Appended to `trench/digest-parity-finish-line.md`:

1. Metric before → after (`N/26`, or UNMEASURABLE + one line on what still blocks it)
2. The P-item worked, and whether it completed
3. Commits landed (SHA + one line each)
4. Mutation-check results for any new guard — a guard that stays green without
   its fix is decoration and does not count
5. At most three decisions needed from Pete, one sentence each
6. Anything that surprised you, especially a measurement that disagreed with an
   assumption


## Branch law (added 2026-08-12, after nights 7–9)

**Engine behavior changes never land on this branch.** This branch is the
instrument lane; its PRs must keep `crates/` byte-identical to master so every
`N/26` receipt stays attributable to master's engine. When a night's P-item
requires an engine change, make it on a fresh branch off **develop** and open
its own PR — then continue instrument work here. Nights 7 and 9 put engine
commits here; the split cost a manual cherry-pick rebuild (atlas/p0-instrument)
and three seats' review time. The night-1 "work on this branch" instruction is
superseded by this rule wherever the two conflict.
