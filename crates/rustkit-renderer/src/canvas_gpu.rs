//! GPU canvas A1 (docs/design/GPU_CANVAS_2D.md, "Plan of record: option
//! A"): a canvas's recording replayed into a texture of its own, which the
//! display list draws like any premultiplied image (it sits in the texture
//! cache under the canvas's key).
//!
//! Each replay draws only the ops handed over since the last one, onto the
//! texture as it was: the texture is copied into a 4x multisampled scratch
//! target, the ops are drawn there and the result is resolved back into the
//! texture. A fill is drawn stencil-then-cover: its triangle fans into the
//! stencil's low 7 bits (non-zero: incremented / decremented by facing,
//! wrapping; even-odd: bit 0 inverted), then its bounds covered where the
//! stencil says inside, which also zeroes the low bits for the next fill.
//! The clip chain is bit 7: set everywhere, then cut by each link's path;
//! rebuilt when the op's clip changes. `putImageData` is a buffer copy into
//! the texture between draw passes, in order.
//!
//! Budget (per renderer): canvas textures at most
//! [`CANVAS_TEXTURE_BUDGET`] bytes (4 per pixel), and the scratch (4x
//! colour + 4x stencil, 20 bytes per pixel) at most
//! [`CANVAS_SCRATCH_BUDGET`]. A canvas past either, or past the device's
//! largest texture, is denied: it stays on the CPU bitmap.

use super::*;
use rustkit_canvas::gpu::{GpuBatch, GpuBlend, GpuDraw, GpuGradient, GpuPaint, GpuPath, GpuReset, MAX_GRADIENT_STOPS};

/// Canvas textures one renderer keeps, in bytes.
pub const CANVAS_TEXTURE_BUDGET: u64 = 128 << 20;
/// The multisampled scratch target and its stencil, in bytes.
pub const CANVAS_SCRATCH_BUDGET: u64 = 128 << 20;
/// The most vertices one canvas replay draws (24 bytes each: 48 MB), and
/// never more than the device's largest buffer holds. A replay that would
/// take more (a frame of curves flattens to far more vertices than its
/// recording's bytes) is not drawn: the canvas starts over from its CPU
/// bitmap at its next batch.
pub const CANVAS_VERTEX_CAP: usize = 2 << 20;
/// Over-cap replays of one canvas in a row (its resets between them not
/// counting as a break) after which it is denied: it shows its CPU bitmap
/// for good rather than a stale texture every other frame.
pub const CANVAS_OVER_CAP_LIMIT: u32 = 3;
const SAMPLES: u32 = 4;
const SCRATCH_BYTES_PER_PX: u64 = 4 * SAMPLES as u64 + SAMPLES as u64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const STENCIL: wgpu::TextureFormat = wgpu::TextureFormat::Stencil8;

/// What a replay did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasReplay {
    /// The texture is up to date.
    Drawn,
    /// The texture is missing (evicted, a new renderer, a lost device) and
    /// the batches did not start it over: the next batch must be a reset.
    NeedsReset,
    /// No texture for this canvas (too large, past the budget, no area):
    /// show its CPU bitmap.
    Denied,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    color: [f32; 4],
}

/// A gradient as the shader reads it (GPU-GRADIENT), one per gradient fill
/// of a pass, in a storage buffer. A cover vertex names it by a negative
/// red: `-(index + 1)` (a real colour channel is never negative).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GradRecord {
    /// Device to user space: (a, b, c, d).
    inv0: [f32; 4],
    /// (e, f, radial 0/1, stop count).
    inv1: [f32; 4],
    /// (x0, y0, x1, y1).
    geo: [f32; 4],
    /// (r1, 0, 0, 0).
    extra: [f32; 4],
    offs: [[f32; 4]; MAX_GRADIENT_STOPS / 4],
    cols: [[f32; 4]; MAX_GRADIENT_STOPS],
}

