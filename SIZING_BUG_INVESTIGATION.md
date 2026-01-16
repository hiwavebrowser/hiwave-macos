# Auto-Height Container Sizing Bug Investigation

## Pattern Identified

When a container has `height: auto`, several layout algorithms incorrectly use a pre-computed
"container_height" value that represents stacked children (block flow) rather than the actual
grid/flex computed height.

This causes:
1. "Remaining space" to be distributed when none exists
2. Percentage heights to resolve against wrong values
3. Content alignment to shift tracks incorrectly

## Fixed Issues

### Grid Track Sizing (FIXED in c2aecd6)
- **Location**: `crates/rustkit-layout/src/grid.rs:1714-1716`
- **Problem**: `size_grid_tracks()` received pre-grid block-layout height for auto-height containers
- **Effect**: Row tracks inflated by ~1500+ pixels each
- **Fix**: Pass 0 for auto-height containers: `let row_container_height = if has_definite_height { container_height } else { 0.0 };`

### Grid Content Alignment (FIXED)
- **Location**: `crates/rustkit-layout/src/grid.rs:1734`
- **Problem**: `apply_content_alignment` for rows used wrong `container_height` for auto-height containers
- **Effect**: `free_space = container_size - used_space` calculated phantom free space
- **Fix**: Skip row alignment entirely for auto-height containers (no free space to distribute)
```rust
if has_definite_height {
    apply_content_alignment(&mut grid.rows, container_height, row_gap, ...);
}
```

## Potential Issues (Lower Priority)

### 1. Grid Auto-Repeat Expansion
- **Location**: `crates/rustkit-layout/src/grid.rs:1360`
- **Code**: `grid.expand_auto_repeats(container_width, container_height)`
- **Concern**: Does this correctly handle auto-height for row auto-repeats?
- **Priority**: Low - row auto-repeat is rarely used

### 2. Flexbox Remaining Space Distribution
- **Location**: `crates/rustkit-layout/src/flex.rs:759`
- **Code**: `let free_space = container_cross - total_line_size - total_gaps;`
- **Concern**: Similar "remaining space" pattern for cross-axis alignment
- **Priority**: Medium - would only affect `align-content` on auto-height flex containers

## Key Check Pattern

Look for code that:
1. Uses `container_height` without checking if height is `Length::Auto`
2. Calculates "remaining space" or "free space" from container dimensions
3. Distributes space to tracks/items based on container size

## Sticky-Scroll 50% Regression Analysis

**NOT a sizing bug** - Layout JSON shows correct widths (640px for 1fr column).

**Root causes from attribution:**
- `text_metrics`: 40.97% - Text positioning/rendering differences (font metrics, not text-transform)
- `gradient_interpolation`: 30.98% - Gradient color rendering differences

### Text-Transform Issue (FIXED in d08db50)

**Problem:** The `apply_text_transform()` function existed in `rustkit-layout/src/text.rs:455` but was only used in tests, never in actual text rendering.

**Fix:** Added call to `apply_text_transform()` in `render_text()` function:
```rust
fn render_text(&mut self, layout_box: &LayoutBox) {
    if let BoxType::Text(ref raw_text) = layout_box.box_type {
        let style = &layout_box.style;
        let text = apply_text_transform(raw_text, style.text_transform);
        // ... use transformed text for rendering
    }
}
```

**Verification:** Debug output confirmed transformation is working:
- "Categories -> CATEGORIES"
- "Tags -> TAGS"
- "Popular Posts -> POPULAR POSTS"

### Remaining Issues (NOT text-transform related)

The 50% diff persists because of:

1. **Gradient Interpolation (31%)** - Color space differences in gradient rendering
   - Top contributors: `div.article-image` elements with linear gradients
   - `div.horizontal-item` with gradient backgrounds

2. **Text Metrics (41%)** - General font rendering differences
   - Character width/spacing differences between RustKit and Chrome
   - Baseline alignment differences
   - NOT related to text-transform (that's now working)

**Next investigation areas:**
1. Gradient color space interpolation (sRGB vs linear)
2. Font metrics alignment with system fonts

---

## Notes for Future

When adding layout features, always ask:
- What if the container has `height: auto`?
- Should we use actual computed content height vs. the passed container_height?
- Is there "remaining space" to distribute, or is the container sizing to content?
