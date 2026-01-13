# CSS Grid Level 1 Implementation Plan

A systematic implementation of the CSS Grid Layout Module Level 1 specification.

**Reference:** https://www.w3.org/TR/css-grid-1/

---

## Progress Tracking

### Changelog

| Date | Phase | Change | Commit |
|------|-------|--------|--------|
| 2026-01-12 | 4.1-4.4 | Implemented dense packing, span-to-name, order property, edge cases | uncommitted |
| 2026-01-12 | 3.2-3.3 | Implemented grid-template-areas and implicit line names | 460e187 |
| 2026-01-12 | 3.1 | Implemented named line resolution | 40f083b |
| 2026-01-12 | 2.2 | Implemented auto-fit with empty track collapsing and gap handling | 8fbed45 |
| 2026-01-12 | 2.1 | Implemented auto-fill expansion with container size calculation | 8fbed45 |
| 2026-01-12 | 1.4 | Implemented fit-content() with limit tracking | f309c95 |
| 2026-01-12 | 1.3 | Added min-content/max-content intrinsic sizing flags | f309c95 |
| 2026-01-12 | 1.2 | Added percentage track resolution with minmax support | f309c95 |
| 2026-01-12 | 1.1 | Added `expand_tracks()` to GridTemplate, updated GridLayout to use it | f309c95 |
| 2026-01-12 | - | Created implementation plan | - |
| 2026-01-12 | - | Fixed grid auto-placement for partial explicit placement | 33a5560 |

### Current Status

**Phase 1: Track Template Foundation** - COMPLETE ✓
- [x] 1.1 Expand repeat() function ✓
- [x] 1.2 Percentage track resolution ✓
- [x] 1.3 Intrinsic sizing (min-content, max-content) ✓
- [x] 1.4 fit-content(length) proper implementation ✓

**Phase 2: Auto-fill and Auto-fit** - COMPLETE ✓
- [x] 2.1 auto-fill implementation ✓
- [x] 2.2 auto-fit implementation (with empty track collapsing) ✓
- [x] 2.3 Gap collapsing for empty tracks ✓

**Phase 3: Named Lines and Areas** - COMPLETE ✓
- [x] 3.1 Named line resolution ✓
- [x] 3.2 grid-template-areas integration ✓
- [x] 3.3 Implicit line names from areas ✓

**Phase 4: Placement Algorithm Completion** - COMPLETE ✓
- [x] 4.1 Dense packing algorithm ✓
- [x] 4.2 Span to named line (SpanName resolution) ✓
- [x] 4.3 Order property (sort items before placement) ✓
- [x] 4.4 Placement edge cases (implicit tracks, overlapping) ✓

**Phase 5: Alignment Properties** - NOT STARTED
**Phase 6: Track Sizing Algorithm** - NOT STARTED
**Phase 7: Edge Cases and Polish** - NOT STARTED

---

## Current State Assessment

### What Exists

**Parsing (rustkit-css):**
- `GridTemplate` - tracks with sizes and line names
- `TrackSize` - px, %, fr, minmax, min-content, max-content, auto, fit-content
- `TrackRepeat` - count, auto-fill, auto-fit (struct exists, expansion implemented for Count)
- `GridAutoFlow` - row, column, row dense, column dense
- `GridLine` - auto, number, name, span, span-name
- `GridPlacement` - column/row start/end
- `GridTemplateAreas` - parsing and area lookup
- `JustifyItems`, `JustifySelf`, `AlignItems`, `AlignSelf`

**Layout (rustkit-layout/grid.rs):**
- Basic track creation from template with repeat() expansion
- Negative line resolution (-1 = last line)
- Four-phase placement algorithm (explicit, column-only, row-only, auto)
- Track sizing with fr units
- Item contribution to track sizing
- Gap support
- justify-self, align-self (stretch, start, end, center)
- Nested grid/flex layout

### What's Missing or Incomplete

1. ~~**repeat() function expansion** - parsed but not expanded~~ ✓ DONE (Phase 1.1)
2. ~~**auto-fill/auto-fit** - parsed, marked for layout-time expansion~~ ✓ DONE (Phase 2)
3. ~~**Named line resolution** - GridLine::Name not resolved~~ ✓ DONE (Phase 3.1)
4. ~~**grid-template-areas integration** - areas parsed but not used in placement~~ ✓ DONE (Phase 3.2-3.3)
5. ~~**min-content/max-content intrinsic sizing** - not properly computed~~ ✓ DONE (Phase 1.3)
6. **Subgrid** - Level 2 feature, out of scope
7. **justify-content, align-content** - grid container alignment
8. **Dense packing** - is_dense() exists but not implemented
9. **order property** - item ordering
10. ~~**Percentage tracks** - not resolved against container~~ ✓ DONE (Phase 1.2)
11. **Baseline alignment** - simplified to flex-start

