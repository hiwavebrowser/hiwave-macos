# GPU Gradient Intensive Testing Plan

## Executive Summary

The about page shows a 4.47% regression with GPU gradients enabled (11.93% CPU → 16.40% GPU).
This plan systematically tests gradient features to isolate the root causes.

### Key Suspects

Based on attribution analysis of the about page:

| Element | Contribution | Gradient Type | Suspected Issue |
|---------|-------------|---------------|-----------------|
| body | 36.16% | `radial-gradient(ellipse at top, rgba(...), transparent 50%)` | Position "at top", transparent interpolation |
| h1.logo | 10.64% | `linear-gradient(135deg, ...)` with 5 stops + text-clip | Multi-stop diagonal, background-clip |
| sponsor-btn | 3.46% | `linear-gradient(135deg, ...)` with border-radius | Diagonal + rounded corners |

---

## Phase 1: Isolated Micro-Tests

Create minimal test cases for each gradient feature to compare CPU vs GPU rendering.

### Test Fixture: `/websuite/micro/gpu-gradient-regression/index.html`

```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>GPU Gradient Regression Tests</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: system-ui; padding: 20px; background: #f5f5f5; }
    h2 { margin: 20px 0 10px; font-size: 14px; color: #333; }
    h3 { margin: 15px 0 5px; font-size: 12px; color: #666; }
    .row { margin-bottom: 20px; display: flex; flex-wrap: wrap; gap: 10px; }
    .test-box { width: 150px; height: 100px; display: inline-block; }
    .test-box-wide { width: 300px; height: 100px; }
    .label { font-size: 10px; text-align: center; margin-top: 4px; }

    /* ========================================
       SECTION A: RADIAL POSITION VARIATIONS
       Tests "at <position>" handling
       ======================================== */
    .radial-center    { background: radial-gradient(ellipse at center, cyan, blue); }
    .radial-top       { background: radial-gradient(ellipse at top, cyan, blue); }
    .radial-bottom    { background: radial-gradient(ellipse at bottom, cyan, blue); }
    .radial-left      { background: radial-gradient(ellipse at left, cyan, blue); }
    .radial-right     { background: radial-gradient(ellipse at right, cyan, blue); }
    .radial-top-left  { background: radial-gradient(ellipse at top left, cyan, blue); }
    .radial-25-75     { background: radial-gradient(ellipse at 25% 75%, cyan, blue); }

    /* ========================================
       SECTION B: TRANSPARENT INTERPOLATION
       Tests rgba to transparent
       ======================================== */
    .trans-cyan-50    { background: radial-gradient(ellipse at top, rgba(6,182,212,0.15), transparent 50%); }
    .trans-red-50     { background: radial-gradient(circle, rgba(255,0,0,0.5), transparent 50%); }
    .trans-linear     { background: linear-gradient(90deg, rgba(255,0,0,0.8), transparent); }
    .trans-multi      { background: linear-gradient(90deg, red, transparent 50%, blue); }

    /* ========================================
       SECTION C: SEMI-TRANSPARENT COLORS
       Tests rgba interpolation
       ======================================== */
    .semi-trans-1     { background: linear-gradient(90deg, rgba(255,0,0,0.5), rgba(0,0,255,0.5)); }
    .semi-trans-2     { background: radial-gradient(circle, rgba(255,255,255,0.3), rgba(0,0,0,0.1)); }
    .semi-trans-3     { background: linear-gradient(135deg, rgba(6,182,212,0.15), rgba(255,255,255,0)); }

    /* ========================================
       SECTION D: ELLIPSE SIZING AT NON-CENTER
       Tests farthest-corner with offset center
       ======================================== */
    .ellipse-fc-top   { background: radial-gradient(ellipse farthest-corner at top, cyan, blue); }
    .ellipse-fc-25-25 { background: radial-gradient(ellipse farthest-corner at 25% 25%, cyan, blue); }
    .ellipse-cs-top   { background: radial-gradient(ellipse closest-side at top, cyan, blue); }
    .ellipse-fs-top   { background: radial-gradient(ellipse farthest-side at top, cyan, blue); }

    /* ========================================
       SECTION E: MULTI-STOP LINEAR (ABOUT PAGE LOGO)
       ======================================== */
    .multi-5-stop     { background: linear-gradient(135deg, #0891b2 0%, #06b6d4 25%, #22d3ee 50%, #06b6d4 75%, #0891b2 100%); }
    .multi-3-stop     { background: linear-gradient(135deg, #0891b2, #22d3ee, #0891b2); }
    .multi-rainbow    { background: linear-gradient(90deg, red, orange, yellow, green, blue, purple); }

    /* ========================================
       SECTION F: DIAGONAL ANGLES WITH BORDER-RADIUS
       ======================================== */
    .diag-45-r8       { background: linear-gradient(45deg, red, blue); border-radius: 8px; }
    .diag-135-r8      { background: linear-gradient(135deg, red, blue); border-radius: 8px; }
    .diag-135-r16     { background: linear-gradient(135deg, red, blue); border-radius: 16px; }
    .diag-135-r50     { background: linear-gradient(135deg, red, blue); border-radius: 50%; }

    /* ========================================
       SECTION G: EXACT ABOUT PAGE REPRODUCTIONS
       ======================================== */
    .about-body       {
      background: radial-gradient(ellipse at top, rgba(6,182,212,0.15), transparent 50%),
                  #1a1a2e;
    }
    .about-logo       {
      background: linear-gradient(135deg, #0891b2 0%, #06b6d4 25%, #22d3ee 50%, #06b6d4 75%, #0891b2 100%);
    }
    .about-btn        {
      background: linear-gradient(135deg, #0891b2, #06b6d4);
      border-radius: 8px;
    }

    /* ========================================
       SECTION H: LAYERED GRADIENTS
       ======================================== */
    .layered-1        {
      background: radial-gradient(circle at 20% 20%, rgba(255,255,255,0.3), transparent 50%),
                  linear-gradient(135deg, #667eea, #764ba2);
    }
    .layered-2        {
      background: radial-gradient(ellipse at top, rgba(6,182,212,0.2), transparent 70%),
                  linear-gradient(180deg, #1a1a2e, #2d3a4a);
    }
  </style>
</head>
<body>
  <h1>GPU Gradient Regression Tests</h1>
  <p>Isolated tests to identify GPU vs CPU gradient differences.</p>
  <p>Run with: <code>RUSTKIT_GPU_GRADIENTS=1</code> vs without</p>

  <h2>A. Radial Position Variations</h2>
  <h3>Tests "at &lt;position&gt;" handling - center, edge, corner positions</h3>
  <div class="row">
    <div><div class="test-box radial-center"></div><div class="label">at center</div></div>
    <div><div class="test-box radial-top"></div><div class="label">at top</div></div>
    <div><div class="test-box radial-bottom"></div><div class="label">at bottom</div></div>
    <div><div class="test-box radial-left"></div><div class="label">at left</div></div>
    <div><div class="test-box radial-right"></div><div class="label">at right</div></div>
    <div><div class="test-box radial-top-left"></div><div class="label">at top left</div></div>
    <div><div class="test-box radial-25-75"></div><div class="label">at 25% 75%</div></div>
  </div>

  <h2>B. Transparent Interpolation</h2>
  <h3>Tests interpolation to/from transparent - key issue in about page body</h3>
  <div class="row">
    <div><div class="test-box trans-cyan-50"></div><div class="label">cyan→trans 50% (about body)</div></div>
    <div><div class="test-box trans-red-50"></div><div class="label">red→trans 50%</div></div>
    <div><div class="test-box trans-linear"></div><div class="label">linear red→trans</div></div>
    <div><div class="test-box trans-multi"></div><div class="label">red→trans→blue</div></div>
  </div>

  <h2>C. Semi-Transparent Color Interpolation</h2>
  <h3>Tests rgba interpolation between semi-transparent colors</h3>
  <div class="row">
    <div><div class="test-box semi-trans-1"></div><div class="label">rgba 0.5 red→blue</div></div>
    <div><div class="test-box semi-trans-2"></div><div class="label">rgba white→black</div></div>
    <div><div class="test-box semi-trans-3"></div><div class="label">rgba cyan→trans</div></div>
  </div>

  <h2>D. Ellipse Sizing at Non-Center Positions</h2>
  <h3>Tests farthest-corner/closest-side calculations with offset center</h3>
  <div class="row">
    <div><div class="test-box ellipse-fc-top"></div><div class="label">farthest-corner at top</div></div>
    <div><div class="test-box ellipse-fc-25-25"></div><div class="label">farthest-corner at 25% 25%</div></div>
    <div><div class="test-box ellipse-cs-top"></div><div class="label">closest-side at top</div></div>
    <div><div class="test-box ellipse-fs-top"></div><div class="label">farthest-side at top</div></div>
  </div>

  <h2>E. Multi-Stop Linear Gradients</h2>
  <h3>Tests about page logo pattern: 5-stop 135deg gradient</h3>
  <div class="row">
    <div><div class="test-box-wide multi-5-stop"></div><div class="label">5-stop 135deg (logo)</div></div>
    <div><div class="test-box multi-3-stop"></div><div class="label">3-stop 135deg</div></div>
    <div><div class="test-box-wide multi-rainbow"></div><div class="label">6-stop rainbow</div></div>
  </div>

  <h2>F. Diagonal Angles with Border-Radius</h2>
  <h3>Tests button styling: diagonal gradient + rounded corners</h3>
  <div class="row">
    <div><div class="test-box diag-45-r8"></div><div class="label">45deg r8</div></div>
    <div><div class="test-box diag-135-r8"></div><div class="label">135deg r8</div></div>
    <div><div class="test-box diag-135-r16"></div><div class="label">135deg r16</div></div>
    <div><div class="test-box diag-135-r50"></div><div class="label">135deg r50%</div></div>
  </div>

  <h2>G. Exact About Page Reproductions</h2>
  <h3>Isolated copies of actual about page gradients</h3>
  <div class="row">
    <div><div class="test-box-wide about-body"></div><div class="label">body background (36% diff)</div></div>
    <div><div class="test-box-wide about-logo"></div><div class="label">logo gradient (11% diff)</div></div>
    <div><div class="test-box about-btn"></div><div class="label">sponsor button (3% diff)</div></div>
  </div>

  <h2>H. Layered Gradients</h2>
  <h3>Tests multiple gradient layers composited</h3>
  <div class="row">
    <div><div class="test-box layered-1"></div><div class="label">spotlight + linear</div></div>
    <div><div class="test-box layered-2"></div><div class="label">ellipse + linear</div></div>
  </div>
</body>
</html>
```

