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
use rustkit_layout::{DisplayCommand, Rect};
use std::sync::Arc;
use thiserror::Error;
use wgpu::util::DeviceExt;

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

    #[error("Texture upload failed: {0}")]
    TextureUpload(String),

    #[error("Glyph rasterization failed: {0}")]
    GlyphRasterization(String),

    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),
}

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
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
    texture_pipeline_rgba: wgpu::RenderPipeline,
    // Blit pipeline for copying RGBA textures (unlike texture_pipeline which treats R as alpha)
    blit_pipeline: wgpu::RenderPipeline,
    // Blit pipeline for Rgba8Unorm targets (for blitting to filter textures)
    blit_pipeline_rgba: wgpu::RenderPipeline,

    // Backdrop filter pipelines (compute shaders for blur + color filters)
    backdrop_filter_pipelines: pipeline::BackdropFilterPipelines,

    // Uniform buffer
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    viewport_size: (u32, u32),

    // Vertex batching
    color_vertices: Vec<ColorVertex>,
    color_indices: Vec<u32>,
    texture_vertices: Vec<TextureVertex>,
    texture_indices: Vec<u32>,

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

        // Create blit pipeline for Rgba8Unorm targets (blitting to filter textures)
        let blit_pipeline_rgba = pipeline::create_blit_pipeline(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &uniform_bind_group_layout,
            &texture_bind_group_layout,
        );

        // Create backdrop filter pipelines (compute shaders for blur + color filters)
        let backdrop_filter_pipelines = pipeline::create_backdrop_filter_pipelines(&device);

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

        Ok(Self {
            device,
            queue,
            color_pipeline,
            texture_pipeline,
            texture_pipeline_rgba,
            blit_pipeline,
            blit_pipeline_rgba,
            backdrop_filter_pipelines,
            uniform_buffer,
            uniform_bind_group,
            viewport_size: (800, 600),
            color_vertices: Vec::with_capacity(4096),
            color_indices: Vec::with_capacity(8192),
            texture_vertices: Vec::with_capacity(4096),
            texture_indices: Vec::with_capacity(8192),
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
    fn flush_batches_to(&mut self, target: &wgpu::TextureView, clear: bool) {
        if self.color_vertices.is_empty() && self.texture_vertices.is_empty() {
            return;
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

            // Draw textured quads
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
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Clear batches after flushing
        self.color_vertices.clear();
        self.color_indices.clear();
        self.texture_vertices.clear();
        self.texture_indices.clear();
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
        self.clip_stack.clear();
        self.stacking_contexts.clear();
        self.transform_stack.clear();

        // Check if there are any blur backdrop filters that need GPU processing
        let has_blur_filters = commands.iter().any(|cmd| {
            matches!(cmd, DisplayCommand::BackdropFilter {
                filter: rustkit_css::BackdropFilter::Blur(r), ..
            } if *r > 0.0)
        });

        if has_blur_filters {
            // Use GPU blur path - render to intermediate texture with GPU blur processing
            self.execute_with_gpu_blur(commands, target)?;
        } else {
            // Fast path - no backdrop blur, process normally
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
                    self.flush_batches_to(&intermediate_view, is_first_flush);
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
        if !self.color_vertices.is_empty() || !self.texture_vertices.is_empty() {
            self.flush_batches_to(&intermediate_view, is_first_flush);
        }

        // Copy intermediate to final target
        self.copy_texture_to_target(&intermediate_view, target);

        Ok(())
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
            } => {
                self.draw_text(
                    text,
                    *x,
                    *y,
                    *color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_style,
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
                size: _,
                position: _,
                repeat: _,
            } => {
                self.draw_image(url, *rect);
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
                // Approximate circle with a square for now
                // TODO: Implement proper circle rendering with triangles
                self.draw_solid_rect(
                    Rect::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0),
                    *color,
                );
            }

            DisplayCommand::StrokeCircle { cx, cy, radius, color, width } => {
                // Approximate with a square border
                let outer = Rect::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0);
                self.draw_border(outer, *color, *width, *width, *width, *width);
            }

            DisplayCommand::FillEllipse { rect, color } => {
                // Approximate with rectangle
                self.draw_solid_rect(*rect, *color);
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
                gradient: _,
                rect: _,
            } => {
                // For now, render gradient text as regular text with a fallback color
                // Full gradient text implementation would require:
                // 1. Render text to an offscreen alpha mask
                // 2. Use the mask to clip a gradient fill
                // TODO: Implement proper gradient text masking
                
                // Use a visible fallback color (gradient's first color stop or magenta for debugging)
                let fallback_color = Color::new(128, 0, 255, 1.0); // Purple as fallback
                
                self.draw_text(
                    text,
                    *x,
                    *y,
                    fallback_color,
                    *font_size,
                    font_family,
                    *font_weight,
                    *font_style,
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

    /// Check if a point is inside a rounded rectangle and return the alpha coverage.
    /// Returns 1.0 if fully inside, 0.0 if fully outside, values in between for AA at corners.
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

        // Top-left corner
        if local_x < r_tl && local_y < r_tl {
            let dx = r_tl - local_x;
            let dy = r_tl - local_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > r_tl + 0.5 {
                return 0.0;
            } else if dist > r_tl - 0.5 {
                return 1.0 - (dist - (r_tl - 0.5));
            }
        }

        // Top-right corner
        if right_x < r_tr && local_y < r_tr {
            let dx = r_tr - right_x;
            let dy = r_tr - local_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > r_tr + 0.5 {
                return 0.0;
            } else if dist > r_tr - 0.5 {
                return 1.0 - (dist - (r_tr - 0.5));
            }
        }

        // Bottom-right corner
        if right_x < r_br && bottom_y < r_br {
            let dx = r_br - right_x;
            let dy = r_br - bottom_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > r_br + 0.5 {
                return 0.0;
            } else if dist > r_br - 0.5 {
                return 1.0 - (dist - (r_br - 0.5));
            }
        }

        // Bottom-left corner
        if local_x < r_bl && bottom_y < r_bl {
            let dx = r_bl - local_x;
            let dy = r_bl - bottom_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > r_bl + 0.5 {
                return 0.0;
            } else if dist > r_bl - 0.5 {
                return 1.0 - (dist - (r_bl - 0.5));
            }
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

        // Normalize color stop positions using high-precision colors
        let mut normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = stop.position.unwrap_or_else(|| {
                if stops.len() == 1 {
                    0.5
                } else {
                    i as f32 / (stops.len() - 1) as f32
                }
            });
            normalized_stops.push((pos, rustkit_css::ColorF32::from_color(stop.color)));
        }

        // For repeating gradients, get the repeat length (last stop position)
        let repeat_length = if repeating && !normalized_stops.is_empty() {
            normalized_stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

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

            for i in 0..step_count {
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

            for i in 0..step_count {
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
            // Diagonal gradient or gradient with border-radius - render using cells
            // Use adaptive cell sizing to prevent GPU buffer overflow for large gradients
            // while maintaining 1px quality for small UI elements
            let area = rect.width * rect.height;
            let max_cells: f32 = 100_000.0; // Limit cells to prevent buffer overflow
            let cell_size: f32 = if area > max_cells {
                (area / max_cells).sqrt().ceil()
            } else {
                1.0
            };

            // Calculate the gradient line length (diagonal of rectangle projected onto gradient line)
            // For CSS gradients, the gradient line goes through the center and extends to the corners
            let half_width = rect.width / 2.0;
            let half_height = rect.height / 2.0;

            // The gradient length is the distance along the gradient line from corner to corner
            let gradient_half_length = (half_width * sin_a.abs() + half_height * cos_a.abs()).max(0.001);

            let mut y = rect.y;
            while y < rect.y + rect.height {
                let cell_h = cell_size.min(rect.y + rect.height - y);
                let mut x = rect.x;

                while x < rect.x + rect.width {
                    let cell_w = cell_size.min(rect.x + rect.width - x);
                    let cell_center_x = x + cell_w / 2.0;
                    let cell_center_y = y + cell_h / 2.0;

                    // Check border-radius clipping
                    let alpha_coverage = Self::point_in_rounded_rect(
                        cell_center_x,
                        cell_center_y,
                        rect,
                        border_radius,
                    );

                    if alpha_coverage > 0.0 {
                        // Calculate position relative to center of rect
                        let px = cell_center_x - rect.x - half_width;
                        let py = cell_center_y - rect.y - half_height;

                        // Project point onto gradient line
                        // Gradient direction: (sin_a, -cos_a) where angle 0 is "to top"
                        let projection = px * sin_a + py * (-cos_a);

                        // Normalize to 0-1 range
                        let t = (projection / gradient_half_length + 1.0) / 2.0;
                        let t_final = apply_t(t);

                        let mut color = Self::interpolate_color_f32(&normalized_stops, t_final);

                        // Apply border-radius alpha
                        if alpha_coverage < 1.0 {
                            color = rustkit_css::ColorF32::new(color.r, color.g, color.b, color.a * alpha_coverage);
                        }

                        if color.a > 0.0 {
                            self.draw_solid_rect_f32(Rect::new(x, y, cell_w, cell_h), color);
                        }
                    }

                    x += cell_size;
                }
                y += cell_size;
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
                        let aspect = rect.width / rect.height.max(1.0);
                        (min_dist, min_dist / aspect)
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
                        let aspect = rect.width / rect.height.max(1.0);
                        (max_dist, max_dist / aspect)
                    }
                }
            }
            rustkit_css::RadialSize::Explicit(r1, r2) => (r1, r2),
        };

        // Normalize color stops using high-precision colors
        let mut normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = stop.position.unwrap_or_else(|| {
                if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
            });
            normalized_stops.push((pos, rustkit_css::ColorF32::from_color(stop.color)));
        }

        // For repeating gradients, get the repeat length (last stop position)
        let repeat_length = if repeating && !normalized_stops.is_empty() {
            normalized_stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

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
        let mut normalized_stops: Vec<(f32, rustkit_css::ColorF32)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = stop.position.unwrap_or_else(|| {
                if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
            });
            normalized_stops.push((pos, rustkit_css::ColorF32::from_color(stop.color)));
        }

        // For repeating gradients, get the repeat length from the last stop
        let repeat_length = if repeating && !normalized_stops.is_empty() {
            normalized_stops.last().map(|(pos, _)| *pos).unwrap_or(1.0).max(0.001)
        } else {
            1.0
        };

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

                // Direct f32 interpolation - no quantization
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
        let mut cursor_x = x;
        let c = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a,
        ];

        // Get atlas size before the loop to avoid borrow issues
        let atlas_size = self.glyph_cache.atlas_size() as f32;

        for ch in text.chars() {
            let key = GlyphKey {
                codepoint: ch,
                font_family: font_family.to_string(),
                font_size: (font_size * 10.0) as u32,
                font_weight,
                font_style,
            };

            // Clone the entry to avoid borrow issues
            if let Some(entry) = self.glyph_cache.get_or_rasterize(&self.device, &self.queue, &key) {
                let glyph_x = cursor_x + entry.offset[0];
                let glyph_y = y + entry.offset[1];
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

                cursor_x += entry.advance;
            } else {
                // Fallback: advance by estimated width
                cursor_x += font_size * 0.6;
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

            let base = self.texture_vertices.len() as u32;

            self.texture_vertices.extend_from_slice(&[
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
            ]);

            self.texture_indices.extend_from_slice(&[
                base, base + 1, base + 2,
                base, base + 2, base + 3,
            ]);
        }
        // If image not loaded, skip (async loading handled elsewhere)
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
    fn flush_to(&mut self, target: &wgpu::TextureView) -> Result<(), RendererError> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

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

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
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

            // Draw textured quads (images and glyphs)
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
        }

        self.queue.submit(std::iter::once(encoder.finish()));

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
}