---

## Implementation Phases

### Phase 1: Track Template Foundation
**Goal:** Robust track definition and sizing

- [x] **1.1 Expand repeat() function** ✓
  - Expand `repeat(3, 1fr)` → 3 tracks of 1fr
  - Expand `repeat(2, 100px 1fr)` → 4 tracks
  - Unit tests for various repeat patterns
  - **Implementation:**
    - Added `GridTemplate::expand_tracks()` in `rustkit-css/src/lib.rs`
    - Updated `GridLayout::new()` in `rustkit-layout/src/grid.rs` to use expanded tracks
    - 7 unit tests covering all expansion cases

- [x] **1.2 Percentage track resolution** ✓
  - Resolve `%` tracks against container size
  - Handle minmax with percentage in min or max
  - **Implementation:**
    - Added `percent` and `max_percent` fields to `GridTrack`
    - Updated `size_grid_tracks()` to resolve percentages in Step 2
    - 5 unit tests for percentage tracks including minmax

- [x] **1.3 Intrinsic sizing (min-content, max-content)** ✓
  - Added `is_min_content` and `is_max_content` flags to `GridTrack`
  - Proper handling in track sizing algorithm
  - **Implementation:**
    - `TrackSize::MinContent` sets `is_min_content = true`
    - `TrackSize::MaxContent` sets `is_max_content = true`
    - `TrackSize::Auto` sets both flags (behaves like minmax(min-content, max-content))
    - Step 2.5 in `size_grid_tracks()` handles intrinsic sizing
    - 4 unit tests for intrinsic sizing

- [x] **1.4 fit-content(length) proper implementation** ✓
  - Clamp between min-content and provided max
  - **Implementation:**
    - Added `fit_content_limit` field to `GridTrack`
    - `TrackSize::FitContent` sets `is_min_content = true` and `fit_content_limit`
    - Step 2.5 in `size_grid_tracks()` clamps growth to the limit
    - 3 unit tests for fit-content

### Phase 2: Auto-fill and Auto-fit
**Goal:** Dynamic track creation

- [x] **2.1 auto-fill implementation** ✓
  - Calculate how many tracks fit in available space
  - Generate that many tracks
  - Handle minmax() in auto-fill
  - **Implementation:**
    - Added `AutoRepeatPattern` struct to store pattern info
    - Added `column_auto_repeat` and `row_auto_repeat` fields to `GridLayout`
    - `extract_auto_repeat()` extracts pattern from template
    - `calculate_auto_repeat_tracks()` calculates repetitions based on container size
    - `expand_auto_repeats()` called in `layout_grid_container()`
    - 8 unit tests for auto-fill

- [x] **2.2 auto-fit implementation** ✓
  - Same as auto-fill but collapse empty tracks
  - Collapsed tracks have zero size but exist for placement
  - **Implementation:**
    - Added `is_auto_fit` field to `GridTrack`
    - `collapse_empty_auto_fit_columns()` and `collapse_empty_auto_fit_rows()` methods
    - Phase 4.5 in `layout_grid_container()` collapses empty tracks
    - Gap collapsing handled in `size_grid_tracks()` Step 5
    - 5 unit tests for auto-fit

- [x] **2.3 Tests for responsive grids** ✓
  - All 13 Phase 2 tests passing

### Phase 3: Named Lines and Areas
**Goal:** Full named grid support

- [x] **3.1 Named line resolution** ✓
  - Resolve `GridLine::Name("header-start")` to line number
  - Support multiple names per line
  - **Implementation:**
    - Added `find_column_line_by_name()` and `find_row_line_by_name()` to GridLayout
    - Added `resolve_column_line()` and `resolve_row_line()` for full GridLine resolution
    - Added `set_placement_with_grid()` to GridItem for placement with grid context
    - Updated `layout_grid_container()` to use named line resolution
    - 4 unit tests for named line resolution

- [x] **3.2 grid-template-areas integration** ✓
  - Generate implicit named lines from areas
  - Place items using `grid-area: header`
  - **Implementation:**
    - Added `template_areas` field to GridLayout
    - Added `set_template_areas()` and `get_area()` methods
    - Updated `find_column_line_by_name()` and `find_row_line_by_name()` to check area names
    - Added `resolve_column_start_line()`, `resolve_column_end_line()` etc for position-aware resolve
    - Updated `set_placement_with_grid()` to use position-aware resolve
    - Updated `layout_grid_container()` to set template_areas from style
    - 4 unit tests for template areas

- [x] **3.3 Implicit line names from areas** ✓
  - Area "header" creates lines "header-start" and "header-end"
  - Implemented as part of 3.2 - implicit line name lookup checks template-areas

### Phase 4: Placement Algorithm Completion
**Goal:** Spec-compliant item placement

