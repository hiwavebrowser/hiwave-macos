# n42 — a padded flex item was max-content plus its padding twice, and a column container placed its rows on a one-line guess it never revisited

Night block 42, 2026-09-04 (macOS seat). Branch `atlas/n42-flex-item-padding-column-basis`
from develop `5b89ed8`. Lane picked by the n41 condition: #174/#173 still open,
option (b) "per-line heights" turned out to live in PR #179's unmerged text path
(`run_line_height` does not exist on develop — doing it from develop would be a
stacked PR by another name), so the fallback was the n37 method: a fresh Gate-A
geometry table on the develop basis and the biggest unclaimed term. That was
flex-positioning (10.11, the board's #3, unclaimed since the n36-reply ledger
named its `.flex-row` dw +40).

## What the table said

`scratch_n42/ytable.py websuite flex-positioning` — every auto-width flex item
was wider than pinned Chrome by a constant per section: `.flex-item` +40.1,
`.justify-item` +30.0, `.nested-item` +24.7. Those are 2×20, 2×15, 2×12 — each
item's horizontal padding, once more. The dump made it exact: RustKit's
`.flex-item` CONTENT width was 83.2, which is Chrome's whole BORDER box (83.1).
And section 4's second `.nested-row` sat at y=534 while the first ended at 537
(Chrome 548): the column container had placed row 2 at row 1's guess (18 line
+ 16 padding = 34, plus the 10 gap) and row 1 then laid out at its real 47.

## Root causes (crates/rustkit-layout/src/flex.rs)

1. **`create_flex_item`, flex-basis auto → `get_intrinsic_main_size() + main_pb`.**
   The helper's contract is a CONTENT figure (every caller adds padding+border),
   but its Block/Inline arm on the horizontal axis returned
   `grid::estimate_max_content_width`, which is a BORDER-box figure — the
   estimator folds `horizontal_padding_border(style)` in itself (the min-content
   path a few lines down already says so and uses it raw). Fix: the arm takes
   that term back out. `horizontal_padding_border` is now `pub(crate)` so the
   subtraction is exactly what was added.

2. **No main-axis re-derivation after child layout.** On the vertical axis the
   same helper returns a line height (there is no height estimator), steps 4–10
   place every item on that guess, step 11 lays the children out for real, and
   11b/11c then correct the CROSS axis from the real content — but nothing
   corrected the MAIN axis. New step 11d: for items whose main size came from
   content (new `FlexItem::main_size_from_content`, set only for auto/content
   bases on non-replaced boxes), take the laid-out height (a nested flex
   container's final content height, or a block's stacked children), clamp to
   min/max, re-run grow/shrink and justification, translate each subtree to
   its new main position, and set the content height. Explicit heights and
   flex-basis lengths are untouched.