impl GradRecord {
    fn of(g: &GpuGradient) -> Self {
        let mut r = Self::zeroed();
        r.inv0 = [g.inv[0], g.inv[1], g.inv[2], g.inv[3]];
        r.inv1 = [g.inv[4], g.inv[5], if g.radial { 1.0 } else { 0.0 }, g.stops.len().min(MAX_GRADIENT_STOPS) as f32];
        r.geo = [g.p0[0], g.p0[1], g.p1[0], g.p1[1]];
        r.extra = [g.r1, 0.0, 0.0, 0.0];
        for (i, (o, c)) in g.stops.iter().take(MAX_GRADIENT_STOPS).enumerate() {
            r.offs[i / 4][i % 4] = *o;
            r.cols[i] = *c;
        }
        r
    }
}

use bytemuck::Zeroable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Pipe {
    Copy,
    Stencil { eo: bool },
    Cover { eo: bool, clip: bool, blend: GpuBlend },
    ClipSet,
    ClipCut { eo: bool },
    ZeroLow,
}

struct Scratch {
    size: (u32, u32),
    _color: wgpu::Texture,
    color: wgpu::TextureView,
    _stencil: wgpu::Texture,
    stencil: wgpu::TextureView,
}

pub(crate) struct CanvasGpu {
    shader: wgpu::ShaderModule,
    plain_layout: wgpu::PipelineLayout,
    paint_bgl: wgpu::BindGroupLayout,
    copy_bgl: wgpu::BindGroupLayout,
    copy_layout: wgpu::PipelineLayout,
    pipes: HashMap<Pipe, wgpu::RenderPipeline>,
    scratch: Option<Scratch>,
    /// The canvas textures this renderer made, by key, and their bytes.
    canvases: HashMap<String, u64>,
    /// Tests: a lower [`CANVAS_VERTEX_CAP`].
    vertex_cap: Option<usize>,
    /// Over-cap replays in a row, by key.
    over_cap: HashMap<String, u32>,
}

const SHADER: &str = r#"
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) p: vec2<f32>, @location(1) c: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.color = c;
    return o;
}

