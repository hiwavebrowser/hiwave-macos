# RustKit Parity Improvement Plan

## Implementation Status

### Phase 1: Background Layer Foundation - INFRASTRUCTURE COMPLETE

**Implemented:**
- Added `BackgroundLayer` struct to `rustkit-css` with `BackgroundImage`, `BackgroundSize`, `BackgroundPosition`, `BackgroundRepeat`, `BackgroundOrigin` types
- Updated `ComputedStyle` to include `background_layers: Vec<BackgroundLayer>`
- Added CSS parsing for `background-size`, `background-position`, `background-repeat`, `background-origin`
- Updated `render_background()` in `rustkit-layout` to iterate through multiple layers

**Files modified:**
- `crates/rustkit-css/src/lib.rs` - New background layer types
- `crates/rustkit-engine/src/lib.rs` - Background property parsing
- `crates/rustkit-layout/src/lib.rs` - Multi-layer rendering

**Remaining for full parity improvement:**
- Add `repeating-linear-gradient` support
- Add `repeating-radial-gradient` support
- Add `conic-gradient` support
- Verify background-image URL loading works correctly
- Fine-tune gradient rendering interpolation

---

## Current State Summary

| Test Case | Diff % | Threshold | Status | Priority |
|-----------|--------|-----------|--------|----------|
| gradient-backgrounds | 80.23% | 15% | FAIL | P0 |
| image-gallery | 72.83% | 10% | FAIL | P0 |
| bg-pure | 41.44% | 15% | FAIL | P0 |
| sticky-scroll | 40.34% | 25% | FAIL | P1 |
| backgrounds | 36.50% | 15% | FAIL | P0 |
| shelf | 34.93% | 15% | FAIL | P1 |
| css-selectors | 32.20% | 15% | FAIL | P0 |
| gradients | 31.53% | 15% | FAIL | P0 |
| flex-positioning | 29.98% | 15% | FAIL | P1 |
| rounded-corners | 29.87% | 15% | FAIL | P1 |
| card-grid | 26.95% | 15% | FAIL | P1 |
| bg-solid | 25.14% | 15% | FAIL | P0 |
| settings | 21.97% | 15% | FAIL | P2 |
| pseudo-classes | 21.52% | 15% | FAIL | P0 |
| combinators | 16.23% | 15% | FAIL | P0 |
| images-intrinsic | 13.26% | 10% | FAIL | P2 |
| specificity | 12.67% | 15% | PASS | - |
| about | 11.03% | 15% | PASS | - |
| article-typography | 10.66% | 20% | PASS | - |
| form-controls | 8.04% | 12% | PASS | - |
| form-elements | 6.06% | 12% | PASS | - |
| chrome_rustkit | 2.57% | 15% | PASS | - |
| new_tab | 1.64% | 15% | PASS | - |

**Current: 7 passed, 16 failed**

---

## Root Cause Analysis

### 1. Background System (affects 6 tests, ~50% combined diff reduction potential)
**Tests affected:** gradient-backgrounds, bg-pure, backgrounds, gradients, bg-solid, image-gallery

**Current limitation:** Single `background_color` + `Option<Gradient>` in ComputedStyle (rustkit-css/src/lib.rs:1284-1285)

**Missing features:**
- Multiple background layers
- `background-size` (cover/contain/length)
- `background-position` (keywords/percentages)
- `background-repeat` (no-repeat/repeat-x/repeat-y)
- `background-origin` / `background-clip`

### 2. Selector Engine (affects 3 tests, ~25% combined diff reduction potential)
**Tests affected:** css-selectors, pseudo-classes, combinators

**Current limitation:** Combinator matching returns early `false` at line 2936-2937 in rustkit-engine/src/lib.rs

**Missing features:**
- Proper backward walk for descendant/child/sibling combinators
- `:not()` with full selector grammar
- `:nth-child(an+b)` negative offsets

### 3. Sticky Positioning (affects 1 test, isolated fix)
**Tests affected:** sticky-scroll

**Current limitation:** Treats `Position::Sticky` as relative (rustkit-layout/src/lib.rs:1052-1061)

**Missing:** Integration with StickyState from scroll.rs

### 4. Flex/Grid Layout (affects 2 tests)
**Tests affected:** flex-positioning, card-grid

**Potential issues:**
- Gap handling in flex containers
- Aspect-ratio interaction with flex items
- Grid item sizing with row/column spans

### 5. Rounded Corners (affects 1 test)
**Tests affected:** rounded-corners

