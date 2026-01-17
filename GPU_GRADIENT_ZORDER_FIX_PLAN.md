# GPU Gradient Z-Order Fix Plan

## Problem Statement

When GPU gradient rendering is enabled (`RUSTKIT_GPU_GRADIENTS=1`), gradients render in incorrect z-order for nested elements. Specifically:

**Expected (CPU behavior):**
1. Parent gradient renders
2. Children backgrounds render ON TOP of parent gradient

**Actual (GPU behavior):**
1. Children backgrounds render (batched content)
2. Parent gradient renders ON TOP of children

This causes visual regressions where parent gradients cover child content.

### Affected Tests
| Test | CPU | GPU | Regression |
|------|-----|-----|------------|
| backgrounds | 18.88% | 25.17% | +6.29% |
| about | 11.93% | 15.96% | +4.03% |
| card-grid | 35.91% | 41.91% | +6.00% |

### Root Cause

The current architecture:
1. Display commands are processed in DOM order via `process_command()`
2. Solid colors, text, images go to **batched vertex buffers**
3. GPU gradients are **queued** (not rendered immediately)
4. At the end, `flush_to()` renders batched content first, then ALL gradients

This deferred rendering breaks z-order for nested elements.

---

## Solution Analysis

### Option 1: Inline GPU Rendering (Rejected)
Render each GPU gradient immediately instead of queueing.

**Pros:** Perfect z-order accuracy
**Cons:**
- Many GPU submissions (one per gradient)
- Pipeline state changes between each gradient
- Poor performance for pages with many gradients

**Verdict:** Too expensive for real-world use.

### Option 2: Z-Index Tracking (Complex)
Track z-index for every draw call and sort all primitives.

**Pros:** Theoretically optimal batching
**Cons:**
- Requires fundamental architecture changes
- Complex sorting logic for mixed primitive types
- Memory overhead for z-index storage

**Verdict:** High implementation complexity, risky.

### Option 3: Flush-Before-Gradient (Recommended)
Flush current batched content BEFORE each GPU gradient.

**Pros:**
- Simple to implement (pattern already exists for backdrop filters)
- Preserves correct z-order
- Still uses GPU for gradient rendering
- Incremental change to existing architecture

**Cons:**
- Reduces batching efficiency when many gradients
- More GPU submissions than current batched approach

**Verdict:** Best balance of correctness and simplicity.

### Option 4: Stacking Context Partitioning (Future Enhancement)
Batch gradients within stacking contexts, flush at boundaries.

**Pros:** Better batching than Option 3
**Cons:** More complex, requires CSS stacking context awareness

**Verdict:** Good enhancement after Option 3 works.

### Option 5: Hybrid CPU/GPU (Fallback)
Detect problematic cases and use CPU rendering.

**Pros:** Safe fallback
**Cons:** Complex detection logic, loses GPU benefits

**Verdict:** Could be used as fallback during transition.

---

## Implementation Plan: Option 3 (Flush-Before-Gradient)

### Phase 1: Modify `execute()` to Handle GPU Gradients

**File:** `crates/rustkit-renderer/src/lib.rs`

Currently, the fast path in `execute()` (lines 988-993) processes all commands then flushes once:

```rust
// Current implementation
for cmd in commands {
    self.process_command(cmd);
}
self.flush_to(target)?;
```

Change to detect gradients and flush before each one:

```rust
// New implementation
let has_gpu_gradients = self.gpu_gradients_enabled && commands.iter().any(|cmd| {
    matches!(cmd,
        DisplayCommand::LinearGradient { .. } |
        DisplayCommand::RadialGradient { .. } |
        DisplayCommand::ConicGradient { .. }
    )
});

if has_gpu_gradients {
    self.execute_with_gpu_gradients(commands, target)?;
} else {
    // Fast path - no GPU gradients
    for cmd in commands {
        self.process_command(cmd);
    }
    self.flush_to(target)?;
}
```

### Phase 2: Implement `execute_with_gpu_gradients()`

New method following the pattern of `execute_with_gpu_blur()`:

