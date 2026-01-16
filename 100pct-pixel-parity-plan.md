# 100% Pixel Parity Plan: The Definitive Guide

**Goal**: Achieve and maintain 100% pixel-perfect parity with Chrome rendering
**Stretch Target**: 0% pixel diff on all test cases with deterministic, reproducible results
**Status**: Draft v1.0

---

## Executive Summary

This plan supersedes the 99% parity plan by addressing critical gaps in:
1. **Determinism** - Eliminating all sources of non-deterministic rendering
2. **Test Coverage** - Comprehensive micro-tests for every CSS feature
3. **Attribution** - Per-element diff attribution for actionable debugging
4. **Stability** - Multi-run verification with statistical confidence
5. **Regression Prevention** - Zero-tolerance regression gates
6. **Live Proofing** - Interactive side-by-side comparison system

---

## Part 1: Critical Gaps in Current Plan

### Gap 1: Loose Thresholds Hide Problems
**Current**: `text_rendering: 20%`, `sticky_scroll: 25%` tolerance
**Problem**: These thresholds mask real rendering bugs
**Fix**: Progressive threshold tightening with per-feature gates

### Gap 2: Non-Deterministic Capture Environment
**Current**: `font_rendering: "default"`, no animation freeze, no color profile lock
**Problem**: Captures vary between runs, machines, and Chrome versions
**Fix**: Fully deterministic capture environment specification

### Gap 3: Missing Element Attribution
**Current**: Pixel diff only tells you "38% different"
**Problem**: No way to know which element/property is wrong
**Fix**: Overlay layout-rects on diff heatmap, compute per-element diff contribution

### Gap 4: No Multi-Run Stability Verification
**Current**: Single capture per test
**Problem**: Flaky results from timing, GPU state, font loading
**Fix**: 3-run minimum with max-min stability requirement

### Gap 5: Incomplete Test Matrix
**Current**: 14 micro-tests, ~8 websuite cases
**Problem**: Missing coverage for shadows, filters, grid, multi-column, stacking
**Fix**: Comprehensive 60+ micro-test battery

### Gap 6: No Live Proofing Workflow
**Current**: Static PNG diffs only
**Problem**: Hard to interactively debug, compare behaviors, test fixes
**Fix**: Live comparison server with synchronized scrolling and interaction

---

## Part 2: Deterministic Capture Environment

### 2.1 Chrome Baseline Pinning

```json
{
  "chrome": {
    "version": "120.0.6099.109",
    "headless": "new",
    "flags": [
      "--disable-gpu-vsync",
      "--disable-features=RendererCodeIntegrity",
      "--force-color-profile=srgb",
      "--disable-font-subpixel-positioning",
      "--disable-lcd-text",
      "--disable-accelerated-2d-canvas",
      "--use-gl=swiftshader",
      "--disable-background-timer-throttling",
      "--disable-backgrounding-occluded-windows",
      "--disable-renderer-backgrounding"
    ]
  }
}
```

### 2.2 Font Environment Lock

```yaml
# Required for deterministic text rendering
font_stack:
  primary: "Hiwave Test Sans"  # Bundled test font (subset of Roboto)
  fallback: "Hiwave Test Serif" # Bundled test font (subset of Noto Serif)
  mono: "Hiwave Test Mono"     # Bundled test font (subset of JetBrains Mono)

font_settings:
  subpixel_positioning: false
  lcd_smoothing: false
  hinting: "none"
  size_adjust: 1.0
```

**Implementation**: Bundle 3 test fonts in `baselines/fonts/` with known metrics.
All test HTML must use explicit `font-family` from this stack.

### 2.3 Animation & Timing Freeze

```javascript
// Inject into all test pages before capture
window.__PARITY_FREEZE__ = true;

// Freeze all animations
CSS.registerProperty && CSS.registerProperty({
  name: '--animation-play-state',
  syntax: 'paused',
  inherits: true,
  initialValue: 'paused'
});

// Override Date.now() for deterministic timestamps
const FROZEN_TIME = 1704067200000; // 2024-01-01T00:00:00Z
Date.now = () => FROZEN_TIME;
Date.prototype.getTime = () => FROZEN_TIME;

// Override requestAnimationFrame to run synchronously
window.requestAnimationFrame = (cb) => cb(FROZEN_TIME);

// Disable transitions
document.documentElement.style.setProperty('--transition-duration', '0s', 'important');
```

### 2.4 Color Profile Normalization

```yaml
capture_settings:
  color_space: "srgb"
  bit_depth: 8
  premultiplied_alpha: false

  # Both Chrome and RustKit must output identical colorspace
  png_color_type: "rgba"
  png_gamma: 2.2
```

### 2.5 Viewport & DPI Lock

```yaml
viewports:
  primary:
    width: 1280
    height: 800
    dpi: 1

  secondary:
    - { width: 800, height: 600, dpi: 1 }
    - { width: 1920, height: 1080, dpi: 1 }
    - { width: 1280, height: 800, dpi: 2 }  # Retina verification

# Strict dimension matching - fail on mismatch
dimension_policy: "exact_match"  # Not "crop_to_smaller"
```

---

## Part 3: Comprehensive Test Matrix

### 3.1 Micro-Test Battery (Target: 80 tests)

