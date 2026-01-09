# Visual Smoke Test Guide

This guide explains how to run visual smoke tests to verify RustKit rendering parity with Chromium. These tests have been instrumental in improving pixel parity from ~19% to ~40%+ and continue to be refined.

## Prerequisites

1. **Build the project**:
   ```bash
   cd /Users/petecopeland/Repos/hiwave-macos
   cargo build --release
   ```

2. **Install Node.js dependencies** (for Chromium baseline capture):
   ```bash
   cd tools/websuite-baseline
   npm install
   ```

3. **Ensure Playwright browsers are installed**:
   ```bash
   npx playwright install chromium
   ```

---

## Quick Start: Run All Tests

```bash
# Full parity baseline (captures all fixtures + built-ins, generates report)
python3 scripts/parity_baseline.py

# Fast rerun of top N worst cases
python3 scripts/parity_rerun.py --top 5

# Run specific case by ID
python3 scripts/parity_rerun.py --case typography
```

---

## Test Categories

### 1. Fixture-Based Tests

Individual HTML fixtures that test specific CSS features.

#### Transform Tests
```bash
# Test 2D transforms (translate, scale, rotate, skew)
./target/release/hiwave-smoke --html-file fixtures/transform-micro/translate.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/transform-micro/scale.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/transform-micro/rotate.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/transform-micro/origin.html --width 800 --height 600
```

#### Sizing Tests (min/max/clamp)
```bash
./target/release/hiwave-smoke --html-file fixtures/minmaxclamp.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/sizing-micro/percent-width.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/sizing-micro/min-height.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/sizing-micro/flex-width.html --width 800 --height 600
```

#### Pseudo-element Tests
```bash
./target/release/hiwave-smoke --html-file fixtures/pseudo-elements.html --width 800 --height 600
```

#### Typography Tests
```bash
./target/release/hiwave-smoke --html-file fixtures/typography.html --width 800 --height 600
```

#### Layout Tests
```bash
./target/release/hiwave-smoke --html-file fixtures/layout_comprehensive.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/inline-wrapper.html --width 800 --height 600
```

#### Form Control Tests
```bash
./target/release/hiwave-smoke --html-file fixtures/forms_basic.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/interactive.html --width 800 --height 600
```

#### Gradient Tests
```bash
./target/release/hiwave-smoke --html-file fixtures/gradient_basic.html --width 800 --height 600
```

#### Shadow and Border Tests
```bash
./target/release/hiwave-smoke --html-file fixtures/shadow_basic.html --width 800 --height 600
./target/release/hiwave-smoke --html-file fixtures/borders_basic.html --width 800 --height 600
```

### 2. Websuite Tests

Comprehensive test cases in `websuite/cases/`:

```bash
# Run all websuite captures
./scripts/websuite_capture.sh

# Run individual websuite case
./target/release/hiwave-smoke --html-file websuite/cases/article-typography/index.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file websuite/cases/card-grid/index.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file websuite/cases/form-elements/index.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file websuite/cases/gradient-backgrounds/index.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file websuite/cases/flex-positioning/index.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file websuite/cases/css-selectors/index.html --width 1280 --height 800
```

### 3. Built-in Page Tests

Test the actual HiWave built-in pages:

```bash
# Capture built-in pages
./scripts/builtins_capture.sh

# Individual built-ins
./target/release/hiwave-smoke --html-file crates/hiwave-app/src/ui/new_tab.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file crates/hiwave-app/src/ui/settings.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file crates/hiwave-app/src/ui/shelf.html --width 1280 --height 800
./target/release/hiwave-smoke --html-file crates/hiwave-app/src/ui/about.html --width 1280 --height 800
```

---

## Frame Capture and Comparison

### Capture a Frame

```bash
# Capture with frame output
./target/release/hiwave-smoke \
  --html-file fixtures/typography.html \
  --width 800 \
  --height 600 \
  2>&1 | grep -i "frame\|capture"

# The frame is saved to: /tmp/rustkit_frame.ppm
```

### Generate Chromium Baseline

```bash
# Extract Chromium oracle (screenshot + layout data)
node tools/websuite-baseline/extract_oracle.js \
  fixtures/typography.html \
  800 600 \
  /tmp/chromium_baseline.png \
  /tmp/chromium_layout.json
```

### Compare Frames

```bash
# Pixel comparison with AA tolerance
node tools/websuite-baseline/compare_pixels.js \
  /tmp/rustkit_frame.ppm \
  /tmp/chromium_baseline.png \
  /tmp/diff.png

# Layout comparison
python3 scripts/compare_layouts.py \
  /tmp/rustkit_layout.json \
  /tmp/chromium_layout.json \
  --output /tmp/layout_comparison.json
```

---

## Parity Gate Workflow

The full parity gate orchestrates all steps:

```bash
# Run parity gate for a single fixture
./scripts/parity_gate.sh fixtures/typography.html typography

# This will:
# 1. Build hiwave-smoke (if needed)
# 2. Capture RustKit frame
# 3. Capture Chromium baseline at same resolution
# 4. Compare pixels and generate diff
# 5. Export layout JSON and compare
# 6. Generate failure packet if mismatch detected
```

