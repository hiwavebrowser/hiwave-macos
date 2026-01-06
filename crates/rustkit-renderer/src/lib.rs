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
                // For now, fall back to solid rect
                // TODO: Implement proper rounded rect rendering with bezier curves
                if radius.is_zero() {
                    self.draw_solid_rect(*rect, *color);
                } else {
                    // Draw a solid rect (rounded corners require more complex rendering)
                    self.draw_solid_rect(*rect, *color);
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

            DisplayCommand::LinearGradient { rect, direction, stops } => {
                self.draw_linear_gradient(*rect, *direction, stops);
            }

            DisplayCommand::RadialGradient { rect, shape, size, center, stops } => {
                self.draw_radial_gradient(*rect, *shape, *size, *center, stops);
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

        self.color_vertices.extend_from_slice(&[
            ColorVertex { position: [rect.x, rect.y], color: c },
            ColorVertex { position: [rect.x + rect.width, rect.y], color: c },
            ColorVertex { position: [rect.x + rect.width, rect.y + rect.height], color: c },
            ColorVertex { position: [rect.x, rect.y + rect.height], color: c },
        ]);

        self.color_indices.extend_from_slice(&[
            base, base + 1, base + 2,
            base, base + 2, base + 3,
        ]);
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
    ) {
        if stops.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        
        // Convert direction to angle in radians
        let angle_deg = direction.to_degrees();
        let angle_rad = angle_deg.to_radians();
        
        // Calculate gradient line length based on angle
        let (dx, dy) = (angle_rad.sin(), -angle_rad.cos());
        
        // Determine number of steps based on gradient length
        let gradient_length = (rect.width * dx.abs() + rect.height * dy.abs()).max(1.0);
        let step_count = (gradient_length / 2.0).max(2.0) as usize;
        
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
        
        // Draw gradient as multiple thin rectangles
        let step_size = 1.0 / step_count as f32;
        for i in 0..step_count {
            let t = (i as f32 + 0.5) * step_size;
            
            // Find the two color stops surrounding this position
            let color = Self::interpolate_color(&normalized_stops, t);
            
            // Calculate the strip position based on angle
            let strip_rect = if (angle_deg - 90.0).abs() < 0.1 || (angle_deg - 270.0).abs() < 0.1 {
                // Horizontal gradient (left to right or right to left)
                let strip_width = rect.width / step_count as f32;
                let x_pos = if angle_deg < 180.0 {
                    rect.x + i as f32 * strip_width
                } else {
                    rect.x + rect.width - (i + 1) as f32 * strip_width
                };
                Rect::new(x_pos, rect.y, strip_width + 0.5, rect.height)
            } else if angle_deg.abs() < 0.1 || (angle_deg - 180.0).abs() < 0.1 {
                // Vertical gradient (top to bottom or bottom to top)
                let strip_height = rect.height / step_count as f32;
                let y_pos = if angle_deg < 90.0 || angle_deg > 270.0 {
                    rect.y + rect.height - (i + 1) as f32 * strip_height
                } else {
                    rect.y + i as f32 * strip_height
                };
                Rect::new(rect.x, y_pos, rect.width, strip_height + 0.5)
            } else {
                // Diagonal gradient - simplified as vertical strips
                let strip_width = rect.width / step_count as f32;
                let x_pos = rect.x + i as f32 * strip_width;
                Rect::new(x_pos, rect.y, strip_width + 0.5, rect.height)
            };
            
            self.draw_solid_rect(strip_rect, color);
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
            rustkit_css::RadialSize::ClosestCorner | rustkit_css::RadialSize::FarthestCorner => {
                // Simplified: use diagonal distance
                let dx = rect.width * 0.5;
                let dy = rect.height * 0.5;
                ((dx * dx + dy * dy).sqrt(), (dx * dx + dy * dy).sqrt())
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
        
        // Draw as concentric rings
        let max_radius = rx.max(ry);
        let ring_count = (max_radius / 4.0).max(8.0) as usize;
        
        for i in (0..ring_count).rev() {
            let t = i as f32 / ring_count as f32;
            let color = Self::interpolate_color(&normalized_stops, t);
            
            let ring_rx = rx * (t + 1.0 / ring_count as f32);
            let ring_ry = ry * (t + 1.0 / ring_count as f32);
            
            // Draw as an axis-aligned ellipse (approximated as rect for now)
            let ring_rect = Rect::new(
                cx - ring_rx,
                cy - ring_ry,
                ring_rx * 2.0,
                ring_ry * 2.0,
            );
            
            // Clip to original rect
            let clipped = Rect::new(
                ring_rect.x.max(rect.x),
                ring_rect.y.max(rect.y),
                (ring_rect.x + ring_rect.width).min(rect.x + rect.width) - ring_rect.x.max(rect.x),
                (ring_rect.y + ring_rect.height).min(rect.y + rect.height) - ring_rect.y.max(rect.y),
            );
            
            if clipped.width > 0.0 && clipped.height > 0.0 {
                self.draw_solid_rect(clipped, color);
            }
        }
    }
    
    /// Interpolate between color stops.
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
                    ((1.0 - local_t) * color0.r as f32 + local_t * color1.r as f32) as u8,
                    ((1.0 - local_t) * color0.g as f32 + local_t * color1.g as f32) as u8,
                    ((1.0 - local_t) * color0.b as f32 + local_t * color1.b as f32) as u8,
                    (1.0 - local_t) * color0.a + local_t * color1.a,
                );
            }
        }
        stops[stops.len() - 1].1
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

                let base = self.texture_vertices.len() as u32;

                self.texture_vertices.extend_from_slice(&[
                    TextureVertex {
                        position: [glyph_x, glyph_y],
                        tex_coords: [entry.tex_coords[0], entry.tex_coords[1]],
                        color: c,
                    },
                    TextureVertex {
                        position: [glyph_x + glyph_w, glyph_y],
                        tex_coords: [entry.tex_coords[2], entry.tex_coords[1]],
                        color: c,
                    },
                    TextureVertex {
                        position: [glyph_x + glyph_w, glyph_y + glyph_h],
                        tex_coords: [entry.tex_coords[2], entry.tex_coords[3]],
                        color: c,
                    },
                    TextureVertex {
                        position: [glyph_x, glyph_y + glyph_h],
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
            let base = self.texture_vertices.len() as u32;

            self.texture_vertices.extend_from_slice(&[
                TextureVertex {
                    position: [rect.x, rect.y],
                    tex_coords: [0.0, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                TextureVertex {
                    position: [rect.x + rect.width, rect.y],
                    tex_coords: [1.0, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                TextureVertex {
                    position: [rect.x + rect.width, rect.y + rect.height],
                    tex_coords: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                },
                TextureVertex {
                    position: [rect.x, rect.y + rect.height],
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

