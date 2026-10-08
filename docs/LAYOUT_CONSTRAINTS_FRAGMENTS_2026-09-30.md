# Layout constraints and fragments — first intrinsic slice

> **Status:** Proposed design pin. Pete reviews architecture. Atlas reviews lane fit.
> **Source:** Atlas #567, Pete-approved guidance `HiWave-Chrome-Parity-Guidance-2026-09-30` §6.
> **Anchors:** checked on `develop` `b946849` (2026-10-01). Atlas verified the same claims at `af4b95d`, an ancestor of this tip; line numbers below are the current ones.
> **Scope:** one constraint/fragment contract and one first slice. This pin does not change `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md`, does not move the 0.5px geometry gate, and does not authorize an engine edit by itself.
> **Companion:** `docs/SHAPED_RUN_CONTRACT_2026-09-30.md` (guidance §5). The two slices ship independently. This slice calls today's text measurement.

## 0. Decision

A layout query takes a typed constraint and returns an explicit fragment. The first query to move is the intrinsic block and inline size of a nested flex or grid item that contains wrapped text or a form control. That query uses the same measurement final layout uses for that subtree.

Multiple passes stay legal. Phase 9.5 and its neighbours exist because an earlier estimate omitted child behavior and a later pass patched the number. The pin replaces that pattern at one call site. It leaves the patch in place everywhere else, as the differential.

## 1. Constraint

A constraint is an input value, not a bag of `f32` with a comment about what `0` means.

| Field | Values |
|---|---|
| Inline size, block size | `Definite(px)` or `Indefinite`. `0` is a definite zero. It is not a synonym for indefinite. |
| Query | `MinContent`, `MaxContent`, `FitContent { available }`, or `Definite` (lay out under the definite sizes). |
| Available space | The inline and block space the query may use. Each axis is definite or indefinite on its own. |
| Percentage basis | Per axis, `Definite(px)` or `Indefinite`. A percentage against an indefinite basis stays a percentage. It does not fall through to a content estimate and it does not resolve against the viewport. |
| Writing mode | The computed mode (`rustkit-css` `WritingMode` at `crates/rustkit-css/src/lib.rs:2040`: `HorizontalTb`, `VerticalRl`, `VerticalLr`). |
| Containing block | Which ancestor supplies the percentage basis and the available size. The fragment records that identity so a later pass can see whose constraint it answered. |

`rustkit-layout` never reads `writing_mode` (no use of the field under `crates/rustkit-layout`). L0 still carries the field. The only mode L0 executes is `HorizontalTb`. Any other mode returns "not this slice" and the caller keeps today's path. That is how a vertical-mode lane keeps shipping without forking the type.

Today a percentage is definite only by accident of a positive container size. `GridItem::get_height_contribution` (`crates/rustkit-layout/src/grid.rs:235`) resolves `Length::Percent` when `container_height > 0.0` (`grid.rs:245`) and otherwise drops into `estimate_content_height`. An indefinite basis is not a value the caller can see.

## 2. Fragment

The result of one query:

| Field | What it carries |
|---|---|
| Border box, content box | The sizes the query asked for, in the box unit from §5. |
| Baseline | The baseline later alignment reads. Absent only when the subtree has none. |
| Overflow | Content overflow of this fragment. Ink overflow (shadows, outlines) is a later field; L0 records content overflow so a scroll container and a visible overflow are not the same number. |
| Constraint echo | The constraint that produced the fragment, including which sizes were definite. |
| Children | Not required in L0 beyond this one fragment. Inline fragments, floats, and fragmentation are later slices. The type has a slot for them so those slices do not invent a side channel. |

A fragment is the authoritative contribution for the call site that asked. A later "correction" pass that adds a second estimate on top of an L0 fragment is a bug in that pass, and the differential in §4 is how that bug stays visible during the migration.

## 3. What develop does today

Confirmed at `b946849`.

**Block size is an estimate, then a repair.** `GridItem::estimate_content_height` (`grid.rs:277`) multiplies line-height by `count_text_lines` (`grid.rs:381`). That counter adds one per text node, recursively, and does not wrap. The comment on the function says a full implementation would do a layout pass. `get_height_contribution` (`grid.rs:235`) is what row sizing sees.

