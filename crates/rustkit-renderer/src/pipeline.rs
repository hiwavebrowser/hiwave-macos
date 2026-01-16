//! Render pipeline creation.

use crate::{ColorVertex, TextureVertex};

/// Filter parameters uniform structure (must match WGSL).
/// Uses explicit padding to avoid vec3 alignment issues.
/// Total size: 32 bytes (8 f32 values)
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FilterParams {
    pub blur_radius: f32,
    pub filter_type: u32,
    pub filter_amount: f32,
    pub texture_width: f32,
    pub texture_height: f32,
    pub _padding0: f32,
    pub _padding1: f32,
    pub _padding2: f32,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            blur_radius: 0.0,
            filter_type: 0,
            filter_amount: 1.0,
            texture_width: 0.0,
            texture_height: 0.0,
            _padding0: 0.0,
            _padding1: 0.0,
            _padding2: 0.0,
        }
    }
}

/// Backdrop filter pipelines and resources.
pub struct BackdropFilterPipelines {
    pub blur_h_pipeline: wgpu::ComputePipeline,
    pub blur_v_pipeline: wgpu::ComputePipeline,
    pub color_filter_pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
}

/// Create the backdrop filter compute pipelines.
pub fn create_backdrop_filter_pipelines(device: &wgpu::Device) -> BackdropFilterPipelines {
    // Load the compute shader
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Backdrop Filter Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/backdrop_filter.wgsl").into()),
    });

    // Create bind group layout for filter operations
    // Binding 0: Uniform buffer with filter params
    // Binding 1: Input texture (read)
    // Binding 2: Output texture (write/storage)
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Backdrop Filter Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Backdrop Filter Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    // Create the three compute pipelines (blur horizontal, blur vertical, color filter)
    let blur_h_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Blur Horizontal Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("blur_horizontal"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let blur_v_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Blur Vertical Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("blur_vertical"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let color_filter_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Color Filter Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("apply_color_filter"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Create uniform buffer for filter parameters
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Filter Params Buffer"),
        size: std::mem::size_of::<FilterParams>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    BackdropFilterPipelines {
        blur_h_pipeline,
        blur_v_pipeline,
        color_filter_pipeline,
        bind_group_layout,
        uniform_buffer,
    }
}

/// Create the color rendering pipeline.
pub fn create_color_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    uniform_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Color Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/color.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Color Pipeline Layout"),
        bind_group_layouts: &[uniform_bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Color Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[ColorVertex::LAYOUT],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

/// Create the texture rendering pipeline.
pub fn create_texture_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    uniform_bind_group_layout: &wgpu::BindGroupLayout,
    texture_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Texture Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/texture.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Texture Pipeline Layout"),
        bind_group_layouts: &[uniform_bind_group_layout, texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Texture Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[TextureVertex::LAYOUT],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

/// Create the blit pipeline for copying RGBA textures.
/// Unlike the texture pipeline (for glyph rendering), this properly samples all 4 channels.
pub fn create_blit_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
    uniform_bind_group_layout: &wgpu::BindGroupLayout,
    texture_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Blit Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blit.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Blit Pipeline Layout"),
        bind_group_layouts: &[uniform_bind_group_layout, texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Blit Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[TextureVertex::LAYOUT],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                // Use REPLACE blend (no blending) for proper texture copying
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}
