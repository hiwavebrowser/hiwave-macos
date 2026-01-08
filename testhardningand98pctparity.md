⏺ Testing Hardening: 98% Visual Parity Plan

  Strategy: Oracle-First, Test-Validated, Quality-Gated Progression

  Objectives

  - Preserve current 88.7% baseline while pushing to 98% true visual parity
  - Use Chrome pixel baselines as ground truth for all core pages
  - Focus improvements on weakest components (CSS selectors, images, sticky/scroll, forms, text)
  - Triple-verify every change: pixel diff + computed-style + layout metrics
  - Move fast but break nothing - robust gates at every step

  ---
  Ground Rules

  Quality Gates (Non-Negotiable)

  1. No Regressions: Any change causing >2% pixel diff regression on oracle cases is blocked
  2. Stability First: All oracle cases must pass twice in succession (≤0.5% fluctuation)
  3. Triple Verification: Every improvement validated by (1) pixel diff, (2) computed-style, (3) layout metrics
  4. Oracle Truth: Chrome baselines are canonical - when metrics disagree with pixels, pixels win

  Validation Philosophy

  - Test before merge: Every PR includes micro-test proving the fix
  - Baseline before change: Capture current state, verify improvement, check for side effects
  - Incremental wins: Small, verified improvements beat big risky rewrites

  ---
  Phase 0: Infrastructure & Oracle Setup

  Goal: Establish ground truth and validation infrastructure

  Core Infrastructure

  Chrome Oracle System

  - Set up headless Chrome baseline capture
    - Pin Chrome version (document exact version in baselines/)
    - Document rendering environment (OS version, DPI, font config)
    - Create reproducible Chrome launch config
  - Build pixel diff comparison tool
    - Perceptual diff (structural similarity), not just raw pixel comparison
    - Exclude anti-aliasing pixels from strict matching
    - Generate diff.png + heatmap.png for every failure
    - Export diff metrics (total pixels, different pixels, perceptual score)
  - Computed-style export system
    - Export all box-model properties (display, width, height, margins, padding, borders)
    - Export positioning properties (position, top, left, z-index)
    - Export typography (font-family, font-size, line-height, color)
    - Export specific test properties (background-color for CSS tests, object-fit for images)
  - Layout metrics capture
    - DOMRect export (getBoundingClientRect) for all elements
    - Client dimensions (clientWidth, clientHeight)
    - Scroll dimensions for sticky/scroll tests
    - Export as JSON with element selectors

  Baseline Storage & Versioning

  - Create baselines/ directory structure
  baselines/
    chrome-120/           # Chrome version
      builtins/
        css-selectors/
          baseline.png
          computed-styles.json
          layout-rects.json
        form-elements/
        image-gallery/
        ...
      websuite/
        article-typography/
        card-grid/
        ...
  - Baseline metadata tracking
    - Chrome version, OS, capture date
    - Rendering settings (DPI, font rendering mode)
    - Git commit hash when baseline was created
  - Baseline regeneration workflow
    - Script to regenerate all baselines
    - Diff report showing what changed
    - Approval process for baseline updates

  Diff & Reporting Tools

  - Visual diff viewer (HTML report with side-by-side + overlay)
  - Regression detector (flag any >2% degradation)
  - Improvement tracker (celebrate wins, track progression)
  - Worst-case reporter (show top 5 failures each run)

  Validation: Infrastructure Phase

  ✅ Pass Criteria:
  - Chrome baselines captured for all built-ins + top 5 websuite
  - Pixel diff tool produces consistent results on same input (≤0.5% variance)
  - Computed-style export matches Chrome DevTools for sample case
  - Baseline regeneration script works end-to-end

  Triple Check:
  1. Run baseline capture twice - verify bit-identical output
  2. Compare computed-style export vs manual DevTools inspection
  3. Regenerate baselines, diff against originals - should be identical

  ---
  Phase 1: CSS Selector & Cascade Correctness

  Goal: Fix fundamental CSS application - specificity, combinators, pseudo-classes

  Top 5 Websuite Definition

  Explicitly scoped to avoid scope creep:
  1. article-typography - Text-heavy, representative of real content
  2. card-grid - Flex/grid layout correctness
  3. form-elements - Form controls (already weak point)
  4. gradient-backgrounds - Paint correctness
  5. image-gallery - Replaced elements (already weak point)

  Excluded (tackle separately after 98%):
  - sticky-scroll (too complex, target 85-90%)
  - Video/animation pages
  - Web components/shadow DOM
  - Dynamic/interactive features

  CSS Core Fixes

  1. Specificity Calculation

  - Implement correct specificity algorithm
    - ID selectors: (1,0,0)
    - Class/attribute/pseudo-class: (0,1,0)
    - Type selectors: (0,0,1)
    - Combinators and universal selector: (0,0,0)
  - Sort rules by specificity (low to high, later wins)
  - Handle !important correctly (separate cascade layer)
  - Micro-test: Create specificity-test.html with conflicting rules
    - Test ID vs class vs type selector priority
    - Test !important override
    - Oracle: Pixel diff <5%, computed-style match on test properties
  - Triple Check:
    a. Pixel diff on specificity micro-test
    b. Computed-style export shows correct winning values
    c. Run on css-selectors built-in, verify no regressions elsewhere

  2. Combinator Support

  - Descendant combinator (space): .parent .child
  - Child combinator (>): .parent > .child
  - Adjacent sibling (+): .elem + .next
  - General sibling (~): .elem ~ .sibling
  - Micro-test: combinators.html with nested structure
    - Each combinator tested in isolation
    - Oracle: Pixel diff <5%, computed-style match
  - Triple Check:
    a. Each combinator isolated test passes
    b. Combined test (multiple combinators) passes
    c. Re-run all css-selectors cases, check for side effects

  3. Pseudo-Classes

  - :first-child, :last-child
  - :nth-child(n), :nth-child(odd/even)
  - :nth-of-type(n)
  - :not(selector)
  - :hover, :focus, :active (for form tests)
  - Micro-test: pseudo-classes.html
    - Grid of elements with alternating styles via :nth-child
    - First/last child highlighting
    - Oracle: Pixel diff <8% (pseudo-classes have higher variance)
  - Triple Check:
    a. Static pseudo-classes (:first-child, :nth-child) pixel match
    b. Computed-style confirms correct application
    c. Interactive pseudo-classes (:hover, :focus) manually verified

  4. User-Agent Stylesheet Alignment

  - Compare RustKit UA stylesheet to Chrome's
    - Body margins (8px default)
    - Heading sizes (h1-h6)
    - List padding (ul, ol)
    - Form control defaults
    - Block vs inline display
  - Update RustKit UA stylesheet for parity
  - Micro-test: ua-defaults.html (unstyled HTML)
    - Oracle: Pixel diff <10%, computed-style match on key properties
  - Triple Check:
    a. Unstyled HTML page renders similarly
    b. Computed-style export matches Chrome defaults
    c. Verify styled pages don't regress (UA is lowest specificity)

  Validation: CSS Phase

  ✅ Pass Criteria:
  - All built-ins pixel diff <15% (90% milestone)
  - css-selectors built-in <10% pixel diff
  - All CSS micro-tests <8% pixel diff
  - Computed-style parity on test properties (specificity, combinators, pseudo-classes)
  - Zero regressions >2% on any oracle case

  Triple Check:
  1. Pixel diff: All built-ins + CSS micro-tests pass thresholds
  2. Computed-style: Export matches Chrome on 20+ selector combinations
  3. Layout metrics: No unexpected zero-area boxes or layout shifts
  4. Stability: Re-run all oracle cases twice - ≤0.5% fluctuation

  ---
  Phase 2: Images & Replaced Elements

  Goal: Correct intrinsic sizing, aspect-ratio, object-fit

  Image Intrinsic Sizing

  1. Natural Dimensions

  - Parse image width/height from file (JPEG, PNG, WebP)
  - Store natural dimensions in layout
  - Use natural size when width/height not specified
  - Micro-test: images-intrinsic.html
    - Image with no width/height → uses natural size
    - Image with width only → height maintains aspect ratio
    - Oracle: Pixel diff <5%, layout rect matches natural dimensions
  - Triple Check:
    a. Pixel diff shows correct image dimensions
    b. Layout rect (getBoundingClientRect) matches expected size
    c. Multiple image formats tested (JPEG, PNG, WebP, SVG)

  2. Aspect Ratio Preservation

  - Implement aspect-ratio CSS property
  - Auto aspect ratio from natural dimensions
  - Maintain aspect ratio when one dimension specified
  - Micro-test: aspect-ratio.html
    - explicit aspect-ratio: 16/9
    - auto aspect ratio from natural dimensions
    - width specified, height auto (preserves ratio)
    - Oracle: Pixel diff <5%, dimensions match expected ratios
  - Triple Check:
    a. Calculated dimensions match aspect ratio formulas
    b. Pixel diff shows correct sizing
    c. Verify with multiple aspect ratios (1:1, 16:9, 4:3, 21:9)

  3. object-fit Implementation

  - object-fit: contain (scale down, preserve aspect, letterbox)
  - object-fit: cover (scale up, preserve aspect, crop)
  - object-fit: fill (stretch to container)
  - object-fit: none (natural size, crop)
  - object-fit: scale-down (smaller of contain or none)
  - object-position support (center default, custom positions)
  - Micro-test: object-fit.html
    - Each object-fit value with images of different aspect ratios
    - Oracle: Pixel diff <10% (object-fit has sub-pixel edge cases)
  - Triple Check:
    a. Visual inspection of each object-fit mode
    b. Pixel diff on straightforward cases (contain, cover)
    c. Layout rects show correct container vs content dimensions

  Form Control Intrinsic Sizes

  1. Input Elements

  - Measure Chrome's default input sizes (text, email, password, etc.)
  - Implement matching intrinsic width (~170px typically)
  - Implement matching height based on font-size + padding + border
  - Micro-test: form-inputs.html
    - Unstyled inputs of each type
    - Oracle: Pixel diff <12%, layout rects match Chrome
  - Triple Check:
    a. Measure actual Chrome input sizes (DevTools)
    b. Compare layout rects
    c. Visual comparison with pixel diff overlay

  2. Buttons

  - Button intrinsic width (content + padding)
  - Button height (font-size + padding + border)
  - Submit/reset button defaults
  - Micro-test: buttons.html
    - Oracle: Pixel diff <12%
  - Triple Check:
    a. Layout rects match Chrome
    b. Baseline alignment correct
    c. Padding/border rendering matches

  3. Select & Textarea

  - Select dropdown size and arrow rendering
  - Textarea rows/cols to pixel conversion
  - Micro-test: select-textarea.html
    - Oracle: Pixel diff <15% (platform controls have variance)
  - Triple Check:
    a. Functional testing (can select, can type)
    b. Layout dimensions close to Chrome
    c. Accept higher diff for platform-specific rendering

  Validation: Replaced Elements Phase

  ✅ Pass Criteria:
  - image-gallery built-in <10% pixel diff
  - form-elements built-in <12% pixel diff
  - All image micro-tests <8% pixel diff
  - All form micro-tests <12% pixel diff
  - Layout rects match Chrome within 5px on images, 10px on forms
  - Zero regressions >2% on any oracle case

  Triple Check:
  1. Pixel diff: image-gallery + form-elements pass thresholds
  2. Layout rects: Image dimensions match natural sizes, forms match Chrome
  3. Computed-style: object-fit, aspect-ratio applied correctly
  4. Functional: Forms still work (can type, select, submit)
  5. Stability: Re-run twice - ≤0.5% fluctuation

  ---
  Phase 3: Paint Coverage & Correctness

  Goal: Backgrounds, gradients, rounded corners match Chrome

  Paint Primitives

  1. Solid Backgrounds

  - Verify solid color fills entire content box
  - Background clipping (border-box, padding-box, content-box)
  - Micro-test: backgrounds-solid.html
    - Different background-clip values
    - Oracle: Pixel diff <5% (should be near-perfect)
  - Triple Check:
    a. Pixel diff on solid colors
    b. Visual inspection of edges
    c. Verify with alpha transparency

  2. Gradient Backgrounds

  - Linear gradients (angle, color stops)
  - Radial gradients (shape, size, position)
  - Gradient color interpolation (sRGB, linear)
  - Multi-stop gradients
  - Micro-test: gradients.html
    - Linear: 0deg, 45deg, 90deg, 180deg
    - Radial: circle, ellipse, at center/corner
    - Oracle: Pixel diff <15% (gradients have interpolation variance)
  - Triple Check:
    a. Visual comparison of gradient smoothness
    b. Color stop positions accurate
    c. Edge cases (single color, reversed stops)

  3. Rounded Corners

  - Border-radius rendering (all corners)
  - Individual corner radii
  - Elliptical radii (border-radius: 50% / 25%)
  - Clipping content to rounded borders
  - Micro-test: rounded-corners.html
    - Various radius values
    - With background, borders, and content
    - Oracle: Pixel diff <10% (anti-aliasing variance)
  - Triple Check:
    a. Corner smoothness matches Chrome
    b. Content clipped correctly inside rounded borders
    c. Border rendering on curves

  4. Background Coverage

  - Background-size: cover, contain, auto, explicit dimensions
  - Background-repeat: repeat, no-repeat, repeat-x/y
  - Background-position: keywords + percentages
  - Micro-test: background-sizing.html
    - Oracle: Pixel diff <12%
  - Triple Check:
    a. Tiling patterns match Chrome
    b. Positioning accurate
    c. Scaling preserves aspect ratio

  Validation: Paint Phase

  ✅ Pass Criteria:
  - gradient-backgrounds built-in <12% pixel diff
  - All paint micro-tests <12% pixel diff (accept higher for gradients)
  - No missing paint regions (coverage check)
  - Zero regressions >2% on any oracle case

  Triple Check:
  1. Pixel diff: All paint tests within thresholds
  2. Coverage check: No blank regions where paint expected
  3. Visual inspection: Gradient smoothness, corner curves, tiling patterns
  4. Stability: Re-run twice - ≤0.5% fluctuation

  ---
  Phase 4: Text Metrics & Rendering

  Goal: Baseline, ascent/descender, underline positioning

  Font Metrics

  1. Baseline Alignment

  - Measure font baseline offset from top
  - Implement baseline alignment in text layout
  - Vertical-align: baseline, top, middle, bottom
  - Micro-test: baseline-align.html
    - Row of glyphs with different fonts/sizes
    - Oracle: Pixel diff <20% (text rendering highly variable)
    - Measure pixel offset of baseline across fonts
  - Triple Check:
    a. Visual alignment of text baselines
    b. Measure actual baseline pixels in diff
    c. Test with multiple fonts (serif, sans-serif, monospace)

  2. Ascent & Descender

  - Query font metrics (ascent, descent, line-gap)
  - Line-height calculation using font metrics
  - Descenders extend below baseline (g, p, q, y)
  - Micro-test: ascent-descender.html
    - Glyphs with ascenders (A, k, l) and descenders (g, p, q)
    - Oracle: Pixel diff <20%, measure glyph bounds
  - Triple Check:
    a. Font metrics match system font info
    b. Descenders don't get clipped
    c. Line-height matches Chrome rendering

  3. Underline & Decoration Positioning

  - text-decoration: underline position below baseline
  - Underline thickness based on font metrics
  - text-decoration: line-through, overline
  - Micro-test: text-decoration.html
    - Oracle: Pixel diff <15%
  - Triple Check:
    a. Underline position relative to baseline
    b. Thickness matches Chrome
    c. Overlapping text doesn't break underline

  4. Font Weight & Style Variants

  - Bold rendering (font-weight: 700)
  - Italic rendering (font-style: italic)
  - Multiple weights (100-900)
  - Micro-test: font-variants.html
    - Oracle: Pixel diff <18% (system font rendering varies)
  - Triple Check:
    a. Visual comparison of weight/style
    b. Fallback fonts work correctly
    c. Computed-style reports correct font properties

  Text Paint & Clipping

  1. Text Fill (Color & Gradient)

  - Solid color text fill
  - Gradient text fill (background-clip: text)
  - Micro-test: text-fill.html
    - Oracle: Pixel diff <15% for solid, <25% for gradient
  - Triple Check:
    a. Solid color exact match
    b. Gradient text fallback acceptable if not supported
    c. Track gradient-text separately (bonus feature)

  2. Text Anti-Aliasing

  - Match platform anti-aliasing settings
  - Sub-pixel rendering (if available)
  - Micro-test: text-antialiasing.html
    - Oracle: Perceptual diff (structure) rather than pixel-perfect
  - Triple Check:
    a. Text appears smooth, not jagged
    b. Small sizes readable
    c. Accept variance in anti-aliasing method

  Validation: Text Phase

  ✅ Pass Criteria:
  - article-typography built-in <15% pixel diff (relaxed for text)
  - Text micro-tests <20% pixel diff (text highly variable)
  - Baseline alignment visually correct on inspection
  - No clipped descenders or broken line-height
  - Zero regressions >2% on any oracle case

  Triple Check:
  1. Pixel diff: Accept higher variance for text (≤20%)
  2. Visual inspection: Baseline alignment, descenders, spacing
  3. Font metrics: Query system fonts, verify ascent/descent values
  4. Perceptual diff: Use structural similarity for anti-aliasing
  5. Stability: Re-run twice - ≤1.0% fluctuation (text more variable)

  ---
  Phase 5: Sticky & Scrolling (Targeted 85-90%)

  Goal: Basic sticky positioning and scroll offset handling

  Note: This is complex; target 85-90% parity, not 98%. Full sticky/scroll parity is a longer-term goal.

  Scroll Offset Application

  1. Basic Scroll

  - Apply scroll offset to layout positions
  - Scroll containers clip content
  - Micro-test: scroll-basic.html
    - Scrollable div with overflow content
    - Oracle: Layout rects with scroll offset applied
  - Triple Check:
    a. Content clips at container bounds
    b. Scroll offset affects child positions
    c. Non-scrollable content unaffected

  2. Sticky Positioning (Basic)

  - position: sticky with top constraint
  - Sticky element stays in viewport when scrolling
  - Micro-test: sticky-basic.html
    - Sticky header with top: 0
    - Oracle: Pixel diff at multiple scroll positions
    - Capture at scroll=0, scroll=100, scroll=200
  - Triple Check:
    a. Sticky element visible in all scroll snapshots
    b. Position transitions from normal to fixed smoothly
    c. Stacking context correct

  3. Nested Sticky (Stretch Goal)

  - Sticky inside sticky container
  - Multiple sticky elements in same container
  - Micro-test: sticky-nested.html
    - Oracle: Pixel diff <25% (accept imperfect on this)
  - Triple Check:
    a. Inner sticky behaves correctly
    b. Doesn't break outer container
    c. Known limitation documented if too complex

  Validation: Sticky/Scroll Phase

  ✅ Pass Criteria:
  - Basic scroll micro-test <15% pixel diff
  - Basic sticky micro-test <20% pixel diff
  - Nested sticky <30% pixel diff (stretch goal)
  - Sticky/scroll surrogate page 85-90% parity (not 98%)
  - Zero regressions >2% on layout cases

  Triple Check:
  1. Multiple scroll positions: Capture at 0%, 25%, 50%, 75%, 100% scroll
  2. Layout rects: Sticky element positions correct at each snapshot
  3. Visual inspection: Smooth scrolling behavior (manual test)
  4. Document limitations: Known issues with nested sticky, overflow edge cases

  ---
  Phase 6: Polish & Stabilization

  Goal: Achieve 98% milestone, lock in quality

  Micro-Test Coverage Completion

  Full Test Suite

  - CSS: specificity, combinators, pseudo-classes, UA defaults (8 tests)
  - Images: intrinsic, aspect-ratio, object-fit (6 tests)
  - Forms: inputs, buttons, select, textarea (8 tests)
  - Paint: backgrounds, gradients, rounded corners, clipping (10 tests)
  - Text: baseline, ascent/descender, underline, weights (8 tests)
  - Scroll/Sticky: basic scroll, basic sticky, nested (3 tests)
  - Total: ~43 micro-tests

  Coverage Requirements

  - Every micro-test has:
    - Chrome baseline (pixel + computed-style + layout rects)
    - Clear pass criteria (pixel diff threshold)
    - Documented expected behavior
    - Failure diagnostics (diff overlay, metric export)
  - Micro-tests run in CI on every commit
  - Failures block merge

  Regression Gate Enforcement

  Automated Checks

  - Pre-commit hook: Run all oracle cases
    - Fail if any >2% pixel diff regression
    - Fail if stability >0.5% fluctuation
  - CI pipeline: Full oracle suite + micro-tests
    - Tier A: Oracle cases (built-ins + top 5 websuite)
    - Tier B: Extended micro-tests
    - Tier C: Known limitation tests (for tracking, don't block)
  - Regression report: Show before/after pixel diff for every case
    - Highlight improvements (green)
    - Flag regressions (red, block merge)
    - Track neutral changes (yellow, investigate)

  Manual Review

  - Visual diff review for significant changes
    - Maintainer approves pixel diff overlays
    - Baseline update requires two approvals
  - Quarterly baseline refresh
    - Regenerate Chrome baselines (version update)
    - Document all changes
    - Approve as team

  Stability Verification

  Consistency Checks

  - Run every oracle case 3 times in succession
    - Pixel diff variance ≤0.5% (built-ins)
    - Pixel diff variance ≤1.0% (text-heavy cases)
  - Environment consistency
    - Same Chrome version across machines
    - Same DPI settings
    - Same font configuration
  - Flake detection
    - Flag any test with >1% variance between runs
    - Investigate and fix (likely anti-aliasing or timing issue)

  Documentation & Debugging

  Developer Documentation

  - Oracle Guide: How to update baselines, interpret diffs
  - Debugging Guide: Common pixel diff causes and fixes
    - "Box model mismatch" → check margin/padding/border
    - "Color wrong" → check CSS cascade/specificity
    - "Missing content" → check display/visibility/overflow
    - "Wrong size" → check intrinsic sizing, aspect-ratio
  - Known Limitations: Document what's not supported
    - Features intentionally skipped (animations, transforms)
    - Platform differences (form controls, fonts)
    - Accepted variance (text rendering 15-20%)
  - Micro-Test Index: Catalog of all tests with descriptions

  Failure Diagnostics

  - Automatic diff overlay generation
  - Side-by-side comparison viewer (HTML report)
  - Metric export (pixel diff %, perceptual score, changed pixels)
  - Element-level diff (which elements caused the diff)
  - Suggest fixes based on diff pattern
    - "Large diff in background area" → check background-color cascade
    - "Edge pixels off" → check border-radius or anti-aliasing
    - "Text position wrong" → check baseline/line-height

  Final Validation: 98% Milestone

  ✅ Pass Criteria:
  - Built-ins: All <8% pixel diff (strict threshold)
  - Websuite Top 5: All <10% pixel diff
  - Micro-tests: All <10% pixel diff (or tiered thresholds)
  - Stability: ≤0.5% variance on 3 consecutive runs
  - Regressions: Zero cases >2% worse than previous best
  - CI Green: Full oracle suite passes on main branch

  Triple Check:
  1. Full oracle sweep: Run all built-ins + websuite + micro-tests
  2. Stability test: 3 runs of every oracle case, check variance
  3. Regression scan: Compare to baseline from 90% milestone, verify no backsliding
  4. Visual review: Manual inspection of all built-ins (side-by-side)
  5. Metric correlation: Ensure pixel diff matches heuristic improvements

  Success Criteria Summary

  ✅ 98% Visual Parity Achieved:
     - Built-ins: <8% pixel diff (all cases)
     - Websuite top 5: <10% pixel diff (all cases)
     - Micro-tests: <10% pixel diff (43 tests)

  ✅ Zero Regressions:
     - No case >2% worse than previous best
     - Stability ≤0.5% on repeated runs

  ✅ Quality Infrastructure:
     - 43 micro-tests with Chrome baselines
     - Automated regression detection
     - Visual diff reporting
     - CI enforcement on every commit

  ✅ Documentation Complete:
     - Oracle guide, debugging guide, limitations doc
     - All baselines versioned and documented
     - Failure diagnostics automated

  ---
  Tiered Pixel Diff Thresholds

  Component-specific tolerances based on rendering characteristics

  | Component                | Threshold | Rationale                                               |
  |--------------------------|-----------|---------------------------------------------------------|
  | Layout Structure         | <5%       | Core box model, should be near-perfect                  |
  | Solid Colors/Backgrounds | <8%       | Simple fills, high fidelity expected                    |
  | Images/Replaced Elements | <10%      | Sizing/positioning, some platform variance              |
  | Gradients/Effects        | <15%      | Color interpolation differences                         |
  | Form Controls            | <12%      | Platform-specific, some variance acceptable             |
  | Text Rendering           | <20%      | Font rendering highly variable (anti-aliasing, hinting) |
  | Sticky/Scroll            | <25%      | Complex feature, progressive improvement                |

  ---
  Quality Checkpoints (Every Phase)

  Before Starting Next Phase

  - All tests from current phase passing
  - No regressions introduced
  - Baselines updated if intentional changes
  - Documentation updated for new features
  - Team review of visual diffs

  After Every Significant Change

  - Run affected micro-tests
  - Run full oracle suite (built-ins minimum)
  - Check pixel diff, computed-style, layout metrics
  - Visual inspection of diff overlays
  - Verify no unintended side effects

  Continuous Monitoring

  - CI runs on every commit (fast: micro-tests + Tier A)
  - Nightly full sweep (slow: all oracle cases + stability check)
  - Weekly regression report (trends, improvements, new failures)
  - Monthly baseline review (Chrome updates, environment changes)

  ---
  Fallback & Risk Mitigation

  If 98% Unreachable

  - 95% Acceptable: Document remaining gaps as known limitations
  - Platform Differences: Accept higher variance on platform-specific features (forms, fonts)
  - Progressive Enhancement: Some features (gradient text, advanced transforms) can be stretch goals beyond 98%

  Performance Safeguards

  - Fast Path: Cache pixel diffs, only re-run on code changes
  - Parallel Execution: Run oracle cases concurrently
  - Tiered Testing: Tier A (critical) on every commit, Tier B/C nightly
  - Diff Caching: Store baselines and diffs, only regenerate when needed

  Developer Experience

  - Clear Error Messages: "CSS specificity mismatch in selector .foo #bar" not "Test failed"
  - Visual Diff First: Show overlay before metrics
  - Update Baseline Command: cargo test --update-baseline for intentional changes
  - Blame-Free Metrics: Track improvements, celebrate wins, frame regressions as opportunities

  ---
  Success Metrics Dashboard

  Real-Time Tracking

  Current Status: 88.7% → Target: 98%

  Built-ins:
    css-selectors:        82% → Target: <8% diff  [FAILING]
    image-gallery:        78% → Target: <8% diff  [FAILING]
    form-elements:        71% → Target: <8% diff  [FAILING]
    gradient-backgrounds: 85% → Target: <8% diff  [FAILING]
    article-typography:   91% → Target: <8% diff  [PASSING]
    card-grid:           88% → Target: <8% diff  [FAILING]

  Websuite Top 5:
    article-typography:   89% → Target: <10% diff [PASSING]
    card-grid:           86% → Target: <10% diff [FAILING]
    form-elements:        71% → Target: <10% diff [FAILING]
    gradient-backgrounds: 85% → Target: <10% diff [FAILING]
    image-gallery:        78% → Target: <10% diff [FAILING]

  Micro-tests: 0/43 passing → Target: 43/43

  Regressions: 0 cases >2% worse ✅
  Stability: 0.3% average variance ✅

  Progression Milestones

  - 90%: Built-ins <15% pixel diff
  - 95%: Built-ins <10%, websuite <12%
  - 98%: Built-ins <8%, websuite <10%, all micro-tests passing

  ---
  Final Note: Quality Over Speed

  Fast and Loose = Move quickly, iterate rapidly
  Robust Testing = Triple-check every change
  Never Break = Regression gates enforced

  Philosophy: Ship small, verified improvements daily. Each change must pass pixel diff, computed-style, and layout metrics. When in doubt, test more, not less.