**Potential issues:**
- Elliptical radii (`50px / 25px`)
- Percentage radii
- Box-shadow with rounded corners
- Outline with rounded corners

---

## Phased Implementation Plan

### Phase 1: Background Layer Foundation (HIGH ROI)
**Target:** Drop gradient-backgrounds from 80% to <20%, fix bg-* tests

**Duration:** 2-3 focused sessions

#### Step 1.1: Data Model Changes
```
File: crates/rustkit-css/src/lib.rs
```
- Add `BackgroundLayer` struct:
  ```rust
  pub struct BackgroundLayer {
      pub image: Option<BackgroundImage>,  // gradient or url()
      pub position: BackgroundPosition,
      pub size: BackgroundSize,
      pub repeat: BackgroundRepeat,
      pub origin: BackgroundOrigin,
      pub clip: BackgroundClip,
  }
  ```
- Replace `background_gradient: Option<Gradient>` with `background_layers: Vec<BackgroundLayer>`
- Add supporting enums: `BackgroundSize`, `BackgroundPosition`, `BackgroundRepeat`

#### Step 1.2: CSS Parsing Updates
```
File: crates/rustkit-engine/src/lib.rs (style application)
File: crates/rustkit-cssparser/src/lib.rs (if separate parser)
```
- Parse `background` shorthand into layers
- Parse `background-size`, `background-position`, `background-repeat`
- Handle comma-separated multiple values

#### Step 1.3: Rendering Updates
```
File: crates/rustkit-layout/src/lib.rs (render_background)
File: crates/rustkit-renderer/src/lib.rs
```
- Iterate layers bottom-to-top
- Apply position/size/repeat per layer
- Respect clip/origin per layer

#### Verification
```bash
python3 scripts/parity_test.py --case bg-pure
python3 scripts/parity_test.py --case bg-solid
python3 scripts/parity_test.py --case backgrounds
python3 scripts/parity_test.py --case gradients
python3 scripts/parity_test.py --case gradient-backgrounds
```

**Expected outcome:** 5 tests move from FAIL to PASS or near-threshold

---

### Phase 2: Selector Engine Fixes (HIGH ROI)
**Target:** Drop css-selectors/combinators/pseudo-classes to <15%

**Duration:** 1-2 focused sessions

#### Step 2.1: Fix Combinator Matching
```
File: crates/rustkit-engine/src/lib.rs (selector_matches, ~line 2904)
```
- Remove early return at line 2936-2937
- Implement proper backward walk through token list
- For descendant (` `): walk ancestors until match
- For child (`>`): check immediate parent only
- For adjacent sibling (`+`): check immediate previous sibling
- For general sibling (`~`): walk all previous siblings

#### Step 2.2: Fix `:not()` Implementation
```
File: crates/rustkit-engine/src/lib.rs (match_pseudo_class)
```
- Parse inner selector in `:not(selector)`
- Recursively call selector matching on inner
- Return inverse of result

#### Step 2.3: Fix `:nth-child` Edge Cases
- Support negative `b` values in `an+b` formula
- Implement `:nth-last-child`

#### Verification
```bash
python3 scripts/parity_test.py --case css-selectors
python3 scripts/parity_test.py --case combinators
python3 scripts/parity_test.py --case pseudo-classes
python3 scripts/parity_test.py --case specificity  # regression check
```

**Expected outcome:** 3 tests move from FAIL to PASS

---

### Phase 3: Sticky Positioning
**Target:** Drop sticky-scroll from 40% to <25%

**Duration:** 1 session

#### Step 3.1: Integrate StickyState
```
File: crates/rustkit-layout/src/lib.rs
File: crates/rustkit-layout/src/scroll.rs
```
- In layout phase, identify sticky elements
- Compute sticky container (nearest scrollable ancestor)
- Store sticky constraints (top/bottom/left/right offsets)

#### Step 3.2: Apply Sticky in Paint
- During paint/composite, adjust sticky element position based on scroll offset
- Clamp to container bounds

#### Verification
```bash
python3 scripts/parity_test.py --case sticky-scroll
```

**Expected outcome:** sticky-scroll moves from FAIL to PASS

---

### Phase 4: Rounded Corners Refinement
**Target:** Drop rounded-corners from 30% to <15%

**Duration:** 1 session

