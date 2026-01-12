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
    _texture_bind_group_layout: wgpu::BindGroupLayout,
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

        // Create caches
        let texture_cache = TextureCache::new(&device, texture_bind_group_layout.clone());
        let glyph_cache = GlyphCache::new(&device, &queue, texture_bind_group_layout.clone())?;

        Ok(Self {
            device,
            queue,
            color_pipeline,
            texture_pipeline,
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
            _texture_bind_group_layout: texture_bind_group_layout,
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

        // Process commands
        for cmd in commands {
            self.process_command(cmd);
        }

        // Render
        self.flush_to(target)?;

        Ok(())
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

            DisplayCommand::LinearGradient { rect, direction, stops, repeating } => {
                self.draw_linear_gradient(*rect, *direction, stops, *repeating);
            }

            DisplayCommand::RadialGradient { rect, shape, size, center, stops, repeating } => {
                self.draw_radial_gradient(*rect, *shape, *size, *center, stops, *repeating);
            }

            DisplayCommand::ConicGradient { rect, from_angle, center, stops, repeating } => {
                self.draw_conic_gradient(*rect, *from_angle, *center, stops, *repeating);
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

    /// Draw a linear gradient.
    fn draw_linear_gradient(
        &mut self,
        rect: Rect,
        direction: rustkit_css::GradientDirection,
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Convert direction to angle in radians
        let angle_deg = direction.to_degrees();
        let angle_rad = angle_deg.to_radians();

        // Calculate gradient direction vector
        let (sin_a, cos_a) = (angle_rad.sin(), angle_rad.cos());

        // Normalize color stop positions
        let mut normalized_stops: Vec<(f32, Color)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = stop.position.unwrap_or_else(|| {
                if stops.len() == 1 {
                    0.5
                } else {
                    i as f32 / (stops.len() - 1) as f32
                }
            });
            normalized_stops.push((pos, stop.color));
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

        if is_horizontal {
            // Horizontal gradient (left to right or right to left)
            // Quality-first: 1px sampling for smooth gradients
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
                let color = Self::interpolate_color(&normalized_stops, t_final);
                let x_pos = rect.x + i as f32 * strip_width;
                self.draw_solid_rect(Rect::new(x_pos, rect.y, strip_width + 0.5, rect.height), color);
            }
        } else if is_vertical {
            // Vertical gradient (top to bottom or bottom to top)
            // Quality-first: 1px sampling for smooth gradients
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
                let color = Self::interpolate_color(&normalized_stops, t_final);
                let y_pos = rect.y + i as f32 * strip_height;
                self.draw_solid_rect(Rect::new(rect.x, y_pos, rect.width, strip_height + 0.5), color);
            }
        } else {
            // Diagonal gradient - render using cells for proper angular gradients
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

                    // Calculate position relative to center of rect
                    let px = x + cell_w / 2.0 - rect.x - half_width;
                    let py = y + cell_h / 2.0 - rect.y - half_height;

                    // Project point onto gradient line
                    // Gradient direction: (sin_a, -cos_a) where angle 0 is "to top"
                    let projection = px * sin_a + py * (-cos_a);

                    // Normalize to 0-1 range
                    let t = (projection / gradient_half_length + 1.0) / 2.0;
                    let t_final = apply_t(t);

                    let color = Self::interpolate_color(&normalized_stops, t_final);

                    if color.a > 0.0 {
                        self.draw_solid_rect(Rect::new(x, y, cell_w, cell_h), color);
                    }

                    x += cell_size;
                }
                y += cell_size;
            }
        }
    }
    
    /// Draw a radial gradient.
    fn draw_radial_gradient(
        &mut self,
        rect: Rect,
        shape: rustkit_css::RadialShape,
        size: rustkit_css::RadialSize,
        center: (f32, f32),
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
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
        
        // Normalize color stops
        let mut normalized_stops: Vec<(f32, Color)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = stop.position.unwrap_or_else(|| {
                if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
            });
            normalized_stops.push((pos, stop.color));
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

                // Calculate distance from center (normalized to ellipse)
                let dx = (x + col_width / 2.0 - cx) / rx.max(0.001);
                let dy = (y + row_height / 2.0 - cy) / ry.max(0.001);
                let t = (dx * dx + dy * dy).sqrt();

                // Apply repeating logic
                let t_final = if repeating {
                    t.rem_euclid(repeat_length)
                } else {
                    t.clamp(0.0, 1.0)
                };

                // Get color at this distance (gamma-correct)
                let color = Self::interpolate_color(&normalized_stops, t_final);

                // Only draw if not fully transparent
                if color.a > 0.0 {
                    self.draw_solid_rect(Rect::new(x, y, col_width, row_height), color);
                }

                x += step_size;
            }
            y += step_size;
        }
    }

    /// Draw a conic gradient.
    fn draw_conic_gradient(
        &mut self,
        rect: Rect,
        from_angle: f32,
        center: (f32, f32),
        stops: &[rustkit_css::ColorStop],
        repeating: bool,
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        // Calculate center position in pixels
        let cx = rect.x + rect.width * center.0;
        let cy = rect.y + rect.height * center.1;

        // Convert from_angle to radians (CSS conic gradients: 0deg = up, clockwise)
        let from_rad = (from_angle - 90.0).to_radians();

        // Normalize color stops
        let mut normalized_stops: Vec<(f32, Color)> = Vec::with_capacity(stops.len());
        for (i, stop) in stops.iter().enumerate() {
            let pos = stop.position.unwrap_or_else(|| {
                if stops.len() == 1 { 0.5 } else { i as f32 / (stops.len() - 1) as f32 }
            });
            normalized_stops.push((pos, stop.color));
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

                // Calculate angle from center
                let dx = x + col_width / 2.0 - cx;
                let dy = y + row_height / 2.0 - cy;
                let angle = dy.atan2(dx) - from_rad;

                // Normalize angle to 0-1 range
                let normalized_angle = ((angle + std::f32::consts::PI) / (2.0 * std::f32::consts::PI)) % 1.0;
                let raw_t = if normalized_angle < 0.0 { normalized_angle + 1.0 } else { normalized_angle };

                // Apply repeating logic
                let t = apply_t(raw_t);

                // Get color at this angle
                let color = Self::interpolate_color(&normalized_stops, t);

                if color.a > 0.0 {
                    self.draw_solid_rect(Rect::new(x, y, col_width, row_height), color);
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
    
    /// Interpolate between color stops with gamma-correct blending (linearRGB).
    /// CSS Images 4 uses this for modern browsers, but kept for future use.
    #[allow(dead_code)]
    fn interpolate_color_gamma(stops: &[(f32, Color)], t: f32) -> Color {
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
                
                // Convert to linear space (0-1 range)
                let r0 = Self::srgb_to_linear(color0.r as f32 / 255.0);
                let g0 = Self::srgb_to_linear(color0.g as f32 / 255.0);
                let b0 = Self::srgb_to_linear(color0.b as f32 / 255.0);
                
                let r1 = Self::srgb_to_linear(color1.r as f32 / 255.0);
                let g1 = Self::srgb_to_linear(color1.g as f32 / 255.0);
                let b1 = Self::srgb_to_linear(color1.b as f32 / 255.0);
                
                // Interpolate in linear space
                let r_lin = (1.0 - local_t) * r0 + local_t * r1;
                let g_lin = (1.0 - local_t) * g0 + local_t * g1;
                let b_lin = (1.0 - local_t) * b0 + local_t * b1;
                
                // Convert back to sRGB
                let r = (Self::linear_to_srgb(r_lin) * 255.0).round() as u8;
                let g = (Self::linear_to_srgb(g_lin) * 255.0).round() as u8;
                let b = (Self::linear_to_srgb(b_lin) * 255.0).round() as u8;
                
                // Alpha is linear
                let a = (1.0 - local_t) * color0.a + local_t * color1.a;
                
                return Color::new(r, g, b, a);
            }
        }
        stops[stops.len() - 1].1
    }
    
    /// Interpolate between color stops in sRGB space.
    /// This matches Chrome's gradient rendering for CSS Images Level 3 compatibility.
    fn interpolate_color(stops: &[(f32, Color)], t: f32) -> Color {
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
                return Color::new(
                    ((1.0 - local_t) * color0.r as f32 + local_t * color1.r as f32).round() as u8,
                    ((1.0 - local_t) * color0.g as f32 + local_t * color1.g as f32).round() as u8,
                    ((1.0 - local_t) * color0.b as f32 + local_t * color1.b as f32).round() as u8,
                    (1.0 - local_t) * color0.a + local_t * color1.a,
                );
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