**Already implemented:**
- Four-phase placement (explicit both, column-only, row-only, full auto)
- `auto_row` and `auto_column` tracking on GridItem

- [x] **4.1 Dense packing algorithm** ✓
  - Implement `grid-auto-flow: row dense` / `column dense`
  - Backfill earlier gaps
  - **Implementation:**
    - Added `find_next_cell_dense()` that starts from (0,0) instead of cursor
    - Refactored `find_next_cell()` to use shared `find_next_cell_impl()` with dense param
    - Updated Phase 4 auto-placement to use dense method when `auto_flow.is_dense()`
    - 3 unit tests for dense packing

- [x] **4.2 span to named line** ✓
  - `grid-column: span header` - span until "header" line
  - `GridLine::SpanName` handling
  - **Implementation:**
    - Updated `SpanName` resolution to return target line number (not span count)
    - Added support for area names in SpanName (position-aware: start vs end)
    - Added support for implicit line names (e.g., "sidebar-end")
    - 3 unit tests for SpanName

- [x] **4.3 order property** ✓
  - Sort items by order before placement
  - Maintain DOM order for equal order values
  - **Implementation:**
    - Added `order()` method to GridItem (reads from layout_box.style.order)
    - Added stable sort by order before placement phases
    - 2 unit tests for order sorting

- [x] **4.4 Placement edge cases** ✓
  - Items placed beyond explicit grid
  - Overlapping items (z-index consideration)
  - **Implementation:**
    - Verified `ensure_tracks()` adds implicit tracks correctly
    - Verified overlapping explicitly-placed items are allowed
    - Verified negative line numbers are preserved for later resolution
    - 3 unit tests for edge cases

### Phase 5: Alignment Properties
**Goal:** Full alignment support

- [ ] **5.1 justify-content / align-content**
  - Distribute space between/around tracks
  - Values: start, end, center, stretch, space-between, space-around, space-evenly

- [ ] **5.2 place-content shorthand**
  - Parse and apply

- [ ] **5.3 Baseline alignment**
  - Proper baseline calculation for align-self: baseline
  - First baseline vs last baseline

### Phase 6: Track Sizing Algorithm (Spec Compliance)
**Goal:** Match the spec's track sizing algorithm exactly

The spec defines a complex multi-step algorithm:

- [ ] **6.1 Initialize track sizes**
  - Set base size and growth limit per spec

- [ ] **6.2 Resolve intrinsic track sizes**
  - Size tracks to fit items with intrinsic sizing
  - Handle spanning items correctly

- [ ] **6.3 Maximize tracks**
  - Grow tracks to their growth limits

- [ ] **6.4 Expand flexible tracks**
  - Distribute free space to fr tracks
  - Handle min/max constraints

- [ ] **6.5 Stretch auto tracks**
  - If align-content/justify-content is stretch

### Phase 7: Edge Cases and Polish
**Goal:** Handle all edge cases

- [ ] **7.1 Empty grid containers**
  - Proper behavior with no items

- [ ] **7.2 Grid item minimum size**
  - Default min-width/min-height of auto
  - Overflow handling

- [ ] **7.3 Absolutely positioned grid items**
  - Position relative to grid area

- [ ] **7.4 Grid item margins**
  - Auto margins for alignment
  - Margin collapsing (or lack thereof in grid)

- [ ] **7.5 Writing modes (future)**
  - RTL support for column ordering

---

## Testing Strategy

### Unit Tests (per feature)
Each phase should include unit tests for the specific feature.

**Completed tests (43 total):**

Phase 1.1 (repeat expansion - rustkit-css):
- `test_expand_tracks_no_repeat` - Template without repeats
- `test_expand_tracks_repeat_count` - Simple repeat(N, track)
- `test_expand_tracks_repeat_multiple_tracks` - repeat(N, track1 track2)
- `test_expand_tracks_mixed` - Tracks before/after repeat
- `test_expand_tracks_auto_fill_returns_unexpanded` - auto-fill deferred
- `test_expand_tracks_auto_fit_returns_unexpanded` - auto-fit deferred
- `test_expand_tracks_with_line_names` - Line names preserved

Phase 1.2 (percentage tracks - rustkit-layout):
- `test_track_sizing_percentage` - Basic percentage track
- `test_track_sizing_percentage_with_gap` - Percentages with gaps
- `test_track_sizing_multiple_percentages` - Multiple percentage tracks
- `test_track_sizing_minmax_with_percentage_min` - minmax(%, 1fr)
- `test_track_sizing_minmax_with_percentage_max` - minmax(100px, %)

Phase 1.3 (intrinsic sizing - rustkit-layout):
- `test_track_min_content_flag` - min-content flag set
- `test_track_max_content_flag` - max-content flag set
- `test_track_auto_is_intrinsic` - auto is min+max content
- `test_track_sizing_min_content` - min-content sizing
- `test_track_sizing_auto` - auto track sizing