---

## Phase 2: Testing Protocol

### 2.1 Baseline Capture

```bash
# Generate Chrome baseline for new test
python3 scripts/generate_baselines.py --case gpu-gradient-regression

# Run CPU test (no GPU gradients)
python3 scripts/parity_test.py --test gpu-gradient-regression

# Run GPU test
RUSTKIT_GPU_GRADIENTS=1 python3 scripts/parity_test.py --test gpu-gradient-regression
```

### 2.2 Per-Section Comparison

For each section (A-H), isolate and run tests:

```bash
# Create section-specific test files
# e.g., gpu-gradient-regression-A.html with only Section A

# Compare CPU vs GPU for each section
for section in A B C D E F G H; do
  echo "=== Section $section ==="
  echo "CPU:"
  python3 scripts/parity_test.py --test gpu-gradient-regression-$section 2>&1 | grep "%"
  echo "GPU:"
  RUSTKIT_GPU_GRADIENTS=1 python3 scripts/parity_test.py --test gpu-gradient-regression-$section 2>&1 | grep "%"
done
```

### 2.3 Pixel-Level Diff Analysis

For failing sections, analyze the diff images:

```bash
# View diff images
open parity-baseline/diffs/gpu-gradient-regression/run-1/diff.png
open parity-baseline/diffs/gpu-gradient-regression/run-1/heatmap.png

# Check attribution
cat parity-baseline/diffs/gpu-gradient-regression/run-1/attribution.json | jq '.topContributors'
```