struct Grad {
    inv0: vec4<f32>,
    inv1: vec4<f32>,
    geo: vec4<f32>,
    extra: vec4<f32>,
    offs: array<vec4<f32>, 4>,
    cols: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var<storage, read> grads: array<Grad>;

fn off(k: u32, i: u32) -> f32 {
    return grads[k].offs[i / 4u][i % 4u];
}

// A gradient at the pixel centre `p` (device space), as tiny-skia
// evaluates it: user space by the inverse transform, t padded to [0, 1],
// the stops interpolated unpremultiplied, then premultiplied. A radial
// gradient is two-point conical from (x0, y0), radius 0, to (x1, y1), r1:
// the larger t whose circle passes through the point; none: transparent.
fn gradient(k: u32, p: vec2<f32>) -> vec4<f32> {
    let inv0 = grads[k].inv0;
    let inv1 = grads[k].inv1;
    let u = vec2<f32>(inv0.x * p.x + inv0.z * p.y + inv1.x, inv0.y * p.x + inv0.w * p.y + inv1.y);
    let p0 = grads[k].geo.xy;
    let d = grads[k].geo.zw - p0;
    var t: f32;
    if (inv1.z < 0.5) {
        t = dot(u - p0, d) / dot(d, d);
    } else {
        let r1 = grads[k].extra.x;
        let q = u - p0;
        let a = dot(d, d) - r1 * r1;
        let b = dot(q, d);
        let c = dot(q, q);
        if (abs(a) < 1e-6) {
            if (b == 0.0) {
                return vec4<f32>(0.0);
            }
            t = c / (2.0 * b);
        } else {
            let disc = b * b - a * c;
            if (disc < 0.0) {
                return vec4<f32>(0.0);
            }
            let s = sqrt(disc);
            let t1 = (b + s) / a;
            let t2 = (b - s) / a;
            t = max(t1, t2);
            if (t * r1 < 0.0) {
                t = min(t1, t2);
            }
        }
        if (t * r1 < 0.0) {
            return vec4<f32>(0.0);
        }
    }
    t = clamp(t, 0.0, 1.0);
    let n = u32(inv1.w);
    var col = grads[k].cols[0];
    if (t > off(k, 0u)) {
        col = grads[k].cols[n - 1u];
        for (var i = 1u; i < n; i = i + 1u) {
            if (t <= off(k, i)) {
                let o0 = off(k, i - 1u);
                let o1 = off(k, i);
                var f = 1.0;
                if (o1 > o0) {
                    f = (t - o0) / (o1 - o0);
                }
                col = mix(grads[k].cols[i - 1u], grads[k].cols[i], f);
                break;
            }
        }
    }
    return vec4<f32>(col.rgb * col.a, col.a);
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    if (i.color.r >= 0.0) {
        return i.color;
    }
    return gradient(u32(-i.color.r - 0.5), i.pos.xy);
}

@group(0) @binding(1) var src: texture_2d<f32>;

@fragment
fn fs_copy(i: VOut) -> @location(0) vec4<f32> {
    return textureLoad(src, vec2<i32>(floor(i.pos.xy)), 0);
}
"#;

impl CanvasGpu {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas_gpu"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let paint_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas_gpu_paint"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let plain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas_gpu_plain"),
            bind_group_layouts: &[&paint_bgl],
            push_constant_ranges: &[],
        });
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas_gpu_copy"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                },
                count: None,
            }],
        });
        let copy_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas_gpu_copy"),
            bind_group_layouts: &[&copy_bgl],
            push_constant_ranges: &[],
        });
        Self { shader, plain_layout, paint_bgl, copy_bgl, copy_layout, pipes: HashMap::new(), scratch: None, canvases: HashMap::new(), vertex_cap: None, over_cap: HashMap::new() }
    }

    fn pipe(&mut self, device: &wgpu::Device, p: Pipe) -> &wgpu::RenderPipeline {
        if !self.pipes.contains_key(&p) {
            let built = self.build(device, p);
            self.pipes.insert(p, built);
        }
        &self.pipes[&p]
    }

    fn build(&self, device: &wgpu::Device, p: Pipe) -> wgpu::RenderPipeline {
        use wgpu::{CompareFunction as C, StencilOperation as S};
        let face = |compare, fail_op, pass_op| wgpu::StencilFaceState { compare, fail_op, depth_fail_op: S::Keep, pass_op };
        let none = wgpu::ColorWrites::empty();
        // (fragment, blend, colour writes, front, back, read mask, write mask)
        let (fs, blend, writes, front, back, read, write) = match p {
            Pipe::Copy => ("fs_copy", None, wgpu::ColorWrites::ALL, face(C::Always, S::Keep, S::Keep), face(C::Always, S::Keep, S::Keep), 0, 0),
            Pipe::Stencil { eo: false } => {
                ("fs_main", None, none, face(C::Always, S::Keep, S::IncrementWrap), face(C::Always, S::Keep, S::DecrementWrap), 0xff, 0x7f)
            }
            Pipe::Stencil { eo: true } => ("fs_main", None, none, face(C::Always, S::Keep, S::Invert), face(C::Always, S::Keep, S::Invert), 0xff, 0x01),
            Pipe::Cover { eo, clip, blend } => {
                // Non-zero: inside when the low bits are not 0 (with the
                // clip: when the whole byte is over 0x80, bit 7 set).
                // Even-odd: bit 0 set (and bit 7). Either way the low bits
                // go back to 0 for the next fill.
                let (compare, read) = match (eo, clip) {
                    (false, false) => (C::Less, 0x7f),
                    (false, true) => (C::Less, 0xff),
                    (true, false) => (C::Equal, 0x01),
                    (true, true) => (C::Equal, 0x81),
                };
                let f = face(compare, S::Zero, S::Zero);
                ("fs_main", Some(blend_state(blend)), wgpu::ColorWrites::ALL, f, f, read, 0x7f)
            }
            Pipe::ClipSet => {
                let f = face(C::Always, S::Keep, S::Replace);
                ("fs_main", None, none, f, f, 0xff, 0x80)
            }
            Pipe::ClipCut { eo } => {
                // Outside the link's path (low bits 0): bit 7 off.
                let f = face(C::Equal, S::Keep, S::Zero);
                ("fs_main", None, none, f, f, if eo { 0x01 } else { 0x7f }, 0x80)
            }
            Pipe::ZeroLow => {
                let f = face(C::Always, S::Zero, S::Zero);
                ("fs_main", None, none, f, f, 0xff, 0x7f)
            }
        };
        let layout = if p == Pipe::Copy { &self.copy_layout } else { &self.plain_layout };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas_gpu"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some(fs),
                targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend, write_mask: writes })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: STENCIL,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState { front, back, read_mask: read, write_mask: write },
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
            cache: None,
        })
    }

    fn scratch(&mut self, device: &wgpu::Device, size: (u32, u32)) -> &Scratch {
        if self.scratch.as_ref().is_none_or(|s| s.size != size) {
            self.scratch = None;
            let make = |format, label| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: SAMPLES,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
            };
            let color = make(FORMAT, "canvas_gpu_msaa");
            let stencil = make(STENCIL, "canvas_gpu_stencil");
            self.scratch = Some(Scratch {
                size,
                color: color.create_view(&Default::default()),
                _color: color,
                stencil: stencil.create_view(&Default::default()),
                _stencil: stencil,
            });
        }
        self.scratch.as_ref().expect("scratch")
    }
}