Phase 1.4 (fit-content - rustkit-layout):
- `test_track_fit_content_flag` - fit-content flag and limit
- `test_track_sizing_fit_content_within_limit` - Content below limit
- `test_track_sizing_fit_content_at_limit` - Content at/above limit

Phase 2.1 (auto-fill - rustkit-layout):
- `test_auto_repeat_pattern_creation` - Template with auto-fill returns unexpanded
- `test_auto_fill_expansion_basic` - Basic auto-fill with fixed tracks
- `test_auto_fill_expansion_with_gap` - Auto-fill accounting for gaps
- `test_auto_fill_expansion_minmax` - Auto-fill with minmax(px, 1fr)
- `test_auto_fill_expansion_multiple_tracks` - Auto-fill pattern with multiple tracks
- `test_auto_fill_minimum_one_repetition` - At least 1 track when container is too small
- `test_auto_fill_with_fr_only` - Pure fr tracks create single repetition
- `test_grid_layout_expand_auto_repeats` - Full integration test

Phase 2.2 (auto-fit - rustkit-layout):
- `test_auto_fit_flag_set` - Tracks created from auto-fit have flag set
- `test_auto_fit_collapse_empty_tracks` - Empty tracks collapse to 0
- `test_auto_fit_collapse_method` - Collapse method works correctly
- `test_auto_fit_gap_collapsing` - Gaps collapse around empty tracks
- `test_auto_fit_all_collapsed` - All tracks collapsed case

Phase 3.1 (named lines - rustkit-layout):
- `test_find_column_line_by_name` - Find line by name in grid
- `test_resolve_column_line_by_name` - Resolve GridLine::Name to number
- `test_set_placement_with_named_lines` - Place item using named lines
- `test_named_line_mixed_with_numbers` - Mix named lines and numbers

Phase 3.2-3.3 (template areas - rustkit-layout):
- `test_grid_template_areas_placement` - Area lookup and coordinates
- `test_placement_with_area_name` - Place item using grid-area: name
- `test_implicit_line_names_from_areas` - area-start/area-end implicit lines
- `test_placement_with_implicit_line_names` - Place using header-start/header-end

### Integration Tests
Create test HTML files exercising grid features:
```
websuite/cases/grid-basic/          # Phase 1
websuite/cases/grid-autofill/       # Phase 2
websuite/cases/grid-named/          # Phase 3
websuite/cases/grid-placement/      # Phase 4
websuite/cases/grid-alignment/      # Phase 5
```

### Parity Tests
Run against Chrome baselines to validate visual correctness.

---

## File Locations

| Component | File |
|-----------|------|
| CSS Types | `crates/rustkit-css/src/lib.rs` |
| CSS Parsing | `crates/rustkit-css/src/lib.rs` (property parsing) |
| Grid Layout | `crates/rustkit-layout/src/grid.rs` |
| Style Resolution | `crates/rustkit-engine/src/lib.rs` |

---

## Priority Order

1. ~~**Phase 1** - Foundation (most impactful for basic grids)~~ ✓ COMPLETE
2. ~~**Phase 2** - Auto-fill/fit (responsive layouts)~~ ✓ COMPLETE
3. ~~**Phase 3** - Named lines (developer ergonomics)~~ ✓ COMPLETE
4. **Phase 4** - Placement (dense packing, named spans)
5. **Phase 5** - Alignment (polish)
6. **Phase 6** - Track sizing (spec compliance)
7. **Phase 7** - Edge cases (completeness)

---

## Success Criteria

- All grid-related parity tests pass (< 15% diff excluding text metrics)
- CSS Grid Level 1 features work as specified
- No regressions in existing tests
- Comprehensive unit test coverage

---

## Notes

- Subgrid (Level 2) and Masonry (Level 3) are out of scope
- Focus on layout correctness; rendering is handled by rustkit-renderer
- Text metrics differences are expected and acceptable

---

## Quick Reference

### Key Functions

**rustkit-css/src/lib.rs:**
- `GridTemplate::expand_tracks()` - Expand repeat() patterns

**rustkit-layout/src/grid.rs:**
- `GridLayout::new()` - Create grid from templates (uses expand_tracks)
- `GridLayout::ensure_tracks()` - Add implicit tracks
- `GridItem::set_placement()` - Resolve grid lines to track indices
- `layout_grid_container()` - Main layout entry point

### How repeat() Works

1. CSS parsing creates `GridTemplate` with `repeats: Vec<(usize, TrackRepeat)>`
2. `expand_tracks()` expands `TrackRepeat::Count(n, tracks)` inline
3. `auto-fill`/`auto-fit` are returned unexpanded for layout-time handling
4. `GridLayout::new()` uses expanded tracks to create `GridTrack` objects