#### Layout Tests (20 tests)
| ID | File | Tests |
|----|------|-------|
| L01 | `layout/block-margin-collapse.html` | Margin collapse rules |
| L02 | `layout/inline-baseline.html` | Inline element baseline alignment |
| L03 | `layout/flex-wrap.html` | Flexbox wrapping behavior |
| L04 | `layout/flex-align.html` | align-items, align-content, align-self |
| L05 | `layout/flex-grow-shrink.html` | flex-grow, flex-shrink, flex-basis |
| L06 | `layout/grid-template.html` | grid-template-rows/columns |
| L07 | `layout/grid-auto.html` | grid-auto-rows/columns/flow |
| L08 | `layout/grid-gap.html` | gap, row-gap, column-gap |
| L09 | `layout/grid-placement.html` | grid-row, grid-column, span |
| L10 | `layout/grid-align.html` | place-items, place-content, place-self |
| L11 | `layout/multi-column.html` | column-count, column-width, column-gap |
| L12 | `layout/absolute-positioning.html` | position: absolute with all anchors |
| L13 | `layout/fixed-positioning.html` | position: fixed within transforms |
| L14 | `layout/sticky-positioning.html` | position: sticky edge cases |
| L15 | `layout/float-clear.html` | float, clear, clearfix patterns |
| L16 | `layout/overflow-scroll.html` | overflow: scroll/auto behavior |
| L17 | `layout/overflow-clip.html` | overflow: clip vs hidden |
| L18 | `layout/writing-mode.html` | vertical-lr, vertical-rl, sideways |
| L19 | `layout/direction-rtl.html` | direction: rtl, unicode-bidi |
| L20 | `layout/table-layout.html` | display: table and friends |

#### Paint Tests (20 tests)
| ID | File | Tests |
|----|------|-------|
| P01 | `paint/background-layers.html` | Multiple background images/gradients |
| P02 | `paint/background-clip.html` | background-clip: border-box/padding-box/content-box/text |
| P03 | `paint/background-origin.html` | background-origin variations |
| P04 | `paint/background-attachment.html` | fixed, scroll, local |
| P05 | `paint/gradient-linear.html` | All linear-gradient syntaxes |
| P06 | `paint/gradient-radial.html` | All radial-gradient syntaxes |
| P07 | `paint/gradient-conic.html` | All conic-gradient syntaxes |
| P08 | `paint/gradient-repeating.html` | Repeating gradient variations |
| P09 | `paint/box-shadow-basic.html` | box-shadow offsets, blur, spread |
| P10 | `paint/box-shadow-inset.html` | inset shadows |
| P11 | `paint/box-shadow-multi.html` | Multiple box shadows |
| P12 | `paint/text-shadow.html` | text-shadow variations |
| P13 | `paint/border-radius.html` | All border-radius combinations |
| P14 | `paint/border-radius-ellipse.html` | Elliptical corner radii |
| P15 | `paint/border-image.html` | border-image-source/slice/repeat |
| P16 | `paint/outline.html` | outline vs border behavior |
| P17 | `paint/filter-blur.html` | filter: blur() |
| P18 | `paint/filter-effects.html` | brightness, contrast, saturate, etc. |
| P19 | `paint/backdrop-filter.html` | backdrop-filter effects |
| P20 | `paint/mix-blend-mode.html` | blend mode variations |

#### Stacking & Clipping Tests (10 tests)
| ID | File | Tests |
|----|------|-------|
| S01 | `stacking/z-index-auto.html` | z-index: auto behavior |
| S02 | `stacking/z-index-negative.html` | Negative z-index stacking |
| S03 | `stacking/stacking-context.html` | What creates stacking contexts |
| S04 | `stacking/isolation.html` | isolation: isolate |
| S05 | `stacking/opacity-stacking.html` | opacity creates stacking context |
| S06 | `clipping/clip-path-basic.html` | clip-path: circle, ellipse, polygon |
| S07 | `clipping/clip-path-inset.html` | clip-path: inset() |
| S08 | `clipping/clip-path-url.html` | clip-path: url() SVG reference |
| S09 | `clipping/mask-image.html` | mask-image, mask-size, mask-position |
| S10 | `clipping/overflow-hidden-radius.html` | overflow: hidden + border-radius |

#### Text Tests (15 tests)
| ID | File | Tests |
|----|------|-------|
| T01 | `text/font-weight-scale.html` | Font weights 100-900 |
| T02 | `text/font-style.html` | italic, oblique angles |
| T03 | `text/font-stretch.html` | Font width variations |
| T04 | `text/font-size-units.html` | px, em, rem, vw, % |
| T05 | `text/line-height.html` | normal, unitless, px, % |
| T06 | `text/letter-spacing.html` | Letter spacing positive/negative |
| T07 | `text/word-spacing.html` | Word spacing variations |
| T08 | `text/text-align.html` | left, right, center, justify |
| T09 | `text/text-decoration.html` | underline, overline, line-through |
| T10 | `text/text-decoration-style.html` | solid, dashed, dotted, wavy |
| T11 | `text/text-transform.html` | uppercase, lowercase, capitalize |
| T12 | `text/text-overflow.html` | text-overflow: ellipsis, clip |
| T13 | `text/white-space.html` | normal, nowrap, pre, pre-wrap |
| T14 | `text/word-break.html` | word-break, overflow-wrap |
| T15 | `text/vertical-align.html` | All vertical-align values |

#### Selector & Pseudo Tests (10 tests)
| ID | File | Tests |
|----|------|-------|
| X01 | `selectors/combinators.html` | >, +, ~, space combinators |
| X02 | `selectors/attribute.html` | [attr], [attr=val], [attr*=val] |
| X03 | `selectors/nth-child.html` | :nth-child, :nth-of-type variants |
| X04 | `selectors/structural.html` | :first-child, :last-child, :only-child |
| X05 | `selectors/not-is-where.html` | :not(), :is(), :where() |
| X06 | `selectors/has.html` | :has() selector |
| X07 | `pseudo/before-after.html` | ::before, ::after content |
| X08 | `pseudo/first-letter-line.html` | ::first-letter, ::first-line |
| X09 | `pseudo/selection.html` | ::selection styling |
| X10 | `pseudo/placeholder.html` | ::placeholder styling |