fn blend_state(b: GpuBlend) -> wgpu::BlendState {
    use wgpu::BlendFactor::*;
    let (src_factor, dst_factor) = match b {
        GpuBlend::Clear => (Zero, Zero),
        GpuBlend::SourceOver => (One, OneMinusSrcAlpha),
        GpuBlend::SourceIn => (DstAlpha, Zero),
        GpuBlend::SourceOut => (OneMinusDstAlpha, Zero),
        GpuBlend::SourceAtop => (DstAlpha, OneMinusSrcAlpha),
        GpuBlend::DestinationOver => (OneMinusDstAlpha, One),
        GpuBlend::DestinationIn => (Zero, SrcAlpha),
        GpuBlend::DestinationOut => (Zero, OneMinusSrcAlpha),
        GpuBlend::DestinationAtop => (OneMinusDstAlpha, SrcAlpha),
        GpuBlend::Lighter => (One, One),
        GpuBlend::Copy => (One, Zero),
        GpuBlend::Xor => (OneMinusDstAlpha, OneMinusSrcAlpha),
    };
    let c = wgpu::BlendComponent { src_factor, dst_factor, operation: wgpu::BlendOperation::Add };
    wgpu::BlendState { color: c, alpha: c }
}

/// One draw of a fill pass: a pipeline, its stencil reference and its
/// vertices.
struct Step {
    pipe: Pipe,
    reference: u32,
    range: std::ops::Range<u32>,
}