### Interpreting Results

The parity gate outputs:
- **Match percentage**: Pixel match rate (target: >95%)
- **Layout issues**: Boxes with incorrect position/size
- **Diff image**: Visual diff highlighting mismatches (red = different)

---

## Baseline Capture and Reporting

### Full Baseline Run

```bash
# Capture baseline for all fixtures and built-ins
python3 scripts/parity_baseline.py

# Output:
# - parity-results/baseline_report.json
# - parity-results/*/frame.ppm (RustKit captures)
# - parity-results/*/baseline.png (Chromium baselines)
# - parity-results/*/diff.png (visual diffs)
# - parity-results/*/layout.json (layout data)
```

### Baseline Report Format

```json
{
  "timestamp": "2026-01-06T...",
  "overall_parity": 0.42,
  "tier_scores": {
    "builtins": 0.35,
    "websuite": 0.45,
    "fixtures": 0.48
  },
  "worst_cases": [
    {"id": "chrome_rustkit", "parity": 0.12, "issues": 245},
    {"id": "shelf", "parity": 0.18, "issues": 189}
  ],
  "issue_clusters": {
    "sizing_layout": 1647,
    "paint": 234,
    "text": 156
  }
}
```

---

## Debugging Failed Tests

### 1. Visual Inspection

Open the diff image to see what's different:
```bash
open /tmp/diff.png
```

### 2. Layout Analysis

Check for layout issues:
```bash
python3 scripts/layout_oracle_gate.py /tmp/rustkit_layout.json
```

Common issues:
- **Zero-size boxes**: Element has `width: 0` or `height: 0`
- **Outside viewport**: Element positioned below/right of visible area
- **Missing boxes**: Element not in layout tree (display: none or filtered)

### 3. Enable Debug Mode

```bash
# Set debug visual mode (clears to magenta, draws green rect)
RUSTKIT_DEBUG_VISUAL=1 ./target/release/hiwave-smoke --html-file fixtures/typography.html
```

### 4. Check Display List

Add tracing to see what commands are generated:
```bash
RUST_LOG=rustkit_engine=debug ./target/release/hiwave-smoke --html-file fixtures/typography.html 2>&1 | grep -i "display\|command"
```

---

## Performance Telemetry

Capture performance metrics:

```bash
./target/release/hiwave-smoke \
  --html-file fixtures/typography.html \
  --width 800 \
  --height 600 \
  --perf-output /tmp/perf.json
```

Check against budgets:
```bash
python3 scripts/perf_check.py /tmp/perf.json
```

---

## CI Integration

For automated testing in CI:

```bash
# Run all tests with failure on regression
python3 scripts/parity_baseline.py --ci --fail-below 0.40

# Quick smoke test (top 3 worst cases only)
python3 scripts/parity_rerun.py --top 3 --ci
```

---

## Test Fixtures Reference

| Fixture | Tests | Expected Behavior |
|---------|-------|-------------------|
| `transform-micro/translate.html` | translateX, translateY, translate() | Boxes shifted by specified pixels |
| `transform-micro/scale.html` | scale, scaleX, scaleY | Boxes scaled around center |
| `transform-micro/rotate.html` | rotate(deg) | Boxes rotated around center |
| `transform-micro/origin.html` | transform-origin | Rotation pivot point changes |
| `minmaxclamp.html` | min(), max(), clamp() | Responsive widths clamped correctly |
| `pseudo-elements.html` | ::before, ::after | Decorative content rendered |
| `typography.html` | Font sizes, weights, decorations | Text styled correctly |
| `forms_basic.html` | input, button, textarea | Form controls rendered |
| `gradient_basic.html` | linear-gradient, radial-gradient | Gradient backgrounds |
| `shadow_basic.html` | box-shadow | Drop shadows rendered |
| `borders_basic.html` | border, border-radius | Borders and rounded corners |

---

## Troubleshooting

### "GPU not found" errors
Tests gracefully skip on systems without GPU. This is expected in headless CI.

### Frame capture shows blank
1. Check if HTML loaded: look for "html_load" in perf output
2. Verify display list has commands: enable RUST_LOG
3. Check viewport size matches content

### Chromium baseline capture fails
1. Ensure Playwright is installed: `npx playwright install chromium`
2. Check file path is absolute or relative to project root
3. Verify HTML file exists and is valid

### Large pixel diff percentage
1. Check if it's a text rendering difference (font fallback)
2. Look for color space issues (sRGB vs linear)
3. Verify transforms are being applied correctly

---

## Next Steps for Improving Parity

Based on current baseline analysis:

1. **Sizing/Layout** (highest impact): Fix zero-size boxes in flex containers
2. **Text Rendering**: Improve font fallback and glyph positioning
3. **Transforms**: Verify 3D transforms and perspective (if needed)
4. **Pseudo-elements**: Ensure positioned pseudo-elements work correctly
5. **Gradients**: Implement full gradient text masking

Run `python3 scripts/parity_baseline.py` regularly to track progress.



