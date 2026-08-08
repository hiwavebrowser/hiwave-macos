# Trench digest — macOS Chrome-parity finish line

Metric: `N/26 finish-line-green` (geometry ∧ paint ∧ stable ∧ no-discrete).
Baseline and rules: `trench/BASELINE-parity-finish-line.md`.

---

## 2026-08-04

**Metric: UNMEASURABLE → UNMEASURABLE.** No oracle exists yet, so there is no
number to move. What changed is that the thing making the oracle *unbuildable*
is gone: RustKit's `layout.json` now carries a join key. Gates A/B/C and
stability enforcement are still unbuilt — that is P0a, next.

**P-item: P0a-0 (export element identity in layout.json). Completed.**

### Commits

- `742521d` — export `element_id` + `tag` + `selector` per element box; anonymous
  and text boxes carry `identity: None` and emit no identity fields.
- `b126404` — corpus test proving the key joins: 1593/1593 Chrome baseline
  selectors reproduced across all 26 cases. Includes the SVG `className` fix it
  caught.

### What the work actually was

The plan's framing was that `LayoutBox` already carries `element_id` and the
export just needed to emit it. That was wrong in a way worth recording:
`set_element_id` has **zero callers**, so `element_id` was always `None`. There
was no identity to export — it had to be produced first, during layout tree
construction, by mirroring Chrome's `getSelector()`.

Two quirks of the capture script are reproduced deliberately, because they are
the committed join key rather than a style choice:

- raw `className` concatenated after one dot, so a multi-class element keys as
  `div.card featured`, space intact — not valid CSS, but it is what 572 baseline
  selectors say
- `:nth-of-type(N)` only when a tag has more than one element sibling

`Option` on the identity is load-bearing and stayed that way: anonymous and text
boxes have no element, so they are excluded from comparison rather than paired
positionally. An oracle that silently pairs an anonymous box with a real element
reports geometry failures that do not exist.

### Mutation-check results

Six guards, each mutated, observed red, and restored:

| Mutation | Test | Result |
|---|---|---|
| multi-class join `.join(" ")` → `.join(".")` | `multi_class_selector_keeps_the_baseline_space_form` | RED |
| `same_tag_total > 1` → `> 0` (always append nth) | `nth_of_type_is_omitted_for_a_lone_sibling…` | RED |
| id short-circuit inverted | `reported_selector_honors_id_short_circuit…` | RED |
| identity insert removed from export | both `export_emits_identity_*` | RED (2 tests) |
| identity insert made unconditional | `export_omits_identity_for_anonymous_and_text_boxes` | RED |
| foreign-content class suppression removed | `every_chrome_baseline_selector_is_reproduced…` | RED, exactly the 3 shelf svg selectors |

Tests avoid `build_layout_from_document` on purpose. It needs a GPU compositor
and `return`s silently when none is present — this runner has no adapter, so any
test routed through it is vacuous here. `test_layout_tree_from_document` prints
"Skipping test: GPU not available" and still reports `ok`. Everything added runs
on any machine.

### Decisions needed from Pete

1. **`tools/parity_oracle/capture_baseline.mjs` has drifted from the script that
   produced `baselines/chrome-148`** — it now emits `div.card.featured` where
   every committed baseline says `div.card featured`, so regenerating baselines
   today would silently break 572 join keys; pin the script back to the committed
   form (my recommendation, plus a test), or regenerate and re-mirror the engine?
2. **The intrinsic-sizing cache in `rustkit-layout/src/intrinsic_cache.rs` is
   entirely dead** — `lookup_*`/`store_*` have no callers outside their own unit
   tests; keep it as planned future work, or is it stale? (Asking because it is
   the documented reason `element_id` exists, and its deadness is what made
   populating `element_id` provably behavior-neutral.)
3. None beyond those two.

### Surprises

- **`element_id` was never populated.** The plan described it as "currently used
  only for the intrinsic-sizing cache". It is used for nothing: the field is
  always `None` and the cache it keys has no call sites at all. This was good
  news — it means setting it cannot change layout, so P0a-0 holds the
  no-engine-behavior-change rule provably rather than by inspection.
- **A classed inline `<svg>` broke the join, and only the corpus test caught it.**
  Chrome's capture guards on `typeof el.className === 'string'`; for SVG elements
  `className` is an `SVGAnimatedString`, so the guard fails and the class is
  dropped — the baseline keys it as plain `svg`. My first implementation appended
  the class and lost `shelf.html`'s svg and both its children. Three elements
  that would have been scored as "no geometry error" because they were never
  compared at all. This is the failure mode the whole campaign is about, and it
  showed up on night 1 in the instrument itself. The unit tests were all green
  when it happened; only running against the real corpus found it.
- **`scripts/layout_oracle_gate.py` already exists and is a stub.** Its
  `extract_layout_from_rustkit` returns `None` with a comment saying layout
  dumping does not exist yet. P0a should finish this file rather than start a
  new one.
- The 26-case corpus spans three source roots (`websuite/cases`,
  `websuite/micro`, `crates/hiwave-app/src/ui`), which is worth knowing before
  P0a wires up case discovery.

### Process note

Mid-session I ran `cargo fmt` and it reformatted 7 files unrelated to the change
— the repo is not fmt-clean — then later lost ~40 minutes of uncommitted work to
a `git checkout --` used to revert a mutation. Both recovered, nothing shipped
wrong, but the lesson for later nights is: commit before mutation-checking, and
never blanket-format in a re-instrument PR where diff attributability is the
whole point.

---

## 2026-08-05