3. **The main size 11d justifies against.** A definite pixel height resolves
   from the container's own style (inner, like `definite_inner_cross`); an
   auto height is `max(content sum, min-height)` with `min-height` resolved
   from style (px, or vh against the box's viewport). Two traps here, both
   caught by the board and the repro, not by the unit tests:
   - The engine hands a column container its own PRE-PASS stacked height as
     the containing block. On new_tab (`body { min-height: 100vh; display:
     flex; flex-direction: column; justify-content: center }`) that was 713.4
     — the container's stale guess-sized stack — so develop centred an 82px
     guess in 713.4 and put `.container` at y=315.7 (Chrome 33.5). The first
     cut of 11d used the content sum for auto heights and put it at 0; the
     second used `container_main_size.max(content)` and still got 0 (713 <
     745). The trace (`scratch_n42/trace.py`, `apply_positions` origin lines)
     is what named 713.4. Resolving `min-height` ourselves gives 800 → 27.3.
   - The repro's definite 160px column grew its rows against 94 (again the
     containing block's number) and came out 44 per row; from style, 148 →
     71 each, Chrome's number.

4. **Step 11b wrote a sum of child HEIGHTS into a column item's WIDTH.** The
   cross-axis recompute measured `children_height` on both axes and, for a
   horizontal cross axis, stored it as `content.width`. new_tab's
   `.container` (max-width 600) was 745.4 wide on develop: 681.4 of stacked
   child heights plus its own 64 of padding. The measure is now per axis
   (widest child's margin-box width, or a nested flex container's final
   width).

## Receipts

- T-RED: with fix 1 toggled to the old expression and 11d gated off, both new
  tests fail with the board's own numbers ("content width must be 43.2, got
  83.2"; row 2 placed on the guess).
- Tests: rustkit-layout 308 → 312 (padded auto-width row item counts padding
  once; column stacks rows at laid-out heights; definite-height column grows
  rows from laid-out heights; min-height column centres a content-sized item
  in the viewport and keeps its width).
- Repro `parity-tests/repro/flex-item-padding-column-basis.html` vs pinned
  Chrome 148 (`scratch_n42/repro_table.py`, Chrome via `scratch_n36/
  chrome_capture.py`): sections A (border-box row), B (content-box row), C
  (column of rows), D (column of wrapped paragraphs), E (definite-height
  column with flex-grow) — every container and item matches within 1px
  except E's inner items (see ledger).
- flex-positioning after: `.flex-item` 83.2/85.3/85.7 at x 45/138.1/233.4
  (Chrome 83.1/85.3/85.7 at 45/138.1/233.4); nested rows at y 491/548
  (Chrome 491/548).
- Campaign board (basis: n39 clean develop `5b89ed8`, unchanged since): 26/26,
  **avg 4.0404 → 3.6097**; flex-positioning 10.1126 → 1.7460 (−8.37pp), shelf
  5.1992 → 4.6165, gradient-backgrounds 1.9365 → 1.0923, gradient-no-radius
  1.9906 → 1.0306, gradient-radius-only 1.7823 → 1.1842, chrome_rustkit
  1.4062 → 1.3180, card-grid −0.04, settings −0.0001; **new_tab 2.2600 →
  2.5394 (+0.28pp)**; 17 of 26 byte-flat. WPT Tier-1 24/26 flat, same two
  fails.
- new_tab, honestly: `.container` goes from (y +282, w +145 vs Chrome) to
  (y −6.2, w exact, h +12.4). What the meter sees is that the shortcuts grid —
  still four 180px columns on develop because `repeat(auto-fit, …)` is
  hardcoded to 4 until PR #182 lands — used to sit 282px too low with half of
  it below the fold, and now sits where Chrome puts a two-column grid. The
  geometry is right; the pixels it reveals belong to #182. Before/after
  RustKit layouts: `scratch_n42/new_tab_{before,after}.layout.json`,
  `scratch_n42/rkdiff.py`.

## Real-page reach

- Every auto-width padded flex item — nav links, pills, tabs, buttons in a
  toolbar, tag chips — was too wide by its horizontal padding, and everything
  after it in the row was pushed right by the same amount. This is the
  `display:flex; gap` + padded-children idiom on essentially every site.
- Every column flex container whose rows are taller than a line (sidebars,
  card stacks, mobile layouts, form rows) stacked its rows at one-line pitch
  and let them overlap; `min-height: 100vh` centring columns centred in a
  stale number.

## Ledger (not chased)

- A nested flex row that 11d grows (repro E) does not re-stretch its own
  items to the new height — the nested layout ran in step 11 before the
  growth; its `align-items: stretch` children keep the pre-growth height
  (Chrome 55, ours 31). Needs a second nested pass or a deferred growth.
- Percent `min-height` on a column container resolves to 0 in 11d (only px
  and vh are read); the block layer's rule reads percent against the
  viewport, which is itself wrong.
- The engine passes a column container its pre-pass stacked height as the
  containing block (the 713.4). 11d no longer depends on it for auto heights,
  but steps 4–8 still do (a `justify-content: center` column with no
  min-height and flex-grow children still sees that number).
- Multi-line runs under #179 (the original option (b)) is still owed once
  #179 lands.
