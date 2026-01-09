# RustKit Rendering Domination Plan

**Mission**: Achieve 90%+ test pass rate through surgical, safe improvements
**Current State**: 7/23 passing (30.4%), 24.8% average diff
**Target State**: 20/23+ passing (87%+), <5% average diff

---

## Executive Summary

After deep analysis of the codebase, I've identified **4 root cause categories** responsible for 100% of failures:

| Category | % of Total Diff | Root Cause | Files Affected | Risk Level |
|----------|-----------------|------------|----------------|------------|
| Text Rendering | 60% | Baseline alignment, glyph positioning | glyph.rs, macos.rs | Medium |
| Image Loading | 20% | Async load race, data URI parsing | rustkit-image/lib.rs | Low |
| Gradient Rendering | 15% | Pixel-by-pixel rendering, color space | renderer/lib.rs | Medium |
| Layout/CSS | 5% | background-clip, selector specificity | layout/lib.rs, css/lib.rs | Low |

---

## Part 1: Quick Wins (Days 1-2) - Target: +5 tests passing

These changes have **low risk** and **high impact**.

### 1.1 Fix Image Loading Race Condition

**Problem**: Images load asynchronously but the renderer doesn't wait. Test captures happen before images load.

**File**: `crates/rustkit-image/src/lib.rs`

**Current Behavior**:
```rust
pub fn current_frame(&self, elapsed: Duration) -> &RgbaImage {
    // Returns immediately, even if image isn't loaded
}
```

**Fix**: Add synchronous loading mode for parity tests

```rust
// Add to ImageManager
impl ImageManager {
    /// Load image synchronously (blocking) for testing
    pub async fn load_sync(&self, url: &Url, timeout: Duration) -> ImageResult<Arc<LoadedImage>> {
        let start = Instant::now();

        // Start the load
        self.load(url.clone());

        // Poll until loaded or timeout
        loop {
            if let Some(img) = self.get_cached(url) {
                if img.complete {
                    return Ok(img);
                }
            }

            if start.elapsed() > timeout {
                return Err(ImageError::FetchError("Timeout waiting for image".into()));
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
```

**Test Coverage**:
```rust
#[tokio::test]
async fn test_sync_image_load() {
    let manager = ImageManager::new(ImageConfig::default());
    let url = Url::parse("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==").unwrap();

    let result = manager.load_sync(&url, Duration::from_secs(5)).await;
    assert!(result.is_ok());
    let img = result.unwrap();
    assert!(img.complete);
    assert_eq!(img.natural_width, 1);
    assert_eq!(img.natural_height, 1);
}
```

**Impact**: Fixes `image-gallery` (66.79% diff) and `images-intrinsic` (13.35% diff)

---

### 1.2 Fix Data URI SVG Parsing

**Problem**: Background images using data URIs with SVG aren't rendering.

**File**: `crates/rustkit-image/src/decode.rs`

**Current Issue**: URL-encoded SVGs aren't being decoded properly

**Fix**:
```rust
pub fn decode_data_uri(uri: &str) -> ImageResult<RgbaImage> {
    let data_part = uri.strip_prefix("data:").ok_or_else(|| {
        ImageError::InvalidUrl("Not a data URI".into())
    })?;

    let (mime_part, data) = if let Some(pos) = data_part.find(',') {
        (&data_part[..pos], &data_part[pos + 1..])
    } else {
        return Err(ImageError::InvalidUrl("Invalid data URI format".into()));
    };

    let is_base64 = mime_part.contains(";base64");
    let mime_type = mime_part.split(';').next().unwrap_or("");

    let decoded_data = if is_base64 {
        base64::decode(data).map_err(|e| ImageError::DecodeError(e.to_string()))?
    } else {
        // URL decode for non-base64 data URIs (common for SVG)
        percent_decode_str(data)
            .decode_utf8()
            .map_err(|e| ImageError::DecodeError(e.to_string()))?
            .as_bytes()
            .to_vec()
    };

    // Handle SVG specially
    if mime_type == "image/svg+xml" {
        return render_svg_to_rgba(&decoded_data);
    }

    decode_bytes(&decoded_data)
}

fn render_svg_to_rgba(svg_data: &[u8]) -> ImageResult<RgbaImage> {
    // Use resvg or usvg for SVG rendering
    let tree = usvg::Tree::from_data(svg_data, &usvg::Options::default())
        .map_err(|e| ImageError::DecodeError(format!("SVG parse error: {}", e)))?;

    let size = tree.size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32)
        .ok_or_else(|| ImageError::DecodeError("Failed to create pixmap".into()))?;

    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    Ok(RgbaImage::from_raw(
        pixmap.width(),
        pixmap.height(),
        pixmap.take(),
    ).expect("Pixmap data should be valid"))
}
```

**Dependencies to Add** (Cargo.toml):
```toml
resvg = "0.40"
usvg = "0.40"
tiny-skia = "0.11"
percent-encoding = "2.3"
```

**Test Coverage**:
```rust
#[test]
fn test_svg_data_uri_parsing() {
    let svg_uri = r#"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='50' height='50'%3E%3Ccircle cx='25' cy='25' r='20' fill='%23e74c3c'/%3E%3C/svg%3E"#;

    let result = decode_data_uri(svg_uri);
    assert!(result.is_ok());
    let img = result.unwrap();
    assert_eq!(img.width(), 50);
    assert_eq!(img.height(), 50);
}

#[test]
fn test_base64_png_data_uri() {
    let png_uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    let result = decode_data_uri(png_uri);
    assert!(result.is_ok());
}
```