#### Transform Tests (5 tests)
| ID | File | Tests |
|----|------|-------|
| R01 | `transform/2d-basic.html` | translate, rotate, scale, skew |
| R02 | `transform/3d-perspective.html` | perspective, rotateX/Y/Z |
| R03 | `transform/origin.html` | transform-origin variations |
| R04 | `transform/matrix.html` | matrix(), matrix3d() |
| R05 | `transform/preserve-3d.html` | transform-style: preserve-3d |

### 3.2 Test Case Structure

Each micro-test follows this structure:

```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=800, height=600">
  <link rel="stylesheet" href="../../common/parity-reset.css">
  <title>Test: [Feature Name]</title>
  <style>
    /* Test-specific styles */
  </style>
</head>
<body data-parity-test="[test-id]">
  <header class="test-header">
    <h1>[Feature Name]</h1>
    <p class="test-meta">Testing: [specific properties]</p>
  </header>

  <main class="test-grid">
    <!-- Each test case in a labeled container -->
    <section class="test-case" data-case="1">
      <h2>Case 1: [Description]</h2>
      <div class="specimen">[Test content]</div>
      <code class="expected">Expected: [description]</code>
    </section>
    <!-- ... more cases ... -->
  </main>
</body>
</html>
```

### 3.3 Common Reset Stylesheet

```css
/* baselines/common/parity-reset.css */
*, *::before, *::after {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
  transition: none !important;
  animation: none !important;
  animation-play-state: paused !important;
}

html {
  font-family: 'Hiwave Test Sans', sans-serif;
  font-size: 16px;
  line-height: 1.5;
  color: #000;
  background: #fff;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  min-height: 100vh;
  padding: 20px;
}

.test-header {
  margin-bottom: 20px;
  padding-bottom: 10px;
  border-bottom: 1px solid #ccc;
}

.test-grid {
  display: grid;
  gap: 20px;
}

.test-case {
  padding: 10px;
  border: 1px solid #ddd;
  background: #fafafa;
}

.test-case h2 {
  font-size: 12px;
  margin-bottom: 10px;
  color: #666;
}

.specimen {
  /* The actual test content */
}

.expected {
  display: block;
  margin-top: 8px;
  font-size: 10px;
  color: #888;
}
```

---

## Part 4: Element-Level Diff Attribution

### 4.1 Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Diff Attribution Pipeline                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌──────────────────────┐ │
│  │ Pixel Diff  │───►│ Layout Rects │───►│ Element Attribution │ │
│  │ Heatmap     │    │ Overlay      │    │ Report               │ │
│  └─────────────┘    └─────────────┘    └──────────────────────┘ │
│         │                  │                      │              │
│         ▼                  ▼                      ▼              │
│  ┌─────────────┐    ┌─────────────┐    ┌──────────────────────┐ │
│  │ diff.png    │    │ overlay.png │    │ attribution.json     │ │
│  │ (red=diff)  │    │ (rects +    │    │ (per-element diff %) │ │
│  │             │    │  diff color)│    │                      │ │
│  └─────────────┘    └─────────────┘    └──────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Attribution Algorithm

```javascript
// tools/parity_oracle/attribute_diff.mjs

export function attributeDiff(diffPixels, layoutRects, options = {}) {
  const { width, height } = options;
  const attribution = [];

  // Build spatial index of layout rects
  const rectIndex = buildSpatialIndex(layoutRects);

  // Count diff pixels per element
  const elementDiffs = new Map();

  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const idx = (y * width + x) * 4;
      const r = diffPixels[idx];

      // Skip non-diff pixels (pixelmatch marks diffs in red)
      if (r < 200) continue;

      // Find all elements containing this pixel
      const elements = rectIndex.query(x, y);

      // Attribute to innermost element (smallest area)
      const innermost = elements.sort((a, b) =>
        (a.width * a.height) - (b.width * b.height)
      )[0];

      if (innermost) {
        const count = elementDiffs.get(innermost.selector) || 0;
        elementDiffs.set(innermost.selector, count + 1);
      }
    }
  }

  // Convert to sorted array with percentages
  for (const [selector, diffCount] of elementDiffs) {
    const rect = layoutRects.find(r => r.selector === selector);
    const elementArea = rect.width * rect.height;
    const diffPercent = (diffCount / elementArea) * 100;

    attribution.push({
      selector,
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      diffPixels: diffCount,
      diffPercent: diffPercent.toFixed(2),
      styles: rect.computedStyles || {},
    });
  }

  // Sort by diff contribution (highest first)
  attribution.sort((a, b) => b.diffPixels - a.diffPixels);

  return {
    totalDiffPixels: Array.from(elementDiffs.values()).reduce((a, b) => a + b, 0),
    topContributors: attribution.slice(0, 10),
    allElements: attribution,
  };
}
```

### 4.3 Attribution Report Format

```json
{
  "case_id": "paint/gradient-linear",
  "total_diff_percent": 3.24,
  "total_diff_pixels": 12960,
  "attribution": [
    {
      "rank": 1,
      "selector": ".test-case:nth-child(3) .specimen",
      "contribution_percent": 45.2,
      "diff_pixels": 5858,
      "element_diff_percent": 18.5,
      "rect": { "x": 200, "y": 150, "width": 200, "height": 100 },
      "likely_cause": "gradient_interpolation",
      "style_deltas": {
        "background": {
          "chrome": "linear-gradient(45deg, rgb(255, 0, 0), rgb(0, 0, 255))",
          "rustkit": "linear-gradient(45deg, rgb(255, 0, 0), rgb(0, 0, 254))"
        }
      }
    },
    {
      "rank": 2,
      "selector": ".test-case:nth-child(5) .specimen",
      "contribution_percent": 28.1,
      "diff_pixels": 3642,
      "element_diff_percent": 12.8,
      "rect": { "x": 200, "y": 350, "width": 200, "height": 100 },
      "likely_cause": "clip_radius",
      "style_deltas": {}
    }
  ],
  "taxonomy": {
    "gradient_interpolation": 45.2,
    "clip_radius": 28.1,
    "unknown": 26.7
  }
}
```