#### Step 4.1: Elliptical Radii Support
```
File: crates/rustkit-css/src/lib.rs
File: crates/rustkit-renderer/src/lib.rs
```
- Parse `border-radius: 50px / 25px` syntax (x-radius / y-radius)
- Update BorderRadius struct to hold x/y pairs
- Update SDF corner drawing for elliptical curves

#### Step 4.2: Percentage Radii
- Resolve percentage radii relative to element dimensions
- Handle `50%` creating circles/ellipses correctly

#### Step 4.3: Box-Shadow + Rounded Corners
- Apply border-radius clipping to box-shadow rendering

#### Verification
```bash
python3 scripts/parity_test.py --case rounded-corners
```

---

### Phase 5: Flex/Grid Refinement
**Target:** Drop flex-positioning and card-grid to <15%

**Duration:** 1-2 sessions

#### Step 5.1: Flex Gap Handling
```
File: crates/rustkit-layout/src/lib.rs (flex layout)
```
- Verify gap is applied correctly between items
- Check row-gap vs column-gap in flex-wrap scenarios

#### Step 5.2: Aspect-Ratio + Flex
- Ensure aspect-ratio is respected in flex item sizing
- Handle min/max constraints with aspect-ratio

#### Step 5.3: Grid Improvements (if needed for card-grid)
- Verify grid-template-columns/rows parsing
- Check span handling

#### Verification
```bash
python3 scripts/parity_test.py --case flex-positioning
python3 scripts/parity_test.py --case card-grid
```

---

### Phase 6: Image Gallery + Aspect Ratio
**Target:** Drop image-gallery from 73% to <15%

**Duration:** 1 session (may be partially solved by Phase 1 + Phase 5)

#### Step 6.1: Verify Background-Size on Gradients
- Ensure `background-size: cover/contain` works for gradient layers

#### Step 6.2: Aspect-Ratio in Grid Context
- Test aspect-ratio behavior in CSS Grid cells

#### Verification
```bash
python3 scripts/parity_test.py --case image-gallery
```

---

## Testing Protocol

### Per-Phase Testing
After each phase, run:
```bash
# Run affected tests
python3 scripts/parity_test.py --case <case_name>

# Run full regression
python3 scripts/parity_test.py --scope all

# Generate comparison report
python3 scripts/parity_compare.py --baseline parity-baseline/previous --current parity-baseline
```

### CI Integration
The parity gate runs automatically on PRs with:
- 4 sharded swarm workers
- Max 25% diff threshold for PR merge
- Regression budget of 0.5% per commit

### Micro-Test Strategy
For targeted debugging, create minimal HTML fixtures:
```
websuite/micro/<feature>/index.html
```
Example for background layers:
```html
<!DOCTYPE html>
<style>
  .test {
    width: 200px;
    height: 200px;
    background:
      linear-gradient(red, blue),
      linear-gradient(to right, green, yellow);
    background-size: 50% 100%, 100% 50%;
  }
</style>
<div class="test"></div>
```

---

## Success Metrics

| Phase | Tests Fixed | Expected Diff Reduction |
|-------|-------------|------------------------|
| Phase 1 | 5-6 | gradient-backgrounds 80%→15%, bg-* tests passing |
| Phase 2 | 3 | css-selectors/combinators/pseudo-classes all <15% |
| Phase 3 | 1 | sticky-scroll <25% |
| Phase 4 | 1 | rounded-corners <15% |
| Phase 5 | 2 | flex-positioning, card-grid <15% |
| Phase 6 | 1 | image-gallery <15% |

**Target end state:** 20+ tests passing (from current 7)

---

## Quick Wins (Parallel Opportunities)

These can be tackled opportunistically:

1. **shelf (34.93%)** - May improve with Phase 1 (background fixes)
2. **settings (21.97%)** - May improve with Phase 1 + Phase 2
3. **images-intrinsic (13.26%)** - Close to threshold, minor fix needed

---

## Appendix: File Reference

| Area | Primary Files |
|------|--------------|
| CSS Data Model | `crates/rustkit-css/src/lib.rs` |
| Style Application | `crates/rustkit-engine/src/lib.rs` |
| Layout | `crates/rustkit-layout/src/lib.rs` |
| Rendering | `crates/rustkit-renderer/src/lib.rs` |
| Scroll/Sticky | `crates/rustkit-layout/src/scroll.rs` |
| Parity Testing | `scripts/parity_test.py`, `scripts/parity_swarm.py` |
| Test Fixtures | `websuite/micro/`, `websuite/cases/` |
| Baselines | `baselines/chrome-120/` |
