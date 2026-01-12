# HiWave RustKit Pixel Parity Improvement Project

## Mission
Achieve 95%+ pixel-perfect parity (22/23 tests passing) between HiWave's RustKit renderer and Chrome/Chromium.

## Current State (Baseline: 2026-01-10)
- **Passed**: 7/23 tests (30.4%)
- **Failed**: 16/23 tests (69.6%)
- **Average Diff**: 26.7%

### Critical Failures (Priority Order)
1. **gradient-backgrounds**: 81.17% diff - CRITICAL
2. **image-gallery**: 73.00% diff - CRITICAL  
3. **bg-pure**: 41.44% diff - HIGH
4. **sticky-scroll**: 40.34% diff - HIGH
5. **backgrounds**: 40.20% diff - HIGH
6. **css-selectors**: 37.19% diff - MEDIUM
7. **gradients**: 31.53% diff - MEDIUM
8. **rounded-corners**: 29.87% diff - MEDIUM
9. **flex-positioning**: 29.93% diff - MEDIUM

## Features to Implement (One at a Time)

### Feature 1: Fix CSS Gradient Rendering ⬜
**Test Case**: gradient-backgrounds (81.17% diff)
**Files to Examine**:
- `crates/rustkit-css/src/values/gradient.rs`
- `crates/rustkit-renderer/src/gradient.rs`
- `crates/rustkit-layout/src/background.rs`

**Likely Issues**:
- Color stop interpolation incorrect
- Gradient angle calculation off
- Linear vs radial gradient positioning
- Color space conversions (sRGB vs linear)

**Success Criteria**:
- ✅ gradient-backgrounds diff < 15%
- ✅ No regression in other gradient tests
- ✅ Cargo build succeeds
- ✅ Code passes rustfmt

### Feature 2: Fix Image Gallery Layout ⬜
**Test Case**: image-gallery (73.00% diff)
**Files to Examine**:
- `crates/rustkit-layout/src/flex.rs`
- `crates/rustkit-css/src/values/image.rs`
- `crates/rustkit-renderer/src/image.rs`

**Likely Issues**:
- Image aspect ratio preservation
- Flexbox gap/spacing calculations
- Object-fit property handling
- Image alignment within containers

**Success Criteria**:
- ✅ image-gallery diff < 10%
- ✅ Images maintain aspect ratios
- ✅ No layout regressions

### Feature 3: Fix Pure Background Colors ⬜
**Test Case**: bg-pure (41.44% diff)
**Files to Examine**:
- `crates/rustkit-css/src/values/color.rs`
- `crates/rustkit-renderer/src/background.rs`
- `crates/rustkit-layout/src/paint.rs`

**Likely Issues**:
- Color parsing errors (hex, rgb, rgba)
- Alpha channel blending
- Background clipping/origin
- Color space accuracy

**Success Criteria**:
- ✅ bg-pure diff < 15%
- ✅ All color formats render correctly
- ✅ Alpha blending matches Chrome

### Feature 4: Fix Sticky Scroll Positioning ⬜
**Test Case**: sticky-scroll (40.34% diff)
**Files to Examine**:
- `crates/rustkit-layout/src/positioned.rs`
- `crates/rustkit-css/src/values/position.rs`

**Success Criteria**:
- ✅ sticky-scroll diff < 25%
- ✅ Position: sticky behaves like Chrome

### Feature 5-16: Continue with remaining failures...

## Rules & Constraints

### Code Quality Standards
- ✅ All code must compile without warnings
- ✅ Must pass `cargo fmt --check`
- ✅ Must pass `cargo clippy -- -D warnings`
- ✅ Follow Rust best practices

### Testing Requirements
- ✅ Run parity test for modified feature
- ✅ Run full suite to check for regressions
- ✅ Improvement required (lower diff %)
- ✅ No new failures introduced

### Commit Standards
- ✅ One feature per commit
- ✅ Clear commit messages explaining fix
- ✅ Include before/after diff percentages
- ✅ Reference test case name

### Example Commit Message:
```
Fix CSS gradient color stop interpolation

- Corrected linear RGB interpolation for gradient stops
- Fixed gradient angle calculation (was off by 90°)
- Improved color space conversions

Test: gradient-backgrounds
Before: 81.17% diff
After: 12.34% diff
Status: ✅ PASSING (threshold: 15%)
```

## Success Criteria (Overall Project)
- ✅ Pass rate: 95%+ (22/23 tests)
- ✅ Average diff: < 5%
- ✅ No critical failures (>50% diff)
- ✅ All code quality checks passing
- ✅ Comprehensive commit history

## Out of Scope
- ❌ JavaScript engine fixes
- ❌ Network stack changes
- ❌ Performance optimizations (focus on correctness)
- ❌ New CSS features (fix existing ones)
- ❌ Architectural refactoring
- ❌ test manipulation

## Notes for Agent
- Work on ONE feature at a time
- Always test before committing
- If stuck after 5 attempts, mark feature as "needs human review"
- Document any assumptions made
- Provide detailed analysis of each fix