### 4.4 Diff Taxonomy Heuristics

```javascript
// tools/parity_oracle/classify_diff.mjs

export function classifyDiff(diffData, elementRect, computedStyles) {
  const analysis = analyzeDiffPattern(diffData, elementRect);

  // Heuristic classification
  if (analysis.isConcentratedAtCorners) {
    if (computedStyles['border-radius'] !== '0px') {
      return 'clip_radius';
    }
  }

  if (analysis.hasGradientPattern) {
    const bg = computedStyles['background'] || '';
    if (bg.includes('gradient')) {
      return 'gradient_interpolation';
    }
  }

  if (analysis.isUniformBlock) {
    const solidBg = !computedStyles['background']?.includes('gradient');
    if (solidBg) {
      return 'paint_solid';
    }
  }

  if (analysis.isNearTextBoundaries) {
    return 'text_metrics';
  }

  if (analysis.hasLargeRectShift) {
    return 'layout_shift';
  }

  const tagName = elementRect.tagName?.toLowerCase();
  if (['img', 'video', 'canvas', 'svg'].includes(tagName)) {
    return 'replaced_content';
  }

  if (['input', 'button', 'select', 'textarea'].includes(tagName)) {
    return 'form_control';
  }

  return 'unknown';
}

function analyzeDiffPattern(diffData, rect) {
  // Analyze spatial distribution of diff pixels within element bounds
  const cornerZones = extractCornerZones(diffData, rect, 10); // 10px corner radius
  const edgeZones = extractEdgeZones(diffData, rect, 2);       // 2px edge strip

  return {
    isConcentratedAtCorners: cornerZones.diffDensity > 0.5,
    isNearTextBoundaries: edgeZones.diffDensity > 0.3,
    hasGradientPattern: detectGradientPattern(diffData, rect),
    isUniformBlock: detectUniformBlock(diffData, rect),
    hasLargeRectShift: false, // Compare to expected rect
  };
}
```

---

## Part 5: Multi-Run Stability Framework

### 5.1 Stability Requirements

```yaml
stability_config:
  min_runs: 3
  max_runs: 5  # If instability detected, run up to 5

  thresholds:
    max_variance: 0.10      # Max-min diff must be < 0.10%
    cv_threshold: 0.05       # Coefficient of variation < 5%

  failure_modes:
    unstable: "variance > max_variance"
    flaky: "any run differs by > 0.5% from median"

  actions:
    on_unstable: "investigate_and_exclude"
    on_flaky: "rerun_with_delay"
```

### 5.2 Stability Analysis

```python
# scripts/parity_stability.py

def analyze_stability(runs: List[float]) -> StabilityReport:
    """Analyze multi-run stability of a test case."""
    if len(runs) < 3:
        raise ValueError("Need at least 3 runs for stability analysis")

    mean = statistics.mean(runs)
    stdev = statistics.stdev(runs)
    cv = stdev / mean if mean > 0 else 0  # Coefficient of variation

    min_val = min(runs)
    max_val = max(runs)
    variance = max_val - min_val
    median = statistics.median(runs)

    # Detect outliers (> 2 stdev from mean)
    outliers = [r for r in runs if abs(r - mean) > 2 * stdev]

    return StabilityReport(
        mean=mean,
        median=median,
        stdev=stdev,
        cv=cv,
        min=min_val,
        max=max_val,
        variance=variance,
        outliers=outliers,
        is_stable=variance < 0.10 and cv < 0.05,
        is_flaky=len(outliers) > 0,
    )
```

### 5.3 Capture Orchestration

```python
# scripts/parity_multi_run.py

async def capture_with_stability(case_id: str, config: CaptureConfig) -> StableResult:
    """Run multiple captures and verify stability."""
    results = []

    for run in range(config.min_runs):
        # Clean state between runs
        await reset_gpu_state()
        await clear_font_cache()
        await wait_for_idle(timeout_ms=1000)

        # Capture
        result = await capture_single(case_id, config)
        results.append(result.diff_percent)

        # Early exit if clearly stable
        if run >= 2:
            stability = analyze_stability(results)
            if stability.is_stable and stability.variance < 0.05:
                break

    # Final stability check
    stability = analyze_stability(results)

    if not stability.is_stable:
        # Try additional runs with delays
        for extra_run in range(config.max_runs - len(results)):
            await asyncio.sleep(2.0)  # 2 second delay
            result = await capture_single(case_id, config)
            results.append(result.diff_percent)

        stability = analyze_stability(results)

    return StableResult(
        case_id=case_id,
        runs=results,
        final_diff=stability.median,  # Use median for final result
        stability=stability,
        is_valid=stability.is_stable,
    )
```

---

## Part 6: Zero-Tolerance Regression Gates

### 6.1 Gate Levels