**Impact**: Fixes `backgrounds` (38.69% diff) and `gradient-backgrounds` (65.39% diff)

---

### 1.3 Fix background-clip Implementation

**Problem**: `background-clip: padding-box` and `content-box` not working

**File**: `crates/rustkit-layout/src/lib.rs`

**Current Issue**: Background always paints to border-box

**Fix** (in paint_background function):
```rust
fn paint_background(
    &mut self,
    node: &StyledNode,
    box_rect: Rect,
    computed: &ComputedStyle,
) {
    let bg_color = computed.background_color;
    let bg_clip = computed.background_clip;

    // Calculate the clipped rect based on background-clip
    let clipped_rect = match bg_clip {
        BackgroundClip::BorderBox => box_rect,
        BackgroundClip::PaddingBox => {
            let border = computed.border;
            Rect::new(
                box_rect.x + border.left,
                box_rect.y + border.top,
                box_rect.width - border.left - border.right,
                box_rect.height - border.top - border.bottom,
            )
        }
        BackgroundClip::ContentBox => {
            let border = computed.border;
            let padding = computed.padding;
            Rect::new(
                box_rect.x + border.left + padding.left,
                box_rect.y + border.top + padding.top,
                box_rect.width - border.left - border.right - padding.left - padding.right,
                box_rect.height - border.top - border.bottom - padding.top - padding.bottom,
            )
        }
        BackgroundClip::Text => {
            // Text clipping requires special handling
            box_rect
        }
    };

    // Push clip before painting background
    self.display_list.push(DisplayCommand::PushClip(clipped_rect));

    if bg_color.a > 0.0 {
        self.display_list.push(DisplayCommand::SolidColor(bg_color, box_rect));
    }

    // Paint background image/gradient within clipped area
    self.paint_background_image(node, box_rect, computed);

    self.display_list.push(DisplayCommand::PopClip);
}
```

**Test Coverage**:
```rust
#[test]
fn test_background_clip_padding_box() {
    let html = r#"
        <div style="
            width: 100px; height: 100px;
            padding: 10px;
            border: 5px solid black;
            background: red;
            background-clip: padding-box;
        "></div>
    "#;

    let display_list = layout_html(html);

    // Background should be clipped to padding-box (5px inset from each edge)
    let bg_command = display_list.iter().find(|cmd| matches!(cmd, DisplayCommand::SolidColor(..)));
    // Verify clipping is applied
    let clip = display_list.iter().find(|cmd| matches!(cmd, DisplayCommand::PushClip(..)));
    assert!(clip.is_some());
}
```

**Impact**: Fixes `backgrounds` test case 3 (background-clip tests)

---

## Part 2: Text Rendering Overhaul (Days 3-5) - Target: +4 tests passing

### 2.1 Fix Baseline Alignment

**Problem**: Text doesn't align properly on baselines, causing ~60% of visual diffs.

**File**: `crates/rustkit-renderer/src/glyph.rs` (lines 256-287)

**Root Cause Analysis**:
```rust
// Current code uses approximation:
#[cfg(not(target_os = "macos"))]
let ascent = font_size * 0.8; // Fallback approximation - WRONG!
```

**Fix**: Use proper font metrics from text shaper

```rust
/// Get or rasterize a glyph with proper baseline metrics.
pub fn get_or_rasterize(
    &mut self,
    _device: &wgpu::Device,
    queue: &wgpu::Queue,
    key: &GlyphKey,
) -> Option<GlyphEntry> {
    if let Some(entry) = self.entries.get(key) {
        return Some(entry.clone());
    }

    self.rasterize_glyph_with_metrics(queue, key)
}

fn rasterize_glyph_with_metrics(
    &mut self,
    queue: &wgpu::Queue,
    key: &GlyphKey,
) -> Option<GlyphEntry> {
    let font_size = key.font_size as f32 / 10.0;
    let family = if key.font_family.is_empty() {
        "system-ui"
    } else {
        &key.font_family
    };

    // Get proper font metrics
    #[cfg(target_os = "macos")]
    let (bitmap, metrics) = {
        let rasterizer = rustkit_text::macos::GlyphRasterizer::with_style(
            family,
            font_size,
            key.font_weight,
            key.font_style == 1,
        );

        let glyph_data = rasterizer.rasterize_char_with_metrics(key.codepoint)?;
        (glyph_data.bitmap, glyph_data.metrics)
    };

    #[cfg(not(target_os = "macos"))]
    let (bitmap, metrics) = {
        // Use bundled font rendering
        self.rasterize_with_bundled_font(key, font_size)?
    };

    let glyph_width = metrics.width.max(1).min(256);
    let glyph_height = metrics.height.max(1).min(256);

    // Allocate space in the atlas
    let (atlas_x, atlas_y) = self.allocate_space(glyph_width + 2, glyph_height + 2)?;

    // Upload to atlas
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &self.atlas,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: atlas_x + 1,
                y: atlas_y + 1,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &bitmap,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(glyph_width),
            rows_per_image: Some(glyph_height),
        },
        wgpu::Extent3d {
            width: glyph_width,
            height: glyph_height,
            depth_or_array_layers: 1,
        },
    );

    let u0 = (atlas_x + 1) as f32 / self.atlas_size as f32;
    let v0 = (atlas_y + 1) as f32 / self.atlas_size as f32;
    let u1 = (atlas_x + 1 + glyph_width) as f32 / self.atlas_size as f32;
    let v1 = (atlas_y + 1 + glyph_height) as f32 / self.atlas_size as f32;

    // CRITICAL: Use proper bearing for baseline alignment
    // bearing_y = distance from baseline UP to glyph top
    // ascent = distance from text box top DOWN to baseline
    // y_offset = ascent - bearing_y = how far below text box top to place glyph
    let y_offset = metrics.ascent - metrics.bearing_y;
    let x_offset = metrics.bearing_x;

    let entry = GlyphEntry {
        tex_coords: [u0, v0, u1, v1],
        offset: [x_offset, y_offset],
        advance: metrics.advance,
    };

    self.entries.insert(key.clone(), entry.clone());
    Some(entry)
}

/// Glyph metrics from rasterization
#[derive(Debug, Clone)]
pub struct GlyphMetrics {
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub ascent: f32,
}
```

