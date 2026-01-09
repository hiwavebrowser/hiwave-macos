# Phase 4 Remediation Plan: Addressing Failed Tests & Key Findings

Based on the test results from `testhardningand98pctparity.md`, this document outlines the next steps to address the 5 failing test cases and move toward 98% visual parity.

---

## Executive Summary

| Test Case | Current Diff | Target | Priority |
|-----------|--------------|--------|----------|
| about | 99.6% | <15% | **CRITICAL** |
| settings | 99.5% | <15% | **CRITICAL** |
| backgrounds | 38.7% | <12% | HIGH |
| gradients | 30.9% | <15% | HIGH |
| rounded-corners | 31.2% | <10% | HIGH |
| pseudo-classes | 21.6% | <8% | MEDIUM |

---

## Critical Issue: Rendering Pipeline Bug

### Root Cause Identified

The investigation revealed a critical bug:
- **CSS Variables**: Working correctly (var(--bg-primary) → #0f172a)
- **Multi-background Parsing**: Fixed (comma-separated layers handled)
- **Rendering**: **BROKEN** - Background colors are parsed correctly but not rendered

**Evidence:** Frame output shows light colors (RGB 202, 229, 234) instead of the expected dark slate (#0f172a = RGB 15, 23, 42).

### Investigation Path

1. **Trace the Display List Generation**
   - File: `crates/rustkit-layout/src/lib.rs` (display list builder)
   - Verify background color is added to display list
   - Add debug output at display list creation point

2. **Trace the Renderer**
   - File: `crates/rustkit-renderer/src/lib.rs` (paint execution)
   - Verify display list items are being processed
   - Check if background fill commands are executed

3. **Check Paint Order / Z-order**
   - Verify body background is painted BEFORE child content
   - Check if root box white background is painting OVER body background

### Micro-Test

Create `parity-tests/rendering-debug.html`:
```html
<!DOCTYPE html>
<html>
<head>
  <style>
    :root { --test-bg: #ff0000; }
    body {
      margin: 0;
      background: var(--test-bg);
      min-height: 100vh;
    }
  </style>
</head>
<body></body>
</html>
```

**Pass Criteria:** Captured frame should be solid red (#ff0000). If not, confirms rendering pipeline bug.

---

## Task 1: Fix Rendering Pipeline (CRITICAL)

### Subtasks

1. **Add debug instrumentation to display list builder**
   ```
   Location: crates/rustkit-layout/src/display_list.rs (or equivalent)
   Action: Log when background fill command is added
   Output: "[DISPLAY_LIST] Adding background fill: color={}, rect={}"
   ```

2. **Add debug instrumentation to renderer**
   ```
   Location: crates/rustkit-renderer/src/lib.rs
   Action: Log when background fill is executed
   Output: "[RENDER] Filling rect {} with color {}"
   ```

3. **Verify paint order**
   - Confirm backgrounds paint before content
   - Check if anything overwrites the background after painting

4. **Check root box behavior**
   - The root box is set to white background
   - Body background should override this, but may not be happening
   - Fix: Either don't paint root box background, or ensure body paints after

### Validation

- Run `rendering-debug.html` test
- Frame should show correct color
- Re-run `about` and `settings` pages
- Target: <15% diff (from 99.6%)

---

## Task 2: Improve Backgrounds Test (38.7% → <12%)

### Current Issues

- Multi-layer backgrounds may not render all layers
- Background-clip values may not be applied correctly
- Background-size/position may have calculation errors

### Subtasks

1. **Create isolated background tests**
   ```
   parity-tests/bg-solid.html       - Solid colors only
   parity-tests/bg-clip.html        - background-clip values
   parity-tests/bg-size.html        - background-size: cover/contain/auto
   parity-tests/bg-position.html    - background-position keywords
   parity-tests/bg-multi.html       - Multiple background layers
   ```

2. **Fix background-clip**
   - Verify content-box, padding-box, border-box clipping
   - Check clipping path calculation

3. **Fix background-size**
   - `cover`: Scale to fill, crop overflow
   - `contain`: Scale to fit, preserve aspect ratio
   - Verify aspect ratio calculations

4. **Fix multi-layer rendering**
   - Layers should paint bottom-to-top (last declared = bottom)
   - Each layer needs correct positioning

### Validation

- Each isolated test <10% diff
- Combined backgrounds test <12% diff

---

## Task 3: Improve Gradients Test (30.9% → <15%)

### Current Issues

- Linear gradient angle calculation
- Radial gradient shape/size
- Color stop interpolation
- Color space (sRGB vs linear)

### Subtasks

1. **Create isolated gradient tests**
   ```
   parity-tests/grad-linear-angles.html    - 0deg, 45deg, 90deg, 135deg, 180deg
   parity-tests/grad-linear-stops.html     - Multi-stop, percentage positions
   parity-tests/grad-radial-shapes.html    - circle, ellipse
   parity-tests/grad-radial-size.html      - closest-side, farthest-corner, etc.
   parity-tests/grad-radial-position.html  - at center, at top left, etc.
   ```

2. **Verify linear gradient math**
   - Angle to direction vector conversion
   - Gradient line length calculation
   - Color stop position mapping

3. **Verify radial gradient math**
   - Shape rendering (circle vs ellipse)
   - Size keyword resolution
   - Center position calculation

4. **Check color interpolation**
   - Chrome uses sRGB interpolation by default
   - Verify color stop blending matches Chrome

### Validation

- Linear gradient tests <12% diff
- Radial gradient tests <18% diff (more complex)
- Combined gradients test <15% diff

---

## Task 4: Improve Rounded Corners Test (31.2% → <10%)

### Current Issues

- Anti-aliasing differences
- Elliptical radius handling
- Corner clipping of content
- Border rendering on curves

### Subtasks

1. **Create isolated corner tests**
   ```
   parity-tests/corners-uniform.html       - Same radius all corners
   parity-tests/corners-individual.html    - Different radius per corner
   parity-tests/corners-elliptical.html    - border-radius: 50% / 25%
   parity-tests/corners-clipping.html      - Content clipped to rounded rect
   parity-tests/corners-borders.html       - Borders on rounded corners
   ```

2. **Verify radius calculation**
   - Percentage to pixel conversion
   - Elliptical radius (horizontal/vertical)
   - Radius clamping when too large

3. **Improve anti-aliasing**
   - Match Chrome's anti-aliasing approach
   - Accept some variance (anti-aliasing is implementation-specific)

4. **Fix content clipping**
   - Content should clip to rounded border
   - Verify clipping path includes border-radius

### Validation

- Each isolated test <8% diff
- Combined corners test <10% diff

---

## Task 5: Improve Pseudo-classes Test (21.6% → <8%)

### Current Issues

- :nth-child formula parsing
- :not() selector matching
- Complex pseudo-class combinations

### Subtasks

1. **Audit :nth-child parsing**
   - Verify `odd`, `even` handling
   - Verify `An+B` formula (e.g., `3n+1`, `2n`, `-n+3`)

2. **Audit :not() matching**
   - Negation should work with any simple selector
   - :not(.class), :not(#id), :not([attr])

3. **Test combinations**
   ```
   parity-tests/pseudo-nth-formulas.html   - All An+B variants
   parity-tests/pseudo-not-complex.html    - :not() with various selectors
   parity-tests/pseudo-combined.html       - Multiple pseudo-classes together
   ```

### Validation

- Pseudo-classes test <8% diff
- Computed-style matches Chrome for test elements

---

## Execution Order

### Phase A: Critical Fix (Rendering Pipeline)
1. Add debug instrumentation
2. Identify where background color is lost
3. Fix the bug
4. Validate with `about` and `settings` pages

### Phase B: Paint Coverage
1. Backgrounds improvements
2. Gradients improvements
3. Rounded corners improvements

### Phase C: CSS Refinement
1. Pseudo-classes improvements

---

## Success Criteria

| Metric | Current | Target |
|--------|---------|--------|
| Passed Tests | 3/5 (60%) | 5/5 (100%) |
| Average Diff | 41.2% | <15% |
| Worst Case | 99.6% (about) | <20% |

### Final Validation Checklist

- [ ] All 5 main tests pass (<15% diff each)
- [ ] `about` page renders with correct dark background
- [ ] `settings` page renders with correct dark background
- [ ] All micro-tests pass thresholds
- [ ] No regressions on previously passing tests (new_tab, chrome_rustkit, shelf)
- [ ] Stability: <0.5% variance on repeated runs

---

## Debug Commands Reference

```bash
# Run single page capture with debug output
RUST_LOG=debug cargo run --release -p parity-capture -- \
  --html crates/hiwave-app/src/ui/about.html \
  --width 800 --height 600 \
  --output /tmp/about-debug.ppm

# Examine frame bytes
xxd /tmp/about-debug.ppm | head -20

# Run full parity test suite
cargo run --release -p parity-capture -- --all

# View diff heatmaps
open parity-baseline/diffs/about/heatmap.png
```