```yaml
gates:
  # Level 1: Per-commit (fast, < 2 minutes)
  commit:
    scope: "micro/critical"  # ~20 critical tests
    max_diff: 1.0            # 1% max diff
    regression_budget: 0.0   # Zero regression allowed
    timeout: 120s

  # Level 2: PR merge (thorough, < 10 minutes)
  pr_merge:
    scope: "micro/*,websuite/*"
    max_diff: 0.5
    regression_budget: 0.1   # 0.1% regression budget per case
    stability: required
    timeout: 600s

  # Level 3: Nightly (comprehensive, < 60 minutes)
  nightly:
    scope: "all"
    viewports: ["800x600", "1280x800", "1920x1080"]
    iterations: 3
    max_diff: 0.25
    regression_budget: 0.0
    stability: required
    trend_tracking: true
    timeout: 3600s

  # Level 4: Release (final verification)
  release:
    scope: "all"
    viewports: ["800x600", "1280x800", "1920x1080", "1280x800@2x"]
    iterations: 5
    max_diff: 0.0           # Perfect parity required
    regression_budget: 0.0
    stability: required
    human_review: required
    timeout: 7200s
```

### 6.2 Regression Detection

```python
# scripts/parity_regression.py

@dataclass
class RegressionCheck:
    case_id: str
    baseline_diff: float
    current_diff: float
    delta: float
    budget: float
    passed: bool
    severity: str  # "none", "minor", "major", "critical"

def check_regression(
    current: Dict[str, float],
    baseline: Dict[str, float],
    budget: float = 0.0
) -> List[RegressionCheck]:
    """Check for regressions against baseline."""

    results = []

    for case_id, current_diff in current.items():
        baseline_diff = baseline.get(case_id, 0.0)
        delta = current_diff - baseline_diff
        passed = delta <= budget

        # Severity classification
        if delta <= 0:
            severity = "none"
        elif delta <= 0.1:
            severity = "minor"
        elif delta <= 0.5:
            severity = "major"
        else:
            severity = "critical"

        results.append(RegressionCheck(
            case_id=case_id,
            baseline_diff=baseline_diff,
            current_diff=current_diff,
            delta=delta,
            budget=budget,
            passed=passed,
            severity=severity,
        ))

    return results

def format_regression_report(checks: List[RegressionCheck]) -> str:
    """Format regression report for CI output."""

    failed = [c for c in checks if not c.passed]

    if not failed:
        return "✅ No regressions detected"

    lines = [
        "❌ REGRESSION DETECTED",
        "",
        "| Case | Baseline | Current | Delta | Severity |",
        "|------|----------|---------|-------|----------|",
    ]

    for c in sorted(failed, key=lambda x: -x.delta):
        lines.append(
            f"| {c.case_id} | {c.baseline_diff:.2f}% | "
            f"{c.current_diff:.2f}% | +{c.delta:.2f}% | {c.severity.upper()} |"
        )

    return "\n".join(lines)
```

### 6.3 CI Integration

```yaml
# .github/workflows/parity.yml

name: Parity Gate

on:
  push:
    branches: [main, master]
  pull_request:

jobs:
  commit-gate:
    runs-on: macos-14
    timeout-minutes: 5
    steps:
      - uses: actions/checkout@v4

      - name: Setup environment
        run: |
          brew install node
          cargo build --release
          cd tools/parity_oracle && npm ci

      - name: Run commit gate
        run: |
          python3 scripts/parity_gate.py \
            --level commit \
            --baseline baselines/main-baseline.json \
            --fail-on-regression 0.0

  pr-gate:
    runs-on: macos-14
    timeout-minutes: 15
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Setup environment
        run: |
          brew install node
          cargo build --release
          cd tools/parity_oracle && npm ci

      - name: Fetch baseline from main
        run: |
          git checkout origin/main -- baselines/main-baseline.json

      - name: Run PR gate
        run: |
          python3 scripts/parity_gate.py \
            --level pr_merge \
            --baseline baselines/main-baseline.json \
            --output parity-report.json \
            --fail-on-regression 0.1

      - name: Upload report
        uses: actions/upload-artifact@v4
        with:
          name: parity-report
          path: |
            parity-report.json
            parity-results/

  nightly:
    runs-on: macos-14
    timeout-minutes: 90
    if: github.event_name == 'schedule'
    steps:
      - uses: actions/checkout@v4

      - name: Run nightly gate
        run: |
          python3 scripts/parity_gate.py \
            --level nightly \
            --viewports 800x600,1280x800,1920x1080 \
            --iterations 3 \
            --output nightly-report.json

      - name: Update trend dashboard
        run: |
          python3 scripts/parity_summary.py \
            --update-trends \
            --output-dir dashboard/
```

---

## Part 7: Live Proofing System

### 7.1 Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                   Live Proofing Server                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────┐              ┌─────────────────┐           │
│  │   Chrome Frame  │◄── WebSocket ──►│  RustKit Frame  │           │
│  │   (Playwright)  │   sync events   │  (hiwave-smoke) │           │
│  └────────┬────────┘              └────────┬────────┘           │
│           │                                │                     │
│           ▼                                ▼                     │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    Side-by-Side Viewer                      ││
│  │  ┌───────────────────┐  ┌───────────────────┐              ││
│  │  │                   │  │                   │              ││
│  │  │   Chrome View     │  │   RustKit View    │              ││
│  │  │                   │  │                   │              ││
│  │  │   (iframe/img)    │  │   (iframe/img)    │              ││
│  │  │                   │  │                   │              ││
│  │  └───────────────────┘  └───────────────────┘              ││
│  │                                                              ││
│  │  ┌─────────────────────────────────────────────────────────┐││
│  │  │ Overlay Controls: [Diff] [Swipe] [Onion] [Sync Scroll] │││
│  │  └─────────────────────────────────────────────────────────┘││
│  │                                                              ││
│  │  ┌─────────────────────────────────────────────────────────┐││
│  │  │ Element Inspector: hover to see style comparison        │││
│  │  └─────────────────────────────────────────────────────────┘││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 7.2 Proofing Server Implementation