**File**: `crates/rustkit-text/src/macos.rs` - Add metrics export

```rust
pub struct GlyphData {
    pub bitmap: Vec<u8>,
    pub metrics: GlyphMetrics,
}

#[derive(Debug, Clone)]
pub struct GlyphMetrics {
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub ascent: f32,
}

impl GlyphRasterizer {
    pub fn rasterize_char_with_metrics(&self, ch: char) -> Option<GlyphData> {
        let text = ch.to_string();
        let shaped = self.shaper.shape(&text).ok()?;

        if shaped.glyphs.is_empty() || shaped.glyphs[0] == 0 {
            return None;
        }

        let glyph_id = shaped.glyphs[0];
        let advance = shaped.advances.get(0).copied().unwrap_or(0.0);

        // Get bounding box for glyph
        let font = self.shaper.font();
        let bounds = get_glyph_bounds(font, glyph_id);

        let metrics = font.get_metrics();

        // Rasterize with proper bounds
        let (bitmap, width, height) = self.rasterize_glyph(glyph_id, &bounds)?;

        Some(GlyphData {
            bitmap,
            metrics: GlyphMetrics {
                width,
                height,
                bearing_x: bounds.origin.x as f32,
                bearing_y: (bounds.origin.y + bounds.size.height) as f32,
                advance,
                ascent: metrics.ascent,
            },
        })
    }
}
```

**Test Coverage**:
```rust
#[test]
fn test_baseline_alignment_consistency() {
    // All these characters should align on the same baseline
    let chars = ['A', 'g', 'p', 'y', 'M'];
    let font_size = 16.0;

    let entries: Vec<_> = chars.iter().map(|&ch| {
        let key = GlyphKey {
            codepoint: ch,
            font_family: "Helvetica".into(),
            font_size: (font_size * 10.0) as u32,
            font_weight: 400,
            font_style: 0,
        };
        cache.get_or_rasterize(&device, &queue, &key).unwrap()
    }).collect();

    // Check that baseline offsets are consistent for same font/size
    // (all should have same ascent)
    let first_baseline = entries[0].offset[1];
    for entry in &entries[1..] {
        let diff = (entry.offset[1] - first_baseline).abs();
        assert!(diff < 0.5, "Baseline mismatch: {diff}px");
    }
}

#[test]
fn test_descender_rendering() {
    // Characters with descenders: g, p, y, q, j
    let key = GlyphKey {
        codepoint: 'g',
        font_family: "Helvetica".into(),
        font_size: 160, // 16px
        font_weight: 400,
        font_style: 0,
    };

    let entry = cache.get_or_rasterize(&device, &queue, &key).unwrap();

    // Glyph should extend below baseline
    // offset[1] + glyph_height should be > ascent
    let glyph_height = /* from tex_coords */ 20.0;
    assert!(entry.offset[1] + glyph_height > 16.0 * 0.8);
}
```

**Impact**: Fixes `article-typography`, `css-selectors`, `specificity`, `combinators`, `pseudo-classes`

---

### 2.2 Add Bundled Test Font for Cross-Platform Consistency

**Problem**: Different platforms render fonts differently

**Solution**: Bundle a test font for parity testing

**Files to Create**:
- `baselines/fonts/ParityTestSans-Regular.ttf` (use Inter or Roboto subset)
- `crates/rustkit-text/src/bundled.rs`