**Metric: UNMEASURABLE → UNMEASURABLE.** Gate A now exists and is honest, but
it has never been pointed at a real RustKit capture: every capture path needs a
GPU adapter and this seat is Linux without one. Gates B and C and the stability
hole are still unbuilt. The number arrives at P0b, on macOS, as planned.

**P-item: P0a (build the four gates). NOT completed — 1 of 4 landed.**
Gate A (geometry) is done. Gate B (paint tolerance + discrete-structural
auto-fail), Gate C (non-gating forensic board), and stability at
`pr_merge`/`nightly` remain. I did not start them; reasoning below.

### Commits

- `00f02db` — Gate A: `scripts/layout_oracle_gate.py` stops being a stub and
  compares RustKit `border_box` against Chrome `getBoundingClientRect` at 0.5px
  per axis, joined on the P0a-0 selector. 13 tests.
- `3970e83` — delete one decorative guard, make the other load-bearing, after
  the mutation sweep caught both.
- `610fb78` — score only the registry-viewport capture; an off-viewport dump is
  unmeasured, not measured wrongly.

Zero engine behavior changes across all three. Instrument only, so P0b's first
`N/26` stays attributable.

### What Gate A refuses to do

Everything it cannot honestly score fails rather than passing quietly:

| Situation | Verdict |
|---|---|
| box differs > 0.5px on x/y/width/height | `delta`, one receipt line per axis |
| Chrome has the box, RustKit does not | `missing_box` |
| two RustKit boxes claim one selector | `ambiguous_selector` — never first-matched |
| RustKit sized something Chrome collapsed | `phantom_box` |
| capture absent, unreadable, or off-viewport | UNMEASURED, which fails |
| run measured nothing at all | FAIL, not "all 0 cases pass" |

Anonymous and text boxes carry no selector and are excluded, never paired
positionally. Chrome's own omissions (zero-size elements; the
script/style/meta/link/head/title/html skip list in `capture_baseline.mjs`) are
mirrored, or the gate would invent phantoms on every page.

Receipt format is the one fixed in plan §2 and nothing else:

```
card-grid · 3 body > div.header:nth-of-type(1) > p · x · 40 · 46.25 · +6.25
card-grid · — body > div.grid:nth-of-type(2) > div.card:nth-of-type(1) > h3 · missing_box · — · — · —
```

### Mutation-check results

14 mutations, each applied, suite observed, fix restored. **14/14 RED.**

| Mutation | Test that caught it |
|---|---|
| tolerance 0.5px → 50px | `every_axis_is_compared_independently` |
| join on `content_rect` instead of `border_box` | `one_perturbed_box_produces_exactly_one_receipt` |
| ambiguous selector first-matched | `a_duplicate_selector_is_reported_not_first_matched` |
| identity `Option` ignored, unselectored boxes paired | `a_box_chrome_would_have_captured…` |
| `missing_box` downgraded to a skip | `a_box_rustkit_never_emitted_is_a_failure` |
| `phantom_box` detection removed | `a_box_chrome_would_have_captured…` |
| Chrome skip-list not mirrored | `chromes_own_omissions_are_not_phantoms` |
| zero-size boxes not exempt from phantom rule | `chromes_own_omissions_are_not_phantoms` |
| unmeasured case recorded as green | `a_run_with_no_captures_fails…` |
| `measured == 0` tripwire removed | `an_unknown_case_filter_discovers_nothing_and_fails` |
| holdout scope allowed to gate | `a_run_with_no_captures_fails…` |
| only x/y compared | `every_axis_is_compared_independently` |
| receipt loses the selector column | `one_perturbed_box_produces_exactly_one_receipt` |
| any viewport's capture accepted | `only_the_registry_viewport_capture_is_scored` |

The first sweep was 12/14. Both failures were in `gate_passes`, and both were
"0 cases discovered is not a pass" checks — the exact guard class this campaign
exists to defend, shipped uncovered. One was genuinely subsumed and is deleted;
the other defends a different failure and is now asserted against the predicate.

### Decisions needed from Pete

1. Night 1's question is still open and now blocks more: `capture_baseline.mjs`
   emits `div.card.featured` where every committed baseline says
   `div.card featured` — regenerating baselines today silently breaks 572 join
   keys and Gate A with them; pin the script back (my recommendation) or
   regenerate and re-mirror the engine?
2. Should Gate A be wired into `.github/workflows/parity.yml` as advisory
   (prints receipts, does not block) for one cycle before it gates, so its
   behavior on real captures is observed on macOS before it can red-lock PRs?
3. Nothing else.

### Surprises

- **The gate's own tripwires were decoration.** I wrote two "a run that measured
  nothing is not a pass" checks, with a comment citing the B3 precedent, and
  both stayed green when removed. The suite that was supposed to prove the
  instrument honest had the instrument's own disease. Only the mutation sweep
  found it — the tests were 13/13 green.
- **My first mutation harness lied twice.** It reported RED for a mutation that
  was actually green, and later reported 13/13 RED on a file with a
  `SyntaxError` — every mutation "failed the suite" because nothing could
  import. A mutation harness that cannot distinguish "the guard caught it" from
  "the file does not parse" produces exactly the confident wrong number this
  campaign is about. It now compiles the mutated file first, and the sweep is
  scripted rather than eyeballed.
- **Selectors are unique per case on the Chrome side** — 1757 elements across
  all 32 baselines, zero duplicates — so the join is a dict lookup, not a
  matching problem. The RustKit side is not guaranteed unique, which is why
  `ambiguous_selector` exists rather than a first-match.