```javascript
// tools/live_proof/server.mjs

import express from 'express';
import { WebSocketServer } from 'ws';
import { chromium } from 'playwright';
import { spawn } from 'child_process';

const app = express();
const PORT = 3333;

// Serve static files
app.use(express.static('tools/live_proof/public'));

// Test case listing
app.get('/api/cases', async (req, res) => {
  const cases = await discoverTestCases();
  res.json(cases);
});

// Capture endpoint
app.post('/api/capture/:caseId', async (req, res) => {
  const { caseId } = req.params;
  const { viewport = '1280x800' } = req.body;

  const [width, height] = viewport.split('x').map(Number);

  // Parallel capture
  const [chrome, rustkit] = await Promise.all([
    captureChrome(caseId, width, height),
    captureRustkit(caseId, width, height),
  ]);

  // Compare
  const diff = await compareCaptures(chrome, rustkit);

  res.json({
    chrome: `/captures/${caseId}/chrome.png`,
    rustkit: `/captures/${caseId}/rustkit.png`,
    diff: `/captures/${caseId}/diff.png`,
    diffPercent: diff.diffPercent,
    attribution: diff.attribution,
  });
});

// WebSocket for live sync
const wss = new WebSocketServer({ noServer: true });

wss.on('connection', (ws, req) => {
  const caseId = new URL(req.url, 'http://localhost').searchParams.get('case');

  ws.on('message', async (message) => {
    const event = JSON.parse(message);

    if (event.type === 'scroll') {
      // Sync scroll position to other viewers
      wss.clients.forEach(client => {
        if (client !== ws) {
          client.send(JSON.stringify({ type: 'scroll', ...event }));
        }
      });
    }

    if (event.type === 'hover') {
      // Get element info at position
      const elementInfo = await getElementAtPosition(caseId, event.x, event.y);
      ws.send(JSON.stringify({ type: 'element', ...elementInfo }));
    }

    if (event.type === 'refresh') {
      // Re-capture and send updated frames
      const result = await captureAndCompare(caseId, event.viewport);
      ws.send(JSON.stringify({ type: 'capture', ...result }));
    }
  });
});

// Start server
const server = app.listen(PORT, () => {
  console.log(`Live proofing server: http://localhost:${PORT}`);
});