---

## Phase 3: Hypothesis Testing

Based on suspected issues:

### Hypothesis 1: "at top" Position Calculation

**Test:** Compare `radial-gradient(ellipse at top, ...)` CPU vs GPU
**Expected:** Ellipse center and sizing differ

**Investigation:**
1. Add debug logging to `calculate_radial_radii()` for "at top" case
2. Compare center position passed to shader vs CPU center
3. Verify ellipse rx/ry calculations

```rust
// Add to render_radial_gradient_inline when center.1 == 0.0 (at top)
eprintln!("RADIAL at top: center=({}, {}), rx={}, ry={}", center.0, center.1, rx, ry);
```

### Hypothesis 2: Transparent Color Interpolation

**Test:** Compare `linear-gradient(90deg, red, transparent)` CPU vs GPU
**Expected:** Different handling of `transparent` (rgba(0,0,0,0))

**Investigation:**
1. Check if `transparent` is parsed as `rgba(0,0,0,0)` or inherits hue
2. Compare premultiplied alpha interpolation
3. CSS spec says `transparent` = `rgba(0,0,0,0)` in legacy color space

```wgsl
// In shader, add debug for transparent handling
if (s0.a < 0.01 || s1.a < 0.01) {
    // Debug: log that we're interpolating with transparent
}
```