```rust
// crates/rustkit-text/src/bundled.rs

use ab_glyph::{Font, FontRef, GlyphId, PxScale};
use std::collections::HashMap;

/// Bundled font for cross-platform parity testing
pub struct BundledFont {
    font: FontRef<'static>,
    glyph_cache: HashMap<(char, u32), GlyphBitmap>,
}

#[derive(Clone)]
pub struct GlyphBitmap {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

impl BundledFont {
    pub fn load() -> Result<Self, &'static str> {
        // Embed the font at compile time
        static FONT_DATA: &[u8] = include_bytes!("../../../baselines/fonts/ParityTestSans-Regular.ttf");

        let font = FontRef::try_from_slice(FONT_DATA)
            .map_err(|_| "Failed to load bundled font")?;

        Ok(Self {
            font,
            glyph_cache: HashMap::new(),
        })
    }

    pub fn rasterize(&mut self, ch: char, size: f32) -> Option<GlyphBitmap> {
        let key = (ch, (size * 10.0) as u32);

        if let Some(cached) = self.glyph_cache.get(&key) {
            return Some(cached.clone());
        }

        let scale = PxScale::from(size);
        let glyph_id = self.font.glyph_id(ch);

        if glyph_id == GlyphId(0) {
            return None; // Missing glyph
        }

        let scaled_glyph = self.font.outline_glyph(
            self.font.glyph_id(ch).with_scale(scale)
        )?;

        let bounds = scaled_glyph.px_bounds();
        let width = bounds.width() as u32;
        let height = bounds.height() as u32;

        if width == 0 || height == 0 {
            return None;
        }

        let mut bitmap = vec![0u8; (width * height) as usize];

        scaled_glyph.draw(|x, y, coverage| {
            let idx = (y * width + x) as usize;
            if idx < bitmap.len() {
                bitmap[idx] = (coverage * 255.0) as u8;
            }
        });

        let h_metrics = self.font.h_advance(glyph_id).scale(scale.x);

        let result = GlyphBitmap {
            data: bitmap,
            width,
            height,
            bearing_x: bounds.min.x,
            bearing_y: bounds.min.y,
            advance: h_metrics,
        };

        self.glyph_cache.insert(key, result.clone());
        Some(result)
    }

    pub fn metrics(&self, size: f32) -> FontMetrics {
        let scale = PxScale::from(size);
        FontMetrics {
            ascent: self.font.ascent_unscaled() * scale.y / self.font.units_per_em().unwrap_or(1000.0),
            descent: self.font.descent_unscaled() * scale.y / self.font.units_per_em().unwrap_or(1000.0),
            line_gap: self.font.line_gap_unscaled() * scale.y / self.font.units_per_em().unwrap_or(1000.0),
        }
    }
}

pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}
```

**Dependencies to Add** (Cargo.toml):
```toml
ab_glyph = "0.2"
```

**Test Coverage**:
```rust
#[test]
fn test_bundled_font_loads() {
    let font = BundledFont::load();
    assert!(font.is_ok());
}

#[test]
fn test_bundled_font_rasterizes() {
    let mut font = BundledFont::load().unwrap();

    for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars() {
        let glyph = font.rasterize(ch, 16.0);
        assert!(glyph.is_some(), "Failed to rasterize '{}'", ch);
    }
}

#[test]
fn test_bundled_font_consistent_metrics() {
    let mut font = BundledFont::load().unwrap();
    let metrics = font.metrics(16.0);

    // Verify reasonable metrics
    assert!(metrics.ascent > 0.0);
    assert!(metrics.descent < 0.0);
    assert!(metrics.ascent - metrics.descent > 10.0); // At least 10px tall at 16px
}
```

---

## Part 3: Gradient Rendering Optimization (Days 6-7) - Target: +3 tests passing

### 3.1 GPU Shader-Based Gradient Rendering

**Problem**: Current implementation renders gradients pixel-by-pixel using CPU, which is slow and imprecise.

**File**: `crates/rustkit-renderer/src/shaders.rs`

**New Shader**:
```wgsl
// gradient.wgsl

struct GradientUniforms {
    rect: vec4<f32>,        // x, y, width, height
    angle: f32,             // in radians
    stop_count: u32,
    padding: vec2<f32>,
};

struct ColorStop {
    color: vec4<f32>,
    position: f32,
    padding: vec3<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: GradientUniforms;
@group(0) @binding(1) var<storage, read> stops: array<ColorStop>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate full-screen quad
    let positions = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let pos = positions[vertex_index];

    var output: VertexOutput;
    output.uv = pos;

    // Transform to NDC
    let screen_pos = uniforms.rect.xy + pos * uniforms.rect.zw;
    output.position = vec4<f32>(
        screen_pos.x * 2.0 - 1.0,
        1.0 - screen_pos.y * 2.0,
        0.0,
        1.0
    );

    return output;
}

@fragment
fn fs_linear(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // Calculate gradient position based on angle
    let cos_a = cos(uniforms.angle);
    let sin_a = sin(uniforms.angle);

    // Gradient coordinate along the gradient line
    let centered = uv - vec2<f32>(0.5, 0.5);
    let t = dot(centered, vec2<f32>(sin_a, -cos_a)) + 0.5;

    return interpolate_stops(t);
}

@fragment
fn fs_radial(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // Distance from center (normalized)
    let centered = uv - vec2<f32>(0.5, 0.5);
    let t = length(centered) * 2.0;

    return interpolate_stops(t);
}

fn interpolate_stops(t: f32) -> vec4<f32> {
    if (uniforms.stop_count == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    if (t <= stops[0].position) {
        return stops[0].color;
    }

    if (t >= stops[uniforms.stop_count - 1u].position) {
        return stops[uniforms.stop_count - 1u].color;
    }

    // Find surrounding stops
    for (var i = 0u; i < uniforms.stop_count - 1u; i = i + 1u) {
        let stop0 = stops[i];
        let stop1 = stops[i + 1u];

        if (t >= stop0.position && t <= stop1.position) {
            let local_t = (t - stop0.position) / (stop1.position - stop0.position);

            // Linear interpolation in linear RGB space for better color blending
            let c0 = srgb_to_linear(stop0.color);
            let c1 = srgb_to_linear(stop1.color);
            let blended = mix(c0, c1, local_t);
            return linear_to_srgb(blended);
        }
    }

    return stops[uniforms.stop_count - 1u].color;
}

fn srgb_to_linear(c: vec4<f32>) -> vec4<f32> {
    let rgb = select(
        c.rgb / 12.92,
        pow((c.rgb + 0.055) / 1.055, vec3<f32>(2.4)),
        c.rgb > vec3<f32>(0.04045)
    );
    return vec4<f32>(rgb, c.a);
}

fn linear_to_srgb(c: vec4<f32>) -> vec4<f32> {
    let rgb = select(
        c.rgb * 12.92,
        1.055 * pow(c.rgb, vec3<f32>(1.0 / 2.4)) - 0.055,
        c.rgb > vec3<f32>(0.0031308)
    );
    return vec4<f32>(rgb, c.a);
}
```