```rust
fn execute_with_gpu_gradients(
    &mut self,
    commands: &[DisplayCommand],
    target: &wgpu::TextureView,
) -> Result<(), RendererError> {
    let mut is_first_flush = true;

    for cmd in commands {
        // Check if this is a GPU gradient command
        let is_gpu_gradient = self.gpu_gradients_enabled && matches!(cmd,
            DisplayCommand::LinearGradient { .. } |
            DisplayCommand::RadialGradient { .. } |
            DisplayCommand::ConicGradient { .. }
        );

        if is_gpu_gradient {
            // Flush batched content FIRST (before gradient)
            // This ensures parent's batched children render before gradient
            self.flush_batches_for_gradient(target, is_first_flush);
            is_first_flush = false;

            // Now render the gradient directly (inline, not queued)
            self.render_gpu_gradient_inline(cmd, target);
        } else {
            // Process command normally (batched)
            self.process_command(cmd);
        }
    }

    // Flush any remaining batched content
    if !self.color_vertices.is_empty() || !self.texture_vertices.is_empty() {
        self.flush_batches_for_gradient(target, is_first_flush);
    }

    Ok(())
}
```

### Phase 3: Implement `flush_batches_for_gradient()`

Similar to `flush_batches_to()` but optimized for gradient interleaving:

```rust
fn flush_batches_for_gradient(&mut self, target: &wgpu::TextureView, clear: bool) {
    if self.color_vertices.is_empty() && self.texture_vertices.is_empty() {
        return;
    }

    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Gradient Interleave Flush"),
    });

    {
        let load_op = if clear {
            wgpu::LoadOp::Clear(wgpu::Color::WHITE)
        } else {
            wgpu::LoadOp::Load
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Batched Content Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Draw solid colors
        if !self.color_vertices.is_empty() {
            // ... (same as flush_batches_to)
        }

        // Draw textured quads
        if !self.texture_vertices.is_empty() {
            // ... (same as flush_batches_to)
        }
    }

    self.queue.submit(std::iter::once(encoder.finish()));

    // Clear batches after flushing
    self.color_vertices.clear();
    self.color_indices.clear();
    self.texture_vertices.clear();
    self.texture_indices.clear();
}
```

### Phase 4: Implement `render_gpu_gradient_inline()`

Render a single gradient immediately (not queued):

```rust
fn render_gpu_gradient_inline(&mut self, cmd: &DisplayCommand, target: &wgpu::TextureView) {
    match cmd {
        DisplayCommand::LinearGradient { rect, direction, stops, repeating, border_radius } => {
            // Convert stops to normalized format
            let normalized_stops = self.normalize_gradient_stops(stops);
            let angle_rad = self.direction_to_angle(*direction);

            // Render immediately using existing GPU method
            self.render_linear_gradient_gpu_with_clear(
                target,
                *rect,
                angle_rad,
                &normalized_stops,
                *repeating,
                *border_radius,
                None, // LoadOp::Load to preserve previous content
            );
        }
        DisplayCommand::RadialGradient { rect, shape, size, center, stops, repeating, border_radius } => {
            let normalized_stops = self.normalize_gradient_stops(stops);
            let (rx, ry) = self.calculate_radial_radii(*size, *shape, *rect, *center);

            self.render_radial_gradient_gpu(
                target,
                *rect,
                rx, ry,
                *center,
                &normalized_stops,
                *repeating,
                *border_radius,
            );
        }
        DisplayCommand::ConicGradient { rect, from_angle, center, stops, repeating, border_radius } => {
            let normalized_stops = self.normalize_gradient_stops(stops);

            self.render_conic_gradient_gpu(
                target,
                *rect,
                from_angle.to_radians(),
                *center,
                &normalized_stops,
                *repeating,
                *border_radius,
            );
        }
        _ => {}
    }
}
```

### Phase 5: Modify `draw_*_gradient()` Methods

The existing `draw_linear_gradient()`, `draw_radial_gradient()`, and `draw_conic_gradient()` methods currently queue gradients when `gpu_gradients_enabled`.

**Option A:** Keep the queue for `flush_to()` path (non-gradient pages)
**Option B:** Remove queueing entirely, always use inline path

Recommend **Option A** initially for safety, then migrate to **Option B** once stable.