- **The swarm's multi-viewport output was a live trap.** Naive discovery would
  have scored a 1920x1080 dump against 800x600 baselines and produced a
  page-wide geometry failure that looks precisely like the layout bugs P1–P6
  are hunting. It would have been believed.

### Why P0a stopped at one gate

Gate A could be validated tonight because its input on the Chrome side is
committed: 1593 real boxes to join against, so "the gate works" is a measured
claim rather than an inspected one. Gate B needs real RustKit frames and Gate C
needs real heatmaps, and neither can be produced on this seat. Writing them
blind would have added two gates whose only evidence is that they look right —
which is the kind of instrument this campaign was opened to stop shipping.
So: one gate, measured. P0a continues next night.

---

## 2026-08-06

**Metric: UNMEASURABLE → UNMEASURABLE.** Gate B now exists alongside Gate A,
but neither has ever been pointed at a real RustKit frame: every capture path
needs a GPU adapter and this seat is Linux without one. Gate C and the
stability hole remain. The number arrives at P0b, on macOS, as planned.

**P-item: P0a (build the four gates). NOT completed — 2 of 4 landed.**
Gate A (geometry, night 2) and Gate B (paint) are done. Gate C (non-gating
forensic board) and stability at `pr_merge`/`nightly` remain. Gate B itself
implements 2 of the 3 discrete kinds; the third is specified and deliberately
unbuilt, reasoning below.

### Commits

- `2559d76` — Gate B: `scripts/paint_oracle_gate.py` + `scripts/parity_image.py`.
  Percentage half at ≥99% within the pinned tolerance, plus discrete structural
  auto-fails. 43 tests.
- `cd4f62a` — close the three gaps the mutation sweep found; sweep goes 23/26 →
  26/26 RED.

Zero engine behavior changes in both, same as Gate A, so P0b's first `N/26`
stays attributable.

### What Gate B is

Two halves, and the second is the one that matters. The percentage half is
deliberately generous — Chrome is not bit-stable against itself on text AA or
gradient dither, so most paint deltas really are noise. That generosity is also
how a percentage gets bought: plan §1's collapsed shelf scored 3.71% and passed.
So structural paint bugs auto-fail regardless of percentage.

Both implemented detectors were shown firing on defects injected into real
committed captures **while the percentage half passes**:

| Injected defect | Case | Percentage | Verdict |
|---|---|---|---|
| flat fill recoloured by 6/255 on `#cb1` | form-controls | 99.9916% (pass) | RED — `wrong_solid_color` |
| 36 corner-notch px painted with the card's own fill | card-grid | 99.9965% (pass) | RED — `missing_clip` |

`#cb1` is 81px of a 960000px viewport — 0.008%. That is #83's shipped class
(form controls painting white on `background: transparent`) and it now fails.

The pinned constant is **read** from `docs/VISUAL_DIFF_POLICY.md`, not copied.

### Measured, not inspected

- **False-positive floor**: Chrome scored against itself, all 26 cases —
  26/26 green, 0 discrete failures. A detector that fires here is broken.
- **Detector surface**: 52 flat-interior elements across 14/26 cases;
  232 testable rounded corners across 12/26 cases.
- **Decoder**: byte-identical to an independent decoder on all 32 committed
  baselines.

### Mutation-check results

26 mutations, each applied, mutant compiled, suite observed, source restored.
**26/26 RED** on the clean run (23/26 on the first pass; all three survivors
resolved and re-swept). Full table in `cd4f62a`. Highlights:

| Mutation | Result |
|---|---|
| pass bar 0.99 → 0.5 | RED |
| tolerance hardcoded instead of cited from the policy | RED |
| only the red channel compared / channels averaged | RED |
| either discrete detector removed | RED |
| `wrong_solid_color` fires without attribution | RED |
| notch demands any pixel rather than the whole notch | RED |
| unmeasured case reported green | RED |
| `gate_passes` `measured == 0` tripwire removed | RED |
| any viewport's capture accepted | RED |
| truncated PPM / short IDAT padded instead of refused | RED |

### Decisions needed from Pete

1. `capture_baseline.mjs` still emits `div.card.featured` where every committed
   baseline says `div.card featured` — regenerating baselines today silently
   breaks 572 join keys and both gates with them; pin the script back (my
   recommendation) or regenerate and re-mirror the engine? **Open since night 1
   and now blocking two gates rather than one.**
2. Should Gates A and B land in `.github/workflows/parity.yml` as advisory
   (print receipts, do not block) for one cycle, so their behaviour on real
   macOS captures is observed before they can red-lock PRs? I have not wired
   either in, pending this.
3. `docs/VISUAL_DIFF_POLICY.md` states two tolerances, not the one plan §2
   assumes: 5 for every gating suite and 10 under "Live Sites (non-gating)". I
   read the pinned value from the default block and allow a section to differ
   only if its heading says non-gating. Confirm that is the intended reading, or
   should the 10 be retired too?

### Surprises

- **The obvious `paint_outside_box` detector is decoration, and measuring first
  is the only reason that was caught.** "Differing pixels outside every Chrome
  element box" sounds like exactly the #86 signature. Across all 26 gating
  cases, **0.00%** of the viewport lies outside the union of Chrome's rects —
  `body` and its block descendants tile the page — so it can never fire on any
  case. It would have shipped green forever and been counted as one of the three
  required kinds. It is not implemented; the docstring records the measurement.
  The attributable version needs the element's geometry known-correct first, or
  a sibling that shifted into the gap paints identical evidence and Gate B
  auto-fails a case for what is really Gate A's layout delta. That is the next
  unit on this gate.