server.on('upgrade', (request, socket, head) => {
  wss.handleUpgrade(request, socket, head, (ws) => {
    wss.emit('connection', ws, request);
  });
});
```

### 7.3 Viewer UI

```html
<!-- tools/live_proof/public/index.html -->
<!DOCTYPE html>
<html>
<head>
  <title>Parity Live Proofing</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }

    body {
      font-family: system-ui, sans-serif;
      background: #1a1a1a;
      color: #fff;
      height: 100vh;
      display: flex;
      flex-direction: column;
    }

    header {
      padding: 12px 20px;
      background: #2a2a2a;
      display: flex;
      align-items: center;
      gap: 20px;
      border-bottom: 1px solid #333;
    }

    .case-select {
      padding: 8px 12px;
      background: #333;
      border: 1px solid #444;
      border-radius: 4px;
      color: #fff;
      min-width: 300px;
    }

    .viewport-select {
      padding: 8px 12px;
      background: #333;
      border: 1px solid #444;
      border-radius: 4px;
      color: #fff;
    }

    .btn {
      padding: 8px 16px;
      background: #4a9eff;
      border: none;
      border-radius: 4px;
      color: #fff;
      cursor: pointer;
    }

    .btn:hover { background: #3a8eef; }

    .diff-badge {
      padding: 4px 12px;
      border-radius: 20px;
      font-size: 14px;
      font-weight: 600;
    }

    .diff-badge.good { background: #2ecc71; }
    .diff-badge.warn { background: #f39c12; }
    .diff-badge.bad { background: #e74c3c; }

    main {
      flex: 1;
      display: flex;
      overflow: hidden;
    }

    .pane {
      flex: 1;
      display: flex;
      flex-direction: column;
      border-right: 1px solid #333;
    }

    .pane:last-child { border-right: none; }

    .pane-header {
      padding: 8px 12px;
      background: #252525;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 1px;
      color: #888;
    }

    .pane-content {
      flex: 1;
      overflow: auto;
      display: flex;
      align-items: flex-start;
      justify-content: center;
      padding: 20px;
    }

    .pane-content img {
      max-width: 100%;
      border: 1px solid #333;
    }

    .controls {
      padding: 12px 20px;
      background: #2a2a2a;
      display: flex;
      gap: 10px;
      align-items: center;
      border-top: 1px solid #333;
    }

    .control-group {
      display: flex;
      gap: 4px;
    }

    .control-btn {
      padding: 6px 12px;
      background: #333;
      border: 1px solid #444;
      border-radius: 4px;
      color: #fff;
      cursor: pointer;
      font-size: 12px;
    }

    .control-btn.active {
      background: #4a9eff;
      border-color: #4a9eff;
    }

    .inspector {
      width: 350px;
      background: #252525;
      border-left: 1px solid #333;
      overflow: auto;
      font-size: 12px;
    }

    .inspector h3 {
      padding: 12px;
      background: #2a2a2a;
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 1px;
      color: #888;
    }

    .style-table {
      width: 100%;
      border-collapse: collapse;
    }

    .style-table td {
      padding: 6px 12px;
      border-bottom: 1px solid #333;
      vertical-align: top;
    }

    .style-table .prop { color: #888; width: 120px; }
    .style-table .chrome { color: #4a9eff; }
    .style-table .rustkit { color: #f39c12; }
    .style-table .match { color: #2ecc71; }
    .style-table .mismatch { background: rgba(231, 76, 60, 0.2); }
  </style>
</head>
<body>
  <header>
    <select class="case-select" id="caseSelect">
      <option value="">Select test case...</option>
    </select>

    <select class="viewport-select" id="viewportSelect">
      <option value="1280x800">1280 x 800</option>
      <option value="800x600">800 x 600</option>
      <option value="1920x1080">1920 x 1080</option>
    </select>

    <button class="btn" id="captureBtn">Capture</button>

    <span class="diff-badge" id="diffBadge">--</span>

    <label>
      <input type="checkbox" id="syncScroll" checked> Sync Scroll
    </label>
  </header>

  <main>
    <div class="pane">
      <div class="pane-header">Chrome Baseline</div>
      <div class="pane-content" id="chromePane"></div>
    </div>

    <div class="pane">
      <div class="pane-header">RustKit Render</div>
      <div class="pane-content" id="rustkitPane"></div>
    </div>

    <div class="pane">
      <div class="pane-header">Diff Overlay</div>
      <div class="pane-content" id="diffPane"></div>
    </div>
  </main>

  <div class="controls">
    <div class="control-group">
      <button class="control-btn active" data-mode="side-by-side">Side by Side</button>
      <button class="control-btn" data-mode="swipe">Swipe</button>
      <button class="control-btn" data-mode="onion">Onion Skin</button>
      <button class="control-btn" data-mode="diff-only">Diff Only</button>
    </div>

    <div class="control-group" style="margin-left: auto">
      <button class="control-btn" id="prevCase">← Prev</button>
      <button class="control-btn" id="nextCase">Next →</button>
    </div>
  </div>

  <div class="inspector" id="inspector">
    <h3>Element Inspector</h3>
    <table class="style-table" id="styleTable">
      <tr><td colspan="3" style="color: #666; padding: 20px;">Hover over an element to inspect</td></tr>
    </table>
  </div>

  <script type="module">
    // WebSocket connection
    let ws;
    let currentCase = null;

    // Initialize
    async function init() {
      // Load test cases
      const res = await fetch('/api/cases');
      const cases = await res.json();

      const select = document.getElementById('caseSelect');
      cases.forEach(c => {
        const opt = document.createElement('option');
        opt.value = c.id;
        opt.textContent = `${c.category}/${c.name}`;
        select.appendChild(opt);
      });

      // Event handlers
      select.onchange = () => capture(select.value);
      document.getElementById('captureBtn').onclick = () => capture(currentCase);
      document.getElementById('viewportSelect').onchange = () => capture(currentCase);

      // Sync scroll
      const panes = [
        document.getElementById('chromePane'),
        document.getElementById('rustkitPane'),
        document.getElementById('diffPane'),
      ];

      panes.forEach(pane => {
        pane.onscroll = () => {
          if (!document.getElementById('syncScroll').checked) return;
          panes.forEach(other => {
            if (other !== pane) {
              other.scrollTop = pane.scrollTop;
              other.scrollLeft = pane.scrollLeft;
            }
          });
        };
      });
    }

    async function capture(caseId) {
      if (!caseId) return;
      currentCase = caseId;

      const viewport = document.getElementById('viewportSelect').value;

      const res = await fetch(`/api/capture/${caseId}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ viewport }),
      });

      const result = await res.json();

      // Update panes
      document.getElementById('chromePane').innerHTML =
        `<img src="${result.chrome}" alt="Chrome">`;
      document.getElementById('rustkitPane').innerHTML =
        `<img src="${result.rustkit}" alt="RustKit">`;
      document.getElementById('diffPane').innerHTML =
        `<img src="${result.diff}" alt="Diff">`;

      // Update badge
      const badge = document.getElementById('diffBadge');
      badge.textContent = `${result.diffPercent.toFixed(2)}% diff`;
      badge.className = 'diff-badge ' + (
        result.diffPercent < 0.5 ? 'good' :
        result.diffPercent < 2 ? 'warn' : 'bad'
      );

      // Update inspector with top contributors
      updateInspector(result.attribution);
    }

    function updateInspector(attribution) {
      const table = document.getElementById('styleTable');

      if (!attribution || !attribution.topContributors) {
        table.innerHTML = '<tr><td colspan="3">No attribution data</td></tr>';
        return;
      }

      let html = '';
      attribution.topContributors.forEach((elem, i) => {
        html += `
          <tr>
            <td colspan="3" style="background: #2a2a2a; font-weight: 600; padding: 8px 12px;">
              #${i + 1}: ${elem.selector} (${elem.contribution_percent}%)
            </td>
          </tr>
        `;

        if (elem.style_deltas) {
          for (const [prop, values] of Object.entries(elem.style_deltas)) {
            const match = values.chrome === values.rustkit;
            html += `
              <tr class="${match ? '' : 'mismatch'}">
                <td class="prop">${prop}</td>
                <td class="chrome">${values.chrome || '—'}</td>
                <td class="rustkit">${values.rustkit || '—'}</td>
              </tr>
            `;
          }
        }
      });

      table.innerHTML = html || '<tr><td colspan="3">All styles match</td></tr>';
    }

    init();
  </script>
</body>
</html>
```

### 7.4 Usage

```bash
# Start live proofing server
cd tools/live_proof && npm start

# Open in browser
open http://localhost:3333

# Features:
# - Select any test case from dropdown
# - View Chrome, RustKit, and Diff side-by-side
# - Synchronized scrolling across all panes
# - Swipe/onion skin comparison modes
# - Element inspector showing style mismatches
# - Real-time re-capture on code changes
```

---

## Part 8: Implementation Roadmap

### Phase 0: Foundation (Days 1-2)
- [ ] Create bundled test fonts and common reset CSS
- [ ] Implement deterministic capture environment
- [ ] Set up pinned Chrome version with required flags
- [ ] Verify stability with 3-run baseline capture

### Phase 1: Test Matrix Expansion (Days 3-5)
- [ ] Create layout micro-tests (L01-L20)
- [ ] Create paint micro-tests (P01-P20)
- [ ] Create stacking/clipping micro-tests (S01-S10)
- [ ] Capture Chrome baselines for all new tests

### Phase 2: Attribution & Taxonomy (Days 6-8)
- [ ] Implement element attribution algorithm
- [ ] Implement diff taxonomy classifier
- [ ] Generate attribution reports for existing failures
- [ ] Prioritize fixes by contribution weight

### Phase 3: Stability & Gates (Days 9-10)
- [ ] Implement multi-run capture orchestration
- [ ] Add stability analysis to reports
- [ ] Configure commit/PR/nightly gate levels
- [ ] Set up CI workflows

### Phase 4: Live Proofing (Days 11-12)
- [ ] Build proofing server with capture API
- [ ] Implement side-by-side viewer UI
- [ ] Add synchronized scrolling and inspection
- [ ] Document proofing workflow

### Phase 5: Reaching 99.5% (Days 13-20)
- [ ] Fix top 10 attribution contributors daily
- [ ] Track progress on trend dashboard
- [ ] Tighten thresholds progressively
- [ ] Eliminate remaining taxonomy categories

### Phase 6: The Final 0.5% (Days 21-25)
- [ ] Sub-pixel anti-aliasing alignment
- [ ] Color space gamma correction
- [ ] Font metrics fine-tuning
- [ ] Final human review of all cases

### Phase 7: Lock-In (Days 26-30)
- [ ] Run release gate with 5-run verification
- [ ] Document all known limitations
- [ ] Update baselines to final state
- [ ] Celebrate 100% parity achievement

---

## Part 9: Success Criteria

### Gate Passing Requirements

| Level | Max Diff | Stability | Regression Budget |
|-------|----------|-----------|-------------------|
| Commit | ≤ 1.0% | N/A | 0.0% |
| PR Merge | ≤ 0.5% | Required | 0.1% |
| Nightly | ≤ 0.25% | Required | 0.0% |
| Release | **0.0%** | Required (5 runs) | 0.0% |

### Per-Category Targets

| Category | Current | 99% Target | 100% Target |
|----------|---------|------------|-------------|
| Layout | ~40% | ≤ 1.0% | 0.0% |
| Gradients | ~60% | ≤ 0.5% | 0.0% |
| Text | ~70% | ≤ 1.0% | 0.0% |
| Borders/Shadows | ~50% | ≤ 0.5% | 0.0% |
| Form Controls | ~30% | ≤ 2.0% | ≤ 0.5% |
| Built-ins | ~35% | ≤ 1.0% | 0.0% |

### Stability Requirements

- All test cases must pass 3-run stability check
- Max-min variance must be < 0.10%
- No flaky tests (> 0.5% variance from median)
- Zero instability-related test exclusions

### Attribution Coverage

- 100% of diff pixels attributed to specific elements
- Top 3 contributors identified for every failing case
- Taxonomy classification for all diffs

---

## Part 10: Maintenance Protocol

### Daily Operations
1. Run nightly gate and review failures
2. Fix top 3 attribution contributors
3. Update trend dashboard
4. Archive run history

### Weekly Operations
1. Review trend sparklines for regressions
2. Tighten thresholds if stable
3. Add new micro-tests for edge cases discovered
4. Update Chrome baseline if version changes

### Release Checklist
1. Run release gate (5-run, all viewports)
2. Verify 0% diff on all cases
3. Human review of all live proofing captures
4. Sign-off on known limitations document
5. Tag baselines with release version

---

## Appendix A: Troubleshooting

### "Unstable test" failures
- Check for animation/transition CSS
- Verify font loading is complete
- Add explicit dimensions to avoid layout shifts
- Increase inter-run delay

### "Dimension mismatch" failures
- Verify viewport size in meta tag
- Check for scrollbar presence differences
- Ensure no content overflow

### "Attribution unclear" issues
- Add more granular test cases isolating the feature
- Check for overlapping elements
- Verify layout-rects export is complete

### "Text metrics drift"
- Ensure test fonts are loaded
- Verify font-family stack is explicit
- Check for font-size-adjust differences

---

## Appendix B: File Structure

```
baselines/
├── fonts/
│   ├── HiwaveTestSans.woff2
│   ├── HiwaveTestSerif.woff2
│   └── HiwaveTestMono.woff2
├── common/
│   ├── parity-reset.css
│   └── parity-freeze.js
├── micro/
│   ├── layout/
│   ├── paint/
│   ├── stacking/
│   ├── text/
│   ├── selectors/
│   ├── pseudo/
│   └── transform/
├── chrome-120/
│   ├── metadata.json
│   └── [captured baselines]
└── main-baseline.json

tools/
├── parity_oracle/
│   ├── capture_chrome.mjs
│   ├── capture_rustkit.mjs
│   ├── compare_pixels.mjs
│   ├── attribute_diff.mjs
│   └── classify_diff.mjs
└── live_proof/
    ├── server.mjs
    └── public/
        └── index.html

scripts/
├── parity_baseline.py
├── parity_gate.py
├── parity_stability.py
├── parity_regression.py
├── parity_multi_run.py
└── parity_summary.py
```

---

*This plan targets 100% pixel parity through deterministic capture, comprehensive testing, actionable attribution, and rigorous gating. The stretch goal is achievable with systematic execution.*