For Option A, modify `process_command()` to NOT call `draw_*_gradient()` when in GPU gradient mode:

```rust
// In process_command():
DisplayCommand::LinearGradient { rect, direction, stops, repeating, border_radius } => {
    if !self.gpu_gradients_enabled {
        // CPU path
        self.draw_linear_gradient(*rect, *direction, stops, *repeating, *border_radius);
    }
    // GPU path handled by execute_with_gpu_gradients() directly
}
```

### Phase 6: Update `flush_to()`

Remove GPU gradient rendering from `flush_to()` since it's now handled inline:

```rust
fn flush_to(&mut self, target: &wgpu::TextureView) -> Result<(), RendererError> {
    // Clear gradient queues (should be empty in new path, but safety)
    self.gradient_queue.clear();
    self.radial_gradient_queue.clear();
    self.conic_gradient_queue.clear();

    // Only render batched content - no GPU gradients here anymore
    // ... existing batched content rendering ...

    Ok(())
}
```

---

## Testing Plan

### Unit Tests

1. **Z-Order Test:** Create test with parent gradient + child solid background
   - Verify child renders on top of parent gradient

2. **Multiple Gradients Test:** Multiple gradients in DOM order
   - Verify they render in correct order

3. **Mixed Content Test:** Gradients interleaved with text and images
   - Verify all z-order is correct

### Parity Tests

Run full parity suite comparing:
- CPU gradients (baseline)
- New GPU gradients with z-order fix

Target: All tests should have ≤ CPU parity (no regressions)

### Performance Tests

Measure frame times for:
- Page with 0 gradients (should be unchanged)
- Page with 1 gradient (slight overhead expected)
- Page with 10 gradients (moderate overhead)
- Page with 100 gradients (stress test)

---

## Performance Optimization (Phase 7)

After correctness is verified, optimize batching:

### Optimization 1: Batch Consecutive Gradients

If multiple gradients appear consecutively without intervening content:
- Batch them together
- Single flush before, render all gradients, single flush after

```rust
// Pseudocode
if is_gpu_gradient(cmd) && is_gpu_gradient(next_cmd) {
    // Don't flush between consecutive gradients
    render_gradient(cmd);
    // Continue to next gradient
} else if is_gpu_gradient(cmd) {
    flush_batches();
    render_gradient(cmd);
}
```

### Optimization 2: Stacking Context Batching

Gradients within the same stacking context can be batched:
- Track stacking context depth
- Only flush when crossing stacking context boundaries

### Optimization 3: Dirty Region Tracking

Only re-render gradients in dirty regions during incremental updates.

---

## Rollback Strategy

If issues arise:
1. Set `RUSTKIT_GPU_GRADIENTS=0` (environment variable)
2. Falls back to CPU gradient rendering
3. No code changes required

---

## Success Criteria

1. **Correctness:**
   - `backgrounds` test: ≤ 18.88% (matches CPU)
   - `about` test: ≤ 11.93% (matches CPU)
   - `card-grid` test: ≤ 35.91% (matches CPU)
   - All gradient tests: ≤ CPU baseline

2. **Performance:**
   - Pages with no gradients: < 1% overhead
   - Pages with few gradients (1-5): < 5% overhead
   - Pages with many gradients (10+): < 20% overhead

3. **Stability:**
   - No crashes or rendering artifacts
   - Consistent results across multiple runs

---

## Timeline Estimate

| Phase | Description | Effort |
|-------|-------------|--------|
| 1 | Modify `execute()` | 1 hour |
| 2 | Implement `execute_with_gpu_gradients()` | 2 hours |
| 3 | Implement `flush_batches_for_gradient()` | 1 hour |
| 4 | Implement `render_gpu_gradient_inline()` | 2 hours |
| 5 | Modify `draw_*_gradient()` methods | 1 hour |
| 6 | Update `flush_to()` | 30 min |
| Testing | Parity and performance testing | 2 hours |
| **Total** | | **~10 hours** |

---

## Files to Modify

| File | Changes |
|------|---------|
| `crates/rustkit-renderer/src/lib.rs` | Main implementation |
| Test files | Add z-order specific tests |

No other crates need modification - the fix is entirely within the renderer.