- **My first `missing_clip` had almost no surface either, for a different
  reason.** Requiring a flat WHOLE interior to attribute the fill left exactly
  **one** testable element in all 26 cases: cards and buttons have text and
  children in the middle, so their interiors are never flat. Sampling the fill
  at the corner instead — inside the arc, where the fill genuinely is — took it
  from 1 element to 232 testable corners. Both versions passed their unit tests
  identically. Only counting the surface told them apart.
- **A same-length mutation leaked through `__pycache__`.** After the Paeth
  mutation the harness called the restored file red. The source was fine; the
  stale `.pyc` was not. Same byte length meant unchanged size and mtime, so
  CPython reused the mutant's bytecode. This surfaced in its harmless
  direction. The harmful one is identical: an unmutated `.pyc` served *during* a
  mutation run reports a real guard as decoration and gets it deleted. Night 2's
  harness lied twice; this is the third way, and it is not one the compile-check
  fix covers. Harness now clears `__pycache__` and sets
  `PYTHONDONTWRITEBYTECODE`. The full re-run reproduced all 23 original REDs
  unchanged, so nothing earlier was contaminated.
- **One "surviving guard" was an invalid mutation, not a gap.** `pa <= pb` →
  `pa < pb` in the Paeth predictor changes nothing: exhaustive check over all
  16,777,216 byte triples finds zero disagreements, because `pa == pb` with
  `a != b` forces `c` to the midpoint and hence `pc == 0`, so both forms fall
  through to `c`. Worth stating because the sweep's first pass would otherwise
  read as three decorative guards when it was two plus a bad probe.

### Why P0a stopped at two gates

Same reason as night 2, and it is not going to change on this seat: Gate C is a
raw-pixel heatmap board, and there are no RustKit frames here to build one from.
Everything Gate B claims tonight is either measured against the 32 committed
Chrome baselines or injected into them. Gate C's output cannot be validated that
way — a heatmap of Chrome against itself is a blank image, which proves nothing
about the board. Writing it blind would add a third gate whose only evidence is
that it looks right.

Stability at `pr_merge` is the remaining piece that *can* be done here, since it
is a change to `parity_gate.py`'s existing logic rather than a new instrument.
Worth flagging for the next night: the hole is narrower than the baseline file
says. `require_stable` does gate — but only for rows with ≥2 runs, and the PR
scout phase runs each case once, so in practice nothing is ever held to the
stability bar at `pr_merge`. Closing it naively red-locks every PR the moment
data flows, which is why the carve-out is there. It needs the scout phase to run
3 iterations, not just a stricter gate, and that is a change to how the swarm is
invoked.

---

## 2026-08-07

**Metric: UNMEASURABLE → UNMEASURABLE.** Three of the four gates now exist and
none of them has ever been pointed at a RustKit frame. That is still the whole
of what blocks the number, and it is unchanged by tonight's work.

**P-item: P0a (build the four gates). NOT completed — 3 of 4 landed.**
Gate A (night 2), Gate B (night 3), stability enforcement (tonight). Gate C,
the non-gating forensic board, remains.

### Commits

- `13e9013` — stability gates at `pr_merge` and `nightly` on evidence rather
  than exemption; the PR and nightly scout phases run `--iterations 3` in the
  same commit so the evidence exists.
- `60e53a9` — close the one real gap the 19-mutation sweep found.

Zero engine behavior changes, same as Gates A and B. P0b's first `N/26` stays
attributable.

### What the defect actually was

Reproduced before anything was edited:

```
gate_test_results({"results": [single-run row]}, require_stable=True)
-> {"failures": [], "total": 1}
```

`require_stable` has been True at `pr_merge` and `nightly` since the levels
were written. It gated nothing. `parity_gate` held only rows with ≥2 runs to
the bar and waived the rest; the scout phase runs each case once;
`--primary-viewport-only` then discards the multi-iteration exploit rows. Every
row that reached the gate was a single-run row and every one was waived.

The check was not lenient — it was unreachable. And the waiver's own comment
says why it was written: failing single-run rows "would permanently red-lock
every PR the moment data flows". That is true, and it is why the fix cannot be
gate-side alone. Tightening the gate without producing the evidence swaps an
inert check for a permanent red lock, which is the same instrument failure
wearing the opposite sign.

So three changes, and all three are load-bearing:

1. A row that cannot show `STABILITY_MIN_RUNS` **measured** iterations fails as
   `stability_unmeasured` — a distinct reason from `unstable`, because "we
   looked once" and "we looked three times and it moved" are different facts.
   Unknown counts as zero.
2. Measured, never attempted. Three captures of which two errored is one
   measurement; `parity_aggregate` now carries `measured_runs` through, and
   `iterations` is deliberately not consulted anywhere in the path.
3. The PR and nightly scout phases run `--iterations 3`. `commit-gate` does not
   gate on stability and still runs once.

This is the baseline file's blank-instrument-row rule turned on the gate's own
output: a row with no measurement reads NOT MEASURED, never green.

### Mutation-check results

19 mutations, each applied, mutant run, source restored. First pass **18/19
RED**; after closing the survivor, **19/19 RED**. Two probes in the first pass
were bad rather than surviving (`if require_stable:` matches twice in
`parity_gate`; a results[] anchor had drifted) — recorded so the first-pass
count is not read as extra decorative guards.