### Hypothesis 3: sRGB vs Linear Color Space

**Test:** Compare multi-stop gradient midpoint colors
**Expected:** Gamma correction causes different midpoint colors

**Investigation:**
1. CPU uses `ColorF32::lerp` - check if it's linear or sRGB
2. GPU shader uses `mix()` - this is linear interpolation
3. CSS Images Level 4 recommends oklab/oklch for better interpolation

```rust
// Check CPU interpolation in ColorF32::lerp
// Add gamma curve comparison
let srgb_mid = lerp_srgb(color1, color2, 0.5);
let linear_mid = lerp_linear(color1, color2, 0.5);
println!("sRGB mid: {:?}, Linear mid: {:?}", srgb_mid, linear_mid);
```

### Hypothesis 4: Ellipse Aspect Ratio at Edges

**Test:** `radial-gradient(ellipse at top, ...)` vs `radial-gradient(ellipse at center, ...)`
**Expected:** Different aspect ratio calculations

**Investigation:**
1. For "at top", cy = 0, so dist_top = 0
2. FarthestCorner should reach bottom corners
3. Ellipse aspect ratio: rx = dist to side corner, ry = full height

```rust
// In calculate_radial_radii for FarthestCorner
// When center is at top edge:
// - Corner distances are to bottom-left and bottom-right
// - Need to maintain proper ellipse aspect ratio
```

---

## Phase 4: Fix Implementation Strategy

Based on testing results, prioritize fixes:

### Priority 1: Largest Impact First

1. **Body gradient (36%)** - Radial ellipse at top with transparent
2. **Logo gradient (11%)** - Multi-stop linear 135deg
3. **Button gradient (3%)** - Linear 135deg with border-radius

### Priority 2: Systemic vs Isolated

- **Systemic:** Color interpolation (affects all gradients)
- **Systemic:** Position calculations (affects off-center radials)
- **Isolated:** Border-radius clipping (affects rounded gradients)

### Implementation Order

1. Fix transparent color handling (most pervasive)
2. Fix ellipse sizing at non-center positions
3. Verify multi-stop interpolation
4. Check border-radius SDF precision

---

## Phase 5: Verification

After each fix:

```bash
# Run full parity suite
RUSTKIT_GPU_GRADIENTS=1 python3 scripts/parity_test.py

# Focus on gradient tests
RUSTKIT_GPU_GRADIENTS=1 python3 scripts/parity_test.py --test gradients --test gradient-backgrounds --test about

# Compare before/after
echo "Target results:"
echo "  about: ≤ 11.93% (CPU baseline)"
echo "  gradients: ≤ 9.57% (CPU baseline)"
echo "  gradient-backgrounds: ≤ 22.97% (CPU baseline)"
```

---

## Data Collection Template

| Test Section | CPU % | GPU % | Delta | Primary Issue |
|--------------|-------|-------|-------|---------------|
| A. Radial positions | | | | |
| B. Transparent interp | | | | |
| C. Semi-transparent | | | | |
| D. Ellipse sizing | | | | |
| E. Multi-stop linear | | | | |
| F. Diagonal + radius | | | | |
| G. About reproductions | | | | |
| H. Layered gradients | | | | |

---

## Quick Reference

### Environment Variables

```bash
RUSTKIT_GPU_GRADIENTS=1  # Enable GPU gradients
RUSTKIT_DEBUG_VISUAL=1   # Debug visualization
```

### Key Files