**Phase 9.5 repairs the estimate after flow.** At `grid.rs:2253`, auto and min-content rows are resized from real flowed heights. The comment records two failures of the estimate: holdout-grid-mosaic tiles measured 110px (the banner) while wrapped text put them at 205px, and `new_tab`'s `.shortcuts` grid charged seven lines for seven text nodes that lay out on one line (rows 143px against a 60px item, Chrome's pitch 72). Phase 9.6 (`grid.rs:2524`) then applies aspect-ratio, and Phase 9.7 (`grid.rs:2579`) applies `height: fit-content`. The comment at `grid.rs:2304` leaves multi-row spans grow-only, and `grid.rs:2362` refuses to shrink under a percentage or other relative block size. Those repairs are spread across callers. One of them can compensate for another.

**Inline estimators are further along, and flex does not trust them for controls.** `estimate_min_content_width` (`grid.rs:2636`) and `own_min_content_width` (`grid.rs:2662`) measure text with the shaper. `form_control_min_content_width` (`grid.rs:2861`) and the max-content arm (`grid.rs:3008`, `own_max_content_width` at `grid.rs:2977`) include a button's label. That width work stands.

Flex still treats a subtree that contains an unsized form control or image as unmeasurable. `estimators_can_measure` (`crates/rustkit-layout/src/flex.rs:1988`) returns false for `BoxType::FormControl` or `Image` without `width: Px`. `fit_content_cross_width` (`flex.rs:2039`) then keeps the previously laid-out width. The comment at `flex.rs:1970` describes the failure mode: a silent zero from an estimator that cannot see the control becomes a box that is silently too narrow, so the old width is preserved instead. The comment's claim that `estimate_max_content_width` has no form-control arm is stale — `own_max_content_width` has one at `grid.rs:3008` — but the flex caller never asks, because `estimators_can_measure` rejects the subtree first.

So the nested case this pin starts from is exactly the one the two mechanisms miss together: a flex or grid item nested inside flex or grid, whose content is wrapped text (block size estimated per text node, repaired later) or a control (inline size refused by the flex caller).

## 4. First slice — L0, nested flex/grid intrinsic

**Name:** L0, intrinsic fragment for a nested flex or grid item containing wrapped text or a form control.

**The query:** one function. Input is the §1 constraint. Output is the §2 fragment. For an item in this class, the function lays the subtree out under that constraint — the same flex and grid layout already used for final layout (`layout_flex_container`, grid layout) — and returns the resulting border box, content box, baseline, and content overflow. It does not call `count_text_lines`.

**Call sites that switch, and only these:**