impl Renderer {
    /// Replays `batches` (oldest first) of the canvas whose texture is
    /// kept under `key`. A reset starts the texture over (and supersedes
    /// every batch before it); without one, the texture must exist at the
    /// batches' size.
    pub fn replay_canvas(&mut self, key: &str, batches: &[GpuBatch]) -> CanvasReplay {
        let start = batches.iter().rposition(|b| b.reset.is_some()).unwrap_or(0);
        let batches = &batches[start..];
        let Some(last) = batches.last() else { return CanvasReplay::Drawn };
        let (w, h) = (last.width, last.height);
        let limit = self.device.limits().max_texture_dimension_2d;
        if w == 0 || h == 0 || w > limit || h > limit || w as u64 * h as u64 * SCRATCH_BYTES_PER_PX > CANVAS_SCRATCH_BUDGET {
            return CanvasReplay::Denied;
        }
        if batches.iter().any(|b| (b.width, b.height) != (w, h)) {
            return CanvasReplay::NeedsReset;
        }
        let have = self.texture_cache.textures.get(key).map(|t| (t.width, t.height));
        let reset = batches[0].reset.clone();
        if reset.is_none() && have != Some((w, h)) {
            return CanvasReplay::NeedsReset;
        }
        let device = self.device.clone();
        let queue = self.queue.clone();
        let cg = self.canvas_gpu.get_or_insert_with(|| Box::new(CanvasGpu::new(&device)));
        let textures = &self.texture_cache.textures;
        cg.canvases.retain(|k, _| textures.contains_key(k));
        // Lowered first, under the vertex cap, before anything touches the
        // GPU: past the cap nothing is drawn and the texture is as it was.
        let fits_buffer = (device.limits().max_buffer_size / std::mem::size_of::<Vertex>() as u64).min(usize::MAX as u64) as usize;
        let mut left = cg.vertex_cap.unwrap_or(CANVAS_VERTEX_CAP).min(fits_buffer);
        let mut draws: Vec<GpuDraw> = Vec::new();
        for b in batches {
            let Some((d, n)) = b.lower_within(left) else {
                let count = cg.over_cap.entry(key.to_string()).or_insert(0);
                *count += 1;
                if *count >= CANVAS_OVER_CAP_LIMIT {
                    cg.over_cap.remove(key);
                    return CanvasReplay::Denied;
                }
                return CanvasReplay::NeedsReset;
            };
            left -= n;
            draws.extend(d);
        }
        let bytes = w as u64 * h as u64 * 4;
        if reset.is_some() && have != Some((w, h)) {
            let used: u64 = cg.canvases.iter().filter(|(k, _)| k.as_str() != key).map(|(_, b)| *b).sum();
            if used + bytes > CANVAS_TEXTURE_BUDGET {
                return CanvasReplay::Denied;
            }
        }
        // Everything below in error scopes: a validation or out-of-memory
        // error drops the texture and asks for a reset, instead of reaching
        // wgpu's default handler (which panics).
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("canvas_gpu") });
        if let Some(reset) = &reset {
            if have != Some((w, h)) {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(key),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &self.texture_cache.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.texture_cache.sampler) },
                    ],
                    label: Some("canvas_gpu_texture"),
                });
                self.texture_cache.textures.insert(
                    key.to_string(),
                    CachedTexture { texture, view, bind_group, width: w, height: h, premultiplied: true },
                );
                cg.canvases.insert(key.to_string(), bytes);
            }
            let t = &self.texture_cache.textures[key];
            match reset {
                GpuReset::Pixels(px) if px.len() as u64 == w as u64 * h as u64 * 4 => {
                    // Before the encoder's passes: a write is done at the
                    // start of the next submit.
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                        px,
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
                        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    );
                }
                _ => {
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("canvas_gpu_blank"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &t.view,
                            resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }
            }
        }
        let t = &self.texture_cache.textures[key];
        let mut i = 0;
        while i < draws.len() {
            if let GpuDraw::Put { x, y, width, height, rgba } = &draws[i] {
                encode_put(&device, &mut encoder, &t.texture, (w, h), (*x, *y, *width, *height), rgba);
                i += 1;
                continue;
            }
            let j = draws[i..].iter().position(|d| matches!(d, GpuDraw::Put { .. })).map_or(draws.len(), |k| i + k);
            encode_fills(cg, &device, &mut encoder, t, (w, h), &draws[i..j]);
            i = j;
        }
        queue.submit(std::iter::once(encoder.finish()));
        let validation = ready(device.pop_error_scope()).flatten();
        let memory = ready(device.pop_error_scope()).flatten();
        if let Some(e) = validation.or(memory) {
            tracing::warn!(error = %e, "canvas GPU replay failed; the canvas starts over from its bitmap");
            self.texture_cache.textures.remove(key);
            if let Some(cg) = self.canvas_gpu.as_mut() {
                cg.canvases.remove(key);
            }
            return CanvasReplay::NeedsReset;
        }
        if batches.iter().any(|b| b.op_count() > 0) {
            // A frame of draws fit: the over-cap run is broken.
            if let Some(cg) = self.canvas_gpu.as_mut() {
                cg.over_cap.remove(key);
            }
        }
        CanvasReplay::Drawn
    }

    /// Tests: a lower vertex cap for canvas replays.
    #[doc(hidden)]
    pub fn set_canvas_vertex_cap(&mut self, cap: usize) {
        let device = self.device.clone();
        self.canvas_gpu.get_or_insert_with(|| Box::new(CanvasGpu::new(&device))).vertex_cap = Some(cap);
    }

    /// Tests and tools: the canvas texture under `key` read back,
    /// premultiplied RGBA rows (a GPU round trip; never on a frame's path).
    #[doc(hidden)]
    pub fn read_canvas_texture(&self, key: &str) -> Option<Vec<u8>> {
        let t = self.texture_cache.textures.get(key)?;
        let readback = screenshot::GpuReadbackBuffer::new(&self.device, t.width, t.height);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        readback.copy_from_texture(&mut encoder, &t.texture);
        self.queue.submit(std::iter::once(encoder.finish()));
        readback.read_data_sync(&self.device).ok()
    }

    /// Tests: canvases with an over-cap count.
    #[doc(hidden)]
    pub fn canvas_over_cap_counts(&self) -> usize {
        self.canvas_gpu.as_ref().map_or(0, |cg| cg.over_cap.len())
    }

    /// The canvas textures this renderer keeps, in bytes.
    pub fn canvas_texture_bytes(&self) -> u64 {
        let Some(cg) = &self.canvas_gpu else { return 0 };
        cg.canvases.iter().filter(|(k, _)| self.texture_cache.contains(k)).map(|(_, b)| *b).sum()
    }
}