| File | Purpose |
|------|---------|
| `crates/rustkit-renderer/src/lib.rs` | GPU gradient rendering |
| `crates/rustkit-renderer/src/shaders/gradient.wgsl` | GPU shader |
| `crates/rustkit-css/src/lib.rs` | Color parsing, ColorF32::lerp |

### Shader Debug Mode

The shader has built-in debug modes (if enabled):
- `debug_mode = 1`: t-value visualization
- `debug_mode = 2`: direction vector
- `debug_mode = 3`: pixel position
- `debug_mode = 4`: border-radius coverage

---

## Test Results (2026-01-17)

### CPU vs GPU Comparison

| Test | CPU | GPU | Delta | Status |
|------|-----|-----|-------|--------|
| about | 11.93% | 16.40% | **+4.47%** | REGRESSION |
| gradient-backgrounds | 22.97% | 22.95% | -0.02% | SAME |
| backgrounds | 18.88% | 19.67% | +0.79% | SLIGHT REGRESSION |
| gradients | 9.57% | 9.28% | **-0.29%** | IMPROVEMENT |

### Key Insight

GPU gradients are **better** for basic gradients (9.28% vs 9.57%) but **worse** for the about page (16.40% vs 11.93%). The issue is isolated to specific gradient patterns.

### About Page Attribution Analysis

| Element | Diff Contribution | Gradient CSS |
|---------|------------------|--------------|
| Body background | **36.16%** | `radial-gradient(ellipse at top, rgba(6,182,212,0.15), transparent 50%)` |
| Logo (h1) | 10.64% | `linear-gradient(135deg, #0891b2 0%, #06b6d4 25%, #22d3ee 50%, #06b6d4 75%, #0891b2 100%)` |
| Sponsor button | 3.46% | `linear-gradient(135deg, var(--accent-dark), var(--accent-hover))` |

---

## Hypothesis

### Primary Hypothesis: Ellipse Sizing at Edge Positions

When radial gradient center is "at top" (cy = 0):
1. Ellipse must reach "farthest-corner" (default size keyword)
2. From top edge, corners are bottom-left and bottom-right
3. CPU and GPU may calculate different ellipse dimensions for edge positions

**Evidence:** Body gradient accounts for 36.16% of diff - by far the largest contributor.

### Secondary Hypothesis: Transparent Color Interpolation

The gradient `rgba(6,182,212,0.15) → transparent 50%` involves:
- `transparent` = `rgba(0, 0, 0, 0)` (black with zero alpha)
- Premultiplied alpha interpolation pulls colors toward black
- This causes color shift in the middle of the gradient

**CSS spec note:** Modern CSS recommends interpolating transparent as `rgba(same_hue, 0)` to avoid this issue, but legacy behavior is `rgba(0,0,0,0)`.

### Tertiary Hypothesis: Multi-Stop Diagonal Gradients

The logo (10.64% diff) uses 5 stops at non-uniform positions:
- 0%, 25%, 50%, 75%, 100%
- 135deg angle
- May have stop position normalization differences

---

## Recommended Fix Order

### Phase 1: Radial Gradient Edge Position (Highest Impact)

1. Add debug logging to `calculate_radial_radii()` for edge positions
2. Compare CPU vs GPU radius calculations for "at top", "at left", etc.
3. Fix any discrepancies in farthest-corner calculation when center is on edge

```rust
// In calculate_radial_radii, add debugging:
if center.1 == 0.0 || center.1 == 1.0 {
    eprintln!("EDGE RADIAL: center=({:.2}, {:.2}), rx={:.2}, ry={:.2}",
              center.0, center.1, rx, ry);
}
```

### Phase 2: Transparent Color Handling (Medium Impact)

1. Check how `transparent` keyword is parsed (should be `rgba(0,0,0,0)`)
2. Verify premultiplied alpha interpolation matches between CPU and GPU
3. Consider modern CSS "same hue" transparent handling as enhancement

### Phase 3: Multi-Stop Position Normalization (Lower Impact)

1. Verify stop positions are normalized identically in CPU and GPU paths
2. Check for floating point precision differences in position calculations

---

## Next Steps

1. **Immediate:** Add radial gradient edge position debugging
2. **Test:** Create minimal reproduction of "ellipse at top" case
3. **Fix:** Address radius calculation for edge-positioned radial gradients
4. **Verify:** Re-run parity tests after each fix