| Mutation | Result |
|---|---|
| drop the insufficient-evidence failure (the fix itself) | RED |
| reinstate the old `>= 2` waiver | RED |
| collapse `stability_unmeasured` into `unstable` | RED |
| `measured_runs` falls back to the attempt count | RED |
| `measured_runs` treats "unknown" as one run | RED |
| `measured_runs` stops reading parity_test's list shape | RED |
| gate copies the bar instead of citing `parity_lib` | RED |
| gate stops checking the producer's `stable` flag | RED |
| gate stops checking variance against its own budget | RED |
| stability enforced where the level does not require it | RED |
| `cases[]` fallback drops the run evidence | RED |
| `aggregate_iterations` copies the bar instead of citing it | RED |
| aggregate stops propagating `measured_runs` | RED |
| aggregate's `_measured_runs` reads the attempt count | RED *(the survivor; see below)* |
| aggregate `results[]` rows drop `measured_runs` | RED |
| PR lane back to one scout iteration | RED |
| nightly lane back to one scout iteration | RED |
| commit lane tripled too, for no gate that reads it | RED |
| PR timeout left at the pre-tripling budget | RED |

**The survivor was real.** Every row in the first draft's tests carried
`iteration_diffs`, so `_measured_runs`'s unknown path was never exercised and
the mutant read the ATTEMPT count with the suite still green — exactly the
conflation the function exists to prevent. Not hypothetical:
`aggregate_from_attribution_files()` rebuilds rows from `attribution.json` on
disk with no per-iteration diffs at all, so under the mutant every one of those
rows would have arrived at the gate claiming a stability sample nobody took.

**One thing I am NOT claiming as a guard.** `parity_test.py` now records
`measured_runs = len(run_diffs)`. It is correct and it is the field the gate
reads, but it is currently unfalsifiable there: every failure path in that
function returns early, so measured always equals attempted and no mutation of
it can go red. It is defensive, not a guard, and it is not in the 19.

### Also fixed on the same line

`int(r.get("pixel_runs") or ...)` raised `TypeError` against `parity_test.py`'s
own schema, where `pixel_runs` is a **list** of per-run diffs and not an int.
Pointing `parity_gate` straight at a `parity_test` report died on the stability
branch instead of grading it. Latent only because CI reads the aggregate. Three
producers spell the same fact three ways; `measured_runs()` now reads all three.

`STABILITY_MIN_RUNS` is pinned in `parity_lib` and cited by `parity_test`,
`aggregate_iterations` and `parity_gate`. It was a bare literal `3` in three
places — the same second-number risk as `aa_tolerance`, and a producer and gate
that drift on it can publish a row as stable that the gate then rejects as
unmeasured. A test moves the constant and asserts both follow.

### Decisions needed from Pete

1. `tools/parity_oracle/capture_baseline.mjs` still emits `div.card.featured`
   where every committed baseline says `div.card featured` — open since night 1,
   now the join key for three gates rather than one; pin the script back to the
   committed form (my recommendation, plus a test) or regenerate and re-mirror?
2. The tightened stability bar is **blocking on merge, not advisory**: the first
   PR after this lands may go red on real instability that has never been
   measured on any of the 26 cases, which is the gate working, but say if you
   want one advisory cycle first (this also covers night 3's same question about
   Gates A and B, which are still not wired into `parity.yml` at all).
3. ~~Gate C cannot be honestly validated from this Linux seat — accept it
   written blind, or move P0a's completion to a macOS seat?~~ **Withdrawn by my
   own measurement later the same night** (see below): the seat renders under
   SwiftShader, so Gate C can be written against real frames here. The question
   that replaces it is narrower — is a SwiftShader/Linux frame an acceptable
   input for *developing and validating* Gate C's plumbing, given its numbers
   are not macOS numbers and can never be the receipt? My reading is yes for
   the board's mechanics and no for anything it prints, but this is the seam
   where a convenient instrument becomes a false one, so I want it said out
   loud rather than assumed.

### Surprises

- **Nightly was enforcing stability on a subset, by coincidence, and that is
  worse than enforcing it nowhere.** Night 3 recorded the hole as "nothing is
  ever held to the bar". Not quite: the exploit phase re-runs the top-10 worst
  cases at three viewports, so a case whose *registry* viewport happened to be
  one of those three landed in the same `(case_id, viewport)` group as its scout
  row, cleared 2 runs, and got checked. Cases 11..26 never did, nor did any case
  registered at a viewport outside that list. A board that is partially covered
  by "whichever cases were worst last run" reads as covered and is not — and
  which cases get checked changes run to run.
- **`pixel_runs` is an `int` in one schema and a `list` in another, and the gate
  called `int()` on it.** `parity_test.py` writes the per-run diff list under the
  same key `parity_aggregate` uses for an attempt count. Pointing `parity_gate`
  at a `parity_test` report raised `TypeError` on the stability branch. Worth
  noting because it is the *good* failure mode — it crashed instead of lying —
  and it survived only because CI happens to read the aggregate. The same
  collision one field over would have gated silently on a wrong number.
- **A pre-existing red in the script suite, not mine and not touched.**
  `scripts/tests/test_no_unguarded_nullable_diff.py` fails on
  `wpt_tier1.py:158,162` (ordering comparison and `round()` on a nullable
  `diff_pct`). It is red at this branch's HEAD before any of tonight's edits. It
  belongs to the WPT lane, so I left it; flagging it because it is an
  instrument-integrity guard sitting red, which is the class of thing this
  campaign exists to stop tolerating.

### The seat can render after all — three nights of "no frames here" was wrong

Nights 2 and 3 both stopped short of Gate C on the same reasoning: this trench
seat is Linux with no GPU, so there are no RustKit frames to build or validate a
forensic board from. I re-tested the assumption instead of inheriting it.