/// A future's output if it is ready at the first poll (wgpu's native
/// error scopes are); `None` if not.
fn ready<F: std::future::Future>(f: F) -> Option<F::Output> {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => Some(v),
        std::task::Poll::Pending => None,
    }
}

fn encode_put(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture, size: (u32, u32), rect: (u32, u32, u32, u32), rgba: &[u8]) {
    let (x, y, w, h) = rect;
    if w == 0 || h == 0 || x >= size.0 || y >= size.1 || rgba.len() < (w * h * 4) as usize {
        return;
    }
    let (cw, ch) = (w.min(size.0 - x), h.min(size.1 - y));
    let row = (cw * 4 + 255) & !255;
    let mut padded = vec![0u8; (row * ch) as usize];
    for r in 0..ch as usize {
        let src = r * w as usize * 4;
        padded[r * row as usize..r * row as usize + cw as usize * 4].copy_from_slice(&rgba[src..src + cw as usize * 4]);
    }
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("canvas_gpu_put"),
        contents: &padded,
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(ch) },
        },
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
        wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
    );
}

/// The fills `draws` (no puts) drawn onto `target`: copied into the
/// scratch, drawn, resolved back.
fn encode_fills(
    cg: &mut CanvasGpu,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &CachedTexture,
    size: (u32, u32),
    draws: &[GpuDraw],
) {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let ndc = |p: [f32; 2]| [p[0] / w * 2.0 - 1.0, 1.0 - p[1] / h * 2.0];
    let mut verts: Vec<Vertex> = Vec::new();
    let push = |verts: &mut Vec<Vertex>, pts: &mut dyn Iterator<Item = [f32; 2]>, color: [f32; 4]| -> std::ops::Range<u32> {
        let a = verts.len() as u32;
        verts.extend(pts.map(|p| Vertex { pos: ndc(p), color }));
        a..verts.len() as u32
    };
    let quad = |r: [f32; 4]| {
        let [x0, y0, x1, y1] = r;
        [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]]
    };
    let full = push(&mut verts, &mut quad([0.0, 0.0, w, h]).into_iter(), [0.0; 4]);
    let mut steps: Vec<Step> = Vec::new();
    let fan = |verts: &mut Vec<Vertex>, steps: &mut Vec<Step>, p: &GpuPath| {
        let range = push(verts, &mut p.tris.iter().copied(), [0.0; 4]);
        steps.push(Step { pipe: Pipe::Stencil { eo: p.even_odd }, reference: 0, range });
    };
    let mut current_clip: Option<usize> = None;
    let mut grads: Vec<GradRecord> = Vec::new();
    let mut grad_index: HashMap<usize, usize> = HashMap::new();
    for d in draws {
        let GpuDraw::Fill { path, paint, blend, clip } = d else { continue };
        let color = match paint {
            GpuPaint::Solid(c) => *c,
            GpuPaint::Gradient(g) => {
                let k = *grad_index.entry(Arc::as_ptr(g) as usize).or_insert_with(|| {
                    grads.push(GradRecord::of(g));
                    grads.len() - 1
                });
                [-(k as f32 + 1.0), 0.0, 0.0, 0.0]
            }
        };
        if let Some(c) = clip {
            if current_clip != Some(c.id) {
                steps.push(Step { pipe: Pipe::ClipSet, reference: 0xff, range: full.clone() });
                for link in &c.links {
                    if !link.tris.is_empty() {
                        fan(&mut verts, &mut steps, link);
                    }
                    steps.push(Step { pipe: Pipe::ClipCut { eo: link.even_odd }, reference: 0, range: full.clone() });
                    steps.push(Step { pipe: Pipe::ZeroLow, reference: 0, range: full.clone() });
                }
                current_clip = Some(c.id);
            }
        }
        fan(&mut verts, &mut steps, path);
        let b = path.bounds;
        let range = push(&mut verts, &mut quad([b[0] - 1.0, b[1] - 1.0, b[2] + 1.0, b[3] + 1.0]).into_iter(), color);
        let has_clip = clip.is_some();
        let reference = match (path.even_odd, has_clip) {
            (false, false) => 0,
            (false, true) => 0x80,
            (true, false) => 0x01,
            (true, true) => 0x81,
        };
        steps.push(Step { pipe: Pipe::Cover { eo: path.even_odd, clip: has_clip, blend: *blend }, reference, range });
    }
    for s in &steps {
        cg.pipe(device, s.pipe);
    }
    cg.pipe(device, Pipe::Copy);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("canvas_gpu_vertices"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let copy_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &cg.copy_bgl,
        entries: &[wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&target.view) }],
        label: Some("canvas_gpu_copy"),
    });
    if grads.is_empty() {
        grads.push(GradRecord::zeroed());
    }
    let grad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("canvas_gpu_gradients"),
        contents: bytemuck::cast_slice(&grads),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let paint_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &cg.paint_bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: grad_buffer.as_entire_binding() }],
        label: Some("canvas_gpu_paint"),
    });
    cg.scratch(device, size);
    let scratch = cg.scratch.as_ref().expect("scratch");
    let stencil_clear = |store| wgpu::RenderPassDepthStencilAttachment {
        view: &scratch.stencil,
        depth_ops: None,
        stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store }),
    };
    {
        // The texture as it is, into every sample.
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas_gpu_load"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &scratch.color,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(stencil_clear(wgpu::StoreOp::Discard)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&cg.pipes[&Pipe::Copy]);
        pass.set_bind_group(0, &copy_bind, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(full.clone(), 0..1);
    }
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("canvas_gpu_draw"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &scratch.color,
            resolve_target: Some(&target.view),
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard },
        })],
        depth_stencil_attachment: Some(stencil_clear(wgpu::StoreOp::Discard)),
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_bind_group(0, &paint_bind, &[]);
    pass.set_vertex_buffer(0, buffer.slice(..));
    for s in &steps {
        pass.set_pipeline(&cg.pipes[&s.pipe]);
        pass.set_stencil_reference(s.reference);
        pass.draw(s.range.clone(), 0..1);
    }
}

#[cfg(test)]
#[path = "canvas_gpu_tests.rs"]
mod canvas_gpu_tests;

#[cfg(test)]
#[path = "canvas_gpu_chrome_tests.rs"]
mod canvas_gpu_chrome_tests;
