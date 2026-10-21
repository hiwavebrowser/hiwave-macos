# GPU Gradient Shader Implementation Notes

## Session: 2026-01-16

This document contains detailed notes for implementing the GPU gradient shader correctly.

---

## 1. Reference: Cell-by-Cell Implementation (CORRECT)

The working implementation in `rustkit-renderer/src/lib.rs:2041-2198` uses this algorithm:

### 1.1 Angle Convention (CSS Standard)
```
- 0deg   = "to top"      → gradient goes upward
- 90deg  = "to right"    → gradient goes rightward
- 180deg = "to bottom"   → gradient goes downward
- 270deg = "to left"     → gradient goes leftward
```

### 1.2 Direction Vector Calculation
```rust
let angle_rad = angle_deg.to_radians();
let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());
// Direction vector is (sin_a, -cos_a)
```

**Verification:**
- At 0deg: sin(0)=0, cos(0)=1 → direction = (0, -1) ✓ upward
- At 90deg: sin(90)=1, cos(90)=0 → direction = (1, 0) ✓ rightward
- At 180deg: sin(180)=0, cos(180)=-1 → direction = (0, 1) ✓ downward
- At 270deg: sin(270)=-1, cos(270)=0 → direction = (-1, 0) ✓ leftward

### 1.3 Gradient Line Half-Length
```rust
let half_width = rect.width / 2.0;
let half_height = rect.height / 2.0;
let gradient_half_length = (half_width * sin_a.abs() + half_height * cos_a.abs()).max(0.001);
```

This is the CSS spec formula for the gradient line length: the projection of the rectangle onto the gradient direction, ensuring the gradient covers corner-to-corner.

### 1.4 Position-to-T Calculation
For a pixel at (cell_center_x, cell_center_y):

```rust
// Position relative to rect center
let px = cell_center_x - rect.x - half_width;
let py = cell_center_y - rect.y - half_height;

// Project onto gradient direction
let projection = px * sin_a + py * (-cos_a);

// Normalize to 0-1 range
let t = (projection / gradient_half_length + 1.0) / 2.0;
```

**Key insight:** The `+ 1.0` and `/ 2.0` shifts the range from [-1, 1] to [0, 1].

---

## 2. My Previous GPU Shader Implementation (INCORRECT)

### 2.1 What I Had in gradient.wgsl

```wgsl
fn linear_gradient_t(pixel_pos: vec2<f32>) -> f32 {
    let angle = gradient_params.param0;  // angle in radians
    let sin_a = sin(angle);
    let cos_a = cos(angle);

    // Gradient direction vector
    let dir = vec2<f32>(sin_a, -cos_a);  // ✓ Correct direction

    // Calculate gradient line length
    let half_w = gradient_params.rect_width * 0.5;
    let half_h = gradient_params.rect_height * 0.5;
    let gradient_length = abs(dir.x) * gradient_params.rect_width + abs(dir.y) * gradient_params.rect_height;
    // ^ BUG: Using full width/height instead of half

    // Pixel position relative to rect center
    let center = vec2<f32>(
        gradient_params.rect_x + half_w,
        gradient_params.rect_y + half_h
    );
    let rel_pos = pixel_pos - center;

    // Project onto gradient direction
    let proj = dot(rel_pos, dir);

    // Normalize to 0-1 range
    return (proj / gradient_length) + 0.5;
    // ^ BUG: Using / gradient_length + 0.5 instead of / gradient_half_length + 1.0) / 2.0
}
```

### 2.2 Bugs Identified

**Bug 1: Gradient Length Calculation**
- I used: `gradient_length = |dir.x| * rect_width + |dir.y| * rect_height`
- Should be: `gradient_half_length = |sin_a| * half_width + |cos_a| * half_height`
- Result: My gradient was using DOUBLE the correct length, making t values span [-0.5, 0.5] instead of [-1, 1]