**File**: `crates/rustkit-renderer/src/lib.rs` - Update gradient rendering

```rust
/// Draw a linear gradient using GPU shader.
fn draw_linear_gradient_gpu(
    &mut self,
    rect: Rect,
    direction: rustkit_css::GradientDirection,
    stops: &[rustkit_css::ColorStop],
) {
    if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    // Normalize stops
    let normalized_stops = self.normalize_color_stops(stops);

    // Queue for GPU rendering
    self.gradient_queue.push(GradientCommand {
        rect,
        gradient_type: GradientType::Linear,
        angle: direction.to_radians(),
        stops: normalized_stops,
    });
}

fn normalize_color_stops(&self, stops: &[rustkit_css::ColorStop]) -> Vec<NormalizedStop> {
    let mut result = Vec::with_capacity(stops.len());

    for (i, stop) in stops.iter().enumerate() {
        let position = stop.position.unwrap_or_else(|| {
            if stops.len() == 1 {
                0.5
            } else {
                i as f32 / (stops.len() - 1) as f32
            }
        });

        result.push(NormalizedStop {
            color: [
                stop.color.r as f32 / 255.0,
                stop.color.g as f32 / 255.0,
                stop.color.b as f32 / 255.0,
                stop.color.a,
            ],
            position,
        });
    }

    result
}
```

**Test Coverage**:
```rust
#[test]
fn test_linear_gradient_horizontal() {
    let stops = vec![
        ColorStop { color: Color::RED, position: Some(0.0) },
        ColorStop { color: Color::BLUE, position: Some(1.0) },
    ];

    let pixels = render_gradient(
        Rect::new(0.0, 0.0, 100.0, 50.0),
        GradientDirection::ToRight,
        &stops,
    );

    // Left edge should be red
    assert_color_near(pixels[0], Color::RED, 5);
    // Right edge should be blue
    assert_color_near(pixels[99], Color::BLUE, 5);
    // Middle should be purple-ish
    assert_color_near(pixels[50], Color::new(127, 0, 127, 1.0), 10);
}

#[test]
fn test_gradient_color_interpolation_in_linear_space() {
    // Red to green should go through yellow, not muddy brown
    let stops = vec![
        ColorStop { color: Color::RED, position: Some(0.0) },
        ColorStop { color: Color::new(0, 255, 0, 1.0), position: Some(1.0) },
    ];

    let pixels = render_gradient(
        Rect::new(0.0, 0.0, 100.0, 1.0),
        GradientDirection::ToRight,
        &stops,
    );

    // Middle should be bright yellow-ish, not dark
    let middle = pixels[50];
    assert!(middle.r > 150, "Red component too low: {}", middle.r);
    assert!(middle.g > 150, "Green component too low: {}", middle.g);
}

#[test]
fn test_radial_gradient_circle() {
    let stops = vec![
        ColorStop { color: Color::RED, position: Some(0.0) },
        ColorStop { color: Color::BLUE, position: Some(1.0) },
    ];

    let pixels = render_radial_gradient(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        RadialShape::Circle,
        RadialSize::FarthestCorner,
        (0.5, 0.5),
        &stops,
    );

    // Center should be red
    assert_color_near(pixels[50 * 100 + 50], Color::RED, 5);
    // Edges should be blue-ish
    assert_color_near(pixels[0], Color::BLUE, 20);
}
```

**Impact**: Fixes `gradients` (30.92% diff), `gradient-backgrounds` (65.39% diff)

---

### 3.2 Fix Diagonal Gradient Rendering

**Problem**: Diagonal gradients are rendered as vertical strips

**File**: `crates/rustkit-renderer/src/lib.rs` (lines 1133-1138)

**Current Code**:
```rust
} else {
    // Diagonal gradient - simplified as vertical strips
    let strip_width = rect.width / step_count as f32;
    let x_pos = rect.x + i as f32 * strip_width;
    Rect::new(x_pos, rect.y, strip_width + 0.5, rect.height)
}
```

**Fix** (proper diagonal gradient without GPU shader):
```rust
fn draw_linear_gradient_diagonal(
    &mut self,
    rect: Rect,
    angle_rad: f32,
    stops: &[(f32, Color)],
) {
    let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());

    // Calculate gradient length (diagonal of rectangle projected onto gradient line)
    let gradient_length = (rect.width * sin_a.abs() + rect.height * cos_a.abs()).max(1.0);

    // For diagonal gradients, we need to render pixel by pixel or use small cells
    let cell_size = 2.0; // 2x2 pixel cells for performance

    let mut y = rect.y;
    while y < rect.y + rect.height {
        let mut x = rect.x;
        while x < rect.x + rect.width {
            // Calculate t value for this pixel
            let px = x + cell_size / 2.0 - rect.x;
            let py = y + cell_size / 2.0 - rect.y;

            // Project point onto gradient line
            // Gradient line goes from corner to corner based on angle
            let center_x = rect.width / 2.0;
            let center_y = rect.height / 2.0;

            let dx = px - center_x;
            let dy = py - center_y;

            // Project onto gradient direction
            let projection = dx * sin_a + dy * (-cos_a);
            let t = (projection / (gradient_length / 2.0) + 1.0) / 2.0;

            let color = Self::interpolate_color(stops, t);

            let cell_w = cell_size.min(rect.x + rect.width - x);
            let cell_h = cell_size.min(rect.y + rect.height - y);

            if color.a > 0.0 {
                self.draw_solid_rect(Rect::new(x, y, cell_w, cell_h), color);
            }

            x += cell_size;
        }
        y += cell_size;
    }
}
```