1. The block-size contribution of such an item inside grid track sizing. Today that is `get_height_contribution` → `estimate_content_height`. L0's fragment block size is the contribution.
2. The min-content, max-content, and fit-content contribution of such an item on a flex axis. Today `fit_content_cross_width` bails out in `estimators_can_measure` when the subtree holds an unsized control. L0's fragment is the contribution, so the control's label is inside the number (`form_control_intrinsic_size` / `form_control_min_content_width` remain the control's own measurement; L0 stops dropping them on the floor).

**What keeps today's path:**

- `estimate_content_height`, `count_text_lines`, Phase 9.5, Phase 9.6, Phase 9.7, `estimate_min_content_width`, and `estimate_max_content_width` stay.
- Items outside the class (multi-row spans, percentage block sizes, aspect-ratio items, replaced images, ordinary blocks, abspos) keep those functions. Phase 9.5 still repairs them.
- For an in-slice item, the old estimate still runs and the receipt records four numbers: the old estimate, the Phase 9.5 correction, the L0 fragment, and the Chrome border box. Track sizing uses the fragment. On the L0 fixture the Phase 9.5 correction for that item goes to zero, because the estimate Phase 9.5 was repairing is no longer the input. If it does not, L0 is not done.
- `HorizontalTb` only. Other writing modes take the old path.
- No screenshot-specific fudge and no edit to the 0.5px geometry gate in the engine PR that implements L0.

Generated fragments (the missing drop cap, list markers) are in this contract's later slices, as guidance §6 says. L0 does not paint a marker to match a screenshot.

## 5. Numerical units

`3ee6cfa` already patched one symptom. `TextShaper::FIT_EPSILON` (`crates/rustkit-layout/src/text.rs:1808`) is `1/64` px, and the comment there says why: a shrink-to-fit width comes back from `(width + padding) - padding` a couple of f32 ulps under the shaper's measurement, the breaker used an exact `<=`, and gradient-no-radius wrapped "to right Pink-Blue" inside a box of its own measured width. Chrome does not see it because LayoutUnit quantises box lengths to 1/64 px. A second epsilon (the `0.01` at `lib.rs:6577` on the ellipsis cut, and the other `0.01` comparisons in layout) is the same kind of local patch.

The policy, which L0 records and only L0's fragment writes obey:

| Quantity | Unit |
|---|---|
| Box and fragment sizes, available space, percentage bases once resolved, the fit test at a fragment boundary | Fixed point, 1/64 CSS px. Rounding happens when the fragment size is written, not inside the text shaper. [Chromium `LayoutUnit`](https://raw.githubusercontent.com/chromium/chromium/main/third_party/blink/renderer/platform/geometry/layout_unit.h) is the precedent for the split, not a tree to import. |
| Glyph advances, cluster offsets, baselines carried for text, transforms | Higher precision. Today's `f32` stays. Rounding every glyph advance to 1/64 px is the wrong imitation of LayoutUnit. |

L0 stores the pre-round text measure beside the snapped fragment size in the differential, so a 1/64 snap is visible and is not confused with a wrap. L0 does not convert the rest of layout to fixed point, and it does not delete `FIT_EPSILON`. That constant remains the line breaker's slack until a later slice writes fragment sizes in the box unit and the breaker compares those. Deleting it in L0 would re-open the ulp wrap on every path L0 does not own.

The geometry gate stays ≤ 0.5px per box, per `docs/PARITY_FINISH_LINE_PLAN_2026-08-04.md`. A tighter gate for pinned-font geometry fixtures is a separate measured proposal. It does not ride the L0 engine PR.

## 6. Acceptance for L0

One reduced fixture, not a new board. A grid of auto rows. Each item is a nested flex row whose content wraps and includes a button. The fixture is the reduced form of the two failures already named in tree: holdout-grid-mosaic (estimate misses wrapped text) and `new_tab` `.shortcuts` (one line counted as many), plus the unsized-control refusal in `estimators_can_measure`.

| Check | Pass |
|---|---|
| Wrapped block size | The item's fragment block size equals the height final layout produces for that subtree. It does not equal `line-height * count_text_lines`. |
| Control contribution | The button's min-content width includes its label, the same figure `form_control_min_content_width` (`grid.rs:2861`) already returns, and the nested flex item's fragment includes it. `estimators_can_measure` returning false is not the answer L0 uses. |
| Chrome geometry | Item border boxes on the fixture match Chrome within the existing 0.5px gate. |
| Differential | The receipt carries old estimate, Phase 9.5 delta, L0 fragment, Chrome rect. In-slice Phase 9.5 delta is 0. Out-of-slice items (a spanning item, a percentage height, an aspect-ratio item on the same fixture's edges) match their `b946849` behavior on the existing grid tests. |
| Units | Fragment sizes are in 1/64 px. The text measure that fed the wrap is still the shaper's `f32`. The receipt shows both. |
| Policy | No threshold edit, no baseline regeneration, no case-specific pixel constant. |

Fractional zoom, percentage cycles, indefinite flex bases, and late image or font changes are the contract's later corpus (guidance §6). They are not L0's exit bar. The constraint type must be able to represent an indefinite basis on day one, which §1 requires, so those later tests have a value to put in the query.

## 7. Other lanes keep shipping

- **Text (S0).** L0 calls `measure_text_with_spacing` and `shape_line_advances` as they exist on `b946849`. It does not wait for the shaped-run contract. When S0 lands, a later slice points this query's text measure at the run. That is a consumer change inside L0's function, not a rewrite of track sizing.
- **Block, abspos, floats, generated content, fragmentation.** Unchanged. They do not take the new query.
- **Grid repairs.** Phase 9.5, 9.6, and 9.7 keep running for every item L0 does not own. A grid lane can keep landing estimate fixes on those paths. An in-slice item ignores a new estimate fix, because its contribution is the fragment; the differential shows that.
- **Writing modes other than `HorizontalTb`.** The type carries them. Behavior stays on today's horizontal layout.
- **Corners, form-control semantics, web fonts, paint.** Unchanged. L0 returns a size for a control. It does not change how the control paints or which face the label uses.

The engine PR that implements L0 touches the two call sites in §4 and adds the types in §1 and §2. A lane that does not call those two sites does not rebase onto L0 to keep landing.