**Bug 2: T Normalization**
- I used: `t = proj / gradient_length + 0.5`
- Should be: `t = (proj / gradient_half_length + 1.0) / 2.0`
- This is equivalent to: `t = proj / (2 * gradient_half_length) + 0.5`
- My formula was: `t = proj / gradient_length + 0.5`
- Since gradient_length = 2 * gradient_half_length, my proj division was correct
- But wait... let me recalculate:
  - Correct: proj ranges from -gradient_half_length to +gradient_half_length
  - Correct: t = (proj / gradient_half_length + 1.0) / 2.0
  - My gradient_length = 2 * gradient_half_length
  - My formula: t = proj / gradient_length + 0.5 = proj / (2 * gradient_half_length) + 0.5
  - For proj = -gradient_half_length: my_t = -0.5 + 0.5 = 0 ✓
  - For proj = +gradient_half_length: my_t = 0.5 + 0.5 = 1 ✓

Actually, my normalization might have been correct, but the gradient_length calculation was wrong because I used full rect dimensions instead of properly calculating the half-length.

Let me re-examine:
- `gradient_length = abs(dir.x) * rect_width + abs(dir.y) * rect_height`
- `= abs(sin_a) * rect_width + abs(cos_a) * rect_height`
- `= abs(sin_a) * 2 * half_width + abs(cos_a) * 2 * half_height`
- `= 2 * gradient_half_length`

So gradient_length = 2 * gradient_half_length.

Then: `t = proj / gradient_length + 0.5 = proj / (2 * gradient_half_length) + 0.5`

For t to equal the correct formula:
- Correct: `t = (proj / gradient_half_length + 1.0) / 2.0 = proj / (2 * gradient_half_length) + 0.5`

So actually my formula WAS mathematically correct! The bug must be elsewhere.

### 2.3 Other Potential Bugs

**Bug 3: Coordinate System**
- In WGSL, the pixel_pos comes from the vertex shader's interpolated position
- I pass pixel_pos = in.position which is the screen-space position AFTER viewport transform
- But `@builtin(position)` gives clip-space position transformed to framebuffer coordinates
- The x goes from 0 to viewport_width, y from 0 to viewport_height
- But y=0 is at the TOP in wgpu (unlike OpenGL where y=0 is bottom)

Let me verify this is handled correctly...

Actually, looking at my vertex shader:
```wgsl
out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
out.pixel_pos = in.position;
```

I'm passing `in.position` which is the VERTEX position (the corners of the quad), not the fragment position. The `pixel_pos` will be interpolated across the quad, which should give correct fragment positions.

Wait - but `in.position` in the vertex shader is the raw vertex data I'm passing, which ARE pixel coordinates. So this should be correct.

**Bug 4: Possible Issue with Repeating**
Let me check the repeating logic...

---

## 3. Test Case Analysis

Let me think about a simple test case: a 200x100 rect with a 90deg gradient (to right).

**Cell-by-cell:**
- angle_deg = 90, angle_rad = PI/2
- sin_a = 1, cos_a = 0
- direction = (1, 0) - rightward
- half_width = 100, half_height = 50
- gradient_half_length = 100 * 1 + 50 * 0 = 100

For pixel at x=rect.x (left edge):
- px = 0 - 100 = -100
- projection = -100 * 1 + 0 * 0 = -100
- t = (-100 / 100 + 1.0) / 2.0 = (0) / 2 = 0 ✓

For pixel at x=rect.x+200 (right edge):
- px = 200 - 100 = 100
- projection = 100 * 1 + 0 * 0 = 100
- t = (100 / 100 + 1.0) / 2.0 = (2) / 2 = 1 ✓

**My GPU shader:**
- gradient_length = |1| * 200 + |0| * 100 = 200

For pixel at x=rect.x (left edge):
- rel_pos.x = 0 - 100 = -100
- proj = -100 * 1 = -100
- t = -100 / 200 + 0.5 = -0.5 + 0.5 = 0 ✓

For pixel at x=rect.x+200 (right edge):
- rel_pos.x = 200 - 100 = 100
- proj = 100 * 1 = 100
- t = 100 / 200 + 0.5 = 0.5 + 0.5 = 1 ✓

Hmm, for this simple case, both produce the same result...

---

## 4. Deeper Investigation Needed

The simple horizontal case works. The bug must be in:
1. **Diagonal gradients** - Let me test 45deg
2. **The coord system in the shader** - Maybe the y-axis is flipped somewhere
3. **The actual shader code I wrote** - Need to re-examine

Let me test 45deg gradient on a 200x200 rect:

**Cell-by-cell:**
- angle_deg = 45, angle_rad = PI/4
- sin_a = 0.707, cos_a = 0.707
- direction = (0.707, -0.707) - toward top-right
- half_width = 100, half_height = 100
- gradient_half_length = 100 * 0.707 + 100 * 0.707 = 141.4