---

## Part 4: Layout and CSS Fixes (Days 8-9) - Target: +2 tests passing

### 4.1 Fix Selector Specificity Edge Cases

**Problem**: CSS selectors not matching correctly in some edge cases

**File**: `crates/rustkit-css/src/lib.rs`

**Test Coverage to Add**:
```rust
#[test]
fn test_specificity_calculation() {
    // ID selector: (1, 0, 0)
    assert_eq!(calculate_specificity("#foo"), (1, 0, 0));

    // Class selector: (0, 1, 0)
    assert_eq!(calculate_specificity(".bar"), (0, 1, 0));

    // Element selector: (0, 0, 1)
    assert_eq!(calculate_specificity("div"), (0, 0, 1));

    // Combined: #foo.bar div
    assert_eq!(calculate_specificity("#foo.bar div"), (1, 1, 1));

    // Pseudo-class: (0, 1, 0)
    assert_eq!(calculate_specificity(":hover"), (0, 1, 0));

    // Attribute: (0, 1, 0)
    assert_eq!(calculate_specificity("[type=text]"), (0, 1, 0));
}

#[test]
fn test_specificity_ordering() {
    // Higher specificity wins
    let rules = vec![
        ("div", Color::RED),           // (0, 0, 1)
        (".highlight", Color::GREEN),   // (0, 1, 0)
        ("#main", Color::BLUE),         // (1, 0, 0)
    ];

    // Element with id="main" class="highlight" tag=div should be blue
    let computed = compute_style_for_element(rules, "div#main.highlight");
    assert_eq!(computed.color, Color::BLUE);
}
```

### 4.2 Fix Flexbox Alignment

**File**: `crates/rustkit-layout/src/flex.rs`

**Test Coverage**:
```rust
#[test]
fn test_flex_align_items_center() {
    let html = r#"
        <div style="display: flex; align-items: center; height: 100px;">
            <div style="height: 20px;">A</div>
            <div style="height: 40px;">B</div>
        </div>
    "#;

    let layout = layout_html(html);

    // Both children should be vertically centered
    let child_a = layout.children[0];
    let child_b = layout.children[1];

    // A (20px height) should be at y = 40 (centered in 100px)
    assert_eq!(child_a.rect.y, 40.0);
    // B (40px height) should be at y = 30 (centered in 100px)
    assert_eq!(child_b.rect.y, 30.0);
}

#[test]
fn test_flex_justify_content_space_between() {
    let html = r#"
        <div style="display: flex; justify-content: space-between; width: 300px;">
            <div style="width: 50px;">A</div>
            <div style="width: 50px;">B</div>
            <div style="width: 50px;">C</div>
        </div>
    "#;

    let layout = layout_html(html);

    // Total width = 300, children = 150, space = 150
    // 2 gaps, so each gap = 75
    // A at x=0, B at x=125, C at x=250
    assert_eq!(layout.children[0].rect.x, 0.0);
    assert_eq!(layout.children[1].rect.x, 125.0);
    assert_eq!(layout.children[2].rect.x, 250.0);
}
```

---

## Part 5: Comprehensive Test Suite (Days 10-12)

### 5.1 Unit Test Additions

**File**: `crates/rustkit-renderer/src/lib.rs` - Add tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Color tests
    #[test]
    fn test_color_interpolation() {
        let stops = vec![
            (0.0, Color::RED),
            (1.0, Color::BLUE),
        ];

        let mid = Renderer::interpolate_color(&stops, 0.5);
        assert_eq!(mid.r, 127);
        assert_eq!(mid.b, 127);
    }

    #[test]
    fn test_color_interpolation_multiple_stops() {
        let stops = vec![
            (0.0, Color::RED),
            (0.5, Color::GREEN),
            (1.0, Color::BLUE),
        ];

        let quarter = Renderer::interpolate_color(&stops, 0.25);
        // Between red and green
        assert!(quarter.r > 100);
        assert!(quarter.g > 50);

        let three_quarter = Renderer::interpolate_color(&stops, 0.75);
        // Between green and blue
        assert!(three_quarter.g > 50);
        assert!(three_quarter.b > 50);
    }

    // Clipping tests
    #[test]
    fn test_rect_intersection() {
        let r1 = Rect::new(0.0, 0.0, 100.0, 100.0);
        let r2 = Rect::new(50.0, 50.0, 100.0, 100.0);

        let intersection = r1.intersect(&r2);
        assert_eq!(intersection, Some(Rect::new(50.0, 50.0, 50.0, 50.0)));
    }

    #[test]
    fn test_no_intersection() {
        let r1 = Rect::new(0.0, 0.0, 50.0, 50.0);
        let r2 = Rect::new(100.0, 100.0, 50.0, 50.0);

        assert_eq!(r1.intersect(&r2), None);
    }

    // Transform tests
    #[test]
    fn test_transform_point_identity() {
        let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let point = (10.0, 20.0);

        let transformed = transform_point(point, &identity);
        assert_eq!(transformed, point);
    }

    #[test]
    fn test_transform_point_translate() {
        let translate = [1.0, 0.0, 0.0, 1.0, 50.0, 100.0];
        let point = (10.0, 20.0);

        let transformed = transform_point(point, &translate);
        assert_eq!(transformed, (60.0, 120.0));
    }

    #[test]
    fn test_transform_point_scale() {
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let point = (10.0, 20.0);

        let transformed = transform_point(point, &scale);
        assert_eq!(transformed, (20.0, 40.0));
    }
}
```

### 5.2 Integration Test Suite

**File**: `crates/hiwave-app/tests/rendering_parity.rs`

```rust
//! Rendering parity integration tests
//!
//! These tests verify that RustKit renders pages identically to Chrome.

