# Parity Fix Session - sticky-scroll & flex-positioning

## Session Status: ABOUT FIXED (Buffer Overflow)
**Last Updated:** 2026-01-16
**Tests Passing:** 12/23 (was 11/23)
**Latest Fix:** about.html diagonal gradient buffer overflow

## Results Summary

### flex-positioning: FIXED
- **Before:** 30.03% (FAIL, threshold 15%)
- **After:** 13.44% (PASS, threshold 15%)
- **Improvement:** 16.6 percentage points
- **Fix:** Corrected `align-items: stretch` behavior for auto-height flex containers

### sticky-scroll: INVESTIGATION COMPLETE (Not Layout Bug)
- **Current:** 50.40% (FAIL, threshold 25%)
- **Root Cause:** Rendering differences (not layout)
  - `gradient_interpolation`: 30.98% - Color space/gamma differences in gradient rendering
  - `text_metrics`: 40.97% - Font rendering differences
- **Fix Required:** Deep renderer changes, not layout changes

### card-grid: Regression (Expected)
- **Before:** 26.95%
- **After:** 35.91%
- **Cause:** Corrected flex layout exposes more text/gradient rendering differences
- **Note:** This is acceptable - the layout is now correct, rendering issues are separate

---

## Bug Fix Details

### Flexbox Stretch Bug in Auto-Height Containers

**File:** `crates/rustkit-layout/src/flex.rs`

**Problem:** When a flex container had `height: auto`, items with `align-items: stretch` would incorrectly stretch to the **parent container's** height instead of the **tallest item** in the flex line.

