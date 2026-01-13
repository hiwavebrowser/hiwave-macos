# CSS Grid Level 1 Implementation Plan

A systematic implementation of the CSS Grid Layout Module Level 1 specification.

**Reference:** https://www.w3.org/TR/css-grid-1/

---

## Progress Tracking

### Changelog

| Date | Phase | Change | Commit |
|------|-------|--------|--------|
| 2026-01-12 | 1.4 | Implemented fit-content() with limit tracking | pending |
| 2026-01-12 | 1.3 | Added min-content/max-content intrinsic sizing flags | pending |
| 2026-01-12 | 1.2 | Added percentage track resolution with minmax support | pending |
| 2026-01-12 | 1.1 | Added `expand_tracks()` to GridTemplate, updated GridLayout to use it | pending |
| 2026-01-12 | - | Created implementation plan | - |
| 2026-01-12 | - | Fixed grid auto-placement for partial explicit placement | 33a5560 |

### Current Status

**Phase 1: Track Template Foundation** - COMPLETE ✓
- [x] 1.1 Expand repeat() function ✓
- [x] 1.2 Percentage track resolution ✓
- [x] 1.3 Intrinsic sizing (min-content, max-content) ✓
- [x] 1.4 fit-content(length) proper implementation ✓

**Phase 2: Auto-fill and Auto-fit** - NOT STARTED
**Phase 3: Named Lines and Areas** - NOT STARTED
**Phase 4: Placement Algorithm Completion** - PARTIALLY COMPLETE (auto-placement fixed)
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
2. **auto-fill/auto-fit** - parsed, marked for layout-time expansion (Phase 2)
3. **Named line resolution** - GridLine::Name not resolved
4. **grid-template-areas integration** - areas parsed but not used in placement
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

- [ ] **2.1 auto-fill implementation**
  - Calculate how many tracks fit in available space
  - Generate that many tracks
  - Handle minmax() in auto-fill
  - **Files:** `rustkit-layout/src/grid.rs` (GridLayout::new or separate method)

- [ ] **2.2 auto-fit implementation**
  - Same as auto-fill but collapse empty tracks
  - Collapsed tracks have zero size but exist for placement

- [ ] **2.3 Tests for responsive grids**
  - `repeat(auto-fill, minmax(200px, 1fr))`
  - `repeat(auto-fit, minmax(200px, 1fr))`

### Phase 3: Named Lines and Areas
**Goal:** Full named grid support

- [ ] **3.1 Named line resolution**
  - Resolve `GridLine::Name("header-start")` to line number
  - Support multiple names per line
  - Support implicit names from areas (e.g., "header-start", "header-end")
  - **Files:** `rustkit-layout/src/grid.rs` (resolve_line_to_number)

- [ ] **3.2 grid-template-areas integration**
  - Generate implicit named lines from areas
  - Place items using `grid-area: header`
  - Validate area rectangularity

- [ ] **3.3 Implicit line names from areas**
  - Area "header" creates lines "header-start" and "header-end"

### Phase 4: Placement Algorithm Completion
**Goal:** Spec-compliant item placement

**Already implemented:**
- Four-phase placement (explicit both, column-only, row-only, full auto)
- `auto_row` and `auto_column` tracking on GridItem

- [ ] **4.1 Dense packing algorithm**
  - Implement `grid-auto-flow: row dense` / `column dense`
  - Backfill earlier gaps

- [ ] **4.2 span to named line**
  - `grid-column: span header` - span until "header" line
  - `GridLine::SpanName` handling

- [ ] **4.3 order property**
  - Sort items by order before placement
  - Maintain DOM order for equal order values

- [ ] **4.4 Placement edge cases**
  - Items placed beyond explicit grid
  - Overlapping items (z-index consideration)

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

**Completed tests (22 total):**

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

1. **Phase 1** - Foundation (most impactful for basic grids) ← CURRENT
2. **Phase 6** - Track sizing (correctness)
3. **Phase 4** - Placement (correctness)
4. **Phase 2** - Auto-fill/fit (responsive layouts)
5. **Phase 3** - Named lines (developer ergonomics)
6. **Phase 5** - Alignment (polish)
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