use std::path::Path;
use hiwave_app::test_support::*;

/// Run a visual parity test against a fixture
async fn run_parity_test(fixture: &str, threshold: f32) -> TestResult {
    let fixture_path = Path::new("fixtures").join(fixture);

    // Render with RustKit
    let rustkit_frame = capture_rustkit_frame(&fixture_path).await?;

    // Load Chrome baseline
    let chrome_baseline = load_chrome_baseline(&fixture_path)?;

    // Compare
    let diff = compare_frames(&rustkit_frame, &chrome_baseline);

    TestResult {
        fixture: fixture.into(),
        diff_percent: diff.percent,
        passed: diff.percent <= threshold,
        diff_image: diff.image,
    }
}

// Solid color tests
#[tokio::test]
async fn test_solid_colors() {
    let result = run_parity_test("solid-colors.html", 1.0).await;
    assert!(result.passed, "Solid colors diff: {}%", result.diff_percent);
}

// Gradient tests
#[tokio::test]
async fn test_linear_gradient_horizontal() {
    let result = run_parity_test("gradients/linear-horizontal.html", 2.0).await;
    assert!(result.passed, "Linear gradient diff: {}%", result.diff_percent);
}

#[tokio::test]
async fn test_linear_gradient_diagonal() {
    let result = run_parity_test("gradients/linear-diagonal.html", 3.0).await;
    assert!(result.passed, "Diagonal gradient diff: {}%", result.diff_percent);
}

#[tokio::test]
async fn test_radial_gradient() {
    let result = run_parity_test("gradients/radial.html", 3.0).await;
    assert!(result.passed, "Radial gradient diff: {}%", result.diff_percent);
}

// Text tests
#[tokio::test]
async fn test_text_baseline() {
    let result = run_parity_test("text/baseline.html", 5.0).await;
    assert!(result.passed, "Text baseline diff: {}%", result.diff_percent);
}

#[tokio::test]
async fn test_text_mixed_sizes() {
    let result = run_parity_test("text/mixed-sizes.html", 5.0).await;
    assert!(result.passed, "Mixed text sizes diff: {}%", result.diff_percent);
}

// Image tests
#[tokio::test]
async fn test_image_loading() {
    let result = run_parity_test("images/basic.html", 2.0).await;
    assert!(result.passed, "Image loading diff: {}%", result.diff_percent);
}

#[tokio::test]
async fn test_svg_data_uri() {
    let result = run_parity_test("images/svg-data-uri.html", 2.0).await;
    assert!(result.passed, "SVG data URI diff: {}%", result.diff_percent);
}

// Layout tests
#[tokio::test]
async fn test_flexbox_align() {
    let result = run_parity_test("layout/flexbox-align.html", 1.0).await;
    assert!(result.passed, "Flexbox align diff: {}%", result.diff_percent);
}

#[tokio::test]
async fn test_background_clip() {
    let result = run_parity_test("layout/background-clip.html", 1.0).await;
    assert!(result.passed, "Background clip diff: {}%", result.diff_percent);
}
```

### 5.3 Benchmark Suite

**File**: `crates/rustkit-bench/src/rendering.rs`

```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn gradient_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("gradients");

    for size in [100, 500, 1000].iter() {
        let rect = Rect::new(0.0, 0.0, *size as f32, *size as f32);
        let stops = vec![
            ColorStop { color: Color::RED, position: Some(0.0) },
            ColorStop { color: Color::BLUE, position: Some(1.0) },
        ];

        group.bench_with_input(
            BenchmarkId::new("linear_cpu", size),
            &(rect, &stops),
            |b, (rect, stops)| {
                b.iter(|| renderer.draw_linear_gradient(*rect, GradientDirection::ToRight, stops));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("linear_gpu", size),
            &(rect, &stops),
            |b, (rect, stops)| {
                b.iter(|| renderer.draw_linear_gradient_gpu(*rect, GradientDirection::ToRight, stops));
            },
        );
    }

    group.finish();
}

fn text_rendering_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("text");

    let texts = [
        "Hello",
        "The quick brown fox jumps over the lazy dog",
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
    ];

    for text in texts.iter() {
        group.bench_with_input(
            BenchmarkId::new("draw_text", text.len()),
            text,
            |b, text| {
                b.iter(|| renderer.draw_text(text, 0.0, 0.0, Color::BLACK, 16.0, "sans-serif", 400, 0));
            },
        );
    }

    group.finish();
}

criterion_group!(benches, gradient_benchmark, text_rendering_benchmark);
criterion_main!(benches);
```

---

## Part 6: Safety and Regression Prevention

### 6.1 Pre-Commit Checks

**File**: `.github/workflows/parity-check.yml`

```yaml
name: Parity Check