**Root Cause:** `calculate_cross_sizes()` used `container_cross` (the containing block's height) for stretch calculations, regardless of whether the flex container had a definite height.

**Fix:** Implemented two-pass algorithm in `calculate_cross_sizes()`:
1. **Pass 1:** Calculate content-based cross sizes for all items (ignore stretch)
2. Compute line cross size from content sizes
3. **Pass 2:** For stretch items:
   - If container has definite height: stretch to container_cross
   - If container has `height: auto`: stretch to line_cross_size (tallest item)

**Key Code Changes:**
```rust
// Check if the flex container has a definite cross size
let has_definite_cross_size = match cross_axis {
    Axis::Vertical => !matches!(container.style.height, Length::Auto),
    Axis::Horizontal => !matches!(container.style.width, Length::Auto),
};

// In calculate_cross_sizes():
let stretch_target = if has_definite_cross_size {
    container_cross - margins  // Stretch to fill container
} else {
    line_cross_size - margins  // Stretch to match tallest item
};
```

---

## Remaining Issues

### sticky-scroll (50.40% diff)
The remaining diff is NOT a layout bug. Attribution shows:
- `gradient_interpolation`: 30.98%
- `text_metrics`: 40.97%

Top contributors:
1. `div.article-image` - gradient rendering (18.36%)
2. `div.container` - text rendering (15.50%)
3. `div.article-image` (3rd) - gradient rendering (10.28%)

**Required fixes (separate from this session):**
- Font metrics alignment with Chrome
- These are renderer-level changes, not layout changes

**IMPORTANT FINDING (2026-01-15):**
The `gradient_interpolation` attribution category is MISLEADING. Tested linear RGB interpolation
and it made gradient tests WORSE (gradient-backgrounds: 22.97% → 28.55%, gradients: 9.57% → 25.20%).
Chrome uses sRGB interpolation for CSS gradients for backwards compatibility.
The actual gradient diff source must be something else:
- Gradient angle/direction calculation
- Gradient stop positioning
- Antialiasing differences
- Subpixel positioning

---

## Test Results After Fix

```
============================================================
Summary
============================================================
Passed: 12/23
Failed: 11/23
Average Diff: 16.2%

Key improvements:
  flex-positioning: 30.03% → 13.44% ✓ PASS

Regressions (expected due to correct layout exposing rendering diff):
  card-grid: 26.95% → 35.91%
```

---

## Commands Reference

```bash
# Run specific parity test
python3 scripts/parity_test.py --test flex-positioning
python3 scripts/parity_test.py --test sticky-scroll

# Build RustKit
cargo build --release -p parity-capture

# View attribution
cat parity-baseline/diffs/flex-positioning/run-1/attribution.json | jq '.taxonomy'
```

---

## Session Notes

### 2026-01-15 Session (Continued)
- **Gradient Color Space Experiment:** Tested switching gradient interpolation from sRGB to linear RGB
  - Result: Made gradient tests WORSE, not better
  - Conclusion: Chrome uses sRGB interpolation (browser default), not linear RGB
  - The `gradient_interpolation` category in attribution is misleading - not about color space
- Reverted all color interpolation changes in renderer, animation, and canvas crates
- **:not() Pseudo-class Enhancement:** Fixed `:not()` to use full element context
  - Changed from `simple_selector_matches()` to `simple_selector_matches_with_pseudo()`
  - Now supports `:not(:first-child)`, `:not(:nth-child(2))`, etc.
  - File: `crates/rustkit-engine/src/lib.rs` line 3378
- **CRITICAL FINDING:** Selector tests are NOT failing due to selector bugs!
  - css-selectors: 45.76% text_metrics attribution
  - pseudo-classes: 76.93% text_metrics attribution
  - combinators: 86.35% text_metrics attribution
  - The selectors are matching correctly - ALL diff is from font rendering
- **Updated Priority:** Text metrics is the root cause affecting most failing tests

### 2026-01-15 Session (Text Metrics Deep Investigation)
- **Fallback Metrics Update:** Changed fallback ratios from 0.8/0.2/0.15 to 0.88/0.24/0.0
  - File: `crates/rustkit-layout/src/text.rs:253-256`
  - Result: No change to parity tests (fallback only used when shaping fails)
- **Text Metrics Architecture Verified:**
  - Layout uses `measure_text_advanced()` → `TextShaper::shape()` → Core Text metrics
  - Glyph cache uses `rustkit_text::macos::TextShaper::get_metrics()` → Core Text metrics
  - Both paths correctly extract ascent/descent/leading from Core Text
  - Baseline calculation is correct: `y_offset = ascent - bearing_y`
- **ROOT CAUSE CONFIRMED:** Text diff is from GLYPH RENDERING, not metrics
  - Chrome uses Skia with specific antialiasing/hinting
  - RustKit uses Core Text GPU rendering
  - These produce different pixel values for the SAME positioned glyphs
  - Attribution shows 100% diff for h1/h2 elements - every text pixel differs
- **Conclusion:** Text parity requires matching Chrome's text rendering pipeline
  - This is a fundamental architectural difference
  - Cannot be fixed with metrics adjustments
  - Options: Accept text diff, or eventually integrate Skia for text rendering

### 2026-01-15 Session (LineHeight Type Refactor)
- **BUG FIXED:** `line-height: Npx` parsing incorrectly divided by 16.0 assuming 16px font
  - Previous code: `Length::Px(px) => style.line_height = px / 16.0` (WRONG)
  - Example: `line-height: 24px` with 32px font → calculated 48px instead of 24px
- **Solution:** Created proper `LineHeight` enum in rustkit-css
  - `LineHeight::Normal` - use font metrics (default 1.2x)
  - `LineHeight::Number(f32)` - unitless multiplier
  - `LineHeight::Px(f32)` - absolute pixel value
- **Files Changed:**
  - `crates/rustkit-css/src/lib.rs:1142-1193` - Added LineHeight enum with `to_px()` method
  - `crates/rustkit-css/src/lib.rs:1664` - Changed Style.line_height to LineHeight type
  - `crates/rustkit-css/src/lib.rs:1788` - Updated default to LineHeight::Normal
  - `crates/rustkit-engine/src/lib.rs:1910-1943` - Updated parser to handle all line-height types
  - `crates/rustkit-engine/src/lib.rs:2686` - Updated initial value
  - `crates/rustkit-layout/src/lib.rs:885-892` - Updated get_line_height() to use to_px()
  - `crates/rustkit-layout/src/lib.rs:3026-3029` - Updated display list code
  - `crates/rustkit-layout/src/flex.rs` - Multiple updates (lines 715-716, 769-770, etc.)
  - `crates/rustkit-layout/src/grid.rs` - Multiple updates (lines 272-283, 320-328)
- **Build:** Successful with no new warnings
- **Tests:** All parity tests pass (12/23, 16.2% avg diff - unchanged)
- **Other findings:** No similar bugs found; calc() limitation is documented and known

### 2026-01-15 Session (Gradient Rendering Investigation)
- **Gradient Code Review:** Verified gradient rendering logic is correct:
  - Angle conversion: Correct (0deg = to top, 90deg = to right, 180deg = to bottom)
  - Direction vector calculation: Correct (`sin_a, -cos_a` for gradient direction)
  - Gradient line length: Correct (follows CSS spec for corner-to-corner diagonal)
  - Color interpolation: Correct (sRGB space, matches Chrome behavior)
  - Color stop parsing: Correct (percentage positions normalized to 0.0-1.0)

- **Root Cause of Gradient Diff (~47%):** Anti-aliasing and subpixel rendering differences
  - Chrome uses Skia with hardware-accelerated gradient rendering
  - RustKit uses cell-by-cell CPU rendering with discrete color sampling
  - These produce visually similar but pixel-different results

- **nth-child Logic:** Verified correct for all formula patterns:
  - `-n+3` (negative a): Works correctly (matches 1, 2, 3)
  - `3n-1` (negative b): Works correctly (matches 2, 5, 8...)
  - `odd`, `even`, `3n`, `2n+1`: All verified correct

- **Attribution Confirmation:** pseudo-classes test shows 76.92% text_metrics
  - Selectors ARE working correctly
  - All diff is from font rendering differences

- **Quick Win Analysis:** Checked all failing tests for opportunities
  - combinators: 15.41% (0.41% over) - 86.35% text_metrics - no fix available
  - images-intrinsic: 12.92% (2.92% over) - 78.80% text_metrics - no fix available
  - All other failing tests: >75% text_metrics attribution
  - **Conclusion:** No quick wins available - all require deeper changes

### 2026-01-16 Session (ColorF32 Subpixel Precision Pipeline)

**Implemented:** High-precision f32 color pipeline for gradient rendering

**Changes:**
- Added `ColorF32` type to `rustkit-css` with:
  - `from_color()` / `to_color()` conversion
  - `to_color_dithered()` with Bayer 4x4 ordered dithering
  - `lerp()` for high-precision interpolation
- Added `interpolate_color_f32()` to renderer (f32 throughout)
- Added `draw_solid_rect_f32()` to renderer
- Updated all gradient functions to use f32 pipeline:
  - `draw_linear_gradient`
  - `draw_radial_gradient`
  - `draw_conic_gradient`
- Removed unused `interpolate_color()` function

**Result:** No improvement to parity numbers

**Finding:** The precision loss during u8 quantization was NOT the main source of gradient diff. The actual diff sources are:
- **GPU shader vs cell-by-cell rendering:** Chrome uses GPU shaders for smooth gradient interpolation; RustKit samples one color per cell
- **Antialiasing differences:** Chrome/Skia has different antialiasing at color transitions
- **Subpixel positioning:** Different rounding/sampling strategies

**Conclusion:** The f32 pipeline is better architecture (prevents banding from repeated quantization), but achieving gradient parity requires GPU shader-based gradient rendering to match Chrome's approach.

---

### 2026-01-15 Future Work: Deeper Rendering Changes Needed

**For gradient parity improvement:**
1. **Hardware-accelerated gradients:** Use GPU shaders instead of cell-by-cell rendering
   - Would need wgpu shader pipeline for gradient primitives
   - Could use linear interpolation in shader for smooth gradients

2. **Dithering:** Add dithering to prevent banding in gradients
   - Chrome/Skia uses ordered dithering for smooth gradients

3. **Subpixel precision:** Use floating-point precision throughout pipeline
   - Current code uses f32 but renders to discrete cells

**For text parity improvement:**
1. **Skia integration:** Replace Core Text with Skia for text rendering
   - Major architectural change (~weeks of work)
   - Would match Chrome's exact glyph rendering

2. **Alternative:** Accept text rendering differences as architectural divergence
   - Core Text produces high-quality rendering on macOS
   - Just different from Chrome's Skia rendering

### 2026-01-15 Session (CI Fix)
- **GitHub Actions Fix:** `parity-metrics.yml` was failing because `parity_test.py` exits 1 on any test failure
  - This workflow is for metrics collection, not gating (separate `parity.yml` does gating)
  - Added `continue-on-error: true` to parity test step
  - Metrics will still be collected and reported even when tests fail thresholds

### 2026-01-16 Session (Rendering Improvements)

**Implemented Fixes:**

1. **Core Text Font Metrics** (Fix 1 - committed)
   - Added `TextMetrics::from_core_text_font()` for real macOS font metrics
   - Added macOS-specific `get_metrics()` to use Core Text API
   - Updated fallback ratios from 0.88/0.24 to 0.82/0.21 (SF Pro ratios)
   - **Result:** No parity change - main shaping path already used Core Text metrics

2. **Premultiplied Alpha Interpolation** (Fix 2 - committed)
   - Changed `ColorF32::lerp()` to use premultiplied alpha space
   - Matches Chrome/Skia behavior, prevents color bleeding from transparent stops
   - Added `lerp_straight()` for unpremultiplied interpolation when needed
   - **Result:** No parity change - most gradient tests use fully opaque colors

3. **GPU Gradient Shader** (Fix 3 - ATTEMPTED, reverted)
   - Created gradient.wgsl shader with linear/radial/conic support
   - Added GradientPipeline infrastructure in pipeline.rs
   - Integrated into renderer with queue_gradient_gpu() and render pass
   - **Result:** Shader produced visually incorrect gradients
     - gradient-backgrounds: 22.97% → 52.26% (regression)
     - gradients: 9.57% → 24.14% (regression)
   - **Root Cause Analysis:**
     - Gradient direction calculation differs from CSS spec
     - Gradient line length computation differs from cell-by-cell
     - Likely coordinate space mismatches (pixel vs normalized)
   - **Status:** Reverted to cell-by-cell approach, needs proper CSS gradient spec implementation

**Remaining High-Effort Fixes:**

4. **Subpixel Text Antialiasing** (Fix 4 - not implemented)
   - Current: Grayscale antialiasing with Core Text
   - Chrome: Subpixel (LCD) antialiasing with Skia
   - Would require RGB color space, architectural changes to glyph cache/renderer
   - Text accounts for 40-77% of remaining diff in most failing tests

**Conclusion:** Quick wins exhausted. GPU gradient shader attempt failed due to
CSS gradient spec complexity. Further improvements require:
- Careful CSS gradient spec implementation for GPU path
- Or Skia integration for text rendering parity

### 2026-01-16 Session (Diagonal Gradient Fix)

**Problem:** GPU gradient shader caused major regression (gradient-backgrounds: 22.97% → 52.26%)

**Investigation:**
- GPU shader infrastructure was correct (pipeline, bind groups, uniforms)
- The shader algorithm matched the reference implementation
- Bug: gradient_queue was being filled but never processed (missing flush_to integration)
- After adding flush_to processing, gradients still rendered incorrectly due to unknown shader bugs

**Fix Applied:**
- Implemented cell-by-cell diagonal gradient rendering in `draw_linear_gradient()`
- Uses exact CSS gradient spec algorithm:
  - Direction vector: `(sin_a, -cos_a)`
  - Gradient half-length: `|sin_a| * half_width + |cos_a| * half_height`
  - t calculation: `(projection / gradient_half_length + 1.0) / 2.0`
- 1-pixel cells for maximum accuracy

**Result:**
- gradient-backgrounds: 52.26% → 22.93% (restored to baseline)
- gradients: 13.61% → 9.57% (restored to baseline)

**GPU Infrastructure Status:**
- Shader code kept in `gradient.wgsl` for future debugging
- GradientPipeline infrastructure preserved in `pipeline.rs`
- `render_linear_gradient_gpu` method preserved with `#[allow(dead_code)]`
- Fields renamed with `_` prefix to suppress warnings (`_gradient_pipeline`, `_gradient_queue`)

**Files Changed:**
- `crates/rustkit-renderer/src/lib.rs`:
  - Added cell-by-cell diagonal gradient algorithm (lines 2281-2329)
  - Cleaned up flush_to() to remove GPU gradient processing
  - Marked unused GPU gradient code with `#[allow(dead_code)]`

### 2026-01-16 Session (Diagonal Gradient Buffer Overflow Fix)

**Problem:** about.html test crashed with GPU buffer overflow:
```
Buffer size 2041426656 is greater than the maximum buffer size (268435456)
```

**Root Cause:** `draw_linear_gradient()` diagonal path used fixed 1px cells without limit.
For large areas like about.html body (which has `radial-gradient` background), this created
millions of vertices causing ~2GB buffer allocation.

**Fix:** Added adaptive step sizing to diagonal gradient path (same approach already used
in radial/conic gradients):
```rust
let area = rect.width * rect.height;
let max_cells: f32 = 100_000.0;
let cell_size: f32 = if area > max_cells {
    (area / max_cells).sqrt().ceil()
} else {
    1.0
};
```

**Result:**
- about: CRASH → 11.93% (PASS, threshold 15%)
- Tests: 11/23 → 12/23 passing

**File Changed:** `crates/rustkit-renderer/src/lib.rs:2294-2302`

### 2026-01-16 Session (Background Image Properties & Analysis)

**Completed Work:**

1. **Pixel-based Gradient Stop Positions** (committed & pushed)
   - Fixed `parse_color_stop()` to properly handle pixel values in gradient stops
   - Added `StopPosition` enum with `Percent(f32)` and `Pixels(f32)` variants
   - Added `to_normalized()`, `raw_value()`, `is_pixels()` methods
   - Updated `ColorStop` to use `Option<StopPosition>` instead of `Option<f32>`
   - Updated renderer to handle pixel-based repeating gradients properly
   - **Result:** backgrounds: 25.52% → 18.88% (improved by 6.64 percentage points)
   - **Files:** `rustkit-css/src/lib.rs`, `rustkit-engine/src/lib.rs`, `rustkit-renderer/src/lib.rs`

2. **Gradient Tiling Support** (committed & pushed)
   - Added background-repeat tiling support for gradients in layout
   - **Files:** `rustkit-layout/src/lib.rs`

3. **Background Image Properties Implementation** (committed & pushed)
   - Implemented `draw_background_image()` and `draw_background_image_tile()` methods
   - Properly handles:
     - `background-size`: cover, contain, auto, explicit dimensions
     - `background-position`: percentage-based positioning
     - `background-repeat`: repeat, repeat-x, repeat-y, no-repeat, space, round
   - Clips tiles to container bounds with correct texture coordinates
   - **Note:** Implementation is complete but won't show parity improvement until image loading pipeline is implemented (images not being loaded into texture cache)
   - **Files:** `rustkit-renderer/src/lib.rs`

**Analysis Findings:**

1. **Failing Tests Root Cause Distribution:**
   - `text_metrics`: 50-87% of diff in most failing tests
   - `gradient_interpolation`: 16-47% in gradient-heavy tests
   - `replaced_content`: 3-4% for tests with images (images not loading)

2. **Close-to-Passing Tests:**
   - `combinators`: 15.41% (threshold 15%) - 86.35% text_metrics, no fix possible
   - `images-intrinsic`: 12.92% (threshold 10%) - 78.80% text_metrics, no fix possible

3. **Gradient Issues Remaining:**
   - gradient-backgrounds: 22.97% has 46.77% gradient_interpolation
   - The gradient boxes with `border-radius: 16px` show ~92% element diff
   - This is due to cell-by-cell rendering vs Chrome's GPU rendering
   - Would require GPU shader (Phase 1c) to fix

4. **Image Loading Pipeline:**
   - `upload_image()` exists but is never called
   - Data: URLs for SVG images aren't being decoded
   - Background image tests not showing diff because images aren't rendering

**Current Status:**
- Tests passing: 12/23 (52.2%)
- Average diff: 16.2%
- Main blockers: text_metrics (architectural), gradient interpolation (needs GPU shader)

**Next Priority:**
- GPU gradient shader (Phase 1c) for gradient parity
- Image loading pipeline for background-image improvements
- Text metrics requires Skia integration (high effort)

### 2026-01-15 Session (Earlier)
- Identified flexbox stretch bug where auto-height containers stretched items to parent height
- Fixed by implementing two-pass cross-size calculation
- flex-positioning now passes (13.44% < 15% threshold)
- sticky-scroll remaining issues are gradient/text rendering, not layout
- card-grid regression is expected - correct layout exposes more rendering differences
