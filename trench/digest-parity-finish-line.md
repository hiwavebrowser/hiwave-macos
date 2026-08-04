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