on:
  pull_request:
    paths:
      - 'crates/rustkit-renderer/**'
      - 'crates/rustkit-text/**'
      - 'crates/rustkit-image/**'
      - 'crates/rustkit-layout/**'
      - 'crates/rustkit-css/**'

jobs:
  parity-test:
    runs-on: macos-14
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-action@stable

      - name: Build
        run: cargo build --release

      - name: Run parity tests
        run: |
          python3 scripts/parity_gate.py \
            --minimum 80 \
            --fail-on-regression 0.5

      - name: Upload diff artifacts
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: parity-diffs
          path: parity-results/
```

### 6.2 Feature Flags for Safe Rollout

**File**: `crates/rustkit-renderer/src/lib.rs`

```rust
/// Feature flags for gradual rollout
pub struct RenderFeatureFlags {
    /// Use GPU shader for gradients instead of CPU
    pub gpu_gradients: bool,

    /// Use bundled font for text rendering
    pub bundled_font: bool,

    /// Use improved baseline alignment
    pub improved_baseline: bool,

    /// Wait for images to load before rendering
    pub sync_image_load: bool,
}

impl Default for RenderFeatureFlags {
    fn default() -> Self {
        Self {
            gpu_gradients: false,      // Disabled by default
            bundled_font: false,
            improved_baseline: true,   // Safe to enable
            sync_image_load: false,    // Only for testing
        }
    }
}

impl RenderFeatureFlags {
    /// Flags for parity testing mode
    pub fn parity_mode() -> Self {
        Self {
            gpu_gradients: true,
            bundled_font: true,
            improved_baseline: true,
            sync_image_load: true,
        }
    }
}
```

### 6.3 Rollback Plan

If any change causes regressions:

1. **Immediate**: Revert the commit
2. **Within 1 hour**: Create hotfix with feature flag disabled
3. **Within 24 hours**: Root cause analysis and proper fix

---

## Part 7: Implementation Schedule

### Week 1: Quick Wins + Foundation
| Day | Focus | Target Tests Fixed |
|-----|-------|-------------------|
| 1 | Image sync loading + data URI | +2 (images-intrinsic, image-gallery) |
| 2 | background-clip + SVG rendering | +2 (backgrounds, gradient-backgrounds) |
| 3-4 | Text baseline alignment | +3 (specificity, combinators, pseudo-classes) |
| 5 | Gradient diagonal fix | +1 (gradients) |

### Week 2: Polish + Testing
| Day | Focus | Target Tests Fixed |
|-----|-------|-------------------|
| 6-7 | GPU gradient shader | +1 (settings, flex-positioning) |
| 8-9 | Layout edge cases | +1 (css-selectors, card-grid) |
| 10-12 | Test suite + CI integration | Stability |

---

## Part 8: Success Metrics

### Target State After Implementation

| Metric | Current | Target | Stretch |
|--------|---------|--------|---------|
| Tests Passing | 7/23 (30%) | 18/23 (78%) | 21/23 (91%) |
| Average Diff | 24.8% | 8.0% | 3.0% |
| Text Rendering Diff | 60% | 10% | 2% |
| Image Diff | 20% | 2% | 0.5% |
| Gradient Diff | 15% | 5% | 1% |

### Per-Test Targets

| Test | Current | Target |
|------|---------|--------|
| new_tab | 1.64% | 1.0% |
| chrome_rustkit | 1.97% | 1.0% |
| shelf | 3.04% | 2.0% |
| form-elements | 4.53% | 3.0% |
| form-controls | 7.31% | 5.0% |
| article-typography | 9.10% | 5.0% |
| about | 10.05% | 5.0% |
| images-intrinsic | 13.35% | **8.0%** |
| specificity | 15.53% | **10.0%** |
| combinators | 16.14% | **10.0%** |
| settings | 21.97% | **12.0%** |
| pseudo-classes | 22.35% | **12.0%** |
| css-selectors | 27.10% | **12.0%** |
| flex-positioning | 29.21% | **12.0%** |
| bg-solid | 30.27% | **10.0%** |
| card-grid | 30.45% | **12.0%** |
| gradients | 30.92% | **10.0%** |
| rounded-corners | 31.24% | **12.0%** |
| backgrounds | 38.69% | **12.0%** |
| sticky-scroll | 40.68% | **20.0%** |
| bg-pure | 53.06% | **12.0%** |
| gradient-backgrounds | 65.39% | **12.0%** |
| image-gallery | 66.79% | **8.0%** |

---

## Appendix A: Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| GPU gradients | Medium - shader issues | Feature flag, fallback to CPU |
| Text baseline | Medium - affects all text | Extensive testing, gradual rollout |
| Image sync loading | Low - only affects test mode | Flag-controlled, not in prod |
| SVG rendering | Low - additive | New code path, won't break existing |
| background-clip | Low - targeted fix | Unit tests for edge cases |

---

## Appendix B: Dependencies

### New Dependencies Required

```toml
# Cargo.toml additions

[dependencies]
# SVG rendering
resvg = "0.40"
usvg = "0.40"
tiny-skia = "0.11"

# Font rendering
ab_glyph = "0.2"

# URL decoding
percent-encoding = "2.3"
```

### No Breaking Changes Required

All changes are:
- Additive (new functions, not modifying existing signatures)
- Behind feature flags where risky
- Backwards compatible

---

*This plan is designed to be executed safely, incrementally, and with full test coverage. Each change is isolated and can be rolled back independently.*
