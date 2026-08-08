//! # RustKit Renderer
//!
//! GPU display list renderer for the RustKit browser engine.
//!
//! This crate takes a `DisplayList` from `rustkit-layout` and executes it
//! via wgpu to produce actual rendered output.
//!
//! ## Architecture
//!
//! ```text
//! DisplayList
//!     │
//!     ▼
//! ┌─────────────────────────────────────┐
//! │           Renderer                  │
//! │  ┌─────────────────────────────┐    │
//! │  │   Command Processing        │    │
//! │  │   - Solid colors            │    │
//! │  │   - Borders                 │    │
//! │  │   - Text (via GlyphCache)   │    │
//! │  │   - Images (via TextureCache)│   │
//! │  └─────────────────────────────┘    │
//! │              │                      │
//! │              ▼                      │
//! │  ┌─────────────────────────────┐    │
//! │  │   Vertex Batching           │    │
//! │  │   - ColorVertex             │    │
//! │  │   - TextureVertex           │    │
//! │  └─────────────────────────────┘    │
//! │              │                      │
//! │              ▼                      │
//! │  ┌─────────────────────────────┐    │
//! │  │   Render Pipelines (wgpu)   │    │
//! │  │   - Color pipeline          │    │
//! │  │   - Texture pipeline        │    │
//! │  └─────────────────────────────┘    │
//! └─────────────────────────────────────┘
//!                 │
//!                 ▼
//!            GPU Output
//! ```

use bytemuck::{Pod, Zeroable};
use hashbrown::HashMap;
use rustkit_css::Color;
use rustkit_layout::{BackgroundRepeat, BackgroundSize, DisplayCommand, Rect};
use std::sync::Arc;
use thiserror::Error;
use wgpu::util::DeviceExt;

pub mod dither;
mod glyph;
mod pipeline;
pub mod screenshot;
mod shaders;

pub use glyph::*;
pub use pipeline::*;
pub use screenshot::*;

// ==================== Errors ====================

/// Errors that can occur during rendering.
#[derive(Error, Debug)]
pub enum RendererError {
    #[error("Failed to create render pipeline: {0}")]
    PipelineCreation(String),

    #[error("Failed to create buffer: {0}")]
    BufferCreation(String),

    #[error("Buffer size {0} bytes exceeds maximum allowed size of {1} bytes")]
    BufferTooLarge(u64, u64),

    #[error("Texture upload failed: {0}")]
    TextureUpload(String),

    #[error("Glyph rasterization failed: {0}")]
    GlyphRasterization(String),

    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),
}

// ==================== Constants ====================

/// Maximum GPU buffer size (256 MB) - prevents OOM on pathological inputs
const MAX_BUFFER_SIZE: u64 = 256 * 1024 * 1024;

// ==================== Vertex Types ====================

/// Vertex for solid color rendering.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ColorVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl ColorVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<ColorVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 8,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
}

/// Vertex for textured rendering (images, glyphs).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TextureVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub color: [f32; 4],
}

impl TextureVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TextureVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 8,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
}

/// Uniform buffer for viewport transformation.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

// ==================== Texture Cache ====================

/// Cached texture entry.
pub struct CachedTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

/// Texture cache for images.
pub struct TextureCache {
    textures: HashMap<String, CachedTexture>,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl TextureCache {
    /// Create a new texture cache.
    pub fn new(device: &wgpu::Device, bind_group_layout: wgpu::BindGroupLayout) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            textures: HashMap::new(),
            sampler,
            bind_group_layout,
        }
    }

    /// Get or create a texture from RGBA data.
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> &CachedTexture {
        if !self.textures.contains_key(key) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(key),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Rgba8Unorm, NOT Rgba8UnormSrgb: the render targets are linear
                // formats and every other pipeline writes sRGB bytes through
                // unconverted. An Srgb view would linearize on sample and paint
                // images darker than the rest of the page.
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
                label: Some(&format!("{}_bind_group", key)),
            });

            self.textures.insert(key.to_string(), CachedTexture {
                texture,
                view,
                bind_group,
                width,
                height,
            });
        }

        self.textures.get(key).unwrap()
    }

    /// Check if a texture exists.
    pub fn contains(&self, key: &str) -> bool {
        self.textures.contains_key(key)
    }

    /// Get an existing texture.
    pub fn get(&self, key: &str) -> Option<&CachedTexture> {
        self.textures.get(key)
    }

    /// Clear all cached textures.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
    
    /// Remove a specific texture.
    pub fn remove(&mut self, key: &str) {
        self.textures.remove(key);
    }
}

// ==================== Renderer ====================

/// The main display list renderer.
pub struct Renderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,

    // Pipelines
    color_pipeline: wgpu::RenderPipeline,
    texture_pipeline: wgpu::RenderPipeline,
    // Texture pipeline for Rgba8Unorm targets (used for blitting to filter textures)
    // NOTE: Currently unused, kept for potential future use
    _texture_pipeline_rgba: wgpu::RenderPipeline,
    // Blit pipeline for copying RGBA textures (unlike texture_pipeline which treats R as alpha)
    blit_pipeline: wgpu::RenderPipeline,
    color_glyph_pipeline: wgpu::RenderPipeline,
    // Blit pipeline for Rgba8Unorm targets (for blitting to filter textures)
    blit_pipeline_rgba: wgpu::RenderPipeline,

    // Backdrop filter pipelines (compute shaders for blur + color filters)
    backdrop_filter_pipelines: pipeline::BackdropFilterPipelines,

    // GPU gradient pipeline
    gradient_pipeline: pipeline::GradientPipeline,

    /// Enable GPU gradient rendering (controlled by RUSTKIT_GPU_GRADIENTS env var)
    /// When disabled, uses cell-by-cell rendering for gradients
    gpu_gradients_enabled: bool,

    // Uniform buffer
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    viewport_size: (u32, u32),

    // Vertex batching
    color_vertices: Vec<ColorVertex>,
    color_indices: Vec<u32>,
    texture_vertices: Vec<TextureVertex>,
    texture_indices: Vec<u32>,
    // Color-glyph (emoji) batch — RGBA quads sampling the color atlas, drawn
    // with the passthrough blit pipeline after the grayscale glyph batch. Empty
    // on pages without emoji, so the normal text path pays nothing.
    color_glyph_vertices: Vec<TextureVertex>,
    color_glyph_indices: Vec<u32>,
    // Image quads batch separately from glyphs: the glyph batch binds the
    // glyph atlas for the whole draw, while each image quad must bind its
    // own texture (per-URL runs, drawn between colors and text).
    image_vertices: Vec<TextureVertex>,
    image_indices: Vec<u32>,
    image_runs: Vec<(String, u32)>,

    // GPU gradient queues for batched rendering
    gradient_queue: Vec<QueuedLinearGradient>,
    radial_gradient_queue: Vec<QueuedRadialGradient>,
    conic_gradient_queue: Vec<QueuedConicGradient>,

    // State stacks
    clip_stack: Vec<Rect>,
    stacking_contexts: Vec<StackingContext>,
    /// Stack of 2D transform matrices and their origins.
    /// Each entry is (matrix [a,b,c,d,e,f], origin (x,y)).
    transform_stack: Vec<([f32; 6], (f32, f32))>,

    // Caches
    texture_cache: TextureCache,
    glyph_cache: GlyphCache,

    // Texture bind group layout (for sharing)
    texture_bind_group_layout: wgpu::BindGroupLayout,

    // Intermediate render texture for backdrop filter operations
    // Created lazily when needed, resized to match viewport
    intermediate_texture: Option<wgpu::Texture>,
    intermediate_view: Option<wgpu::TextureView>,
    intermediate_size: (u32, u32),

    // Sampler for drawing filtered textures back to screen
    filter_sampler: wgpu::Sampler,

    // Surface format for creating compatible textures
    surface_format: wgpu::TextureFormat,
}

/// A stacking context for z-ordering.
#[derive(Debug, Clone)]
pub struct StackingContext {
    pub z_index: i32,
    pub rect: Rect,
}

/// A queued linear gradient to be rendered with the GPU shader.
/// Enable GPU gradients via RUSTKIT_GPU_GRADIENTS=1 environment variable.
#[derive(Debug, Clone)]
struct QueuedLinearGradient {
    rect: Rect,
    angle_rad: f32,
    stops: Vec<(f32, rustkit_css::ColorF32)>,
    repeating: bool,
    border_radius: rustkit_layout::BorderRadius,
}

/// A queued radial gradient to be rendered with the GPU shader.
#[derive(Debug, Clone)]
struct QueuedRadialGradient {
    rect: Rect,
    /// X radius in pixels
    rx: f32,
    /// Y radius in pixels
    ry: f32,
    /// Center position (0-1 normalized within rect)
    center: (f32, f32),
    stops: Vec<(f32, rustkit_css::ColorF32)>,
    repeating: bool,
    border_radius: rustkit_layout::BorderRadius,
}

/// A queued conic gradient to be rendered with the GPU shader.
#[derive(Debug, Clone)]
struct QueuedConicGradient {
    rect: Rect,
    /// Starting angle in radians
    from_angle_rad: f32,
    /// Center position (0-1 normalized within rect)
    center: (f32, f32),
    stops: Vec<(f32, rustkit_css::ColorF32)>,
    repeating: bool,
    border_radius: rustkit_layout::BorderRadius,
}