For pixel at top-right corner (x=200, y=0):
- px = 200 - 100 = 100
- py = 0 - 100 = -100
- projection = 100 * 0.707 + (-100) * (-0.707) = 70.7 + 70.7 = 141.4
- t = (141.4 / 141.4 + 1.0) / 2.0 = (2) / 2 = 1 ✓

For pixel at bottom-left corner (x=0, y=200):
- px = 0 - 100 = -100
- py = 200 - 100 = 100
- projection = -100 * 0.707 + 100 * (-0.707) = -70.7 - 70.7 = -141.4
- t = (-141.4 / 141.4 + 1.0) / 2.0 = (0) / 2 = 0 ✓

**My GPU shader:**
- gradient_length = 0.707 * 200 + 0.707 * 200 = 282.8

For pixel at top-right corner (x=200, y=0):
- rel_pos = (100, -100)
- dir = (0.707, -0.707)
- proj = 100 * 0.707 + (-100) * (-0.707) = 70.7 + 70.7 = 141.4
- t = 141.4 / 282.8 + 0.5 = 0.5 + 0.5 = 1 ✓

For pixel at bottom-left corner (x=0, y=200):
- rel_pos = (-100, 100)
- proj = -100 * 0.707 + 100 * (-0.707) = -70.7 - 70.7 = -141.4
- t = -141.4 / 282.8 + 0.5 = -0.5 + 0.5 = 0 ✓

Both produce the same result for 45deg too!

---

## 5. The Real Bug

If the math is correct, what's wrong? Let me look at other aspects:

### 5.1 Y-Axis Direction in WebGPU

In wgpu, the framebuffer y-coordinate increases DOWNWARD (y=0 is top).
In CSS, y increases DOWNWARD too.
So this should be consistent.

BUT - in my shader, I compute:
```wgsl
let py = cell_center_y - rect.y - half_h
```

Wait, I need to look at the ACTUAL shader code I wrote, not what I thought I wrote.

### 5.2 Need to Check: What did I actually write?

I need to recreate the shader carefully this time.

---

## 6. Correct Implementation Plan

1. **Create shader with EXACT same math as cell-by-cell**
2. **Test with simple cases first (0deg, 90deg, 180deg, 270deg)**
3. **Then test diagonal (45deg, 135deg)**
4. **Then test with different rect sizes**
5. **Finally add border-radius support**

### 6.1 Correct Shader Algorithm

```wgsl
fn linear_gradient_t(pixel_pos: vec2<f32>) -> f32 {
    // param0 = angle in radians
    let angle_rad = gradient_params.param0;
    let sin_a = sin(angle_rad);
    let cos_a = cos(angle_rad);

    // Direction vector: (sin_a, -cos_a) for CSS convention
    // At 0deg (to top): (0, -1)
    // At 90deg (to right): (1, 0)

    // Half dimensions
    let half_w = gradient_params.rect_width * 0.5;
    let half_h = gradient_params.rect_height * 0.5;

    // Gradient half-length (CSS spec)
    let gradient_half_length = abs(sin_a) * half_w + abs(cos_a) * half_h;

    // Rect center position
    let center_x = gradient_params.rect_x + half_w;
    let center_y = gradient_params.rect_y + half_h;

    // Position relative to rect center
    let px = pixel_pos.x - center_x;
    let py = pixel_pos.y - center_y;

    // Project onto gradient direction
    let projection = px * sin_a + py * (-cos_a);

    // Normalize to 0-1 range (matching cell-by-cell exactly)
    let t = (projection / gradient_half_length + 1.0) * 0.5;

    return t;
}
```

---

## 7. Files to Create/Modify

1. `crates/rustkit-renderer/src/shaders/gradient.wgsl` - The WGSL shader
2. `crates/rustkit-renderer/src/pipeline.rs` - GradientPipeline structs and creation
3. `crates/rustkit-renderer/src/lib.rs` - Integration code

---

## 8. Verification Strategy

After implementation, verify by:
1. Running `parity_test.py --test gradients` - Should not regress from 9.57%
2. Running `parity_test.py --test gradient-backgrounds` - Should not regress from 22.97%
3. If both are equal or better, the implementation is correct

---

## End of Notes
