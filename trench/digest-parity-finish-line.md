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