There is a software Vulkan driver on this box — SwiftShader, shipped with the
bundled Playwright Chromium at
`/opt/pw-browsers/chromium-1194/chrome-linux/vk_swiftshader_icd.json`. wgpu picks
it up with `VK_ICD_FILENAMES` set. With that one environment variable,
`parity-capture` captured **32/32 registry cases, 0 failures**, frames and layout
trees both, in about four minutes of wall clock after a 4m20s release build.

The layout trees carry night 1's join key intact (32 of 33 boxes on `bg-pure`
have a selector; the one without is the anonymous viewport root, which is
correct).

**Gate A then ran end-to-end against real engine output for the first time**, and
produced per-box attributions in exactly the schema plan §2 fixes:

```
sticky-scroll · 0.0.0.1 body > header > div.header-content > nav · x · 835.8438 · 852 · +16.1562
shelf         · —       #closeBtn                                · missing_box · — · — · —
```

Its summary on this seat: 26 gating cases, 26 measured, 0 unmeasured, **2 green**
(`bg-pure`, `specificity`), 24 red, 2703 geometry failures, 115 join failures.

**That is not `N/26` and nobody should read it as one.** Three independent
reasons, any one of which disqualifies it:

1. It is Gate A alone. The metric is a conjunction of four conditions; paint,
   stability and discrete-structural are not in this number.
2. The font stack is Linux, not CoreText. Plan §4's P4 exists precisely because
   metric-exact advances depend on CoreText being on both sides, and it is not
   on this side.
3. The rasterizer is SwiftShader, not Metal.

I also cannot cleanly separate real defects from platform noise here, and it is
worth being explicit that I tried and failed. The axis histogram leans hard
vertical — y 1190, height 784, against x 306 and width 423 — which is what line
boxes driving block flow looks like, and would support "mostly text metrics".
But `specificity` is text-bearing and scored geometry-green, so text presence
alone does not predict failure. The split needs a macOS run to make, not a
cleverer analysis of this one.

What it does change, concretely: Gate C's plumbing can now be written against
real frames instead of blind, and P0b can be **rehearsed** on this seat before it
is run for real on macOS. That is the next unit, and it is a much better position
than the last two nights assumed.

One small trap found while doing it: `parity-capture` writes its `tracing`
warnings to **stdout**, interleaved with the JSON result line, so a consumer
doing `json.load(stdout)` breaks. My first tally script did exactly that and
reported all 32 cases as crashed when all 32 had succeeded — a two-minute scare
that was entirely my harness. `parity_test.py` is immune only by accident: it
checks the return code and whether the frame file exists, and never parses
stdout. (The return codes themselves are honest — 1 on no-adapter, 1 on missing
file, 0 on success. I checked before writing this down, having initially
misread `tail`'s exit status as the binary's.)


---

## 2026-08-08

**Metric: UNMEASURABLE → UNMEASURABLE.** All four gates now exist and are wired
into CI, which is the whole of P0a. What still blocks the number is unchanged
and is now the *only* thing blocking it: none of the gates has run on macOS.
Every figure below came off a Linux/SwiftShader seat and none of it is a
receipt. P0b is next and it has to run somewhere with CoreText and Metal.

**P-item: P0a (build the four gates). COMPLETED.** Gate A night 2, Gate B
night 3, stability night 4, Gate C tonight — plus the CI wiring that all four
were missing, and the join-key guard the campaign had been assuming since
night 1.

### Commits

- `19116d5` — join-key verifier; extracts `getSelector` from
  `capture_baseline.mjs` and asserts it reproduces all 1757 committed
  selectors. Blocking in CI.
- `c9121d4` — Gate C, the forensic board, non-gating by construction. Wires
  Gates A/B/C into the PR and nightly lanes.

Zero engine behavior changes across both, same as nights 2–4. P0b's first
`N/26` stays attributable.

### The selector drift never existed

Ratified decision 1 asked me to pin `capture_baseline.mjs` back to the
committed selector form. I measured before editing, and the premise is false:
the script reproduces **1757/1757** committed selectors across all 32 cases,
zero missing. It has never drifted.

The claim has been in every digest since night 1 and was escalated to Pete as a
blocker for three gates. It came from reading the code's intent —
`split(/\s+/).filter(...).join('.')` obviously yields `div.card.featured` — and
not its bytes. The source says `/\\s+/`, a regex matching a literal backslash
followed by `s`. The split never fires, the raw `className` survives, and the
key comes out `div.card featured` with the space intact, which is exactly what
the 305 space-form baseline selectors require.

So the form is load-bearing **by accident of a typo**, which is worse than
drift and is the real thing to fix. A comment cannot hold it — someone tidies
the regex and 305 join keys break silently, because an unmatched element is not
reported as a failure, it drops out of comparison and scores as "no geometry
error" for never having been compared. I did not rewrite the regex: on a night
whose job is Gate C, changing the string three gates join on, to fix nothing
measured, is the wrong trade. The guard is what holds it.

Worth saying plainly: I spent four nights of digest space on a blocker that a
thirty-second measurement would have retired, and Pete spent a ratification on
it. The instruction to inspect rather than assume is one this campaign keeps
writing down and I still had to be caught by it.

### Gate C, and what "non-gating" had to mean

The design risk in Gate C is not the numbers, it is that "non-gating" quietly
becomes "always exits 0" — at which point a forensic board can stop being
published and nobody notices, because the way you notice is by reading it. So
the split is explicit and tested end to end:

```
ran, published, numbers terrible   -> exit 0
ran, published, numbers perfect    -> exit 0
could not run / measured nothing   -> exit 1
```