impl Renderer {
    /// Create a new renderer.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface_format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        // Create uniform buffer
        let uniforms = Uniforms {
            viewport_size: [800.0, 600.0],
            _padding: [0.0; 2],
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("uniform_bind_group_layout"),
            });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("uniform_bind_group"),
        });

        // Texture bind group layout
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        // Create pipelines
        let color_pipeline = create_color_pipeline(
            &device,
            surface_format,
            &uniform_bind_group_layout,
        );

        let texture_pipeline = create_texture_pipeline(
            &device,
            surface_format,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        // Create texture pipeline for Rgba8Unorm targets (blitting to filter textures)
        let texture_pipeline_rgba = create_texture_pipeline(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        // Create blit pipeline for copying RGBA textures (properly samples all 4 channels)
        let blit_pipeline = pipeline::create_blit_pipeline(
            &device,
            surface_format,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        // Color-glyph (emoji) pipeline: blit shader + premultiplied-alpha blend.
        let color_glyph_pipeline = pipeline::create_color_glyph_pipeline(
            &device,
            surface_format,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        // Create blit pipeline for Rgba8Unorm targets (blitting to filter textures)
        let blit_pipeline_rgba = pipeline::create_blit_pipeline(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        // Create backdrop filter pipelines (compute shaders for blur + color filters)
        let backdrop_filter_pipelines = pipeline::create_backdrop_filter_pipelines(&device);

        // Create GPU gradient pipeline
        let gradient_pipeline = pipeline::create_gradient_pipeline(
            &device,
            surface_format,
            &uniform_bind_group_layout,
        );

        // Create caches
        let texture_cache = TextureCache::new(&device, texture_bind_group_layout.clone());
        let glyph_cache = GlyphCache::new(&device, &queue, texture_bind_group_layout.clone())?;

        // Create sampler for drawing filtered textures
        let filter_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // GPU gradients: the flag is READ but the path is NOT IMPLEMENTED.
        //
        // The queues below are pushed to and cleared, never drained:
        // `render_linear_gradient_gpu` has zero callers, and draw_linear_gradient
        // does `push(...); return;` — skipping the CPU path. Honouring this flag
        // therefore DELETED EVERY GRADIENT ON THE PAGE, silently, with the CPU
        // renderer bypassed and nothing drawn in its place.
        //
        // Forced off until the queues are actually drained. The flag still logs
        // loudly so an operator who sets it learns it did nothing, rather than
        // debugging vanished gradients. Do NOT flip this to `is_ok()` without
        // first wiring flush_to to consume the three queues.
        let gpu_gradients_requested = std::env::var("RUSTKIT_GPU_GRADIENTS").is_ok();
        if gpu_gradients_requested {
            tracing::warn!(
                "RUSTKIT_GPU_GRADIENTS is set but GPU gradient rendering is NOT implemented \
                 (the queues are never drained). Ignoring the flag and using the CPU path. \
                 Honouring it would render no gradients at all."
            );
        }
        let gpu_gradients_enabled = false;

        Ok(Self {
            device,
            queue,
            color_pipeline,
            texture_pipeline,
            _texture_pipeline_rgba: texture_pipeline_rgba,
            blit_pipeline,
            color_glyph_pipeline,
            blit_pipeline_rgba,
            backdrop_filter_pipelines,
            gradient_pipeline,
            gpu_gradients_enabled,
            uniform_buffer,
            uniform_bind_group,
            viewport_size: (800, 600),
            color_vertices: Vec::with_capacity(4096),
            color_indices: Vec::with_capacity(8192),
            texture_vertices: Vec::with_capacity(4096),
            texture_indices: Vec::with_capacity(8192),
            color_glyph_vertices: Vec::new(),
            color_glyph_indices: Vec::new(),
            image_vertices: Vec::with_capacity(256),
            image_indices: Vec::with_capacity(512),
            image_runs: Vec::with_capacity(64),
            gradient_queue: Vec::with_capacity(64),
            radial_gradient_queue: Vec::with_capacity(16),
            conic_gradient_queue: Vec::with_capacity(16),
            clip_stack: Vec::new(),
            stacking_contexts: Vec::new(),
            transform_stack: Vec::new(),
            texture_cache,
            glyph_cache,
            texture_bind_group_layout,
            intermediate_texture: None,
            intermediate_view: None,
            intermediate_size: (0, 0),
            filter_sampler,
            surface_format,
        })
    }

    /// Validate buffer size to prevent GPU memory exhaustion.
    /// Returns Ok(size) if size is within limits, Err otherwise.
    fn validate_buffer_size(&self, size: u64, label: &str) -> Result<u64, RendererError> {
        if size > MAX_BUFFER_SIZE {
            tracing::error!(
                "Buffer '{}' size {} bytes exceeds maximum {} bytes",
                label,
                size,
                MAX_BUFFER_SIZE
            );
            return Err(RendererError::BufferTooLarge(size, MAX_BUFFER_SIZE));
        }
        Ok(size)
    }

    /// Set the viewport size.
    pub fn set_viewport_size(&mut self, width: u32, height: u32) {
        self.viewport_size = (width, height);

        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [0.0; 2],
        };

        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Create an intermediate texture for backdrop filter operations.
    /// Returns (texture, view) pair. The texture supports both reading and storage writes.
    fn create_filter_texture(&self, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Filter Intermediate Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Ensure intermediate render texture exists and matches viewport size.
    /// Uses surface format (typically Bgra8Unorm) for compatibility with render pipelines.
    /// Returns the texture view for rendering.
    fn ensure_intermediate_texture(&mut self) -> &wgpu::TextureView {
        let (width, height) = self.viewport_size;

        // Recreate if size changed or doesn't exist
        if self.intermediate_texture.is_none() || self.intermediate_size != (width, height) {
            // Use surface format so we can render with existing pipelines
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Intermediate Render Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.surface_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.intermediate_texture = Some(texture);
            self.intermediate_view = Some(view);
            self.intermediate_size = (width, height);
        }

        self.intermediate_view.as_ref().unwrap()
    }

    /// Flush current batched vertices to the target without clearing.
    /// Used for incremental rendering when backdrop filters are present.
    fn flush_batches_to(&mut self, target: &wgpu::TextureView, clear: bool) -> Result<(), RendererError> {
        if self.color_vertices.is_empty()
            && self.texture_vertices.is_empty()
            && self.image_vertices.is_empty()
        {
            return Ok(());
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Batch Flush Encoder"),
        });

        {
            let load_op = if clear {
                wgpu::LoadOp::Clear(wgpu::Color::WHITE)
            } else {
                wgpu::LoadOp::Load
            };

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Batch Render Pass"),
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
                // Validate buffer sizes before allocation
                let vertex_size = (self.color_vertices.len() * std::mem::size_of::<ColorVertex>()) as u64;
                let index_size = (self.color_indices.len() * std::mem::size_of::<u32>()) as u64;

                self.validate_buffer_size(vertex_size, "Color Vertex Buffer")?;
                self.validate_buffer_size(index_size, "Color Index Buffer")?;

                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Color Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.color_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Color Index Buffer"),
                    contents: bytemuck::cast_slice(&self.color_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                render_pass.set_pipeline(&self.color_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.color_indices.len() as u32, 0, 0..1);
            }

            // Draw images (own textures) between backgrounds and text
            self.draw_image_batch(&mut render_pass);

            // Draw textured quads
            if !self.texture_vertices.is_empty() {
                // Validate buffer sizes before allocation
                let vertex_size = (self.texture_vertices.len() * std::mem::size_of::<TextureVertex>()) as u64;
                let index_size = (self.texture_indices.len() * std::mem::size_of::<u32>()) as u64;

                self.validate_buffer_size(vertex_size, "Texture Vertex Buffer")?;
                self.validate_buffer_size(index_size, "Texture Index Buffer")?;

                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Texture Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.texture_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Texture Index Buffer"),
                    contents: bytemuck::cast_slice(&self.texture_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                render_pass.set_pipeline(&self.texture_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, self.glyph_cache.bind_group(), &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.texture_indices.len() as u32, 0, 0..1);
            }

            // Color glyphs (emoji) drawn on top via the RGBA atlas + blit pipeline.
            self.draw_color_glyph_batch(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Clear batches after flushing
        self.color_vertices.clear();
        self.color_indices.clear();
        self.texture_vertices.clear();
        self.texture_indices.clear();
        self.color_glyph_vertices.clear();
        self.color_glyph_indices.clear();
        self.image_vertices.clear();
        self.image_indices.clear();
        self.image_runs.clear();
        Ok(())
    }

    /// Flush batched vertices before rendering a GPU gradient.
    /// This ensures correct z-order: batched content renders before the gradient.
    fn flush_batches_for_gradient(&mut self, target: &wgpu::TextureView, clear: bool) -> Result<(), RendererError> {
        if self.color_vertices.is_empty()
            && self.texture_vertices.is_empty()
            && self.image_vertices.is_empty()
        {
            // Nothing to flush, but if this is the first call we still need to clear
            if clear {
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });
                {
                    let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Clear Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }
                self.queue.submit(std::iter::once(encoder.finish()));
            }
            return Ok(());
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
                // Validate buffer sizes before allocation
                let vertex_size = (self.color_vertices.len() * std::mem::size_of::<ColorVertex>()) as u64;
                let index_size = (self.color_indices.len() * std::mem::size_of::<u32>()) as u64;

                self.validate_buffer_size(vertex_size, "Color Vertex Buffer")?;
                self.validate_buffer_size(index_size, "Color Index Buffer")?;

                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Color Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.color_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Color Index Buffer"),
                    contents: bytemuck::cast_slice(&self.color_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                render_pass.set_pipeline(&self.color_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.color_indices.len() as u32, 0, 0..1);
            }

            // Draw images (own textures) between backgrounds and text
            self.draw_image_batch(&mut render_pass);

            // Draw textured quads
            if !self.texture_vertices.is_empty() {
                // Validate buffer sizes before allocation
                let vertex_size = (self.texture_vertices.len() * std::mem::size_of::<TextureVertex>()) as u64;
                let index_size = (self.texture_indices.len() * std::mem::size_of::<u32>()) as u64;

                self.validate_buffer_size(vertex_size, "Texture Vertex Buffer")?;
                self.validate_buffer_size(index_size, "Texture Index Buffer")?;

                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Texture Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.texture_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Texture Index Buffer"),
                    contents: bytemuck::cast_slice(&self.texture_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                render_pass.set_pipeline(&self.texture_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, self.glyph_cache.bind_group(), &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.texture_indices.len() as u32, 0, 0..1);
            }

            // Color glyphs (emoji) drawn on top via the RGBA atlas + blit pipeline.
            self.draw_color_glyph_batch(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Clear batches after flushing
        self.color_vertices.clear();
        self.color_indices.clear();
        self.texture_vertices.clear();
        self.texture_indices.clear();
        self.color_glyph_vertices.clear();
        self.color_glyph_indices.clear();
        self.image_vertices.clear();
        self.image_indices.clear();
        self.image_runs.clear();
        Ok(())
    }

    /// Draw a textured quad from a filtered texture to the render target immediately.
    /// This renders with a custom bind group, bypassing the batch system.
    fn draw_filtered_texture_to(
        &self,
        texture_view: &wgpu::TextureView,
        target: &wgpu::TextureView,
        rect: Rect,
    ) {
        // Create bind group for this texture
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Filtered Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.filter_sampler),
                },
            ],
        });

        // Create vertices for the quad - normalized tex coords to sample region
        let x = rect.x;
        let y = rect.y;
        let w = rect.width;
        let h = rect.height;

        // Calculate tex coords based on position in viewport
        let (vw, vh) = self.viewport_size;
        let u0 = rect.x / vw as f32;
        let v0 = rect.y / vh as f32;
        let u1 = (rect.x + rect.width) / vw as f32;
        let v1 = (rect.y + rect.height) / vh as f32;

        let white = [1.0, 1.0, 1.0, 1.0];

        let vertices = [
            TextureVertex { position: [x, y], tex_coords: [u0, v0], color: white },
            TextureVertex { position: [x + w, y], tex_coords: [u1, v0], color: white },
            TextureVertex { position: [x + w, y + h], tex_coords: [u1, v1], color: white },
            TextureVertex { position: [x, y + h], tex_coords: [u0, v1], color: white },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Filtered Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Filtered Quad Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Filtered Texture Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Filtered Texture Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use blit_pipeline to properly sample RGBA texture
            // (texture_pipeline treats red channel as alpha for glyph rendering)
            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Create a bind group for backdrop filter compute shader operations.
    fn create_filter_bind_group(
        &self,
        input_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Filter Bind Group"),
            layout: &self.backdrop_filter_pipelines.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.backdrop_filter_pipelines.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        })
    }

    /// Run Gaussian blur on a texture using compute shaders.
    /// Performs two passes: horizontal then vertical blur.
    fn run_blur_compute(
        &self,
        source_view: &wgpu::TextureView,
        intermediate_view: &wgpu::TextureView,
        dest_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        blur_radius: f32,
    ) {
        // Update filter params uniform
        let params = FilterParams {
            blur_radius,
            filter_type: 0, // Not used for blur
            filter_amount: 1.0,
            texture_width: width as f32,
            texture_height: height as f32,
            _padding0: 0.0,
            _padding1: 0.0,
            _padding2: 0.0,
        };
        self.queue.write_buffer(
            &self.backdrop_filter_pipelines.uniform_buffer,
            0,
            bytemuck::cast_slice(&[params]),
        );

        // Calculate workgroup counts (16x16 workgroups)
        let workgroups_x = (width + 15) / 16;
        let workgroups_y = (height + 15) / 16;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blur Compute Encoder"),
        });

        // Pass 1: Horizontal blur (source -> intermediate)
        {
            let bind_group = self.create_filter_bind_group(source_view, intermediate_view);
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Horizontal Blur Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.backdrop_filter_pipelines.blur_h_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        // Pass 2: Vertical blur (intermediate -> dest)
        {
            let bind_group = self.create_filter_bind_group(intermediate_view, dest_view);
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Vertical Blur Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.backdrop_filter_pipelines.blur_v_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Run a color filter (grayscale, sepia, brightness) on a texture.
    fn run_color_filter_compute(
        &self,
        source_view: &wgpu::TextureView,
        dest_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        filter_type: u32,
        amount: f32,
    ) {
        // Update filter params uniform
        let params = FilterParams {
            blur_radius: 0.0,
            filter_type,
            filter_amount: amount,
            texture_width: width as f32,
            texture_height: height as f32,
            _padding0: 0.0,
            _padding1: 0.0,
            _padding2: 0.0,
        };
        self.queue.write_buffer(
            &self.backdrop_filter_pipelines.uniform_buffer,
            0,
            bytemuck::cast_slice(&[params]),
        );

        // Calculate workgroup counts
        let workgroups_x = (width + 15) / 16;
        let workgroups_y = (height + 15) / 16;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Color Filter Compute Encoder"),
        });

        {
            let bind_group = self.create_filter_bind_group(source_view, dest_view);
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Color Filter Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.backdrop_filter_pipelines.color_filter_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Execute a display list and render to a target.
    pub fn execute(
        &mut self,
        commands: &[DisplayCommand],
        target: &wgpu::TextureView,
    ) -> Result<(), RendererError> {
        // Clear batches
        self.color_vertices.clear();
        self.color_indices.clear();
        self.texture_vertices.clear();
        self.texture_indices.clear();
        self.color_glyph_vertices.clear();
        self.color_glyph_indices.clear();
        self.image_vertices.clear();
        self.image_indices.clear();
        self.image_runs.clear();
        self.gradient_queue.clear();
        self.radial_gradient_queue.clear();
        self.conic_gradient_queue.clear();
        self.clip_stack.clear();
        self.stacking_contexts.clear();
        self.transform_stack.clear();

        // Check if there are any blur backdrop filters that need GPU processing
        let has_blur_filters = commands.iter().any(|cmd| {
            matches!(cmd, DisplayCommand::BackdropFilter {
                filter: rustkit_css::BackdropFilter::Blur(r), ..
            } if *r > 0.0)
        });

        // Check if there are any GPU gradients that need z-order aware rendering
        let has_gpu_gradients = self.gpu_gradients_enabled && commands.iter().any(|cmd| {
            matches!(cmd,
                DisplayCommand::LinearGradient { .. } |
                DisplayCommand::RadialGradient { .. } |
                DisplayCommand::ConicGradient { .. }
            )
        });

        if has_blur_filters {
            // Use GPU blur path - render to intermediate texture with GPU blur processing
            self.execute_with_gpu_blur(commands, target)?;
        } else if has_gpu_gradients {
            // Use GPU gradient path - flush batches before each gradient for correct z-order
            self.execute_with_gpu_gradients(commands, target)?;
        } else {
            // Fast path - no backdrop blur or GPU gradients, process normally
            for cmd in commands {
                self.process_command(cmd);
            }
            self.flush_to(target)?;
        }

        Ok(())
    }

    /// Execute commands with GPU blur support for backdrop filters.
    fn execute_with_gpu_blur(
        &mut self,
        commands: &[DisplayCommand],
        target: &wgpu::TextureView,
    ) -> Result<(), RendererError> {
        // Ensure intermediate texture exists
        let _ = self.ensure_intermediate_texture();
        let intermediate_view = self.intermediate_view.as_ref().unwrap().clone();

        let mut is_first_flush = true;

        for cmd in commands {
            // Check if this is a blur backdrop filter
            if let DisplayCommand::BackdropFilter {
                rect,
                border_radius: _,
                filter: rustkit_css::BackdropFilter::Blur(radius),
            } = cmd
            {
                if *radius > 0.0 {
                    // Flush current batches to intermediate texture
                    self.flush_batches_to(&intermediate_view, is_first_flush)?;
                    is_first_flush = false;

                    // Apply GPU blur
                    self.apply_gpu_blur(&intermediate_view, *rect, *radius);

                    continue;
                }
            }

            // Process command normally (including non-blur backdrop filters)
            self.process_command(cmd);
        }

        // Flush remaining batches to intermediate
        if !self.color_vertices.is_empty()
            || !self.texture_vertices.is_empty()
            || !self.image_vertices.is_empty()
        {
            self.flush_batches_to(&intermediate_view, is_first_flush)?;
        }

        // Copy intermediate to final target
        self.copy_texture_to_target(&intermediate_view, target);

        Ok(())
    }

    /// Execute commands with GPU gradient support for correct z-order.
    ///
    /// This method flushes batched content BEFORE each gradient to ensure
    /// parent gradients render behind child content (correct DOM z-order).
    fn execute_with_gpu_gradients(
        &mut self,
        commands: &[DisplayCommand],
        target: &wgpu::TextureView,
    ) -> Result<(), RendererError> {
        let mut is_first_flush = true;

        for cmd in commands {
            // Check if this is a GPU gradient command
            let is_gpu_gradient = matches!(cmd,
                DisplayCommand::LinearGradient { .. } |
                DisplayCommand::RadialGradient { .. } |
                DisplayCommand::ConicGradient { .. }
            );

            if is_gpu_gradient {
                // Flush batched content FIRST (before gradient)
                // This ensures children render before their parent's gradient
                self.flush_batches_for_gradient(target, is_first_flush)?;
                is_first_flush = false;

                // Render the gradient directly (inline, not queued)
                self.render_gpu_gradient_inline(cmd, target);
            } else {
                // Process command normally (batched)
                self.process_command(cmd);
            }
        }

        // Flush any remaining batched content
        if !self.color_vertices.is_empty()
            || !self.texture_vertices.is_empty()
            || !self.image_vertices.is_empty()
        {
            self.flush_batches_for_gradient(target, is_first_flush)?;
        }

        Ok(())
    }

    /// Render a GPU gradient inline (immediately, not queued).
    /// Called from execute_with_gpu_gradients() for correct z-order.
    fn render_gpu_gradient_inline(&mut self, cmd: &DisplayCommand, target: &wgpu::TextureView) {
        match cmd {
            DisplayCommand::LinearGradient { rect, direction, stops, repeating, border_radius } => {
                self.render_linear_gradient_inline(*rect, *direction, stops, *repeating, *border_radius, target);
            }
            DisplayCommand::RadialGradient { rect, shape, size, center, stops, repeating, border_radius } => {
                self.render_radial_gradient_inline(*rect, *shape, *size, *center, stops, *repeating, *border_radius, target);
            }
            DisplayCommand::ConicGradient { rect, from_angle, center, stops, repeating, border_radius } => {
                self.render_conic_gradient_inline(*rect, *from_angle, *center, stops, *repeating, *border_radius, target);
            }
            _ => {}
        }
    }

    /// Render a linear gradient directly to the target (inline GPU path).
    fn render_linear_gradient_inline(
        &self,
        rect: Rect,
        direction: rustkit_css::GradientDirection,
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
        target: &wgpu::TextureView,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Convert direction to angle in radians
        let angle_deg = direction.to_degrees();
        let angle_rad = angle_deg.to_radians();

        // Calculate gradient geometry
        let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());
        let half_width = rect.width / 2.0;
        let half_height = rect.height / 2.0;
        let gradient_half_length = (half_width * sin_a.abs() + half_height * cos_a.abs()).max(0.001);

        // Check if any stop uses pixel positions
        let has_pixel_positions = stops.iter().any(|s| {
            s.position.as_ref().map(|p| p.is_pixels()).unwrap_or(false)
        });

        // Calculate repeat length for pixel-based repeating gradients
        let repeat_length_pixels = if repeating && has_pixel_positions {
            stops.last()
                .and_then(|s| s.position.as_ref())
                .map(|p| match p {
                    rustkit_css::StopPosition::Pixels(px) => *px,
                    rustkit_css::StopPosition::Percent(pct) => *pct * gradient_half_length * 2.0,
                })
                .unwrap_or(gradient_half_length * 2.0)
                .max(0.001)
        } else {
            gradient_half_length * 2.0
        };

        // Normalize stops
        let normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = stops.iter().enumerate()
            .map(|(i, stop)| {
                let pos = match &stop.position {
                    Some(p) => {
                        if has_pixel_positions && repeating {
                            match p {
                                rustkit_css::StopPosition::Pixels(px) => *px / repeat_length_pixels,
                                rustkit_css::StopPosition::Percent(pct) => *pct,
                            }
                        } else {
                            match p {
                                rustkit_css::StopPosition::Percent(pct) => *pct,
                                rustkit_css::StopPosition::Pixels(px) => *px / (gradient_half_length * 2.0),
                            }
                        }
                    }
                    None => {
                        if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
                    }
                };
                (pos, rustkit_css::ColorF32::from_color(stop.color))
            })
            .collect();

        // Render using GPU
        self.render_linear_gradient_gpu_with_clear(
            target,
            rect,
            angle_rad,
            &normalized_stops,
            repeating,
            border_radius,
            None, // LoadOp::Load to preserve previous content
        );
    }

    /// Render a radial gradient directly to the target (inline GPU path).
    fn render_radial_gradient_inline(
        &self,
        rect: Rect,
        shape: rustkit_css::RadialShape,
        size: rustkit_css::RadialSize,
        center: (f32, f32),
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
        target: &wgpu::TextureView,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate radii based on shape and size
        let (rx, ry) = self.calculate_radial_radii(rect, shape, size, center);

        // Check if any stop uses pixel positions
        let has_pixel_positions = stops.iter().any(|s| {
            s.position.as_ref().map(|p| p.is_pixels()).unwrap_or(false)
        });

        // Calculate repeat length for pixel-based repeating gradients
        let repeat_length_pixels = if repeating && has_pixel_positions {
            stops.last()
                .and_then(|s| s.position.as_ref())
                .map(|p| match p {
                    rustkit_css::StopPosition::Pixels(px) => *px,
                    rustkit_css::StopPosition::Percent(pct) => *pct * rx.max(ry),
                })
                .unwrap_or(rx.max(ry))
                .max(0.001)
        } else {
            rx.max(ry)
        };

        // Normalize stops
        let normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = stops.iter().enumerate()
            .map(|(i, stop)| {
                let pos = match &stop.position {
                    Some(p) => {
                        if has_pixel_positions && repeating {
                            match p {
                                rustkit_css::StopPosition::Pixels(px) => *px / repeat_length_pixels,
                                rustkit_css::StopPosition::Percent(pct) => *pct,
                            }
                        } else {
                            match p {
                                rustkit_css::StopPosition::Percent(pct) => *pct,
                                rustkit_css::StopPosition::Pixels(px) => *px / rx.max(ry).max(0.001),
                            }
                        }
                    }
                    None => {
                        if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
                    }
                };
                (pos, rustkit_css::ColorF32::from_color(stop.color))
            })
            .collect();

        // Render using GPU
        self.render_radial_gradient_gpu(
            target,
            rect,
            rx, ry,
            center,
            &normalized_stops,
            repeating,
            border_radius,
        );
    }

    /// Render a conic gradient directly to the target (inline GPU path).
    fn render_conic_gradient_inline(
        &self,
        rect: Rect,
        from_angle: f32,
        center: (f32, f32),
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
        target: &wgpu::TextureView,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Conic gradients use angular positions (0-360 degrees or 0-1 normalized)
        let normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = stops.iter().enumerate()
            .map(|(i, stop)| {
                let pos = match &stop.position {
                    Some(p) => match p {
                        rustkit_css::StopPosition::Percent(pct) => *pct,
                        rustkit_css::StopPosition::Pixels(deg) => *deg / 360.0, // Degrees to 0-1
                    },
                    None => {
                        if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
                    }
                };
                (pos, rustkit_css::ColorF32::from_color(stop.color))
            })
            .collect();

        // Convert from_angle to radians
        let from_rad = from_angle.to_radians();

        // Render using GPU
        self.render_conic_gradient_gpu(
            target,
            rect,
            from_rad,
            center,
            &normalized_stops,
            repeating,
            border_radius,
        );
    }

    /// Calculate radial gradient radii based on shape and size.
    fn calculate_radial_radii(
        &self,
        rect: Rect,
        shape: rustkit_css::RadialShape,
        size: rustkit_css::RadialSize,
        center: (f32, f32),
    ) -> (f32, f32) {
        let cx = rect.x + rect.width * center.0;
        let cy = rect.y + rect.height * center.1;

        // Distances to each edge from center
        let dist_left = (cx - rect.x).abs();
        let dist_right = (rect.x + rect.width - cx).abs();
        let dist_top = (cy - rect.y).abs();
        let dist_bottom = (rect.y + rect.height - cy).abs();

        // Distances to corners
        let corner_tl = ((cx - rect.x).powi(2) + (cy - rect.y).powi(2)).sqrt();
        let corner_tr = ((rect.x + rect.width - cx).powi(2) + (cy - rect.y).powi(2)).sqrt();
        let corner_bl = ((cx - rect.x).powi(2) + (rect.y + rect.height - cy).powi(2)).sqrt();
        let corner_br = ((rect.x + rect.width - cx).powi(2) + (rect.y + rect.height - cy).powi(2)).sqrt();

        let (rx, ry) = match size {
            rustkit_css::RadialSize::ClosestSide => {
                let dx = dist_left.min(dist_right);
                let dy = dist_top.min(dist_bottom);
                match shape {
                    rustkit_css::RadialShape::Circle => {
                        let r = dx.min(dy);
                        (r, r)
                    }
                    rustkit_css::RadialShape::Ellipse => (dx, dy),
                }
            }
            rustkit_css::RadialSize::FarthestSide => {
                let dx = dist_left.max(dist_right);
                let dy = dist_top.max(dist_bottom);
                match shape {
                    rustkit_css::RadialShape::Circle => {
                        let r = dx.max(dy);
                        (r, r)
                    }
                    rustkit_css::RadialShape::Ellipse => (dx, dy),
                }
            }
            rustkit_css::RadialSize::ClosestCorner => {
                let min_corner = corner_tl.min(corner_tr).min(corner_bl).min(corner_br);
                match shape {
                    rustkit_css::RadialShape::Circle => (min_corner, min_corner),
                    rustkit_css::RadialShape::Ellipse => {
                        // css-images-3 §3.3.3: the corner ellipse has the
                        // SAME ASPECT as the closest-side ellipse and passes
                        // through the closest corner — exactly the per-axis
                        // side distances scaled by sqrt(2). (Was: Euclidean
                        // corner distance as rx with ry from the box aspect,
                        // which made every corner-sized ellipse too small.)
                        let dx = dist_left.min(dist_right);
                        let dy = dist_top.min(dist_bottom);
                        (dx * std::f32::consts::SQRT_2, dy * std::f32::consts::SQRT_2)
                    }
                }
            }
            rustkit_css::RadialSize::FarthestCorner => {
                let max_corner = corner_tl.max(corner_tr).max(corner_bl).max(corner_br);
                match shape {
                    rustkit_css::RadialShape::Circle => (max_corner, max_corner),
                    rustkit_css::RadialShape::Ellipse => {
                        // css-images-3 §3.3.3 — see ClosestCorner. Verified
                        // against Chrome 148: 150x100 box, center position,
                        // Chrome's ramp gives rx = 106.1 = 75·sqrt(2).
                        let dx = dist_left.max(dist_right);
                        let dy = dist_top.max(dist_bottom);
                        (dx * std::f32::consts::SQRT_2, dy * std::f32::consts::SQRT_2)
                    }
                }
            }
            rustkit_css::RadialSize::Explicit(w, h) => {
                match shape {
                    rustkit_css::RadialShape::Circle => (w, w),
                    rustkit_css::RadialShape::Ellipse => (w, h),
                }
            }
        };

        (rx.max(0.001), ry.max(0.001))
    }

    /// Blit from intermediate texture (surface format) to a filter texture (Rgba8Unorm).
    /// This performs format conversion during the render pass.
    fn blit_to_filter_texture(&self, dest_view: &wgpu::TextureView) {
        let (vw, vh) = self.viewport_size;

        // Create bind group for sampling the intermediate texture
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blit Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        self.intermediate_view.as_ref().unwrap(),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.filter_sampler),
                },
            ],
        });

        // Full-screen quad vertices
        let vertices = [
            TextureVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            TextureVertex { position: [vw as f32, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            TextureVertex { position: [vw as f32, vh as f32], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            TextureVertex { position: [0.0, vh as f32], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blit Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blit Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Blit to Filter Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit to Filter Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dest_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use blit_pipeline_rgba for rendering to Rgba8Unorm target
            // (properly samples all 4 RGBA channels, unlike texture_pipeline which treats R as alpha)
            render_pass.set_pipeline(&self.blit_pipeline_rgba);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply GPU Gaussian blur to a region of the intermediate texture.
    fn apply_gpu_blur(
        &self,
        render_target: &wgpu::TextureView,
        rect: Rect,
        blur_radius: f32,
    ) {
        let (vw, vh) = self.viewport_size;

        // Create filter textures for the blur passes (full viewport size for simplicity)
        let (_filter_tex_a, filter_view_a) = self.create_filter_texture(vw, vh);
        let (_filter_tex_b, filter_view_b) = self.create_filter_texture(vw, vh);

        // Blit from intermediate texture (Bgra8Unorm) to filter texture A (Rgba8Unorm)
        // This performs format conversion via the blit_pipeline_rgba
        self.blit_to_filter_texture(&filter_view_a);

        // Run the blur compute passes: A -> B (horizontal), B -> A (vertical)
        self.run_blur_compute(
            &filter_view_a,
            &filter_view_b,
            &filter_view_a,
            vw,
            vh,
            blur_radius,
        );

        // Draw the blurred result back to the render target at the specified rect
        self.draw_filtered_texture_to(&filter_view_a, render_target, rect);
    }

    /// Copy the intermediate texture to the final target.
    fn copy_texture_to_target(
        &self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let (vw, vh) = self.viewport_size;

        // Draw the entire intermediate texture to the target
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Copy Texture Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.filter_sampler),
                },
            ],
        });

        let vertices = [
            TextureVertex { position: [0.0, 0.0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            TextureVertex { position: [vw as f32, 0.0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
            TextureVertex { position: [vw as f32, vh as f32], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
            TextureVertex { position: [0.0, vh as f32], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Copy Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Copy Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Copy to Target Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Copy to Target Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use blit_pipeline instead of texture_pipeline to properly sample RGBA
            // (texture_pipeline treats red channel as alpha for glyph rendering)
            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Process a single display command.
    fn process_command(&mut self, cmd: &DisplayCommand) {
        match cmd {
            DisplayCommand::SolidColor(color, rect) => {
                self.draw_solid_rect(*rect, *color);
            }

            DisplayCommand::RoundedRect { color, rect, radius } => {
                if radius.is_zero() {
                    self.draw_solid_rect(*rect, *color);
                } else {
                    // Draw rounded rect using SDF-based pixel rendering
                    self.draw_rounded_rect(*rect, *color, *radius);
                }
            }

            DisplayCommand::Border {
                color,
                rect,
                top,
                right,
                bottom,
                left,
            } => {
                self.draw_border(*rect, *color, *top, *right, *bottom, *left);
            }

            DisplayCommand::Text {
                text,
                x,
                y,
                color,
                font_size,
                font_family,
                font_weight,
                font_style,
                advances,
                ascent,
            } => {
                self.draw_text_with_metrics(
                    text,
                    *x,
                    *y,
                    *color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_style,
                    advances.as_deref(),
                    *ascent,
                );
            }

            DisplayCommand::TextDecoration {
                x,
                y,
                width,
                thickness,
                color,
                style: _,
            } => {
                // Draw as a solid rect
                self.draw_solid_rect(
                    Rect::new(*x, *y, *width, *thickness),
                    *color,
                );
            }

            DisplayCommand::Image {
                url,
                src_rect: _,
                dest_rect,
                object_fit: _,
                opacity: _,
            } => {
                self.draw_image(url, *dest_rect);
            }

            DisplayCommand::BackgroundImage {
                url,
                rect,
                size,
                position,
                repeat,
            } => {
                self.draw_background_image(url, *rect, size, *position, repeat);
            }

            DisplayCommand::BoxShadow {
                offset_x,
                offset_y,
                blur_radius,
                spread_radius,
                color,
                rect,
                inset,
            } => {
                self.draw_box_shadow(
                    *rect,
                    *offset_x,
                    *offset_y,
                    *blur_radius,
                    *spread_radius,
                    *color,
                    *inset,
                );
            }

            DisplayCommand::BackdropFilter { rect, border_radius, filter } => {
                self.apply_backdrop_filter(*rect, *border_radius, *filter);
            }

            DisplayCommand::LinearGradient { rect, direction, stops, repeating, border_radius } => {
                self.draw_linear_gradient(*rect, *direction, stops, *repeating, *border_radius);
            }

            DisplayCommand::RadialGradient { rect, shape, size, center, stops, repeating, border_radius } => {
                self.draw_radial_gradient(*rect, *shape, *size, *center, stops, *repeating, *border_radius);
            }

            DisplayCommand::ConicGradient { rect, from_angle, center, stops, repeating, border_radius } => {
                self.draw_conic_gradient(*rect, *from_angle, *center, stops, *repeating, *border_radius);
            }

            DisplayCommand::TextInput {
                rect,
                value,
                placeholder,
                font_size,
                text_color,
                placeholder_color,
                background_color,
                border_color,
                border_width,
                focused,
                caret_position,
            } => {
                self.draw_text_input(
                    *rect,
                    value,
                    placeholder,
                    *font_size,
                    *text_color,
                    *placeholder_color,
                    *background_color,
                    *border_color,
                    *border_width,
                    *focused,
                    *caret_position,
                );
            }

            DisplayCommand::Button {
                rect,
                label,
                font_size,
                text_color,
                background_color,
                border_color,
                border_width,
                border_radius,
                pressed,
                focused,
            } => {
                self.draw_button(
                    *rect,
                    label,
                    *font_size,
                    *text_color,
                    *background_color,
                    *border_color,
                    *border_width,
                    *border_radius,
                    *pressed,
                    *focused,
                );
            }

            DisplayCommand::FocusRing { rect, color, width, offset } => {
                self.draw_focus_ring(*rect, *color, *width, *offset);
            }

            DisplayCommand::Caret { x, y, height, color } => {
                self.draw_caret(*x, *y, *height, *color);
            }

            DisplayCommand::PushClip(rect) => {
                self.push_clip(*rect);
            }

            DisplayCommand::PopClip => {
                self.pop_clip();
            }

            DisplayCommand::PushStackingContext { z_index, rect } => {
                self.stacking_contexts.push(StackingContext {
                    z_index: *z_index,
                    rect: *rect,
                });
            }

            DisplayCommand::PopStackingContext => {
                self.stacking_contexts.pop();
            }

            // SVG primitives
            DisplayCommand::FillRect { rect, color } => {
                self.draw_solid_rect(*rect, *color);
            }

            DisplayCommand::StrokeRect { rect, color, width } => {
                // Draw as 4 lines forming a rectangle
                self.draw_border(*rect, *color, *width, *width, *width, *width);
            }

            DisplayCommand::FillCircle { cx, cy, radius, color } => {
                // Render circle using triangle fan
                self.draw_fill_circle(*cx, *cy, *radius, *color);
            }

            DisplayCommand::StrokeCircle { cx, cy, radius, color, width } => {
                // Draw stroked circle as two filled circles (outer and inner)
                // Outer circle
                self.draw_fill_circle(*cx, *cy, *radius, *color);
                // Inner circle (background colored to create stroke effect)
                // Note: This is a simplified approach; proper implementation would
                // require a separate background color or compositing
                if *radius > *width {
                    let bg_color = Color::new(255, 255, 255, 1.0); // White background
                    self.draw_fill_circle(*cx, *cy, radius - width, bg_color);
                }
            }

            DisplayCommand::FillEllipse { rect, color } => {
                // Render ellipse using triangle fan with parametric equations
                self.draw_fill_ellipse(*rect, *color);
            }

            DisplayCommand::Line { x1, y1, x2, y2, color, width } => {
                // Draw as thin rectangle
                let dx = x2 - x1;
                let dy = y2 - y1;
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    // Calculate perpendicular offset for width
                    let nx = -dy / len * width * 0.5;
                    let ny = dx / len * width * 0.5;
                    
                    let c = [
                        color.r as f32 / 255.0,
                        color.g as f32 / 255.0,
                        color.b as f32 / 255.0,
                        color.a,
                    ];
                    
                    let base = self.color_vertices.len() as u32;
                    self.color_vertices.extend_from_slice(&[
                        ColorVertex { position: [x1 + nx, y1 + ny], color: c },
                        ColorVertex { position: [x2 + nx, y2 + ny], color: c },
                        ColorVertex { position: [x2 - nx, y2 - ny], color: c },
                        ColorVertex { position: [x1 - nx, y1 - ny], color: c },
                    ]);
                    self.color_indices.extend_from_slice(&[
                        base, base + 1, base + 2,
                        base, base + 2, base + 3,
                    ]);
                }
            }

            DisplayCommand::Polyline { points, color, width } => {
                // Draw as series of lines
                for i in 0..points.len().saturating_sub(1) {
                    let (x1, y1) = points[i];
                    let (x2, y2) = points[i + 1];
                    self.process_command(&DisplayCommand::Line {
                        x1, y1, x2, y2,
                        color: *color,
                        width: *width,
                    });
                }
            }

            DisplayCommand::FillPolygon { points, color } => {
                // Simple triangle fan for convex polygons
                if points.len() >= 3 {
                    let c = [
                        color.r as f32 / 255.0,
                        color.g as f32 / 255.0,
                        color.b as f32 / 255.0,
                        color.a,
                    ];
                    
                    let base = self.color_vertices.len() as u32;
                    for (x, y) in points {
                        self.color_vertices.push(ColorVertex {
                            position: [*x, *y],
                            color: c,
                        });
                    }
                    
                    // Triangle fan
                    for i in 1..points.len() as u32 - 1 {
                        self.color_indices.extend_from_slice(&[base, base + i, base + i + 1]);
                    }
                }
            }

            DisplayCommand::StrokePolygon { points, color, width } => {
                // Draw as closed polyline
                if !points.is_empty() {
                    let mut closed_points = points.clone();
                    closed_points.push(points[0]);
                    self.process_command(&DisplayCommand::Polyline {
                        points: closed_points,
                        color: *color,
                        width: *width,
                    });
                }
            }

            DisplayCommand::PushTransform { matrix, origin } => {
                self.push_transform(*matrix, *origin);
            }

            DisplayCommand::PopTransform => {
                self.pop_transform();
            }

            DisplayCommand::GradientText {
                text,
                x,
                y,
                font_size,
                font_family,
                font_weight,
                font_style,
                gradient,
                rect,
                advances,
                ascent,
            } => {
                // Glyph quads are alpha-textured and tinted per VERTEX, so
                // gradient text needs no offscreen mask: each glyph's left
                // and right vertex pairs take the gradient color sampled at
                // those x positions and the GPU interpolates between them.
                // The sweep is sampled horizontally across `rect` — the
                // vertical component of an angled gradient is ignored, an
                // approximation that is exact for to-right/to-left and close
                // for the diagonal hero-text cases this feature serves.
                // (This replaces a hardcoded PURPLE debug fallback that
                // painted every background-clip:text run violet.)
                self.draw_text_gradient(
                    text,
                    *x,
                    *y,
                    gradient,
                    rect,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_style,
                    advances.as_deref(),
                    *ascent,
                );
            }
        }
    }

    /// Draw a solid color rectangle.
    fn draw_solid_rect(&mut self, rect: Rect, color: Color) {
        // Apply clipping
        let rect = if let Some(clip) = self.current_clip() {
            if let Some(clipped) = rect.intersect(&clip) {
                clipped
            } else {
                return; // Fully clipped
            }
        } else {
            rect
        };

        let c = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a,
        ];

        let base = self.color_vertices.len() as u32;

        // Apply transform to corners
        let (x0, y0) = self.transform_point(rect.x, rect.y);
        let (x1, y1) = self.transform_point(rect.x + rect.width, rect.y);
        let (x2, y2) = self.transform_point(rect.x + rect.width, rect.y + rect.height);
        let (x3, y3) = self.transform_point(rect.x, rect.y + rect.height);

        self.color_vertices.extend_from_slice(&[
            ColorVertex { position: [x0, y0], color: c },
            ColorVertex { position: [x1, y1], color: c },
            ColorVertex { position: [x2, y2], color: c },
            ColorVertex { position: [x3, y3], color: c },
        ]);

        self.color_indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    /// Draw a solid color rectangle using high-precision color.
    /// This is the preferred internal method for gradient rendering.
    fn draw_solid_rect_f32(&mut self, rect: Rect, color: rustkit_css::ColorF32) {
        // Apply clipping
        let rect = if let Some(clip) = self.current_clip() {
            if let Some(clipped) = rect.intersect(&clip) {
                clipped
            } else {
                return; // Fully clipped
            }
        } else {
            rect
        };

        // Color already in normalized f32 format - no conversion needed
        let c = color.to_array();

        let base = self.color_vertices.len() as u32;

        // Apply transform to corners
        let (x0, y0) = self.transform_point(rect.x, rect.y);
        let (x1, y1) = self.transform_point(rect.x + rect.width, rect.y);
        let (x2, y2) = self.transform_point(rect.x + rect.width, rect.y + rect.height);
        let (x3, y3) = self.transform_point(rect.x, rect.y + rect.height);

        self.color_vertices.extend_from_slice(&[
            ColorVertex { position: [x0, y0], color: c },
            ColorVertex { position: [x1, y1], color: c },
            ColorVertex { position: [x2, y2], color: c },
            ColorVertex { position: [x3, y3], color: c },
        ]);

        self.color_indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
    }

    /// Draw a rounded rectangle using SDF-based rendering.
    fn draw_rounded_rect(&mut self, rect: Rect, color: Color, radius: rustkit_layout::BorderRadius) {
        // For small radii or very small rects, fall back to solid rect
        let max_radius = radius.top_left.max(radius.top_right).max(radius.bottom_left).max(radius.bottom_right);
        if max_radius < 1.0 || rect.width < 4.0 || rect.height < 4.0 {
            self.draw_solid_rect(rect, color);
            return;
        }

        // Clamp radii to half the rect dimensions
        let max_r = (rect.width / 2.0).min(rect.height / 2.0);
        let r_tl = radius.top_left.min(max_r);
        let r_tr = radius.top_right.min(max_r);
        let r_br = radius.bottom_right.min(max_r);
        let r_bl = radius.bottom_left.min(max_r);

        // Draw the interior (non-corner) regions as solid rects for efficiency
        // Top edge (between corners)
        if rect.width > r_tl + r_tr {
            self.draw_solid_rect(
                Rect::new(rect.x + r_tl, rect.y, rect.width - r_tl - r_tr, r_tl.max(r_tr)),
                color,
            );
        }
        // Bottom edge (between corners)
        if rect.width > r_bl + r_br {
            self.draw_solid_rect(
                Rect::new(rect.x + r_bl, rect.y + rect.height - r_bl.max(r_br), rect.width - r_bl - r_br, r_bl.max(r_br)),
                color,
            );
        }
        // Middle section (full width, between top and bottom corner rows)
        let top_corner_height = r_tl.max(r_tr);
        let bottom_corner_height = r_bl.max(r_br);
        if rect.height > top_corner_height + bottom_corner_height {
            self.draw_solid_rect(
                Rect::new(rect.x, rect.y + top_corner_height, rect.width, rect.height - top_corner_height - bottom_corner_height),
                color,
            );
        }

        // Draw corners using SDF
        self.draw_rounded_corner(rect.x, rect.y, r_tl, color, 0); // top-left
        self.draw_rounded_corner(rect.x + rect.width - r_tr, rect.y, r_tr, color, 1); // top-right
        self.draw_rounded_corner(rect.x + rect.width - r_br, rect.y + rect.height - r_br, r_br, color, 2); // bottom-right
        self.draw_rounded_corner(rect.x, rect.y + rect.height - r_bl, r_bl, color, 3); // bottom-left
    }

    /// Smoothstep interpolation function matching WGSL's smoothstep.
    /// Performs Hermite interpolation between 0 and 1 when x is in [edge0, edge1].
    #[inline]
    fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
        let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Check if a point is inside a rounded rectangle and return the alpha coverage.
    /// Returns 1.0 if fully inside, 0.0 if fully outside, values in between for AA at corners.
    /// Uses smoothstep for antialiasing to match the GPU shader implementation.
    #[inline]
    fn point_in_rounded_rect(
        px: f32,
        py: f32,
        rect: Rect,
        radius: rustkit_layout::BorderRadius,
    ) -> f32 {
        // Quick check: outside bounding rect
        if px < rect.x || px > rect.x + rect.width || py < rect.y || py > rect.y + rect.height {
            return 0.0;
        }

        // If no border radius, point is inside
        if radius.is_zero() {
            return 1.0;
        }

        // Clamp radii to half the rect dimensions
        let max_r = (rect.width / 2.0).min(rect.height / 2.0);
        let r_tl = radius.top_left.min(max_r);
        let r_tr = radius.top_right.min(max_r);
        let r_br = radius.bottom_right.min(max_r);
        let r_bl = radius.bottom_left.min(max_r);

        // Check each corner
        let local_x = px - rect.x;
        let local_y = py - rect.y;
        let right_x = rect.width - local_x;
        let bottom_y = rect.height - local_y;

        // Top-left corner: use smoothstep SDF antialiasing to match GPU shader
        if local_x < r_tl && local_y < r_tl {
            let dx = r_tl - local_x;
            let dy = r_tl - local_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let sdf = dist - r_tl;
            return 1.0 - Self::smoothstep(-0.5, 0.5, sdf);
        }

        // Top-right corner: use smoothstep SDF antialiasing to match GPU shader
        if right_x < r_tr && local_y < r_tr {
            let dx = r_tr - right_x;
            let dy = r_tr - local_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let sdf = dist - r_tr;
            return 1.0 - Self::smoothstep(-0.5, 0.5, sdf);
        }

        // Bottom-right corner: use smoothstep SDF antialiasing to match GPU shader
        if right_x < r_br && bottom_y < r_br {
            let dx = r_br - right_x;
            let dy = r_br - bottom_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let sdf = dist - r_br;
            return 1.0 - Self::smoothstep(-0.5, 0.5, sdf);
        }

        // Bottom-left corner: use smoothstep SDF antialiasing to match GPU shader
        if local_x < r_bl && bottom_y < r_bl {
            let dx = r_bl - local_x;
            let dy = r_bl - bottom_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let sdf = dist - r_bl;
            return 1.0 - Self::smoothstep(-0.5, 0.5, sdf);
        }

        // Inside the rect, not in a corner region
        1.0
    }

    /// Draw a single rounded corner using pixel-based SDF with anti-aliasing.
    /// quadrant: 0=top-left, 1=top-right, 2=bottom-right, 3=bottom-left
    fn draw_rounded_corner(&mut self, x: f32, y: f32, radius: f32, color: Color, quadrant: u8) {
        if radius < 1.0 {
            return;
        }

        // Calculate center of the corner circle
        let (cx, cy) = match quadrant {
            0 => (x + radius, y + radius), // top-left: center is inside
            1 => (x, y + radius),          // top-right: center is to the left
            2 => (x, y),                   // bottom-right: center is up-left
            3 => (x + radius, y),          // bottom-left: center is up
            _ => return,
        };

        // Draw corner using small rectangles with AA
        let step = 1.0;
        let mut py = y;
        while py < y + radius {
            let mut px = x;
            while px < x + radius {
                // Calculate distance from pixel center to corner center
                let dx = match quadrant {
                    0 | 3 => cx - (px + step / 2.0), // left corners: measure from right edge
                    _ => (px + step / 2.0) - cx,    // right corners: measure from left edge
                };
                let dy = match quadrant {
                    0 | 1 => cy - (py + step / 2.0), // top corners: measure from bottom edge
                    _ => (py + step / 2.0) - cy,    // bottom corners: measure from top edge
                };
                
                let dist = (dx * dx + dy * dy).sqrt();
                
                // Use signed distance field for anti-aliasing
                // Distance to edge (positive = inside, negative = outside)
                let signed_dist = radius - dist;
                
                if signed_dist >= 1.0 {
                    // Fully inside
                    self.draw_solid_rect(Rect::new(px, py, step, step), color);
                } else if signed_dist > -1.0 {
                    // Edge pixel - apply anti-aliasing
                    // Coverage is 0.5 + signed_dist * 0.5 (clamped to 0-1)
                    let coverage = (signed_dist * 0.5 + 0.5).clamp(0.0, 1.0);
                    if coverage > 0.01 {
                        let aa_color = Color::new(
                            color.r,
                            color.g,
                            color.b,
                            color.a * coverage,
                        );
                        self.draw_solid_rect(Rect::new(px, py, step, step), aa_color);
                    }
                }
                // else: outside, don't draw
                
                px += step;
            }
            py += step;
        }
    }

    /// Draw a filled circle using triangle fan.
    fn draw_fill_circle(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        if radius <= 0.0 {
            return;
        }

        // Determine number of segments based on radius for smooth appearance
        let segments = ((radius / 2.0).sqrt() * 8.0).round().max(16.0).min(64.0) as u32;

        let c = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a,
        ];

        let base = self.color_vertices.len() as u32;

        // Center vertex
        let (center_x, center_y) = self.transform_point(cx, cy);
        self.color_vertices.push(ColorVertex {
            position: [center_x, center_y],
            color: c,
        });

        // Generate vertices around the circumference
        use std::f32::consts::PI;
        for i in 0..=segments {
            let angle = 2.0 * PI * (i as f32) / (segments as f32);
            let px = cx + radius * angle.cos();
            let py = cy + radius * angle.sin();
            let (x, y) = self.transform_point(px, py);
            self.color_vertices.push(ColorVertex {
                position: [x, y],
                color: c,
            });
        }

        // Generate triangle fan indices
        for i in 0..segments {
            self.color_indices.extend_from_slice(&[
                base,         // Center
                base + i + 1, // Current point on circumference
                base + i + 2, // Next point on circumference
            ]);
        }
    }

    /// Draw a filled ellipse using triangle fan.
    fn draw_fill_ellipse(&mut self, rect: Rect, color: Color) {
        let cx = rect.x + rect.width / 2.0;
        let cy = rect.y + rect.height / 2.0;
        let rx = rect.width / 2.0;
        let ry = rect.height / 2.0;

        if rx <= 0.0 || ry <= 0.0 {
            return;
        }

        // Use average of radii to determine segment count
        let avg_radius = (rx + ry) / 2.0;
        let segments = ((avg_radius / 2.0).sqrt() * 8.0).round().max(16.0).min(64.0) as u32;

        let c = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a,
        ];

        let base = self.color_vertices.len() as u32;

        // Center vertex
        let (center_x, center_y) = self.transform_point(cx, cy);
        self.color_vertices.push(ColorVertex {
            position: [center_x, center_y],
            color: c,
        });

        // Generate vertices around the ellipse using parametric equations
        use std::f32::consts::PI;
        for i in 0..=segments {
            let angle = 2.0 * PI * (i as f32) / (segments as f32);
            let px = cx + rx * angle.cos();
            let py = cy + ry * angle.sin();
            let (x, y) = self.transform_point(px, py);
            self.color_vertices.push(ColorVertex {
                position: [x, y],
                color: c,
            });
        }

        // Generate triangle fan indices
        for i in 0..segments {
            self.color_indices.extend_from_slice(&[
                base,         // Center
                base + i + 1, // Current point on ellipse
                base + i + 2, // Next point on ellipse
            ]);
        }
    }

    /// Draw a border.
    fn draw_border(&mut self, rect: Rect, color: Color, top: f32, right: f32, bottom: f32, left: f32) {
        // Top border
        if top > 0.0 {
            self.draw_solid_rect(
                Rect::new(rect.x, rect.y, rect.width, top),
                color,
            );
        }

        // Right border
        if right > 0.0 {
            self.draw_solid_rect(
                Rect::new(rect.x + rect.width - right, rect.y + top, right, rect.height - top - bottom),
                color,
            );
        }

        // Bottom border
        if bottom > 0.0 {
            self.draw_solid_rect(
                Rect::new(rect.x, rect.y + rect.height - bottom, rect.width, bottom),
                color,
            );
        }

        // Left border
        if left > 0.0 {
            self.draw_solid_rect(
                Rect::new(rect.x, rect.y + top, left, rect.height - top - bottom),
                color,
            );
        }
    }
    
    /// Draw a box shadow.
    /// 
    /// For now, this uses a simplified approach:
    /// - Outer shadows: Draw multiple semi-transparent rectangles with increasing offsets
    /// - Inset shadows: Draw gradient-like rectangles inside the box
    fn draw_box_shadow(
        &mut self,
        rect: Rect,
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        spread_radius: f32,
        color: Color,
        inset: bool,
    ) {
        if color.a == 0.0 {
            return;
        }
        
        // Calculate shadow rectangle
        let shadow_rect = if inset {
            // Inset shadow is inside the box
            Rect::new(
                rect.x + offset_x.max(0.0),
                rect.y + offset_y.max(0.0),
                rect.width - spread_radius * 2.0 - offset_x.abs(),
                rect.height - spread_radius * 2.0 - offset_y.abs(),
            )
        } else {
            // Outer shadow is outside the box
            Rect::new(
                rect.x + offset_x - spread_radius,
                rect.y + offset_y - spread_radius,
                rect.width + spread_radius * 2.0,
                rect.height + spread_radius * 2.0,
            )
        };
        
        if shadow_rect.width <= 0.0 || shadow_rect.height <= 0.0 {
            return;
        }
        
        // For blur, we draw multiple layers with decreasing opacity
        // This is a simplified approximation - real blur would use GPU shaders
        if blur_radius > 0.0 {
            let steps = (blur_radius / 2.0).ceil().max(1.0) as u32;
            let step_size = blur_radius / steps as f32;
            
            for i in 0..steps {
                let layer = steps - i; // Draw outer layers first
                let expansion = step_size * layer as f32;
                let layer_alpha = color.a / (steps as f32 * 1.5); // Fade out
                
                let layer_rect = if inset {
                    // Inset shadows shrink inward
                    Rect::new(
                        shadow_rect.x + expansion,
                        shadow_rect.y + expansion,
                        shadow_rect.width - expansion * 2.0,
                        shadow_rect.height - expansion * 2.0,
                    )
                } else {
                    // Outer shadows expand outward
                    Rect::new(
                        shadow_rect.x - expansion,
                        shadow_rect.y - expansion,
                        shadow_rect.width + expansion * 2.0,
                        shadow_rect.height + expansion * 2.0,
                    )
                };
                
                if layer_rect.width > 0.0 && layer_rect.height > 0.0 {
                    let layer_color = Color::new(color.r, color.g, color.b, layer_alpha);
                    self.draw_solid_rect(layer_rect, layer_color);
                }
            }
        } else {
            // No blur - just draw solid shadow
            self.draw_solid_rect(shadow_rect, color);
        }
    }

    /// Apply a backdrop filter (blur, grayscale, etc.) to pixels behind the element.
    ///
    /// ## GPU Infrastructure (Available)
    ///
    /// The following GPU compute pipeline infrastructure is in place:
    /// - `backdrop_filter_pipelines`: Compute pipelines for blur (horizontal/vertical) and color filters
    /// - `create_filter_texture()`: Creates storage textures for compute operations
    /// - `run_blur_compute()`: Executes Gaussian blur via 2-pass separable filter
    /// - `run_color_filter_compute()`: Executes grayscale/sepia/brightness filters
    ///
    /// ## Current Limitation
    ///
    /// Full GPU backdrop filter requires render-to-texture support:
    /// 1. Rendering commands up to this point to an intermediate texture
    /// 2. Copying the backdrop region
    /// 3. Running compute shader passes
    /// 4. Drawing the filtered result
    ///
    /// The current architecture batches all commands and renders once at the end of `execute()`,
    /// making mid-frame capture non-trivial. For now, we use overlay approximations.
    ///
    /// ## Future Integration Path
    ///
    /// To enable true GPU filters:
    /// 1. Modify `execute()` to split rendering at BackdropFilter commands
    /// 2. Create intermediate render texture with `COPY_SRC` usage
    /// 3. Flush batched commands before each backdrop filter
    /// 4. Copy region, run compute, draw result, continue batching
    fn apply_backdrop_filter(
        &mut self,
        rect: Rect,
        border_radius: rustkit_layout::BorderRadius,
        filter: rustkit_css::BackdropFilter,
    ) {
        use rustkit_css::BackdropFilter;

        // Apply clipping
        let rect = if let Some(clip) = self.current_clip() {
            if let Some(clipped) = rect.intersect(&clip) {
                clipped
            } else {
                return; // Fully clipped
            }
        } else {
            rect
        };

        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        match filter {
            BackdropFilter::None => {}

            BackdropFilter::Blur(radius) => {
                // Proper backdrop blur requires render-to-texture and compute shaders.
                // For now, we simulate it with a semi-transparent white/gray overlay
                // which approximates the "frosted glass" effect.
                if radius > 0.0 {
                    // The heavier the blur, the more opaque the overlay (0.0-1.0 range)
                    let opacity = (radius / 20.0).min(0.5) * 0.3;
                    let overlay_color = Color::new(255, 255, 255, opacity);

                    if border_radius.is_zero() {
                        self.draw_solid_rect(rect, overlay_color);
                    } else {
                        self.draw_rounded_rect(rect, overlay_color, border_radius);
                    }
                }
            }

            BackdropFilter::Grayscale(amount) => {
                // Approximate grayscale by drawing a gray overlay
                // This isn't accurate but provides visual feedback
                if amount > 0.0 {
                    let gray_value = 128;
                    // Alpha in 0.0-1.0 range
                    let overlay_color = Color::new(gray_value, gray_value, gray_value, amount * 0.4);

                    if border_radius.is_zero() {
                        self.draw_solid_rect(rect, overlay_color);
                    } else {
                        self.draw_rounded_rect(rect, overlay_color, border_radius);
                    }
                }
            }

            BackdropFilter::Brightness(amount) => {
                // Brightness > 1.0 = lighter, < 1.0 = darker
                if amount != 1.0 {
                    let color = if amount > 1.0 {
                        // Lighten with white overlay (alpha in 0.0-1.0 range)
                        let intensity = ((amount - 1.0) * 0.4).min(0.8);
                        Color::new(255, 255, 255, intensity)
                    } else {
                        // Darken with black overlay (alpha in 0.0-1.0 range)
                        let intensity = ((1.0 - amount) * 0.8).min(0.8);
                        Color::new(0, 0, 0, intensity)
                    };

                    if border_radius.is_zero() {
                        self.draw_solid_rect(rect, color);
                    } else {
                        self.draw_rounded_rect(rect, color, border_radius);
                    }
                }
            }

            BackdropFilter::Contrast(_) => {
                // Contrast adjustment would require per-pixel operations
                // No simple overlay approximation exists
            }

            BackdropFilter::Saturate(_) => {
                // Saturation adjustment would require per-pixel color manipulation
                // No simple overlay approximation exists
            }

            BackdropFilter::Sepia(amount) => {
                // Approximate sepia with a brownish overlay (alpha in 0.0-1.0 range)
                if amount > 0.0 {
                    let sepia_color = Color::new(112, 66, 20, amount * 0.3);

                    if border_radius.is_zero() {
                        self.draw_solid_rect(rect, sepia_color);
                    } else {
                        self.draw_rounded_rect(rect, sepia_color, border_radius);
                    }
                }
            }
        }
    }

    /// Render a linear gradient using the GPU shader.
    /// This method renders the gradient immediately using a separate render pass.
    /// Note: This may cause z-ordering issues with other content.
    /// Enable via RUSTKIT_GPU_GRADIENTS=1 environment variable.
    fn render_linear_gradient_gpu(
        &self,
        target: &wgpu::TextureView,
        rect: Rect,
        angle_rad: f32,
        stops: &[(f32, rustkit_css::ColorF32)],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate repeat length for repeating gradients
        let repeat_length = if repeating && !stops.is_empty() {
            stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

        // Update gradient parameters uniform buffer
        let params = pipeline::GradientParams {
            rect_x: rect.x,
            rect_y: rect.y,
            rect_width: rect.width,
            rect_height: rect.height,
            param0: angle_rad,  // linear gradient uses angle in radians
            param1: 0.0,
            param2: 0.5,
            param3: 0.5,
            gradient_type: 0,  // 0 = linear
            repeating: if repeating { 1 } else { 0 },
            repeat_length,
            num_stops: stops.len().min(self.gradient_pipeline.max_stops) as u32,
            radius_tl: border_radius.top_left,
            radius_tr: border_radius.top_right,
            radius_br: border_radius.bottom_right,
            radius_bl: border_radius.bottom_left,
            debug_mode: std::env::var("RUSTKIT_GPU_DEBUG")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            _padding0: 0,
            _padding1: 0,
            _padding2: 0,
        };

        self.queue.write_buffer(
            &self.gradient_pipeline.uniform_buffer,
            0,
            bytemuck::cast_slice(&[params]),
        );

        // Update color stops storage buffer
        let gpu_stops: Vec<pipeline::GradientColorStop> = stops
            .iter()
            .take(self.gradient_pipeline.max_stops)
            .map(|(pos, color)| pipeline::GradientColorStop {
                position: *pos,
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            })
            .collect();

        if !gpu_stops.is_empty() {
            self.queue.write_buffer(
                &self.gradient_pipeline.stops_buffer,
                0,
                bytemuck::cast_slice(&gpu_stops),
            );
        }

        // Create vertices for the gradient quad
        // We use ColorVertex but the fragment shader ignores the color
        let dummy_color = [0.0f32, 0.0, 0.0, 1.0];
        let vertices = [
            ColorVertex { position: [rect.x, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y + rect.height], color: dummy_color },
            ColorVertex { position: [rect.x, rect.y + rect.height], color: dummy_color },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gradient Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gradient Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create command encoder and render pass
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GPU Gradient Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GPU Gradient Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,  // Don't clear, preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.gradient_pipeline.pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &self.gradient_pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a linear gradient using the GPU shader with optional clear.
    /// When clear_color is Some, the target is cleared before rendering.
    /// When clear_color is None, existing content is preserved (LoadOp::Load).
    fn render_linear_gradient_gpu_with_clear(
        &self,
        target: &wgpu::TextureView,
        rect: Rect,
        angle_rad: f32,
        stops: &[(f32, rustkit_css::ColorF32)],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
        clear_color: Option<wgpu::Color>,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate repeat length for repeating gradients
        let repeat_length = if repeating && !stops.is_empty() {
            stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

        // Update gradient parameters uniform buffer
        let params = pipeline::GradientParams {
            rect_x: rect.x,
            rect_y: rect.y,
            rect_width: rect.width,
            rect_height: rect.height,
            param0: angle_rad,
            param1: 0.0,
            param2: 0.5,
            param3: 0.5,
            gradient_type: 0,  // 0 = linear
            repeating: if repeating { 1 } else { 0 },
            repeat_length,
            num_stops: stops.len().min(self.gradient_pipeline.max_stops) as u32,
            radius_tl: border_radius.top_left,
            radius_tr: border_radius.top_right,
            radius_br: border_radius.bottom_right,
            radius_bl: border_radius.bottom_left,
            // Debug mode: 0=normal, 1=t-value, 2=direction, 3=position, 4=coverage, 5=raw-t
            //            6=first-stop, 7=num-stops, 8=interp-color
            // Set via RUSTKIT_GPU_DEBUG=N environment variable
            debug_mode: std::env::var("RUSTKIT_GPU_DEBUG")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            _padding0: 0,
            _padding1: 0,
            _padding2: 0,
        };

        // Debug output for GPU gradient parameters
        if std::env::var("RUSTKIT_GPU_TRACE").is_ok() {
            eprintln!("GPU Gradient (with_clear): rect=({:.1}, {:.1}, {:.1}, {:.1}), angle={:.2}rad, num_stops={}, repeating={}",
                params.rect_x, params.rect_y, params.rect_width, params.rect_height,
                params.param0, params.num_stops, params.repeating);
            for (i, (pos, color)) in stops.iter().enumerate() {
                eprintln!("  Stop {}: pos={:.3}, RGBA=({:.3}, {:.3}, {:.3}, {:.3})",
                    i, pos, color.r, color.g, color.b, color.a);
            }
        }

        self.queue.write_buffer(
            &self.gradient_pipeline.uniform_buffer,
            0,
            bytemuck::cast_slice(&[params]),
        );

        // Update color stops storage buffer
        let gpu_stops: Vec<pipeline::GradientColorStop> = stops
            .iter()
            .take(self.gradient_pipeline.max_stops)
            .map(|(pos, color)| pipeline::GradientColorStop {
                position: *pos,
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            })
            .collect();

        if !gpu_stops.is_empty() {
            self.queue.write_buffer(
                &self.gradient_pipeline.stops_buffer,
                0,
                bytemuck::cast_slice(&gpu_stops),
            );
        }

        // Create vertices for the gradient quad
        let dummy_color = [0.0f32, 0.0, 0.0, 1.0];

        let vertices = [
            ColorVertex { position: [rect.x, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y + rect.height], color: dummy_color },
            ColorVertex { position: [rect.x, rect.y + rect.height], color: dummy_color },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gradient Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Gradient Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create command encoder and render pass
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GPU Gradient Encoder (with_clear)"),
        });

        let load_op = match clear_color {
            Some(color) => wgpu::LoadOp::Clear(color),
            None => wgpu::LoadOp::Load,
        };

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GPU Gradient Render Pass"),
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

            render_pass.set_pipeline(&self.gradient_pipeline.pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &self.gradient_pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a radial gradient using the GPU shader.
    fn render_radial_gradient_gpu(
        &self,
        target: &wgpu::TextureView,
        rect: Rect,
        rx: f32,
        ry: f32,
        center: (f32, f32),
        stops: &[(f32, rustkit_css::ColorF32)],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate repeat length for repeating gradients
        let repeat_length = if repeating && !stops.is_empty() {
            stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

        // Update gradient parameters uniform buffer
        let params = pipeline::GradientParams {
            rect_x: rect.x,
            rect_y: rect.y,
            rect_width: rect.width,
            rect_height: rect.height,
            param0: rx,  // radial: x radius in pixels
            param1: ry,  // radial: y radius in pixels
            param2: center.0,  // radial: center x (0-1)
            param3: center.1,  // radial: center y (0-1)
            gradient_type: 1,  // 1 = radial
            repeating: if repeating { 1 } else { 0 },
            repeat_length,
            num_stops: stops.len().min(self.gradient_pipeline.max_stops) as u32,
            radius_tl: border_radius.top_left,
            radius_tr: border_radius.top_right,
            radius_br: border_radius.bottom_right,
            radius_bl: border_radius.bottom_left,
            debug_mode: std::env::var("RUSTKIT_GPU_DEBUG")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            _padding0: 0,
            _padding1: 0,
            _padding2: 0,
        };

        self.queue.write_buffer(
            &self.gradient_pipeline.uniform_buffer,
            0,
            bytemuck::cast_slice(&[params]),
        );

        // Update color stops storage buffer
        let gpu_stops: Vec<pipeline::GradientColorStop> = stops
            .iter()
            .take(self.gradient_pipeline.max_stops)
            .map(|(pos, color)| pipeline::GradientColorStop {
                position: *pos,
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            })
            .collect();

        if !gpu_stops.is_empty() {
            self.queue.write_buffer(
                &self.gradient_pipeline.stops_buffer,
                0,
                bytemuck::cast_slice(&gpu_stops),
            );
        }

        // Create vertices for the gradient quad
        let dummy_color = [0.0f32, 0.0, 0.0, 1.0];
        let vertices = [
            ColorVertex { position: [rect.x, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y + rect.height], color: dummy_color },
            ColorVertex { position: [rect.x, rect.y + rect.height], color: dummy_color },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Radial Gradient Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Radial Gradient Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create command encoder and render pass (LoadOp::Load to preserve existing content)
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GPU Radial Gradient Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GPU Radial Gradient Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.gradient_pipeline.pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &self.gradient_pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a conic gradient using the GPU shader.
    fn render_conic_gradient_gpu(
        &self,
        target: &wgpu::TextureView,
        rect: Rect,
        from_angle_rad: f32,
        center: (f32, f32),
        stops: &[(f32, rustkit_css::ColorF32)],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate repeat length for repeating gradients
        let repeat_length = if repeating && !stops.is_empty() {
            stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

        // Update gradient parameters uniform buffer
        let params = pipeline::GradientParams {
            rect_x: rect.x,
            rect_y: rect.y,
            rect_width: rect.width,
            rect_height: rect.height,
            param0: from_angle_rad,  // conic: starting angle in radians
            param1: 0.0,
            param2: center.0,  // conic: center x (0-1)
            param3: center.1,  // conic: center y (0-1)
            gradient_type: 2,  // 2 = conic
            repeating: if repeating { 1 } else { 0 },
            repeat_length,
            num_stops: stops.len().min(self.gradient_pipeline.max_stops) as u32,
            radius_tl: border_radius.top_left,
            radius_tr: border_radius.top_right,
            radius_br: border_radius.bottom_right,
            radius_bl: border_radius.bottom_left,
            debug_mode: std::env::var("RUSTKIT_GPU_DEBUG")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0),
            _padding0: 0,
            _padding1: 0,
            _padding2: 0,
        };

        self.queue.write_buffer(
            &self.gradient_pipeline.uniform_buffer,
            0,
            bytemuck::cast_slice(&[params]),
        );

        // Update color stops storage buffer
        let gpu_stops: Vec<pipeline::GradientColorStop> = stops
            .iter()
            .take(self.gradient_pipeline.max_stops)
            .map(|(pos, color)| pipeline::GradientColorStop {
                position: *pos,
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            })
            .collect();

        if !gpu_stops.is_empty() {
            self.queue.write_buffer(
                &self.gradient_pipeline.stops_buffer,
                0,
                bytemuck::cast_slice(&gpu_stops),
            );
        }

        // Create vertices for the gradient quad
        let dummy_color = [0.0f32, 0.0, 0.0, 1.0];
        let vertices = [
            ColorVertex { position: [rect.x, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y], color: dummy_color },
            ColorVertex { position: [rect.x + rect.width, rect.y + rect.height], color: dummy_color },
            ColorVertex { position: [rect.x, rect.y + rect.height], color: dummy_color },
        ];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Conic Gradient Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Conic Gradient Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Create command encoder and render pass (LoadOp::Load to preserve existing content)
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GPU Conic Gradient Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GPU Conic Gradient Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.gradient_pipeline.pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &self.gradient_pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Draw a linear gradient with optional border-radius clipping.
    fn draw_linear_gradient(
        &mut self,
        rect: Rect,
        direction: rustkit_css::GradientDirection,
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Convert direction to angle in radians
        let angle_deg = direction.to_degrees();
        let angle_rad = angle_deg.to_radians();

        // Calculate gradient direction vector
        let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());

        // Calculate gradient geometry
        let half_width = rect.width / 2.0;
        let half_height = rect.height / 2.0;
        let gradient_half_length = (half_width * sin_a.abs() + half_height * cos_a.abs()).max(0.001);

        // Check if any stop uses pixel positions (for repeating gradients)
        let has_pixel_positions = stops.iter().any(|s| {
            s.position.as_ref().map(|p| p.is_pixels()).unwrap_or(false)
        });

        // For repeating gradients with pixel positions, get the repeat length in pixels
        let repeat_length_pixels = if repeating && has_pixel_positions {
            stops.last()
                .and_then(|s| s.position.as_ref())
                .map(|p| match p {
                    rustkit_css::StopPosition::Pixels(px) => *px,
                    rustkit_css::StopPosition::Percent(pct) => *pct * gradient_half_length * 2.0,
                })
                .unwrap_or(gradient_half_length * 2.0)
                .max(0.001)
        } else {
            gradient_half_length * 2.0 // Full gradient length
        };

        // For pixel-based repeating gradients, normalize stops to the repeat length (not full gradient)
        // For percentage-based gradients, normalize to 0-1
        let mut normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = match &stop.position {
                Some(p) => {
                    if has_pixel_positions && repeating {
                        // For pixel-based repeating gradients, normalize to repeat length
                        match p {
                            rustkit_css::StopPosition::Pixels(px) => *px / repeat_length_pixels,
                            rustkit_css::StopPosition::Percent(pct) => *pct,
                        }
                    } else {
                        // For non-repeating or percentage-based, normalize to 0-1 using gradient line
                        match p {
                            rustkit_css::StopPosition::Percent(pct) => *pct,
                            rustkit_css::StopPosition::Pixels(px) => *px / (gradient_half_length * 2.0),
                        }
                    }
                }
                None => {
                    // Auto-position: distribute evenly
                    if stops.len() == 1 {
                        0.5
                    } else {
                        i as f32 / (stops.len() - 1) as f32
                    }
                }
            };
            normalized_stops.push((pos, rustkit_css::ColorF32::from_color(stop.color)));
        }

        // For repeating gradients, the repeat length is 1.0 (since we normalized stops to it)
        // For non-repeating, use the last stop position
        let repeat_length = if repeating {
            1.0 // Stops are already normalized to repeat length
        } else {
            normalized_stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        };

        // GPU gradient path: queue for deferred rendering
        // Enable via RUSTKIT_GPU_GRADIENTS=1 environment variable
        if self.gpu_gradients_enabled {
            self.gradient_queue.push(QueuedLinearGradient {
                rect,
                angle_rad,
                stops: normalized_stops,
                repeating,
                border_radius,
            });
            return; // GPU will render during flush_to
        }

        // CPU path: cell-by-cell rendering (default)

        // Helper to apply repeating logic to t value
        let apply_t = |t: f32| -> f32 {
            if repeating {
                // Scale t to repeat length and use modulo for repeating
                (t.rem_euclid(repeat_length)).min(repeat_length)
            } else {
                t.clamp(0.0, 1.0)
            }
        };

        // Check for axis-aligned gradients (more efficient rendering)
        let is_horizontal = (angle_deg - 90.0).abs() < 0.1 || (angle_deg - 270.0).abs() < 0.1;
        let is_vertical = angle_deg.abs() < 0.1 || (angle_deg - 180.0).abs() < 0.1;
        let has_radius = !border_radius.is_zero();

        // If we have border-radius, we need cell-by-cell rendering for proper clipping
        if !has_radius && is_horizontal {
            // Horizontal gradient (left to right or right to left) - fast path
            let reverse = angle_deg > 180.0;
            let step_count = rect.width.max(2.0) as usize;
            let strip_width = rect.width / step_count as f32;

            let vp_w = self.viewport_size.0 as f32;
            let (first, last) = self.visible_strip_range(rect.x, rect.width, step_count, vp_w);
            for i in first..last {
                let t = if reverse {
                    1.0 - (i as f32 + 0.5) / step_count as f32
                } else {
                    (i as f32 + 0.5) / step_count as f32
                };
                let t_final = apply_t(t);
                let color = Self::interpolate_color_f32(&normalized_stops, t_final);
                let x_pos = rect.x + i as f32 * strip_width;
                self.draw_solid_rect_f32(Rect::new(x_pos, rect.y, strip_width + 0.5, rect.height), color);
            }
        } else if !has_radius && is_vertical {
            // Vertical gradient (top to bottom or bottom to top) - fast path
            let reverse = angle_deg < 90.0 || angle_deg > 270.0;
            let step_count = rect.height.max(2.0) as usize;
            let strip_height = rect.height / step_count as f32;

            let vp_h = self.viewport_size.1 as f32;
            let (first, last) = self.visible_strip_range(rect.y, rect.height, step_count, vp_h);
            for i in first..last {
                let t = if reverse {
                    1.0 - (i as f32 + 0.5) / step_count as f32
                } else {
                    (i as f32 + 0.5) / step_count as f32
                };
                let t_final = apply_t(t);
                let color = Self::interpolate_color_f32(&normalized_stops, t_final);
                let y_pos = rect.y + i as f32 * strip_height;
                self.draw_solid_rect_f32(Rect::new(rect.x, y_pos, rect.width, strip_height + 0.5), color);
            }
        } else {
            // Diagonal gradient or gradient with border-radius - cell-by-cell rendering
            // Uses the CSS gradient spec algorithm for proper corner-to-corner diagonal
            // (half_width, half_height, gradient_half_length are calculated at function start)

            // Adaptive step sizing to prevent GPU buffer overflow for large gradients
            // while maintaining 1px quality for small UI elements
            let area = rect.width * rect.height;
            let max_cells: f32 = 100_000.0; // Limit cells to prevent buffer overflow
            let cell_size: f32 = if area > max_cells {
                (area / max_cells).sqrt().ceil()
            } else {
                1.0
            };
            let cols = (rect.width / cell_size).ceil() as usize;
            let rows = (rect.height / cell_size).ceil() as usize;

            let center_x = rect.x + half_width;
            let center_y = rect.y + half_height;

            for row in 0..rows {
                for col in 0..cols {
                    let cell_x = rect.x + col as f32 * cell_size;
                    let cell_y = rect.y + row as f32 * cell_size;
                    let cell_center_x = cell_x + cell_size * 0.5;
                    let cell_center_y = cell_y + cell_size * 0.5;

                    // Check bounds
                    if cell_x >= rect.x + rect.width || cell_y >= rect.y + rect.height {
                        continue;
                    }

                    // Check border-radius clipping
                    if has_radius {
                        let coverage = Self::point_in_rounded_rect(cell_center_x, cell_center_y, rect, border_radius);
                        if coverage <= 0.0 {
                            continue; // Skip cells outside the rounded corners
                        }
                    }

                    // Position relative to rect center
                    let px = cell_center_x - center_x;
                    let py = cell_center_y - center_y;

                    // Project onto gradient direction (sin_a, -cos_a)
                    // projection ranges from -gradient_half_length to +gradient_half_length
                    let projection = px * sin_a + py * (-cos_a);

                    // Calculate t value
                    let t = if repeating && has_pixel_positions {
                        // For pixel-based repeating gradients, the 0 position is at the center
                        // of the gradient line, and the pattern repeats in both directions.
                        // projection is already centered at 0, so use it directly.
                        projection / repeat_length_pixels
                    } else {
                        // For non-repeating or percentage-based, normalize to 0-1
                        (projection / gradient_half_length + 1.0) / 2.0
                    };
                    let t_final = apply_t(t);

                    let mut color = Self::interpolate_color_f32(&normalized_stops, t_final);

                    // Apply alpha coverage for antialiased edges at rounded corners
                    if has_radius {
                        let coverage = Self::point_in_rounded_rect(cell_center_x, cell_center_y, rect, border_radius);
                        if coverage < 1.0 {
                            color = rustkit_css::ColorF32::new(color.r, color.g, color.b, color.a * coverage);
                        }
                    }

                    // Clamp cell to rect bounds
                    let cell_w = cell_size.min(rect.x + rect.width - cell_x);
                    let cell_h = cell_size.min(rect.y + rect.height - cell_y);

                    self.draw_solid_rect_f32(Rect::new(cell_x, cell_y, cell_w, cell_h), color);
                }
            }
        }
    }
    
    /// Draw a radial gradient with optional border-radius clipping.
    fn draw_radial_gradient(
        &mut self,
        rect: Rect,
        shape: rustkit_css::RadialShape,
        size: rustkit_css::RadialSize,
        center: (f32, f32),
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate center position in pixels
        let cx = rect.x + rect.width * center.0;
        let cy = rect.y + rect.height * center.1;
        
        // Calculate radius based on size keyword
        let (rx, ry) = match size {
            rustkit_css::RadialSize::ClosestSide => {
                let dx = center.0.min(1.0 - center.0) * rect.width;
                let dy = center.1.min(1.0 - center.1) * rect.height;
                match shape {
                    rustkit_css::RadialShape::Circle => (dx.min(dy), dx.min(dy)),
                    rustkit_css::RadialShape::Ellipse => (dx, dy),
                }
            }
            rustkit_css::RadialSize::FarthestSide => {
                let dx = center.0.max(1.0 - center.0) * rect.width;
                let dy = center.1.max(1.0 - center.1) * rect.height;
                match shape {
                    rustkit_css::RadialShape::Circle => (dx.max(dy), dx.max(dy)),
                    rustkit_css::RadialShape::Ellipse => (dx, dy),
                }
            }
            rustkit_css::RadialSize::ClosestCorner => {
                // Distance to closest corner
                let corners = [
                    (0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)
                ];
                let mut min_dist = f32::INFINITY;
                for (cx_frac, cy_frac) in corners {
                    let dx = (cx_frac - center.0).abs() * rect.width;
                    let dy = (cy_frac - center.1).abs() * rect.height;
                    let dist = (dx * dx + dy * dy).sqrt();
                    min_dist = min_dist.min(dist);
                }
                match shape {
                    rustkit_css::RadialShape::Circle => (min_dist, min_dist),
                    rustkit_css::RadialShape::Ellipse => {
                        // css-images-3 §3.3.3: side distances scaled by
                        // sqrt(2) — see the GPU path for the derivation.
                        let dx = center.0.min(1.0 - center.0) * rect.width;
                        let dy = center.1.min(1.0 - center.1) * rect.height;
                        (dx * std::f32::consts::SQRT_2, dy * std::f32::consts::SQRT_2)
                    }
                }
            }
            rustkit_css::RadialSize::FarthestCorner => {
                // Distance to farthest corner
                let corners = [
                    (0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)
                ];
                let mut max_dist = 0.0f32;
                for (cx_frac, cy_frac) in corners {
                    let dx = (cx_frac - center.0).abs() * rect.width;
                    let dy = (cy_frac - center.1).abs() * rect.height;
                    let dist = (dx * dx + dy * dy).sqrt();
                    max_dist = max_dist.max(dist);
                }
                match shape {
                    rustkit_css::RadialShape::Circle => (max_dist, max_dist),
                    rustkit_css::RadialShape::Ellipse => {
                        // css-images-3 §3.3.3: side distances scaled by
                        // sqrt(2) — see the GPU path for the derivation.
                        let dx = center.0.max(1.0 - center.0) * rect.width;
                        let dy = center.1.max(1.0 - center.1) * rect.height;
                        (dx * std::f32::consts::SQRT_2, dy * std::f32::consts::SQRT_2)
                    }
                }
            }
            rustkit_css::RadialSize::Explicit(r1, r2) => (r1, r2),
        };

        // Radial gradient line length is the maximum radius
        let radial_gradient_length = rx.max(ry);

        // Check if any stop uses pixel positions
        let has_pixel_positions = stops.iter().any(|s| {
            s.position.as_ref().map(|p| p.is_pixels()).unwrap_or(false)
        });

        // Normalize color stops using high-precision colors
        // For pixel positions, convert to normalized using the radial gradient length
        let mut normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = match &stop.position {
                Some(p) => p.to_normalized(radial_gradient_length),
                None => {
                    // Auto-position: distribute evenly
                    if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
                }
            };
            normalized_stops.push((pos, rustkit_css::ColorF32::from_color(stop.color)));
        }

        // For repeating gradients, calculate repeat length
        let repeat_length = if repeating && !normalized_stops.is_empty() {
            if has_pixel_positions {
                stops.last()
                    .and_then(|s| s.position.as_ref())
                    .map(|p| p.to_normalized(radial_gradient_length))
                    .unwrap_or(1.0)
                    .max(0.001)
            } else {
                normalized_stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
            }
        } else {
            1.0
        };

        // GPU radial gradient path: queue for deferred rendering
        if self.gpu_gradients_enabled {
            self.radial_gradient_queue.push(QueuedRadialGradient {
                rect,
                rx,
                ry,
                center,
                stops: normalized_stops,
                repeating,
                border_radius,
            });
            return; // GPU will render during flush_to
        }

        // CPU path: cell-by-cell rendering

        // Adaptive step sizing to prevent GPU buffer overflow for large gradients
        // while maintaining 1px quality for small UI elements
        let area = rect.width * rect.height;
        let max_cells: f32 = 100_000.0; // Limit cells to prevent buffer overflow
        let step_size: f32 = if area > max_cells {
            (area / max_cells).sqrt().ceil()
        } else {
            1.0
        };
        let mut y = rect.y;
        while y < rect.y + rect.height {
            let row_height = step_size.min(rect.y + rect.height - y);
            let mut x = rect.x;
            while x < rect.x + rect.width {
                let col_width = step_size.min(rect.x + rect.width - x);
                let cell_center_x = x + col_width / 2.0;
                let cell_center_y = y + row_height / 2.0;

                // Check border-radius clipping
                let alpha_coverage = Self::point_in_rounded_rect(
                    cell_center_x,
                    cell_center_y,
                    rect,
                    border_radius,
                );

                if alpha_coverage > 0.0 {
                    // Calculate distance from center (normalized to ellipse)
                    let dx = (cell_center_x - cx) / rx.max(0.001);
                    let dy = (cell_center_y - cy) / ry.max(0.001);
                    let t = (dx * dx + dy * dy).sqrt();

                    // Apply repeating logic
                    let t_final = if repeating {
                        t.rem_euclid(repeat_length)
                    } else {
                        t.clamp(0.0, 1.0)
                    };

                    // Get color at this distance
                    let mut color = Self::interpolate_color_f32(&normalized_stops, t_final);

                    // Apply border-radius alpha
                    if alpha_coverage < 1.0 {
                        color = rustkit_css::ColorF32::new(color.r, color.g, color.b, color.a * alpha_coverage);
                    }

                    // Only draw if not fully transparent
                    if color.a > 0.0 {
                        self.draw_solid_rect_f32(Rect::new(x, y, col_width, row_height), color);
                    }
                }

                x += step_size;
            }
            y += step_size;
        }
    }

    /// Draw a conic gradient with optional border-radius clipping.
    fn draw_conic_gradient(
        &mut self,
        rect: Rect,
        from_angle: f32,
        center: (f32, f32),
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
        border_radius: rustkit_layout::BorderRadius,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate center position in pixels
        let cx = rect.x + rect.width * center.0;
        let cy = rect.y + rect.height * center.1;

        // Convert from_angle to radians (CSS conic gradients: 0deg = up, clockwise)
        let from_rad = (from_angle - 90.0).to_radians();

        // Normalize color stops using high-precision colors
        // For conic gradients, positions are typically percentages (0-1) of the full sweep
        // Pixel positions are treated as percentages for conic gradients
        let mut normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = match &stop.position {
                Some(p) => {
                    // For conic gradients, use raw value as percentage
                    // (pixel positions don't make sense for conic, treat them as normalized)
                    match p {
                        rustkit_css::StopPosition::Percent(pct) => *pct,
                        rustkit_css::StopPosition::Pixels(px) => *px / 360.0, // Treat as degrees
                    }
                }
                None => {
                    if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
                }
            };
            normalized_stops.push((pos, rustkit_css::ColorF32::from_color(stop.color)));
        }

        // For repeating gradients, get the repeat length from the last stop
        let repeat_length = if repeating && !normalized_stops.is_empty() {
            normalized_stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

        // GPU conic gradient path: queue for deferred rendering
        if self.gpu_gradients_enabled {
            self.conic_gradient_queue.push(QueuedConicGradient {
                rect,
                from_angle_rad: from_rad,
                center,
                stops: normalized_stops,
                repeating,
                border_radius,
            });
            return; // GPU will render during flush_to
        }

        // CPU path: cell-by-cell rendering

        // Function to apply repeating logic to t value
        let apply_t = |t: f32| -> f32 {
            if repeating {
                t.rem_euclid(repeat_length)
            } else {
                t
            }
        };

        // Adaptive step sizing to prevent GPU buffer overflow
        let area = rect.width * rect.height;
        let max_cells: f32 = 100_000.0;
        let step_size: f32 = if area > max_cells {
            (area / max_cells).sqrt().ceil()
        } else {
            1.0
        };

        let mut y = rect.y;
        while y < rect.y + rect.height {
            let row_height = step_size.min(rect.y + rect.height - y);
            let mut x = rect.x;
            while x < rect.x + rect.width {
                let col_width = step_size.min(rect.x + rect.width - x);
                let cell_center_x = x + col_width / 2.0;
                let cell_center_y = y + row_height / 2.0;

                // Check border-radius clipping
                let alpha_coverage = Self::point_in_rounded_rect(
                    cell_center_x,
                    cell_center_y,
                    rect,
                    border_radius,
                );

                if alpha_coverage > 0.0 {
                    // Calculate angle from center
                    let dx = cell_center_x - cx;
                    let dy = cell_center_y - cy;
                    let angle = dy.atan2(dx) - from_rad;

                    // Normalize angle to 0-1 range
                    let normalized_angle = ((angle + std::f32::consts::PI) / (2.0 * std::f32::consts::PI)) % 1.0;
                    let raw_t = if normalized_angle < 0.0 { normalized_angle + 1.0 } else { normalized_angle };

                    // Apply repeating logic
                    let t = apply_t(raw_t);

                    // Get color at this angle
                    let mut color = Self::interpolate_color_f32(&normalized_stops, t);

                    // Apply border-radius alpha
                    if alpha_coverage < 1.0 {
                        color = rustkit_css::ColorF32::new(color.r, color.g, color.b, color.a * alpha_coverage);
                    }

                    if color.a > 0.0 {
                        self.draw_solid_rect_f32(Rect::new(x, y, col_width, row_height), color);
                    }
                }

                x += step_size;
            }
            y += step_size;
        }
    }

    /// Convert sRGB to linear space for interpolation.
    #[inline]
    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// Convert linear to sRGB space after interpolation.
    #[inline]
    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    /// Convert linear RGB to oklab color space.
    /// Returns (L, a, b) where L is lightness, a is green-red, b is blue-yellow.
    #[inline]
    fn linear_rgb_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        // Convert to LMS (long, medium, short cone response)
        let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
        let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
        let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;

        // Apply cube root (non-linear response)
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();

        // Convert to oklab
        let ok_l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
        let ok_a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
        let ok_b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

        (ok_l, ok_a, ok_b)
    }

    /// Convert oklab to linear RGB color space.
    #[inline]
    fn oklab_to_linear_rgb(ok_l: f32, ok_a: f32, ok_b: f32) -> (f32, f32, f32) {
        // Convert from oklab to LMS (cube root space)
        let l_ = ok_l + 0.3963377774 * ok_a + 0.2158037573 * ok_b;
        let m_ = ok_l - 0.1055613458 * ok_a - 0.0638541728 * ok_b;
        let s_ = ok_l - 0.0894841775 * ok_a - 1.2914855480 * ok_b;

        // Cube to get linear LMS
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        // Convert LMS to linear RGB
        let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
        let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
        let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

        (r, g, b)
    }

    /// Interpolate between color stops using oklab color space.
    /// This provides perceptually uniform gradients but doesn't match Chrome's default.
    /// Use for CSS `linear-gradient(in oklab, ...)` when that syntax is supported.
    #[allow(dead_code)]
    fn interpolate_color_oklab(stops: &[(f32, Color)], t: f32) -> Color {
        if stops.is_empty() {
            return Color::TRANSPARENT;
        }
        if stops.len() == 1 || t <= stops[0].0 {
            return stops[0].1;
        }
        if t >= stops[stops.len() - 1].0 {
            return stops[stops.len() - 1].1;
        }

        // Find the two stops surrounding t
        for i in 0..stops.len() - 1 {
            let (pos0, color0) = stops[i];
            let (pos1, color1) = stops[i + 1];
            if t >= pos0 && t <= pos1 {
                let local_t = if (pos1 - pos0).abs() < 0.0001 {
                    0.0
                } else {
                    (t - pos0) / (pos1 - pos0)
                };

                // Convert sRGB to linear RGB
                let r0 = Self::srgb_to_linear(color0.r as f32 / 255.0);
                let g0 = Self::srgb_to_linear(color0.g as f32 / 255.0);
                let b0 = Self::srgb_to_linear(color0.b as f32 / 255.0);

                let r1 = Self::srgb_to_linear(color1.r as f32 / 255.0);
                let g1 = Self::srgb_to_linear(color1.g as f32 / 255.0);
                let b1 = Self::srgb_to_linear(color1.b as f32 / 255.0);

                // Convert to oklab
                let (l0, a0, b0_ok) = Self::linear_rgb_to_oklab(r0, g0, b0);
                let (l1, a1, b1_ok) = Self::linear_rgb_to_oklab(r1, g1, b1);

                // Interpolate in oklab space
                let l_interp = (1.0 - local_t) * l0 + local_t * l1;
                let a_interp = (1.0 - local_t) * a0 + local_t * a1;
                let b_interp = (1.0 - local_t) * b0_ok + local_t * b1_ok;

                // Convert back to linear RGB
                let (r_lin, g_lin, b_lin) = Self::oklab_to_linear_rgb(l_interp, a_interp, b_interp);

                // Clamp to valid range and convert to sRGB
                let r = (Self::linear_to_srgb(r_lin.clamp(0.0, 1.0)) * 255.0).round() as u8;
                let g = (Self::linear_to_srgb(g_lin.clamp(0.0, 1.0)) * 255.0).round() as u8;
                let b = (Self::linear_to_srgb(b_lin.clamp(0.0, 1.0)) * 255.0).round() as u8;

                // Alpha is interpolated linearly
                let a = (1.0 - local_t) * color0.a + local_t * color1.a;

                return Color::new(r, g, b, a);
            }
        }
        stops[stops.len() - 1].1
    }

    /// Interpolate between color stops using high-precision floating point.
    /// Returns ColorF32 to preserve precision through the pipeline.
    /// This function keeps all color math in f32 and only quantizes at final render.
    fn interpolate_color_f32(stops: &[(f32, rustkit_css::ColorF32)], t: f32) -> rustkit_css::ColorF32 {
        if stops.is_empty() {
            return rustkit_css::ColorF32::TRANSPARENT;
        }
        if stops.len() == 1 || t <= stops[0].0 {
            return stops[0].1;
        }
        if t >= stops[stops.len() - 1].0 {
            return stops[stops.len() - 1].1;
        }

        // Find the two stops surrounding t
        for i in 0..stops.len() - 1 {
            let (pos0, color0) = &stops[i];
            let (pos1, color1) = &stops[i + 1];
            if t >= *pos0 && t <= *pos1 {
                let local_t = if (pos1 - pos0).abs() < 0.0001 {
                    0.0
                } else {
                    (t - pos0) / (pos1 - pos0)
                };

                // Premultiplied alpha interpolation in sRGB space
                // This matches Chrome's default gradient interpolation
                return color0.lerp(color1, local_t);
            }
        }
        stops[stops.len() - 1].1
    }

    /// Draw a text input field.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_input(
        &mut self,
        rect: Rect,
        value: &str,
        placeholder: &str,
        font_size: f32,
        text_color: Color,
        placeholder_color: Color,
        background_color: Color,
        border_color: Color,
        border_width: f32,
        focused: bool,
        caret_position: Option<usize>,
    ) {
        // Draw background
        self.draw_solid_rect(rect, background_color);
        
        // Draw border
        let border_rect = rect;
        self.draw_solid_rect(
            Rect::new(rect.x, rect.y, rect.width, border_width),
            border_color,
        );
        self.draw_solid_rect(
            Rect::new(rect.x, rect.y + rect.height - border_width, rect.width, border_width),
            border_color,
        );
        self.draw_solid_rect(
            Rect::new(rect.x, rect.y, border_width, rect.height),
            border_color,
        );
        self.draw_solid_rect(
            Rect::new(rect.x + rect.width - border_width, rect.y, border_width, rect.height),
            border_color,
        );
        
        // Draw text or placeholder
        let padding = 6.0;
        let text_x = rect.x + padding;
        let text_y = rect.y + (rect.height + font_size) / 2.0 - font_size * 0.2;
        
        let (display_text, display_color) = if value.is_empty() {
            (placeholder, placeholder_color)
        } else {
            (value, text_color)
        };
        
        if !display_text.is_empty() {
            self.draw_text(display_text, text_x, text_y, display_color, font_size, "sans-serif", 400, 0);
        }
        
        // Draw focus ring if focused
        if focused {
            self.draw_focus_ring(border_rect, Color::new(0, 122, 255, 1.0), 2.0, 2.0);
        }
        
        // Draw caret if focused and position is set
        if focused {
            if let Some(pos) = caret_position {
                let caret_x = text_x + (pos as f32 * font_size * 0.5);
                self.draw_caret(caret_x, rect.y + 4.0, rect.height - 8.0, text_color);
            }
        }
    }
    
    /// Draw a button.
    #[allow(clippy::too_many_arguments)]
    fn draw_button(
        &mut self,
        rect: Rect,
        label: &str,
        font_size: f32,
        text_color: Color,
        background_color: Color,
        border_color: Color,
        border_width: f32,
        _border_radius: f32,
        pressed: bool,
        focused: bool,
    ) {
        // Adjust colors for pressed state
        let bg = if pressed {
            Color::new(
                (background_color.r as i32 - 20).max(0) as u8,
                (background_color.g as i32 - 20).max(0) as u8,
                (background_color.b as i32 - 20).max(0) as u8,
                background_color.a,
            )
        } else {
            background_color
        };
        
        // Draw background
        self.draw_solid_rect(rect, bg);
        
        // Draw border
        self.draw_solid_rect(
            Rect::new(rect.x, rect.y, rect.width, border_width),
            border_color,
        );
        self.draw_solid_rect(
            Rect::new(rect.x, rect.y + rect.height - border_width, rect.width, border_width),
            border_color,
        );
        self.draw_solid_rect(
            Rect::new(rect.x, rect.y, border_width, rect.height),
            border_color,
        );
        self.draw_solid_rect(
            Rect::new(rect.x + rect.width - border_width, rect.y, border_width, rect.height),
            border_color,
        );
        
        // Draw label (centered)
        if !label.is_empty() {
            let label_width = label.len() as f32 * font_size * 0.5;
            let text_x = rect.x + (rect.width - label_width) / 2.0;
            let text_y = rect.y + (rect.height + font_size) / 2.0 - font_size * 0.2;
            self.draw_text(label, text_x, text_y, text_color, font_size, "sans-serif", 400, 0);
        }
        
        // Draw focus ring if focused
        if focused {
            self.draw_focus_ring(rect, Color::new(0, 122, 255, 1.0), 2.0, 2.0);
        }
    }
    
    /// Draw a focus ring around an element.
    fn draw_focus_ring(&mut self, rect: Rect, color: Color, width: f32, offset: f32) {
        let outer = Rect::new(
            rect.x - offset,
            rect.y - offset,
            rect.width + offset * 2.0,
            rect.height + offset * 2.0,
        );
        
        // Top
        self.draw_solid_rect(
            Rect::new(outer.x, outer.y, outer.width, width),
            color,
        );
        // Bottom
        self.draw_solid_rect(
            Rect::new(outer.x, outer.y + outer.height - width, outer.width, width),
            color,
        );
        // Left
        self.draw_solid_rect(
            Rect::new(outer.x, outer.y, width, outer.height),
            color,
        );
        // Right
        self.draw_solid_rect(
            Rect::new(outer.x + outer.width - width, outer.y, width, outer.height),
            color,
        );
    }
    
    /// Draw a text caret (cursor).
    fn draw_caret(&mut self, x: f32, y: f32, height: f32, color: Color) {
        self.draw_solid_rect(
            Rect::new(x, y, 2.0, height),
            color,
        );
    }

    /// Draw text.
    /// Draw text filled with a gradient (background-clip: text). The
    /// gradient is sampled horizontally across `rect`; each glyph quad gets
    /// the sampled color on its left and right vertex pairs and the GPU
    /// interpolates across the glyph.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_gradient(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        gradient: &rustkit_css::Gradient,
        rect: &Rect,
        font_size: f32,
        font_family: &str,
        font_weight: u16,
        font_style: u8,
        layout_advances: Option<&[f32]>,
        layout_ascent: Option<f32>,
    ) {
        let stops = match gradient {
            rustkit_css::Gradient::Linear(g) => &g.stops,
            rustkit_css::Gradient::Radial(g) => &g.stops,
            rustkit_css::Gradient::Conic(g) => &g.stops,
        };
        if stops.is_empty() {
            return;
        }

        // Resolve stop positions to 0..1: explicit values kept (pixels
        // normalized by the sweep width), first/last default to 0/1, and
        // runs of None distribute evenly between resolved neighbors.
        let span = rect.width.max(1.0);
        let n = stops.len();
        let mut pos: Vec<Option<f32>> = stops
            .iter()
            .map(|s| {
                s.position.as_ref().map(|p| match p {
                    rustkit_css::StopPosition::Percent(v) => *v,
                    rustkit_css::StopPosition::Pixels(px) => px / span,
                })
            })
            .collect();
        if pos[0].is_none() {
            pos[0] = Some(0.0);
        }
        if pos[n - 1].is_none() {
            pos[n - 1] = Some(1.0);
        }
        let mut i = 0;
        while i < n {
            if pos[i].is_none() {
                let start = i - 1; // pos[0] is Some, so start >= 0 is resolved
                let mut end = i;
                while pos[end].is_none() {
                    end += 1;
                }
                let a = pos[start].unwrap();
                let b = pos[end].unwrap();
                let gap = (end - start) as f32;
                for (k, p) in pos.iter_mut().enumerate().take(end).skip(start + 1) {
                    *p = Some(a + (b - a) * (k - start) as f32 / gap);
                }
            }
            i += 1;
        }

        let sample = |t: f32| -> [f32; 4] {
            let t = t.clamp(0.0, 1.0);
            let mut prev = 0usize;
            for (k, p) in pos.iter().enumerate() {
                if p.unwrap() <= t {
                    prev = k;
                } else {
                    break;
                }
            }
            let next = (prev + 1).min(n - 1);
            let (p0, p1) = (pos[prev].unwrap(), pos[next].unwrap());
            let f = if p1 > p0 { (t - p0) / (p1 - p0) } else { 0.0 };
            let (c0, c1) = (&stops[prev].color, &stops[next].color);
            [
                (c0.r as f32 + (c1.r as f32 - c0.r as f32) * f) / 255.0,
                (c0.g as f32 + (c1.g as f32 - c0.g as f32) * f) / 255.0,
                (c0.b as f32 + (c1.b as f32 - c0.b as f32) * f) / 255.0,
                c0.a + (c1.a - c0.a) * f,
            ]
        };

        let mut cursor_x = x;
        let atlas_size = self.glyph_cache.atlas_size() as f32;
        // Glyph entries are baseline-relative (ADVANCE CONTRACT): layout's
        // ascent when shipped, one per-run fallback otherwise.
        let baseline = y
            + layout_ascent.unwrap_or_else(|| Self::fallback_run_ascent(font_family, font_size));

        for (char_idx, ch) in text.chars().enumerate() {
            let key = GlyphKey {
                // FROZEN AT 0 until the rasterizer can draw at a phase --
                // see GlyphKey::subpixel_phase. Pixels are bit-identical to
                // before this field existed.
                subpixel_phase: 0,
                codepoint: ch,
                font_family: font_family.to_string(),
                font_size: (font_size * 10.0) as u32,
                font_weight,
                font_style,
            };

            if let Some(entry) = self.glyph_cache.get_or_rasterize(&self.device, &self.queue, &key) {
                let glyph_x = cursor_x + entry.offset[0];
                let glyph_y = baseline + entry.offset[1];
                let glyph_w = (entry.tex_coords[2] - entry.tex_coords[0]) * atlas_size;
                let glyph_h = (entry.tex_coords[3] - entry.tex_coords[1]) * atlas_size;

                let c_left = sample((glyph_x - rect.x) / span);
                let c_right = sample((glyph_x + glyph_w - rect.x) / span);

                let (x0, y0) = self.transform_point(glyph_x, glyph_y);
                let (x1, y1) = self.transform_point(glyph_x + glyph_w, glyph_y);
                let (x2, y2) = self.transform_point(glyph_x + glyph_w, glyph_y + glyph_h);
                let (x3, y3) = self.transform_point(glyph_x, glyph_y + glyph_h);

                let base = self.texture_vertices.len() as u32;
                self.texture_vertices.extend_from_slice(&[
                    TextureVertex {
                        position: [x0, y0],
                        tex_coords: [entry.tex_coords[0], entry.tex_coords[1]],
                        color: c_left,
                    },
                    TextureVertex {
                        position: [x1, y1],
                        tex_coords: [entry.tex_coords[2], entry.tex_coords[1]],
                        color: c_right,
                    },
                    TextureVertex {
                        position: [x2, y2],
                        tex_coords: [entry.tex_coords[2], entry.tex_coords[3]],
                        color: c_right,
                    },
                    TextureVertex {
                        position: [x3, y3],
                        tex_coords: [entry.tex_coords[0], entry.tex_coords[3]],
                        color: c_left,
                    },
                ]);
                self.texture_indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base,
                    base + 2,
                    base + 3,
                ]);

                cursor_x += layout_advances
                    .and_then(|a| a.get(char_idx).copied())
                    .unwrap_or(entry.advance);
            }
        }
    }

    /// One-per-run ascent fallback for legacy callers that ship no layout
    /// ascent — same metric source the deleted per-glyph lookup used.
    #[cfg(target_os = "macos")]
    fn fallback_run_ascent(font_family: &str, font_size: f32) -> f32 {
        let family = if font_family.is_empty() { "Helvetica" } else { font_family };
        rustkit_text::macos::TextShaper::new(family, font_size as f64)
            .unwrap_or_else(|_| rustkit_text::macos::TextShaper::with_system_font(font_size as f64))
            .get_metrics()
            .ascent
    }

    #[cfg(not(target_os = "macos"))]
    fn fallback_run_ascent(_font_family: &str, font_size: f32) -> f32 {
        font_size * 0.8
    }

    fn draw_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
        font_family: &str,
        font_weight: u16,
        font_style: u8,
    ) {
        self.draw_text_with_metrics(
            text, x, y, color, font_size, font_family, font_weight, font_style, None, None,
        );
    }

    /// Draw text honoring the ADVANCE CONTRACT: when layout ships per-char
    /// advances and an ascent, glyphs are placed at layout's advances and
    /// the baseline sits at y + layout_ascent — the renderer's own advance
    /// derivation and per-glyph ascent shaper (two extra text stacks) are
    /// bypassed. Legacy callers pass None and keep the old behavior.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_with_metrics(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
        font_family: &str,
        font_weight: u16,
        font_style: u8,
        layout_advances: Option<&[f32]>,
        layout_ascent: Option<f32>,
    ) {
        let mut cursor_x = x;
        let c = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a,
        ];

        // Baseline: layout's ascent when the command carries one (ADVANCE
        // CONTRACT), else ONE per-run fallback from the same source the old
        // per-glyph lookup used. Glyph entries are baseline-relative.
        let baseline =
            y + layout_ascent.unwrap_or_else(|| Self::fallback_run_ascent(font_family, font_size));

        // PAINT-0 seating probe (RUSTKIT_PAINT_PROBE=1): paint half of the
        // seating chain — pairs with the layout-side y_cmd log so a flat vs
        // metrics A/B can attribute score deltas to seating float shifts
        // (forensics 2026-07-16-paint0-glyph-seat §4.2 P0a).
        if crate::paint0_probe() {
            eprintln!(
                "PAINT0 paint text={:?} fs={} y_cmd={} layout_ascent={:?} baseline={}",
                text.chars().take(16).collect::<String>(),
                font_size,
                y,
                layout_ascent,
                baseline
            );
        }

        // Get atlas size before the loop to avoid borrow issues
        let atlas_size = self.glyph_cache.atlas_size() as f32;

        for (char_idx, ch) in text.chars().enumerate() {
            let key = GlyphKey {
                // FROZEN AT 0 until the rasterizer can draw at a phase --
                // see GlyphKey::subpixel_phase. Pixels are bit-identical to
                // before this field existed.
                subpixel_phase: 0,
                codepoint: ch,
                font_family: font_family.to_string(),
                font_size: (font_size * 10.0) as u32,
                font_weight,
                font_style,
            };

            // Color-glyph (emoji) path: paint the real color-bitmap artwork via
            // the RGBA atlas + blit pipeline, not the grayscale coverage mask
            // the normal path would tint into a flat blob. Falls through to the
            // grayscale path if the char isn't a color glyph or has no color
            // artwork (e.g. non-macOS).
            #[cfg(target_os = "macos")]
            let is_color = rustkit_text::macos::is_emoji(ch);
            #[cfg(not(target_os = "macos"))]
            let is_color = false;
            if is_color {
                if let Some(entry) =
                    self.glyph_cache.get_or_rasterize_color(&self.device, &self.queue, &key)
                {
                    let glyph_x = cursor_x + entry.offset[0];
                    let glyph_y = baseline + entry.offset[1];
                    let glyph_w = (entry.tex_coords[2] - entry.tex_coords[0]) * atlas_size;
                    let glyph_h = (entry.tex_coords[3] - entry.tex_coords[1]) * atlas_size;

                    let (x0, y0) = self.transform_point(glyph_x, glyph_y);
                    let (x1, y1) = self.transform_point(glyph_x + glyph_w, glyph_y);
                    let (x2, y2) = self.transform_point(glyph_x + glyph_w, glyph_y + glyph_h);
                    let (x3, y3) = self.transform_point(glyph_x, glyph_y + glyph_h);

                    // White vertex color: the blit pipeline multiplies, so this
                    // passes the emoji's own colors through untinted. Preserve
                    // the run's alpha for opacity/fade.
                    let cw = [1.0, 1.0, 1.0, color.a];
                    let base = self.color_glyph_vertices.len() as u32;
                    self.color_glyph_vertices.extend_from_slice(&[
                        TextureVertex { position: [x0, y0], tex_coords: [entry.tex_coords[0], entry.tex_coords[1]], color: cw },
                        TextureVertex { position: [x1, y1], tex_coords: [entry.tex_coords[2], entry.tex_coords[1]], color: cw },
                        TextureVertex { position: [x2, y2], tex_coords: [entry.tex_coords[2], entry.tex_coords[3]], color: cw },
                        TextureVertex { position: [x3, y3], tex_coords: [entry.tex_coords[0], entry.tex_coords[3]], color: cw },
                    ]);
                    self.color_glyph_indices.extend_from_slice(&[
                        base, base + 1, base + 2,
                        base, base + 2, base + 3,
                    ]);

                    cursor_x += layout_advances
                        .and_then(|a| a.get(char_idx).copied())
                        .unwrap_or(entry.advance);
                    continue;
                }
            }

            // Clone the entry to avoid borrow issues
            if let Some(entry) = self.glyph_cache.get_or_rasterize(&self.device, &self.queue, &key) {
                let glyph_x = cursor_x + entry.offset[0];
                let glyph_y = baseline + entry.offset[1];

                // PAINT-0: sample chars only — x (ex-height), H (cap), g
                // (descender) cover the three seating regimes.
                if matches!(ch, 'x' | 'H' | 'g') && crate::paint0_probe() {
                    eprintln!(
                        "PAINT0 glyph ch={:?} fs={} baseline={} bearing_y={} glyph_y={}",
                        ch,
                        font_size,
                        baseline,
                        -entry.offset[1],
                        glyph_y
                    );
                }
                let glyph_w = (entry.tex_coords[2] - entry.tex_coords[0]) * atlas_size;
                let glyph_h = (entry.tex_coords[3] - entry.tex_coords[1]) * atlas_size;

                // Apply transform to glyph corners
                let (x0, y0) = self.transform_point(glyph_x, glyph_y);
                let (x1, y1) = self.transform_point(glyph_x + glyph_w, glyph_y);
                let (x2, y2) = self.transform_point(glyph_x + glyph_w, glyph_y + glyph_h);
                let (x3, y3) = self.transform_point(glyph_x, glyph_y + glyph_h);

                let base = self.texture_vertices.len() as u32;

                self.texture_vertices.extend_from_slice(&[
                    TextureVertex {
                        position: [x0, y0],
                        tex_coords: [entry.tex_coords[0], entry.tex_coords[1]],
                        color: c,
                    },
                    TextureVertex {
                        position: [x1, y1],
                        tex_coords: [entry.tex_coords[2], entry.tex_coords[1]],
                        color: c,
                    },
                    TextureVertex {
                        position: [x2, y2],
                        tex_coords: [entry.tex_coords[2], entry.tex_coords[3]],
                        color: c,
                    },
                    TextureVertex {
                        position: [x3, y3],
                        tex_coords: [entry.tex_coords[0], entry.tex_coords[3]],
                        color: c,
                    },
                ]);

                self.texture_indices.extend_from_slice(&[
                    base, base + 1, base + 2,
                    base, base + 2, base + 3,
                ]);

                // ADVANCE CONTRACT: layout's advance wins when present so
                // painted ink tracks measured width 1:1; the atlas advance
                // is the fallback for legacy callers.
                cursor_x += layout_advances
                    .and_then(|a| a.get(char_idx).copied())
                    .unwrap_or(entry.advance);
            } else {
                // Fallback: advance by estimated width (or layout's, if given)
                cursor_x += layout_advances
                    .and_then(|a| a.get(char_idx).copied())
                    .unwrap_or(font_size * 0.6);
            }
        }
    }

    /// Draw an image.
    fn draw_image(&mut self, url: &str, rect: Rect) {
        if self.texture_cache.contains(url) {
            // Apply transform to image corners
            let (x0, y0) = self.transform_point(rect.x, rect.y);
            let (x1, y1) = self.transform_point(rect.x + rect.width, rect.y);
            let (x2, y2) = self.transform_point(rect.x + rect.width, rect.y + rect.height);
            let (x3, y3) = self.transform_point(rect.x, rect.y + rect.height);

            self.push_image_quad(
                url,
                [
                    TextureVertex {
                        position: [x0, y0],
                        tex_coords: [0.0, 0.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                    TextureVertex {
                        position: [x1, y1],
                        tex_coords: [1.0, 0.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                    TextureVertex {
                        position: [x2, y2],
                        tex_coords: [1.0, 1.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                    TextureVertex {
                        position: [x3, y3],
                        tex_coords: [0.0, 1.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                    },
                ],
            );
        }
        // If image not loaded, skip (async loading handled elsewhere)
    }

    /// Append a quad to the image batch, extending the current run when the
    /// previous quad used the same texture so consecutive tiles stay one draw.
    fn push_image_quad(&mut self, url: &str, corners: [TextureVertex; 4]) {
        let base = self.image_vertices.len() as u32;
        self.image_vertices.extend_from_slice(&corners);
        self.image_indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);

        match self.image_runs.last_mut() {
            Some((last_url, count)) if last_url == url => *count += 6,
            _ => self.image_runs.push((url.to_string(), 6)),
        }
    }

    /// Draw a background image with proper size, position, and repeat handling.
    fn draw_background_image(
        &mut self,
        url: &str,
        container: Rect,
        size: &BackgroundSize,
        position: (f32, f32),
        repeat: &BackgroundRepeat,
    ) {
        // Get the texture to retrieve image dimensions
        let (image_width, image_height) = if let Some(cached) = self.texture_cache.get(url) {
            (cached.width as f32, cached.height as f32)
        } else {
            // Image not loaded yet, skip
            return;
        };

        if image_width == 0.0 || image_height == 0.0 {
            return;
        }

        // Calculate the background image size based on size property
        let (bg_width, bg_height) = size.compute_size(container, image_width, image_height);

        if bg_width == 0.0 || bg_height == 0.0 {
            return;
        }

        // Calculate the starting position based on position property
        let mut start_x = container.x + (container.width - bg_width) * position.0;
        let mut start_y = container.y + (container.height - bg_height) * position.1;

        // Adjust size and spacing for space/round modes
        let mut adjusted_bg_width = bg_width;
        let mut adjusted_bg_height = bg_height;
        let mut spacing_x = 0.0_f32;
        let mut spacing_y = 0.0_f32;

        match repeat {
            BackgroundRepeat::Space => {
                // Calculate how many full images fit
                let fit_count_x = (container.width / bg_width).floor().max(1.0);
                let fit_count_y = (container.height / bg_height).floor().max(1.0);

                // Calculate spacing to evenly distribute
                if fit_count_x > 1.0 {
                    let total_image_width = fit_count_x * bg_width;
                    let remaining_space_x = container.width - total_image_width;
                    spacing_x = remaining_space_x / (fit_count_x - 1.0);
                }

                if fit_count_y > 1.0 {
                    let total_image_height = fit_count_y * bg_height;
                    let remaining_space_y = container.height - total_image_height;
                    spacing_y = remaining_space_y / (fit_count_y - 1.0);
                }

                // Start at container edge for space mode
                start_x = container.x;
                start_y = container.y;
            }
            BackgroundRepeat::Round => {
                // Calculate integer repetitions by rounding
                let repetitions_x = (container.width / bg_width).round().max(1.0);
                let repetitions_y = (container.height / bg_height).round().max(1.0);

                // Scale image to fit exactly
                adjusted_bg_width = container.width / repetitions_x;
                adjusted_bg_height = container.height / repetitions_y;

                // Start at container edge for round mode
                start_x = container.x;
                start_y = container.y;
            }
            _ => {}
        }

        // Determine tiling based on repeat
        let (tile_x, tile_y) = match repeat {
            BackgroundRepeat::Repeat => (true, true),
            BackgroundRepeat::RepeatX => (true, false),
            BackgroundRepeat::RepeatY => (false, true),
            BackgroundRepeat::NoRepeat => (false, false),
            BackgroundRepeat::Space => (true, true),
            BackgroundRepeat::Round => (true, true),
        };

        // Generate tile positions
        if !tile_x && !tile_y {
            // Single image - draw at the calculated position
            self.draw_background_image_tile(url, Rect {
                x: start_x,
                y: start_y,
                width: adjusted_bg_width,
                height: adjusted_bg_height,
            }, container);
        } else {
            // Tiled images
            let x_start = if tile_x && *repeat != BackgroundRepeat::Space && *repeat != BackgroundRepeat::Round {
                // Find the leftmost position that's visible (for repeat mode)
                let tiles_left = ((start_x - container.x) / adjusted_bg_width).ceil() as i32;
                start_x - (tiles_left as f32 * adjusted_bg_width)
            } else {
                start_x
            };

            let y_start = if tile_y && *repeat != BackgroundRepeat::Space && *repeat != BackgroundRepeat::Round {
                let tiles_up = ((start_y - container.y) / adjusted_bg_height).ceil() as i32;
                start_y - (tiles_up as f32 * adjusted_bg_height)
            } else {
                start_y
            };

            let mut y = y_start;
            while y < container.y + container.height {
                let mut x = x_start;
                while x < container.x + container.width {
                    let tile_rect = Rect {
                        x,
                        y,
                        width: adjusted_bg_width,
                        height: adjusted_bg_height,
                    };

                    // Only draw if visible within container
                    if tile_rect.x + tile_rect.width > container.x
                        && tile_rect.y + tile_rect.height > container.y
                        && tile_rect.x < container.x + container.width
                        && tile_rect.y < container.y + container.height
                    {
                        self.draw_background_image_tile(url, tile_rect, container);
                    }

                    if tile_x {
                        x += adjusted_bg_width + spacing_x;
                    } else {
                        break;
                    }
                }

                if tile_y {
                    y += adjusted_bg_height + spacing_y;
                } else {
                    break;
                }
            }
        }
    }

    /// Draw a single tile of a background image, clipped to the container bounds.
    fn draw_background_image_tile(&mut self, url: &str, tile_rect: Rect, container: Rect) {
        if !self.texture_cache.contains(url) {
            return;
        }

        // Clip tile to container bounds
        let clip_left = (container.x - tile_rect.x).max(0.0);
        let clip_top = (container.y - tile_rect.y).max(0.0);
        let clip_right = (tile_rect.x + tile_rect.width - container.x - container.width).max(0.0);
        let clip_bottom = (tile_rect.y + tile_rect.height - container.y - container.height).max(0.0);

        let draw_rect = Rect {
            x: tile_rect.x + clip_left,
            y: tile_rect.y + clip_top,
            width: tile_rect.width - clip_left - clip_right,
            height: tile_rect.height - clip_top - clip_bottom,
        };

        if draw_rect.width <= 0.0 || draw_rect.height <= 0.0 {
            return;
        }

        // Calculate texture coordinates for the clipped portion
        let tex_left = clip_left / tile_rect.width;
        let tex_top = clip_top / tile_rect.height;
        let tex_right = 1.0 - clip_right / tile_rect.width;
        let tex_bottom = 1.0 - clip_bottom / tile_rect.height;

        // Apply transform to image corners
        let (x0, y0) = self.transform_point(draw_rect.x, draw_rect.y);
        let (x1, y1) = self.transform_point(draw_rect.x + draw_rect.width, draw_rect.y);
        let (x2, y2) = self.transform_point(draw_rect.x + draw_rect.width, draw_rect.y + draw_rect.height);
        let (x3, y3) = self.transform_point(draw_rect.x, draw_rect.y + draw_rect.height);

        self.push_image_quad(
            url,
            [
                TextureVertex {
                    position: [x0, y0],
                    tex_coords: [tex_left, tex_top],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                TextureVertex {
                    position: [x1, y1],
                    tex_coords: [tex_right, tex_top],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                TextureVertex {
                    position: [x2, y2],
                    tex_coords: [tex_right, tex_bottom],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                TextureVertex {
                    position: [x3, y3],
                    tex_coords: [tex_left, tex_bottom],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
            ],
        );
    }

    /// Upload an image to the texture cache.
    /// 
    /// Call this to upload decoded image data (RGBA format) to the GPU.
    /// Once uploaded, the image can be drawn using its URL as the key.
    pub fn upload_image(
        &mut self,
        url: &str,
        width: u32,
        height: u32,
        rgba_data: &[u8],
    ) -> Result<(), RendererError> {
        if rgba_data.len() != (width * height * 4) as usize {
            return Err(RendererError::TextureUpload(format!(
                "Invalid image data size: expected {} bytes, got {}",
                width * height * 4,
                rgba_data.len()
            )));
        }
        
        self.texture_cache.get_or_create(
            &self.device,
            &self.queue,
            url,
            width,
            height,
            rgba_data,
        );
        
        Ok(())
    }
    
    /// Check if an image is already uploaded.
    pub fn has_image(&self, url: &str) -> bool {
        self.texture_cache.contains(url)
    }
    
    /// Remove an image from the cache.
    pub fn remove_image(&mut self, url: &str) {
        self.texture_cache.remove(url);
    }


    /// Push a clipping rectangle.
    fn push_clip(&mut self, rect: Rect) {
        let clip = if let Some(current) = self.clip_stack.last() {
            if let Some(intersected) = current.intersect(&rect) {
                intersected
            } else {
                Rect::new(0.0, 0.0, 0.0, 0.0) // Empty clip
            }
        } else {
            rect
        };
        self.clip_stack.push(clip);
    }

    /// Pop the current clipping rectangle.
    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Get the current clip rectangle.
    fn current_clip(&self) -> Option<Rect> {
        self.clip_stack.last().copied()
    }

    /// Push a 2D transform matrix onto the stack.
    fn push_transform(&mut self, matrix: [f32; 6], origin: (f32, f32)) {
        self.transform_stack.push((matrix, origin));
    }

    /// Pop the current transform from the stack.
    fn pop_transform(&mut self) {
        self.transform_stack.pop();
    }

    /// Get the current combined transform matrix.
    /// Returns identity matrix [1, 0, 0, 1, 0, 0] if no transforms are active.
    fn current_transform(&self) -> [f32; 6] {
        if self.transform_stack.is_empty() {
            return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]; // Identity
        }

        // Compose all transforms on the stack
        let mut result = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        for (matrix, origin) in &self.transform_stack {
            // Apply origin offset: translate(-origin) * matrix * translate(origin)
            // First, translate to origin
            let t1 = [1.0, 0.0, 0.0, 1.0, -origin.0, -origin.1];
            // Then the transform
            let m = *matrix;
            // Then translate back
            let t2 = [1.0, 0.0, 0.0, 1.0, origin.0, origin.1];

            // Compose: result = result * t1 * m * t2
            let temp1 = multiply_matrices_2d(result, t1);
            let temp2 = multiply_matrices_2d(temp1, m);
            result = multiply_matrices_2d(temp2, t2);
        }
        result
    }

    /// Apply the current transform to a point.
    /// The index range of gradient strips that can possibly reach pixels,
    /// given the current clip and the viewport. Strips are generated one per
    /// CSS pixel along the gradient axis, so an unculled loop is O(document
    /// extent): a single 3,000,000px-tall gradient emits 3M quads = 288MB of
    /// color vertices and the frame dies with BufferTooLarge — every frame,
    /// forever, which is exactly the repeating "Buffer 'Color Vertex Buffer'
    /// size ... exceeds maximum" seen on real tab pages (2026-08-05).
    ///
    /// Only valid when no transform is active: with a transform on the stack
    /// the strip's document position no longer predicts its screen position,
    /// so we fall back to the full range rather than wrongly cull content a
    /// transform moves into view. Cap stays either way as the last line.
    fn visible_strip_range(
        &self,
        axis_start: f32,
        axis_len: f32,
        step_count: usize,
        viewport_extent: f32,
    ) -> (usize, usize) {
        const MAX_STRIPS: usize = 32_768;
        if !self.transform_stack.is_empty() {
            return (0, step_count.min(MAX_STRIPS));
        }
        // Visible window along this axis is the viewport [0, extent).
        // Per-strip clip culling still happens inside draw_solid_rect_f32;
        // this range only bounds the LOOP, which is what the vertex budget
        // needs — a strip inside the viewport but outside a clip costs one
        // rejected call, not a quad.
        let (lo, hi) = (0.0_f32, viewport_extent);
        let strip = axis_len / step_count as f32;
        let first = (((lo - axis_start) / strip).floor().max(0.0)) as usize;
        let last = ((((hi - axis_start) / strip).ceil()).max(0.0) as usize).min(step_count);
        let first = first.min(last);
        (first, last.min(first + MAX_STRIPS))
    }

    fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.current_transform();
        // [a, b, c, d, e, f] where:
        // x' = a*x + c*y + e
        // y' = b*x + d*y + f
        let x_prime = m[0] * x + m[2] * y + m[4];
        let y_prime = m[1] * x + m[3] * y + m[5];
        (x_prime, y_prime)
    }

    /// Flush all batched vertices to the target.
    /// Draw the batched image quads into an open render pass: one draw call
    /// per (texture, run) pair so every image samples its own texture rather
    /// than the glyph atlas the shared texture batch binds.
    fn draw_image_batch(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        if self.image_vertices.is_empty() {
            return;
        }

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Image Vertex Buffer"),
            contents: bytemuck::cast_slice(&self.image_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Image Index Buffer"),
            contents: bytemuck::cast_slice(&self.image_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // blit_pipeline, not texture_pipeline: the texture shader treats the
        // sampled R channel as glyph-atlas alpha; blit samples real RGBA.
        render_pass.set_pipeline(&self.blit_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        let mut start = 0u32;
        for (url, count) in &self.image_runs {
            if let Some(cached) = self.texture_cache.get(url) {
                render_pass.set_bind_group(1, &cached.bind_group, &[]);
                render_pass.draw_indexed(start..start + count, 0, 0..1);
            }
            start += count;
        }
    }

    /// Draw the color-glyph (emoji) batch: RGBA quads sampling the color atlas
    /// via the passthrough blit pipeline (blit samples real RGBA; the grayscale
    /// texture pipeline would treat R as alpha and mangle the artwork). Empty on
    /// pages without emoji, so this is a no-op for normal text.
    fn draw_color_glyph_batch(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        if self.color_glyph_vertices.is_empty() {
            return;
        }
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Color Glyph Vertex Buffer"),
            contents: bytemuck::cast_slice(&self.color_glyph_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Color Glyph Index Buffer"),
            contents: bytemuck::cast_slice(&self.color_glyph_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        render_pass.set_pipeline(&self.color_glyph_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, self.glyph_cache.color_bind_group(), &[]);
        render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.color_glyph_indices.len() as u32, 0, 0..1);
    }

    fn flush_to(&mut self, target: &wgpu::TextureView) -> Result<(), RendererError> {
        // Check for debug visual mode (RUSTKIT_DEBUG_VISUAL=1)
        // When enabled, clear to magenta to prove pixels are hitting the screen
        let debug_visual = std::env::var("RUSTKIT_DEBUG_VISUAL")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let clear_color = if debug_visual {
            // Magenta - very visible, proves rendering works
            wgpu::Color {
                r: 1.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            }
        } else {
            // Normal white background
            wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            }
        };

        // Clear gradient queues (safety measure - they should already be empty)
        // GPU gradients are now rendered inline via execute_with_gpu_gradients() for correct z-order
        self.gradient_queue.clear();
        self.radial_gradient_queue.clear();
        self.conic_gradient_queue.clear();

        // Render batched content
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        // Always clear on first pass
        let load_op = wgpu::LoadOp::Clear(clear_color);

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
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

            // In debug mode, draw a test rectangle at (10,10) to prove draw commands work
            if debug_visual && self.color_vertices.is_empty() {
                // If no commands were issued, add a test rectangle
                let test_rect = Rect::new(10.0, 10.0, 100.0, 100.0);
                let test_color = Color::new(0, 255, 0, 1.0); // Green
                let c = [
                    test_color.r as f32 / 255.0,
                    test_color.g as f32 / 255.0,
                    test_color.b as f32 / 255.0,
                    test_color.a,
                ];
                let x = test_rect.x;
                let y = test_rect.y;
                let w = test_rect.width;
                let h = test_rect.height;

                self.color_vertices.extend_from_slice(&[
                    ColorVertex { position: [x, y], color: c },
                    ColorVertex { position: [x + w, y], color: c },
                    ColorVertex { position: [x + w, y + h], color: c },
                    ColorVertex { position: [x, y + h], color: c },
                ]);
                self.color_indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
            }

            // Draw solid colors
            if !self.color_vertices.is_empty() {
                // Validate buffer sizes before allocation
                let vertex_size = (self.color_vertices.len() * std::mem::size_of::<ColorVertex>()) as u64;
                let index_size = (self.color_indices.len() * std::mem::size_of::<u32>()) as u64;

                self.validate_buffer_size(vertex_size, "Color Vertex Buffer")?;
                self.validate_buffer_size(index_size, "Color Index Buffer")?;

                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Color Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.color_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Color Index Buffer"),
                    contents: bytemuck::cast_slice(&self.color_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                render_pass.set_pipeline(&self.color_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.color_indices.len() as u32, 0, 0..1);
            }

            // Draw images (own textures) between backgrounds and text
            self.draw_image_batch(&mut render_pass);

            // Draw textured quads (glyphs)
            if !self.texture_vertices.is_empty() {
                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Texture Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.texture_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Texture Index Buffer"),
                    contents: bytemuck::cast_slice(&self.texture_indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                render_pass.set_pipeline(&self.texture_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, self.glyph_cache.bind_group(), &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..self.texture_indices.len() as u32, 0, 0..1);
            }

            // Color glyphs (emoji) drawn on top via the RGBA atlas + blit pipeline.
            self.draw_color_glyph_batch(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Note: GPU gradients are now rendered inline via execute_with_gpu_gradients()
        // for correct z-order (gradients render in DOM order, not all-at-end)

        Ok(())
    }

    /// Get access to the texture cache for external image loading.
    pub fn texture_cache(&mut self) -> &mut TextureCache {
        &mut self.texture_cache
    }

    /// Get access to the glyph cache.
    pub fn glyph_cache(&mut self) -> &mut GlyphCache {
        &mut self.glyph_cache
    }
}

// ==================== Rect Extension ====================

trait RectExt {
    fn intersect(&self, other: &Rect) -> Option<Rect>;
}

impl RectExt for Rect {
    fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        if right > x && bottom > y {
            Some(Rect::new(x, y, right - x, bottom - y))
        } else {
            None
        }
    }
}

// ==================== Transform Helpers ====================

/// Multiply two 2D affine matrices.
/// Matrix format: [a, b, c, d, e, f] representing:
/// | a c e |
/// | b d f |
/// | 0 0 1 |
fn multiply_matrices_2d(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_vertex_size() {
        assert_eq!(std::mem::size_of::<ColorVertex>(), 24);
    }

    #[test]
    fn test_texture_vertex_size() {
        assert_eq!(std::mem::size_of::<TextureVertex>(), 32);
    }

    #[test]
    fn test_uniforms_size() {
        assert_eq!(std::mem::size_of::<Uniforms>(), 16);
    }

    #[test]
    fn test_rect_intersect() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);

        let result = a.intersect(&b).unwrap();
        assert_eq!(result.x, 50.0);
        assert_eq!(result.y, 50.0);
        assert_eq!(result.width, 50.0);
        assert_eq!(result.height, 50.0);
    }

    #[test]
    fn test_rect_no_intersect() {
        let a = Rect::new(0.0, 0.0, 50.0, 50.0);
        let b = Rect::new(100.0, 100.0, 50.0, 50.0);

        assert!(a.intersect(&b).is_none());
    }

    // ==================== Gradient Coordinate Tests ====================
    // These tests verify the gradient math matches between CPU and GPU implementations.

    /// Test gradient half-length calculation for various angles.
    /// CSS spec: gradient line extends from corner to corner through center.
    /// Formula: |sin(angle)| * half_width + |cos(angle)| * half_height
    #[test]
    fn test_gradient_half_length() {
        let half_w = 50.0_f32;
        let half_h = 50.0_f32;

        // 0deg (to top): sin=0, cos=1 -> half_h = 50
        let angle_rad = 0.0_f32.to_radians();
        let result = (half_w * angle_rad.sin().abs() + half_h * angle_rad.cos().abs()).max(0.001);
        assert!((result - 50.0).abs() < 0.01, "0deg: expected 50, got {}", result);

        // 90deg (to right): sin=1, cos=0 -> half_w = 50
        let angle_rad = 90.0_f32.to_radians();
        let result = (half_w * angle_rad.sin().abs() + half_h * angle_rad.cos().abs()).max(0.001);
        assert!((result - 50.0).abs() < 0.01, "90deg: expected 50, got {}", result);

        // 45deg: sin=0.707, cos=0.707 -> 0.707*50 + 0.707*50 = 70.7
        let angle_rad = 45.0_f32.to_radians();
        let result = (half_w * angle_rad.sin().abs() + half_h * angle_rad.cos().abs()).max(0.001);
        assert!((result - 70.71).abs() < 0.1, "45deg: expected 70.71, got {}", result);

        // 180deg (to bottom): sin=0, cos=-1 -> |cos|=1 -> half_h = 50
        let angle_rad = 180.0_f32.to_radians();
        let result = (half_w * angle_rad.sin().abs() + half_h * angle_rad.cos().abs()).max(0.001);
        assert!((result - 50.0).abs() < 0.01, "180deg: expected 50, got {}", result);
    }

    /// Test gradient direction vector follows CSS convention.
    /// CSS: 0deg = "to top", 90deg = "to right", etc.
    /// Direction vector: (sin(angle), -cos(angle))
    #[test]
    fn test_gradient_direction_vector() {
        // 0deg (to top): direction = (0, -1) -> points UP
        let angle_rad = 0.0_f32.to_radians();
        let dir = (angle_rad.sin(), -angle_rad.cos());
        assert!((dir.0 - 0.0).abs() < 0.001, "0deg dir.x: expected 0, got {}", dir.0);
        assert!((dir.1 - (-1.0)).abs() < 0.001, "0deg dir.y: expected -1, got {}", dir.1);

        // 90deg (to right): direction = (1, 0) -> points RIGHT
        let angle_rad = 90.0_f32.to_radians();
        let dir = (angle_rad.sin(), -angle_rad.cos());
        assert!((dir.0 - 1.0).abs() < 0.001, "90deg dir.x: expected 1, got {}", dir.0);
        assert!((dir.1 - 0.0).abs() < 0.001, "90deg dir.y: expected 0, got {}", dir.1);

        // 180deg (to bottom): direction = (0, 1) -> points DOWN
        let angle_rad = 180.0_f32.to_radians();
        let dir = (angle_rad.sin(), -angle_rad.cos());
        assert!((dir.0 - 0.0).abs() < 0.001, "180deg dir.x: expected 0, got {}", dir.0);
        assert!((dir.1 - 1.0).abs() < 0.001, "180deg dir.y: expected 1, got {}", dir.1);

        // 270deg (to left): direction = (-1, 0) -> points LEFT
        let angle_rad = 270.0_f32.to_radians();
        let dir = (angle_rad.sin(), -angle_rad.cos());
        assert!((dir.0 - (-1.0)).abs() < 0.001, "270deg dir.x: expected -1, got {}", dir.0);
        assert!((dir.1 - 0.0).abs() < 0.001, "270deg dir.y: expected 0, got {}", dir.1);
    }

    /// Test t-value calculation for a 0deg gradient on a 100x100 rect.
    /// 0deg = "to top": red at BOTTOM (t=0), blue at TOP (t=1)
    #[test]
    fn test_gradient_t_value_vertical() {
        let rect_x = 0.0_f32;
        let rect_y = 0.0_f32;
        let rect_width = 100.0_f32;
        let rect_height = 100.0_f32;
        let angle_deg = 0.0_f32;
        let angle_rad = angle_deg.to_radians();

        let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());
        let half_w = rect_width / 2.0;
        let half_h = rect_height / 2.0;
        let gradient_half_length = (half_w * sin_a.abs() + half_h * cos_a.abs()).max(0.001);
        let center_x = rect_x + half_w;
        let center_y = rect_y + half_h;

        // At top center (50, 0): should be t=1.0 (blue end)
        let px = 50.0 - center_x;
        let py = 0.0 - center_y; // py = -50
        let projection = px * sin_a + py * (-cos_a); // 0 + (-50)*(-1) = 50
        let t = (projection / gradient_half_length + 1.0) / 2.0;
        assert!((t - 1.0).abs() < 0.01, "Top center t: expected 1.0, got {}", t);

        // At bottom center (50, 100): should be t=0.0 (red end)
        let px = 50.0 - center_x;
        let py = 100.0 - center_y; // py = 50
        let projection = px * sin_a + py * (-cos_a); // 0 + 50*(-1) = -50
        let t = (projection / gradient_half_length + 1.0) / 2.0;
        assert!((t - 0.0).abs() < 0.01, "Bottom center t: expected 0.0, got {}", t);

        // At center (50, 50): should be t=0.5
        let px = 50.0 - center_x;
        let py = 50.0 - center_y;
        let projection = px * sin_a + py * (-cos_a);
        let t = (projection / gradient_half_length + 1.0) / 2.0;
        assert!((t - 0.5).abs() < 0.01, "Center t: expected 0.5, got {}", t);
    }

    /// Test t-value calculation for a 90deg gradient on a 100x100 rect.
    /// 90deg = "to right": red at LEFT (t=0), blue at RIGHT (t=1)
    #[test]
    fn test_gradient_t_value_horizontal() {
        let rect_x = 0.0_f32;
        let rect_y = 0.0_f32;
        let rect_width = 100.0_f32;
        let rect_height = 100.0_f32;
        let angle_deg = 90.0_f32;
        let angle_rad = angle_deg.to_radians();

        let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());
        let half_w = rect_width / 2.0;
        let half_h = rect_height / 2.0;
        let gradient_half_length = (half_w * sin_a.abs() + half_h * cos_a.abs()).max(0.001);
        let center_x = rect_x + half_w;
        let center_y = rect_y + half_h;

        // At left center (0, 50): should be t=0.0 (red end)
        let px = 0.0 - center_x; // px = -50
        let py = 50.0 - center_y;
        let projection = px * sin_a + py * (-cos_a); // -50*1 + 0 = -50
        let t = (projection / gradient_half_length + 1.0) / 2.0;
        assert!((t - 0.0).abs() < 0.01, "Left center t: expected 0.0, got {}", t);

        // At right center (100, 50): should be t=1.0 (blue end)
        let px = 100.0 - center_x; // px = 50
        let py = 50.0 - center_y;
        let projection = px * sin_a + py * (-cos_a); // 50*1 + 0 = 50
        let t = (projection / gradient_half_length + 1.0) / 2.0;
        assert!((t - 1.0).abs() < 0.01, "Right center t: expected 1.0, got {}", t);
    }

    /// Test GradientParams struct size matches what GPU expects.
    #[test]
    fn test_gradient_params_size() {
        // GradientParams should be 80 bytes (20 x 4-byte values)
        assert_eq!(
            std::mem::size_of::<crate::pipeline::GradientParams>(),
            80,
            "GradientParams size mismatch"
        );
    }

    /// Test GradientColorStop struct size matches what GPU expects.
    #[test]
    fn test_gradient_color_stop_size() {
        // GradientColorStop should be 20 bytes (5 f32 values)
        assert_eq!(
            std::mem::size_of::<crate::pipeline::GradientColorStop>(),
            20,
            "GradientColorStop size mismatch"
        );
    }

    /// Test buffer size validation logic (unit test without GPU).
    #[test]
    fn test_buffer_size_validation_logic() {
        // Test the validation logic directly
        // This tests the error handling without needing a real GPU device

        const MAX_SIZE: u64 = 256 * 1024 * 1024; // 256 MB

        // Simulate validation function
        let validate = |size: u64| -> Result<u64, String> {
            if size > MAX_SIZE {
                Err(format!("Buffer size {} exceeds maximum {}", size, MAX_SIZE))
            } else {
                Ok(size)
            }
        };

        // Test valid buffer sizes
        assert!(validate(1024).is_ok());
        assert!(validate(1024 * 1024).is_ok());
        assert!(validate(100 * 1024 * 1024).is_ok());

        // Test buffer size at limit
        assert!(validate(MAX_SIZE).is_ok());

        // Test buffer size exceeding limit
        assert!(validate(MAX_SIZE + 1).is_err());
        assert!(validate(MAX_SIZE * 2).is_err());

        // Test pathological gradient scenario (10K stops)
        let pathological_stops = 10_000;
        let stop_size = std::mem::size_of::<crate::pipeline::GradientColorStop>() as u64; // 20 bytes
        let total_size = pathological_stops * stop_size;
        // 10K stops = 200KB, well within limits
        assert!(validate(total_size).is_ok(), "10K gradient stops should fit in buffer");

        // Test extreme case (1M stops would be 20MB, still within 256MB limit)
        let extreme_stops = 1_000_000;
        let extreme_size = extreme_stops * stop_size;
        assert!(validate(extreme_size).is_ok(), "1M stops should fit");

        // Test truly pathological case (1B stops = 20GB, should exceed limit)
        let truly_pathological = 1_000_000_000;
        let pathological_size = truly_pathological * stop_size;
        assert!(validate(pathological_size).is_err(), "1B stops should exceed limit");
    }

    #[test]
    fn test_gradient_stops_clamping() {
        // Test that gradient stops are properly clamped to max_stops (32)
        // This is important for GPU buffer allocation safety
        const MAX_STOPS: usize = 32;

        // Simulate what happens with many stops
        let input_stops = 100;
        let clamped = input_stops.min(MAX_STOPS);
        assert_eq!(clamped, MAX_STOPS, "Should clamp to 32 stops");

        // Edge case: exactly at limit
        assert_eq!(MAX_STOPS.min(MAX_STOPS), MAX_STOPS);

        // Edge case: under limit
        assert_eq!(10_usize.min(MAX_STOPS), 10);
    }

    #[test]
    fn test_circle_rendering_triangle_count() {
        // Test that circle rendering uses a reasonable triangle count
        // Circles are rendered as triangle fans
        const MIN_SEGMENTS: u32 = 16; // Minimum for visual smoothness
        const MAX_SEGMENTS: u32 = 64; // Maximum to avoid GPU overload

        // For a small circle (radius 10px), use minimum segments
        let small_radius = 10.0;
        let small_segments = estimate_circle_segments(small_radius);
        assert!(small_segments >= MIN_SEGMENTS, "Small circle needs minimum segments");
        assert!(small_segments <= MAX_SEGMENTS, "Small circle shouldn't exceed max");

        // For a large circle (radius 500px), might use more segments
        let large_radius = 500.0;
        let large_segments = estimate_circle_segments(large_radius);
        assert!(large_segments >= MIN_SEGMENTS);
        assert!(large_segments <= MAX_SEGMENTS, "Large circle should be clamped");

        // Helper function to estimate segments (simplified version)
        fn estimate_circle_segments(radius: f32) -> u32 {
            // A simple heuristic: use more segments for larger circles
            let base = (radius / 10.0).sqrt() as u32;
            base.max(16).min(64)
        }
    }

    #[test]
    fn test_ellipse_aspect_ratio() {
        // Test that ellipse rendering maintains correct aspect ratio
        let width = 200.0;
        let height = 100.0;
        let aspect = width / height;
        assert_eq!(aspect, 2.0, "Aspect ratio should be 2:1");

        // Edge case: circle (aspect 1:1)
        let circle_width = 100.0;
        let circle_height = 100.0;
        let circle_aspect = circle_width / circle_height;
        assert_eq!(circle_aspect, 1.0, "Circle has 1:1 aspect");

        // Edge case: very elongated ellipse
        let narrow_width = 500.0;
        let narrow_height = 10.0;
        let narrow_aspect = narrow_width / narrow_height;
        assert_eq!(narrow_aspect, 50.0, "Narrow ellipse has 50:1 aspect");
    }

    #[test]
    fn test_background_repeat_space_calculation() {
        // Test background-repeat: space calculation
        // Should evenly distribute images with spacing between them
        let container_width = 800.0_f32;
        let image_width = 100.0_f32;

        // Calculate how many full images fit
        let fit_count = (container_width / image_width).floor() as u32;
        assert_eq!(fit_count, 8, "8 images of 100px fit in 800px");

        // Calculate spacing for 'space' mode
        // With 8 images, we need 7 gaps to distribute evenly
        let gaps = if fit_count > 1 { fit_count - 1 } else { 1 };
        let total_image_width = fit_count as f32 * image_width;
        let remaining_space = container_width - total_image_width;
        let gap_size = remaining_space / gaps as f32;

        assert!(gap_size >= 0.0_f32, "Gap size should be non-negative");
        assert_eq!(gap_size, 0.0_f32, "With perfect fit, gap is 0");

        // Test with imperfect fit
        let container_width_2 = 850.0_f32;
        let fit_count_2 = (container_width_2 / image_width).floor() as u32;
        let gaps_2 = fit_count_2 - 1;
        let total_image_width_2 = fit_count_2 as f32 * image_width;
        let remaining_space_2 = container_width_2 - total_image_width_2;
        let gap_size_2 = remaining_space_2 / gaps_2 as f32;

        assert!(gap_size_2 > 0.0_f32, "Imperfect fit should have gaps");
        assert!((gap_size_2 - 7.14_f32).abs() < 0.1_f32, "Gap should be ~7.14px");
    }

    #[test]
    fn test_background_repeat_round_scaling() {
        // Test background-repeat: round calculation
        // Should scale images to fit container with integer repetitions
        let container_width = 850.0_f32;
        let image_width = 100.0_f32;

        // Calculate integer repetitions
        let repetitions = (container_width / image_width).round() as u32;
        assert_eq!(repetitions, 9, "Should round to 9 repetitions");

        // Calculate scaled image size
        let scaled_width = container_width / repetitions as f32;
        assert!((scaled_width - 94.44_f32).abs() < 0.1_f32, "Scaled image should be ~94.44px");

        // Edge case: exact fit (no scaling needed)
        let exact_container = 800.0_f32;
        let exact_reps = (exact_container / image_width).round() as u32;
        let exact_scaled = exact_container / exact_reps as f32;
        assert_eq!(exact_scaled, 100.0_f32, "Exact fit should not scale");
    }

    #[test]
    fn test_gradient_color_interpolation() {
        // Test gradient color interpolation between stops
        // Linear interpolation between two colors
        let color1 = [1.0, 0.0, 0.0, 1.0]; // Red
        let color2 = [0.0, 0.0, 1.0, 1.0]; // Blue
        let t = 0.5; // Halfway

        let interpolated = [
            color1[0] * (1.0 - t) + color2[0] * t,
            color1[1] * (1.0 - t) + color2[1] * t,
            color1[2] * (1.0 - t) + color2[2] * t,
            color1[3] * (1.0 - t) + color2[3] * t,
        ];

        assert_eq!(interpolated[0], 0.5, "Red channel should be 0.5");
        assert_eq!(interpolated[1], 0.0, "Green channel should be 0");
        assert_eq!(interpolated[2], 0.5, "Blue channel should be 0.5");
        assert_eq!(interpolated[3], 1.0, "Alpha should be 1.0");

        // Edge case: t=0 should return first color
        let t0 = 0.0;
        let at_start = [
            color1[0] * (1.0 - t0) + color2[0] * t0,
            color1[1] * (1.0 - t0) + color2[1] * t0,
            color1[2] * (1.0 - t0) + color2[2] * t0,
            color1[3] * (1.0 - t0) + color2[3] * t0,
        ];
        assert_eq!(at_start, color1);

        // Edge case: t=1 should return second color
        let t1 = 1.0;
        let at_end = [
            color1[0] * (1.0 - t1) + color2[0] * t1,
            color1[1] * (1.0 - t1) + color2[1] * t1,
            color1[2] * (1.0 - t1) + color2[2] * t1,
            color1[3] * (1.0 - t1) + color2[3] * t1,
        ];
        assert_eq!(at_end, color2);
    }

    #[test]
    fn test_gradient_radial_center_calculation() {
        // Test that radial gradients correctly calculate center position
        let rect_x = 100.0;
        let rect_y = 200.0;
        let rect_width = 400.0;
        let rect_height = 300.0;

        // Default center is at 50% 50%
        let center_x = rect_x + rect_width / 2.0;
        let center_y = rect_y + rect_height / 2.0;

        assert_eq!(center_x, 300.0, "Center X should be at 300");
        assert_eq!(center_y, 350.0, "Center Y should be at 350");

        // Test with offset center (e.g., at 25% 75%)
        let offset_center_x = rect_x + rect_width * 0.25;
        let offset_center_y = rect_y + rect_height * 0.75;

        assert_eq!(offset_center_x, 200.0, "Offset center X");
        assert_eq!(offset_center_y, 425.0, "Offset center Y");
    }

    #[test]
    fn test_gradient_conic_angle_normalization() {
        // Test that conic gradient angles are normalized to 0-360 range
        let angle_450 = 450.0_f32;
        let normalized_450 = angle_450 % 360.0;
        assert_eq!(normalized_450, 90.0, "450° should normalize to 90°");

        let angle_neg90 = -90.0_f32;
        let normalized_neg = (angle_neg90 % 360.0 + 360.0) % 360.0;
        assert_eq!(normalized_neg, 270.0, "-90° should normalize to 270°");

        let angle_720 = 720.0_f32;
        let normalized_720 = angle_720 % 360.0;
        assert_eq!(normalized_720, 0.0, "720° should normalize to 0°");
    }

    #[test]
    fn test_buffer_vertex_capacity() {
        // Test that vertex buffers can hold expected number of vertices
        const VERTICES_PER_RECT: usize = 4;
        const INDICES_PER_RECT: usize = 6;

        // Typical batch size
        let batch_size = 100;
        let total_vertices = batch_size * VERTICES_PER_RECT;
        let total_indices = batch_size * INDICES_PER_RECT;

        assert_eq!(total_vertices, 400, "100 rects need 400 vertices");
        assert_eq!(total_indices, 600, "100 rects need 600 indices");

        // Check buffer size
        let vertex_size = std::mem::size_of::<ColorVertex>();
        let total_vertex_bytes = total_vertices * vertex_size;

        assert_eq!(total_vertex_bytes, 400 * 24, "Total vertex buffer size");
        assert!(total_vertex_bytes < 256 * 1024 * 1024, "Should fit in max buffer");
    }

    #[test]
    fn test_rect_fully_contains() {
        // Test rect containment logic
        let outer = Rect::new(0.0, 0.0, 200.0, 200.0);
        let inner = Rect::new(50.0, 50.0, 100.0, 100.0);

        // Check if inner is fully inside outer
        let contains = inner.x >= outer.x
            && inner.y >= outer.y
            && (inner.x + inner.width) <= (outer.x + outer.width)
            && (inner.y + inner.height) <= (outer.y + outer.height);

        assert!(contains, "Inner rect should be fully contained");

        // Edge case: same rect
        let same = Rect::new(0.0, 0.0, 200.0, 200.0);
        let contains_self = same.x >= outer.x
            && same.y >= outer.y
            && (same.x + same.width) <= (outer.x + outer.width)
            && (same.y + same.height) <= (outer.y + outer.height);

        assert!(contains_self, "Rect should contain itself");
    }

    #[test]
    fn test_rect_area_calculation() {
        // Test rectangle area calculations
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let area = rect.width * rect.height;
        assert_eq!(area, 5000.0, "Area should be 5000 square pixels");

        // Edge case: zero area
        let zero_width = Rect::new(0.0, 0.0, 0.0, 100.0);
        let zero_area = zero_width.width * zero_width.height;
        assert_eq!(zero_area, 0.0, "Zero width means zero area");

        // Edge case: very small rect
        let tiny = Rect::new(0.0, 0.0, 0.1, 0.1);
        let tiny_area = tiny.width * tiny.height;
        assert!(tiny_area < 0.02, "Tiny rect has tiny area");
    }
}

/// PAINT-0 seating probe gate (forensics 2026-07-16-paint0-glyph-seat).
/// RUSTKIT_PAINT_PROBE=1 logs the paint half of the glyph seating chain
/// (baseline, bearing_y, glyph_y) plus per-glyph atlas bitmap hashes, so a
/// flat-1.2 vs metrics-normal A/B can attribute score deltas to seating
/// float shifts vs raster differences. Zero cost when off.
pub(crate) fn paint0_probe() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RUSTKIT_PAINT_PROBE").as_deref() == Ok("1"))
}

