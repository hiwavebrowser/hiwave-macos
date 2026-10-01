# Trench baseline — macOS Chrome-parity finish line

**Started:** 2026-08-04 · **Authorized by:** Pete (plan ratified 2026-08-04)
**Plan:** `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md` §4 (queue) · §5 (config)
**Branch:** `atlas/trench-parity-finish-line`

> This file was referenced by the campaign's night order before it existed.
> Created on night 1 to stop the reference dangling. It carries the metric and
> the reason the metric is not yet a number.

---

## WHERE THE CAMPAIGN ACTUALLY IS (added 2026-09-04 — read this before the rest)

The night order below still opens *"the first unit is P0a-0"*. **P0a-0 was done
on night 1 and every P-item through P0b has landed.** Two nights running
(09-03, 09-04) a fresh seat has spent its first stretch discovering that. Facts,
so the third does not:

- **The queue runs off `develop`, not `master`.** Engine work goes on its own
  branch cut from `develop` and opens its own PR; this branch stays the
  instrument lane (see *Branch law* at the foot of this file).
- **The standing metric is `3/26`** (`bg-pure`, `bg-solid`, `gradients`) — run
  36100178666, `macos-14`, 2026-09-25. The `1/26` recorded under *The one
  metric* below is P0b's original receipt and is kept as history, not as
  current state.
- **The queue is geometry-first** — ratified 2026-09-27, see the decision block
  below. Do not open a night on a paint-only case.
- **This digest was not the whole record.** Nights 10–41 were written to
  `trench/forensics/`, `docs/MACOS_PR*_R1_*.md` and PR bodies; this file's
  entries jumped 2026-08-12 → 2026-09-02 on this branch and 08-12 → 09-03 on
  `develop`'s copy. 09-03's entry has been merged back in here so the file reads
  continuously again. If an entry you expect is missing, check the other
  lineage before concluding the night did not happen.
- **The bottleneck is review, not work.** Ten PRs are open against `develop`
  and none has merged since 08-31. One of them, #176, removes 55% of the
  corpus's Gate A `sum|Δ|` on its own (measured 09-04).

Rewriting the ORDER itself is Pete's call and is decision 3 of the 09-04 entry.
This block states where things are; it changes nothing.

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
develop  (2026-09-01):  2/26 finish-line-green   (develop 5b89ed8, run 33474303817)
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

## Decision RATIFIED by Pete (2026-09-27) — geometry first

**Decision 4 (open since 09-24) is settled: the queue works the geometry
failures down. It does not turn to paint.** Pete, 2026-09-27, on being asked
whether to run the close paint cases as macOS-CI experiments or stay on
geometry: *"if its up to me i'd close those failures."*

What that means for a night:

- **Pick the unit from Gate A, not Gate B.** The next unit is whatever the
  latest digest entry records — on 2026-09-27 that was the FormControl hole in
  `own_min_content_width` (a button's min-content floor is its longest word
  plus padding). `settings` is still the largest geometry row (256 on macOS)
  but was worked on 09-27 (#298). Run `trench/tools/n67_confound_census.py`
  before choosing, since most of `settings`' `y` failures are confounded on the
  Linux seat.
- **The eleven paint-only cases wait.** They are geometry-, discrete- and
  stability-green and blocked by paint alone. Ten of them are below the Linux
  seat's paint floor (2026-09-26, `scripts/seat_control_paint_report.py`), so
  this seat could not work them anyway. Do not start one here and do not argue
  the tolerance.
- **Paint still gets measured, not chased.** Every night still reports geometry
  and paint as a pair. A geometry fix that moves a case's paint is a finding;
  a night that picks its unit off the paint column is off-queue.
- **Engine changes still go on a branch cut from `develop`**, per *Branch law*.

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

## Seat law (added 2026-10-01, night 71)

**This seat does not shape text, and until it does, no geometry taken here
attributes to RustKit.**

`TextShaper::shape` (`crates/rustkit-layout/src/text.rs:1635`) has three
bodies. The one compiled on any target that is neither Windows nor macOS
assigns `font_size * 0.5` to every ASCII character, `font_size` to every wider
one, reads no font, and **returns `Ok`**. Nothing detects it: the `Err(_)`
fallback in `shape_text_metrics` is unreachable here, and a `layout.json` from
this seat was byte-indistinguishable from one that really shaped.

The wording this supersedes is the Gate A row above — *"Linux font stack, not
CoreText"*. That is too kind by a category. There is no font stack on this
seat's RustKit side at all. Measured: `shape(" ")`, `shape("mm")` and
`shape("iiii")` return 8.0, 16.0 and 32.0 for every family asked, **including
`Arial`, which is not installed**.

What it costs, measured on the 26 gating cases at `develop b946849`:

| | |
|---|---|
| Gate A geometry failures | 2577 |
| …**text-exposed** (own 2121 + flow 305) | **2426 — 94.14%** |
| …neither relation, and still not clean (intrinsic sizing propagates upward) | 151 |
| RustKit inter-inline-block space | 8.0000px |
| Chrome, this seat / macOS baseline | 5.0938px / 4.1875px |

**The seat control does not fix this and makes it worse.**
`capture_seat_control.mjs` exists to subtract the platform confound by putting
the seat's own fonts on both sides, and it cannot: RustKit's side has no fonts
in it. `Δ_real = RustKit_seat − Chrome_seat` is therefore *more* persuasive and
no more true than the census it refines. Night 71 followed it to an 80-root,
five-page defect class whose deltas were exact integer multiples of one
constant — every property that reads as a real finding — and all of it was the
stub. **A better instrument on a broken foundation is not safer.**

So, as of `dd075ec`, the instrument refuses rather than reporting: Gate A
cannot return PASS on a capture whose advances came from no font, and
`finish_line_receipt.py` scores such a geometry column **UNMEASURED
(`text_metrics_not_font_derived`)** rather than RED — the stub can mask a
defect as easily as invent one, so RED would be a claim about RustKit that this
board cannot support in either direction.

Corollary, and it is the useful half: **three of the `rustkit-layout` test
failures carried as "seat noise" since at least 09-29 are the stub reporting
itself** — `a_long_first_run_keeps_its_last_line_open_for_the_next_sibling`,
`bare_control_widths_match_chrome`,
`justified_wrapped_lines_fill_the_container_except_the_last`. All three are
text-metric tests. The repository's own suite had the finding before any
bespoke tooling did, and three nights of entries called it noise.

**~~Unresolved and awaiting Pete (night 71 decisions 1–3)~~ — ANSWERED the same
night; see *Decisions RATIFIED by Pete (2026-10-01, night 71)* below.** The three
questions were: whether this seat may propose units from Gate A magnitudes at
all; whether a real font shaper may be wired into the Linux body as seat
infrastructure despite the macOS-only scope and the cross-platform-port ban; and
the `*.blob.core.windows.net` allowance, asked six nights running, which under
decision 1 stops being a convenience and becomes the only way this seat picks a
unit. Answers: **yes to 1, hold 2, yes to 3** — and 3 is applied. The paragraph
is kept struck rather than deleted because the rest of this section is night
71's reasoning *before* the answers, and silently rewriting it would make the
record read as though the night already knew.

### Latent, found while reading the corpus, nobody's unit

`baselines/common/parity-reset.css` declares four `@font-face` rules for a
bundled `ParityTest` family (Noto Sans, committed under
`baselines/common/fonts/`) — and **nothing in the corpus ever uses that
family.** The `src` is also root-absolute (`/baselines/common/fonts/...`), so
under a `file://` capture it resolves outside the repo and RustKit logs four
`could not read local font file` warnings per case. Wiring it up would put the
same font file on both sides on every seat and remove the font half of the
confound everywhere — but it would also invalidate all 26 committed baselines,
and corpus changes are banned from this loop. Recorded, not touched.

## Decisions RATIFIED by Pete (2026-10-01, night 71) — the seat's numbers are not units

Tonight's three decisions, put to Pete and answered live. **Yes to 1, yes to 3,
hold 2.**

1. **RATIFIED — this seat may no longer propose a UNIT from its own Gate A
   magnitudes.** Units come from the macOS `gate-a.json`. The Linux gates may
   still be run, and their output is still useful as *mechanics* — a code path
   observed, an A/B showing a change moves nothing, a capture hash — but no
   night may read a delta, a root count or a defect class off this seat and call
   it a defect to fix. Three of five nights' "largest defect" claims from here
   were retracted on measurement (09-29's flex factors, 10-01's 0.5em space, and
   by implication every text-exposed row before them). The pattern is not bad
   luck; the seat does not shape text, so its magnitudes are a ruler.

2. **HELD — do not wire a real font shaper into the Linux `shape()` body.**
   It would make this seat measure again, and it is still out of scope: the
   campaign is macOS-only and cross-platform ports are banned from this loop.
   Pete's reasoning, adopted: with decision 1 in force the seat reads macOS
   numbers anyway, so fixing the stub buys nothing and would leave **two**
   divergent text stacks to reason about instead of removing one. The stub stays,
   declared and refused (`dd075ec`), which is the correct end state rather than a
   deferral.

3. **RATIFIED — allow `*.blob.core.windows.net`** so a night here can read the
   macOS `gate-a.json` and Gate C's board directly. Asked six nights running;
   under decision 1 it stops being a convenience and becomes **the only way this
   seat picks a unit at all**. **APPLIED by Pete and VERIFIED 2026-10-01**: the
   `parity-oracle` artifact of run 36847605935 downloaded, its sha256 matched
   GitHub's recorded digest (`5922d140…`), and it unpacked to `gate-a.json`,
   `gate-b.json`, `finish-line.json`, `finish-line.md` and `forensic/<case>/`
   (one heatmap per case).

**Decision 3 is applied, so the seat can select units again — from the macOS
artifact only.** Recipe (GitHub MCP tools; no token handling needed):

1. `actions_list` `list_workflow_runs` on `parity.yml`, status `completed` — pick
   the newest run whose engine you mean to measure (a `develop` push or nightly
   for a receipt; a PR run measures that PR's merge ref, not `develop`).
2. `actions_list` `list_workflow_run_artifacts` on that run → the
   `parity-oracle` artifact id (kept 14 days; `parity-shard-N` holds the raw
   captures, kept 7).
3. `actions_get` `download_workflow_run_artifact` → a short-lived
   `*.blob.core.windows.net` URL; `curl -L` it and unzip in the scratchpad.
4. Check the zip's sha256 against the artifact's `digest` before trusting it.

If the download is refused again, the policy change has been lost: say so,
and work instrument or recorded units rather than fall back to numbers
decision 1 retired.
Night 71 obtained its macOS receipt by reading the `pr-aggregate` job logs
through the GitHub API, which worked and is not a method to depend on — the
receipt is in the job summary and the artifact, and only the artifact carries
the per-case detail a unit needs.

### Explicitly NOT ratified: the queue order

Night 71 measured that `settings` reads **201 of 243** geometry failures
text-exposed on **real Core Text**, i.e. the largest geometry row on the board
is mostly P4 — which the ratified §4 queue places fourth. That tension was
reported as context for decision 1 and **Pete did not rule on it.** The 08-12
geometry-first amendment stands as written. A next night may not read "units
come from macOS" as "P4 is now first"; if the queue is to move, that is its own
decision with its own ratification.

## Branch law v2 (RATIFIED by Pete 2026-10-01) — SUPERSEDES the 08-12 branch law

**The trench branch and `develop` merge continuously, both ways.** Pete's
direction, verbatim in substance: the two must keep merging to and from each
other or we repeat or undo work; a night **starts** from the latest merged
`develop`, with an eye on PRs that may land before the night ends, and **ends**
by promoting its own PR back to `develop`.

### What the old law got wrong

The 08-12 law said engine changes never land here and PRs must keep `crates/`
byte-identical to **master**. Its goal — an attributable `N/26` — was right. Its
mechanism froze the branch: it never said to *take* `develop`, so the branch sat
at `e2dba9c` (08-09) for seven weeks. By 10-01 it was **839 behind, 138 ahead**,
`crates/` diverged by 74 files / −51,395 lines. The law's own test read as
compliant the whole time, because `crates/` *was* byte-identical to a master —
just one from seven weeks earlier. **A frozen branch satisfies "byte-identical"
perfectly and means nothing.**

Attributability never depended on the freeze anyway. It comes from the PR's own
macOS lane measuring its own head, plus a base-matched A/B. That is how every
receipt since P0b was actually taken.

### The two failures it caused, both observed on night 71

- **Work stranded.** `scripts/geometry_attribution.py` + its test — 1,511 lines,
  three commits, mutation-checked — existed only here and had **never reached
  `develop`**. Recovered by the 10-01 merge.
- **Work repeated.** Not knowing that, night 71 wrote
  `trench/tools/n71_root_defects.py` and `n71_root_classes.py` from scratch to
  split Gate A's failures into roots — **which `geometry_attribution.py` already
  did, better** (it subtracts the nearest common ancestor's delta per axis;
  the n71 tool only asks whether every ancestor is green). It also has a
  font-sensitivity board. Two tools, same job, written seven weeks apart by the
  same campaign, because one was invisible from the other's branch.

### Night procedure

1. **Start:** `git fetch`, then branch from the **latest merged `develop`**.
   List open PRs against `develop` and note any that could land tonight and
   touch the same files.
2. **During:** engine or gate changes go on that develop-cut branch as usual.
3. **End:** open the PR back to `develop` and drive it to green. The trench
   record (`trench/`) rides the same lane — it is no longer a separate
   long-lived branch's private property.

### Guards — what exists, and what does not yet

Pete's standard: *enough guards that neither chain poisons the other; if not,
identify new ones.* Honest status:

| | guard | state |
|---|---|---|
| **existing** | the PR's own macOS lane measures its own head | works |
| | the ratchet's committed per-case floors catch regressions | works |
| | Gate A / B / receipt refusals — unmeasured is never green, and since `dd075ec` unattributable is never green | works |
| **G1** | **start-of-night preflight**: assert the working branch contains `origin/develop`'s tip; refuse to pick a unit otherwise | **NOT BUILT** — night 71 caught the 839-commit drift by chance |
| **G2** | **merge safety**: after merging `develop`, assert no deletions outside the night's declared files | run by hand on 10-01, **not automated** |
| **G3** | **`crates/` parity**: `git diff origin/develop -- crates/` empty unless the night declares an engine change — the live replacement for "byte-identical to master" | **NOT BUILT** |
| **G4** | **single record**: the digest and baseline here must be a superset of `develop`'s; refuse if `develop` holds a line this branch lacks | **NOT BUILT** — the baseline *had* silently diverged (09-04 block missing from `develop`) |
| **G5** | **receipt base provenance**: record the base SHA in `gate-a.json` / `finish-line.json`; flag any comparison across different bases | **NOT BUILT** — the base-drift trap has been rediscovered five nights running, and on 10-01 it caught the agent's own prediction |
| **G7** | **one branch, one writer**: before pushing to the trench branch, `git fetch` and merge the remote; never force-push. Two sessions held this branch at once on 10-01 and the merge-base commit `e2dba9c` is literally *"docs: record why two trench sessions overlapped on one branch"* — it has happened at least twice | **NOT BUILT** — a rejected push is the only thing that currently catches it |
| **G6** | **do not rebuild what exists**: before writing a new trench tool, search `scripts/` and `trench/tools/` for one that already does the job | process, **not enforceable in code** — and it is the one that cost the most on 10-01 |

**Build order if a night is given guard work:** G5 first (it has failed five
times and silently corrupts conclusions), then G1 and G3 (cheap, and they make
the whole arrangement self-checking), then G4. G2 is nearly free once G1 exists.

**No guard here may be read as making an unattributable number attributable.**
These protect the *branches* from each other. The seat law above still governs
what this seat's numbers mean.