That is the baseline file's blank-row rule turned on the board's own exit
status. `test_process_exits_zero_when_the_numbers_are_catastrophic` inverts a
real baseline and asserts exit 0; `test_process_exits_nonzero_when_it_measured_nothing`
asserts the other side.

The tolerance sweep (0, 1x, 2x, 4x the one pinned constant) is the part I think
earns its place. `bg-pure` on this seat reads **2.083% raw diff, 0.000% above
tolerance, max delta 1** — a raw board would show it 2% broken and it is
pixel-perfect within tolerance. That is the mean-diff failure mode reproducing
itself in miniature on the cleanest case in the corpus.

### Mutation-check results

**Gate C: 17 mutations, 17 RED, control green.** Non-gating removed; `board_ran`
always true; Gate C made to fail on high raw diff; `count_above` `>=` instead of
`>`; sweep thresholds hardcoded rather than derived; tiles ranked raw; severity
tiebreak removed; sub-tolerance tiles listed; attribution by largest box; overlap
check dropped; agreement painted pure black; ramp continuous through the
tolerance; unmeasured cases dropped; size mismatch compared; holdout charted by
default; the "cannot fail a PR" warning removed from the board; shape classifier
ignoring the sweep.

**Join key: 6 mutations, 5 RED.** Regex "fixed" to real whitespace (549
failures), nth-of-type always appended (1024), id short-circuit removed (63),
`getSelector` renamed (extraction throws rather than shrugging), no baselines at
all (empty-run tripwire). The sixth — `slice(0, 2)` to `slice(0, 1)` — stayed
**GREEN and is not a guard**: the broken split means the class list always holds
exactly one element, so slicing to 2 or to 1 is the same operation. Recorded so
the count is not read as six.

### Decisions needed from Pete

1. P0b cannot run from this seat — it needs CoreText and Metal — so the first
   real `N/26` needs a macOS runner or a macOS seat; is the nightly `macos-14`
   lane the intended vehicle, or should the trench hand P0b off?
2. Gates A and B are advisory for one cycle as ratified; say if you want the
   flip to blocking to wait for the first *macOS* receipt rather than the first
   CI cycle, since a Linux-only advisory cycle proves the plumbing and nothing
   about the numbers.
3. None beyond those two.

### Surprises

- **A registry case is not a gating case, and the board is 26 of 32.** The six
  holdout cases are discovered, capturable and scored on request, and excluded
  from the gate set by default. Nothing new, but it is the first night all three
  gates agreed on the same 26 and it is worth having said once.
- **Gate C's first real run exposed a ranking defect in Gate C.** Whole regions
  saturate — every tile over a mis-positioned block has all 1024 pixels above
  tolerance — so ranking on count alone returned eight adjacent tiles from one
  `image-gallery` defect, the first at max delta 162 and the rest at 14–16,
  ordered purely by position. A worse defect further down the page would never
  have made worst-N. Severity now breaks the tie first. The unit tests were
  green when this happened; the corpus found it. That is night 1's lesson
  repeating exactly, in the instrument again.
- **The captures were already in the CI artifacts and I nearly wrote a plan
  around them not being.** `parity_test.py` writes to `parity-baseline/captures/`,
  which is not uploaded, and I got as far as sketching a per-shard gate run to
  work around it. `parity_lib.py` — the path the swarms actually use — writes to
  `parity-results/<run>/<case>/<viewport>/iter-N/capture/`, which *is* the
  uploaded artifact. Checking took two minutes and saved a job restructure.
- **`parity-capture`'s flags are `--html-file`, `--dump-frame`, `--dump-layout`.**
  Night 4's digest describes the SwiftShader capture without recording them, and
  my first sweep failed 32/32 on `--html`. Noting them so night 6 does not spend
  the same five minutes.
- Gate C costs about 21 seconds for 26 cases in pure Python, heatmaps included,
  and the whole board is 2MB. I had expected to need numpy and to have to argue
  for the dependency.

### What P0a completing does and does not mean

Four gates exist, are tested, are mutation-checked, and run in CI. Not one of
them has produced a number anyone should act on, because not one has run on the
platform the campaign is about. The instrument is built; it has not yet been
pointed at the thing it measures. `N/26` stays UNMEASURABLE tonight and the
reason is now a one-liner: no macOS run.

### Post-push addendum — the first CI failure was mine and was informative

PR #130's `selector-key` job went red immediately: `npm ci` installs the
playwright package but not its browsers, and this is the **first job in the
repo that has ever launched a browser in CI**. Every other lane uses the oracle
only for pngjs/pixelmatch diffing against committed baselines, so the gap was
invisible until something needed a real DOM. Fixed with an explicit
`npx playwright install --with-deps chromium` (`fee8a8c`); the check itself is
unchanged and still blocking.

Worth recording rather than just fixing: a missing browser is a **did-not-run
wearing a red X**, and it failed in the right direction — loudly, before
checking a single selector, rather than checking zero selectors and reporting
success. That is the empty-run tripwire's whole purpose, and the first thing to
exercise it was the environment rather than the code.

### Second addendum — the PR lane was red-locked, and the cause was not the gate

PR #130's `pr-aggregate` failed on all three pushes, deterministically:
**22 of 26 cases `stability_unmeasured`**. My first reading was "night 4's
stability bar working as designed on its first real PR". That reading was
wrong, and the correct one is worse.

Night 4 shipped two changes that are each correct — the gate fails a row that
cannot show `STABILITY_MIN_RUNS` measured iterations, and the scouts run
`--iterations 3` so the evidence exists. `shard_work_units` sits between them,
was not part of either change, and quietly made their combination meaningless.
Units are generated with iterations innermost and sharding was `i %
shard_count`; with 3 iterations and 4 shards, coprime and the exact numbers the
PR lane uses, each cell's three iterations landed in three different shards.
Every shard aggregated one run per cell and `parity_aggregate` does not
recombine runs across shards.

The four survivors are the proof: they are the exploit phase's top cases, which
pick up 2 extra runs at their own viewport. 1 + 2 = 3 exactly.

So night 4 tripled the scout, widened the PR timeout from 20 to 35 minutes to
pay for it, and bought **zero** additional stability evidence. The digest that
night said tightening the gate without producing the evidence "swaps an inert
check for a permanent red lock, which is the same instrument failure wearing
the opposite sign". It then shipped exactly that, because the evidence it
produced never survived sharding, and nothing tested the sharded path.

Fixed in `a81431a` by sharding on the (case, viewport) cell. 9 mutations, 9
RED after closing two survivors.

**Both survivors were real, and the second is the more embarrassing.** My
lane-check asserted `--iterations 3` appeared in the step, and it passed on the
*comment* above the flag, which reads "--iterations 3, not 1" in prose. A guard
satisfied by a comment describing the thing it guards is decoration — the exact
category this campaign keeps writing down. It took a mutation to find it, which
is what mutations are for, but it is the second time in one night that a test I
wrote was weaker than the sentence I wrote about it.

**And one defect of my own in the same lane.** Gates A/B/C had
`continue-on-error: true` but no `if: always()`. That stops a step from failing
the job; it does not run a step after an earlier one failed. Because `Gate
check` failed, all three gates were SKIPPED on every run of this PR — the
advisory cycle collected nothing, on precisely the PRs where a forensic board
is most useful. The receipt step is the one part that behaved: it printed
"produced no receipt — it did not run. This is not a pass." I built that line
against a hypothetical and it caught a real one within the hour.

**Correction to tonight's decision 1 for Pete.** `pr-aggregate` runs on
`macos-14`, and so does `pr-swarm`. The captures the gates read are therefore
already macOS captures — CoreText and Metal — which means P0b may not need a
separate seat at all. I do not want to overclaim this: nothing has been
measured, because the gates were skipped on all three runs. But "the trench
seat is Linux so P0b needs a macOS seat" is the same kind of inherited
assumption as the selector drift, and it should be tested rather than
ratified. The next run with `if: always()` in place is the test.

### Third addendum — CI green, and the first macOS gate receipts exist

Run 31242407101, all jobs green, `mergeable_state: clean`.

**The sharding fix is confirmed by direct evidence**, not inference. The gate
that failed 22/26 `stability_unmeasured` on three consecutive pushes now reads:

```
Require stable: True   Max variance: 0.1%
✓ PASS: All 26 case(s) within max diff 25.0%
GATE: PASSED
```

Same gate, same bar, same lane. The only change was where a cell's iterations
live. Night 4's stability enforcement is now actually enforcing, four days
after it was written.

**And with `if: always()` in place, all three gates ran on macOS for the first
time.** These are CoreText and Metal, not SwiftShader:

| Gate | Result |
|---|---|
| A geometry | **4/26 green**, 0 unmeasured, 1691 geometry failures, 115 join failures |
| B paint | **1/26 green**, 0 unmeasured, 51 discrete structural auto-fails |
| C forensic | 26/26 measured, mean raw 23.23% (diagnostic only) |

**This is not `N/26` and must not be recorded as one.** P0b is a dual-oracle
baseline receipt taken on master and committed as one; this is a PR-branch run
whose gates were advisory. What it is worth noting is that P0a's zero-engine-
change rule means this branch's engine *is* master's engine, so the formal P0b
number should land close to these. Taking it properly is the next unit.

Three things in these numbers deserve recording now.

**The platform caveat I insisted on all night was correct, and measurably so.**
`bg-pure` read raw 2.083% / maxΔ 1 on the SwiftShader seat and reads **0.000% /
maxΔ 0 / clean** on macOS. The 2% was entirely rasterizer noise. Every figure I
labelled "not a receipt" tonight deserved the label.

**115 join failures, identical on both platforms.** SwiftShader reported 115 and
macOS reports 115. A number that does not move across a rasterizer and a font
stack is not noise — it is a real gap where Chrome's baseline names a selector
the engine's layout tree never produces. `verify_selector_key.mjs` proves
Chrome's side still reproduces its 1757, so the gap is on the engine mirror or
in boxes RustKit does not create at all. This is P0a-0 work that P0a-0 did not
finish, and nobody had the instrument to see it until tonight.

**Gate B's discrete half is earning its place immediately.** 51 structural
auto-fails, and they are `missing_clip` on rounded corners with named selectors
and fills — `image-gallery` 17, `sticky-scroll` 12, `new_tab` 10 — e.g.

```
image-gallery · body > div.gallery:nth-of-type(1) > div.gallery-item tall:nth-of-type(1)
              · missing_clip · radius 12px top-left · fill #667de9 across all 17 notch px
```

That is P1's corner-notch defect, found by the gate rather than by a human
staring at a diff, and it is exactly the class the plan predicted at §4.

**The honest headline.** The old board read mean 6.64% and "~93% raw pixel
agreement". The new instrument, pointed at the same engine on the same
platform, reads 4/26 geometry-green and 1/26 paint-green. Nothing regressed
tonight — the engine was never touched. The difference is entirely that the
conjunction is being asked instead of the mean. That gap *is* the campaign's
thesis, and this is the first time it has been a measurement rather than an
argument.
