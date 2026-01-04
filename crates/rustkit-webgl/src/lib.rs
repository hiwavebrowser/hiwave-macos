//! # RustKit WebGL
//!
//! WebGL API implementation for the RustKit browser engine.
//!
//! ## Features
//!
//! - **WebGLRenderingContext**: WebGL 1.0 context
//! - **WebGL2RenderingContext**: WebGL 2.0 extensions
//! - **Shaders**: Compile and link GLSL shaders
//! - **Buffers**: Vertex and index buffers
//! - **Textures**: 2D and cubemap textures
//! - **Framebuffers**: Off-screen rendering
//!
//! ## Architecture
//!
//! This module provides the WebGL API surface. Actual GPU rendering
//! is delegated to rustkit-compositor which uses wgpu.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use thiserror::Error;

// ==================== Errors ====================

/// Errors that can occur in WebGL operations.
#[derive(Error, Debug, Clone)]
pub enum WebGLError {
    #[error("Invalid operation")]
    InvalidOperation,

    #[error("Invalid value")]
    InvalidValue,

    #[error("Invalid enum")]
    InvalidEnum,

    #[error("Out of memory")]
    OutOfMemory,

    #[error("Context lost")]
    ContextLost,

    #[error("Shader compile error: {0}")]
    ShaderCompileError(String),

    #[error("Program link error: {0}")]
    ProgramLinkError(String),
}

// ==================== Constants ====================

/// WebGL constants (matches OpenGL ES 2.0).
pub mod constants {
    // Clear bits
    pub const COLOR_BUFFER_BIT: u32 = 0x00004000;
    pub const DEPTH_BUFFER_BIT: u32 = 0x00000100;
    pub const STENCIL_BUFFER_BIT: u32 = 0x00000400;

    // Primitive types
    pub const POINTS: u32 = 0x0000;
    pub const LINES: u32 = 0x0001;
    pub const LINE_LOOP: u32 = 0x0002;
    pub const LINE_STRIP: u32 = 0x0003;
    pub const TRIANGLES: u32 = 0x0004;
    pub const TRIANGLE_STRIP: u32 = 0x0005;
    pub const TRIANGLE_FAN: u32 = 0x0006;

    // Buffer types
    pub const ARRAY_BUFFER: u32 = 0x8892;
    pub const ELEMENT_ARRAY_BUFFER: u32 = 0x8893;

    // Buffer usage
    pub const STATIC_DRAW: u32 = 0x88E4;
    pub const DYNAMIC_DRAW: u32 = 0x88E8;
    pub const STREAM_DRAW: u32 = 0x88E0;

    // Data types
    pub const BYTE: u32 = 0x1400;
    pub const UNSIGNED_BYTE: u32 = 0x1401;
    pub const SHORT: u32 = 0x1402;
    pub const UNSIGNED_SHORT: u32 = 0x1403;
    pub const INT: u32 = 0x1404;
    pub const UNSIGNED_INT: u32 = 0x1405;
    pub const FLOAT: u32 = 0x1406;

    // Shader types
    pub const VERTEX_SHADER: u32 = 0x8B31;
    pub const FRAGMENT_SHADER: u32 = 0x8B30;

    // Shader parameters
    pub const COMPILE_STATUS: u32 = 0x8B81;
    pub const LINK_STATUS: u32 = 0x8B82;
    pub const DELETE_STATUS: u32 = 0x8B80;

    // Texture targets
    pub const TEXTURE_2D: u32 = 0x0DE1;
    pub const TEXTURE_CUBE_MAP: u32 = 0x8513;

    // Texture parameters
    pub const TEXTURE_MAG_FILTER: u32 = 0x2800;
    pub const TEXTURE_MIN_FILTER: u32 = 0x2801;
    pub const TEXTURE_WRAP_S: u32 = 0x2802;
    pub const TEXTURE_WRAP_T: u32 = 0x2803;

    // Texture filter values
    pub const NEAREST: u32 = 0x2600;
    pub const LINEAR: u32 = 0x2601;
    pub const NEAREST_MIPMAP_NEAREST: u32 = 0x2700;
    pub const LINEAR_MIPMAP_NEAREST: u32 = 0x2701;
    pub const NEAREST_MIPMAP_LINEAR: u32 = 0x2702;
    pub const LINEAR_MIPMAP_LINEAR: u32 = 0x2703;

    // Texture wrap values
    pub const REPEAT: u32 = 0x2901;
    pub const CLAMP_TO_EDGE: u32 = 0x812F;
    pub const MIRRORED_REPEAT: u32 = 0x8370;

    // Texture formats
    pub const RGB: u32 = 0x1907;
    pub const RGBA: u32 = 0x1908;
    pub const ALPHA: u32 = 0x1906;
    pub const LUMINANCE: u32 = 0x1909;
    pub const LUMINANCE_ALPHA: u32 = 0x190A;

    // Blend functions
    pub const ZERO: u32 = 0;
    pub const ONE: u32 = 1;
    pub const SRC_COLOR: u32 = 0x0300;
    pub const ONE_MINUS_SRC_COLOR: u32 = 0x0301;
    pub const SRC_ALPHA: u32 = 0x0302;
    pub const ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
    pub const DST_ALPHA: u32 = 0x0304;
    pub const ONE_MINUS_DST_ALPHA: u32 = 0x0305;
    pub const DST_COLOR: u32 = 0x0306;
    pub const ONE_MINUS_DST_COLOR: u32 = 0x0307;

    // Blend equations
    pub const FUNC_ADD: u32 = 0x8006;
    pub const FUNC_SUBTRACT: u32 = 0x800A;
    pub const FUNC_REVERSE_SUBTRACT: u32 = 0x800B;

    // Depth test functions
    pub const NEVER: u32 = 0x0200;
    pub const LESS: u32 = 0x0201;
    pub const EQUAL: u32 = 0x0202;
    pub const LEQUAL: u32 = 0x0203;
    pub const GREATER: u32 = 0x0204;
    pub const NOTEQUAL: u32 = 0x0205;
    pub const GEQUAL: u32 = 0x0206;
    pub const ALWAYS: u32 = 0x0207;

    // Capabilities
    pub const BLEND: u32 = 0x0BE2;
    pub const CULL_FACE: u32 = 0x0B44;
    pub const DEPTH_TEST: u32 = 0x0B71;
    pub const DITHER: u32 = 0x0BD0;
    pub const POLYGON_OFFSET_FILL: u32 = 0x8037;
    pub const SAMPLE_ALPHA_TO_COVERAGE: u32 = 0x809E;
    pub const SAMPLE_COVERAGE: u32 = 0x80A0;
    pub const SCISSOR_TEST: u32 = 0x0C11;
    pub const STENCIL_TEST: u32 = 0x0B90;

    // Face culling
    pub const FRONT: u32 = 0x0404;
    pub const BACK: u32 = 0x0405;
    pub const FRONT_AND_BACK: u32 = 0x0408;

    // Error codes
    pub const NO_ERROR: u32 = 0;
    pub const INVALID_ENUM: u32 = 0x0500;
    pub const INVALID_VALUE: u32 = 0x0501;
    pub const INVALID_OPERATION: u32 = 0x0502;
    pub const OUT_OF_MEMORY: u32 = 0x0505;

    // Framebuffer
    pub const FRAMEBUFFER: u32 = 0x8D40;
    pub const RENDERBUFFER: u32 = 0x8D41;
    pub const COLOR_ATTACHMENT0: u32 = 0x8CE0;
    pub const DEPTH_ATTACHMENT: u32 = 0x8D00;
    pub const STENCIL_ATTACHMENT: u32 = 0x8D20;
    pub const DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
    pub const FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
}

// ==================== Object IDs ====================

/// WebGL object handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WebGLObject(u32);

impl WebGLObject {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    pub fn id(&self) -> u32 {
        self.0
    }

    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

pub type WebGLBuffer = WebGLObject;
pub type WebGLShader = WebGLObject;
pub type WebGLProgram = WebGLObject;
pub type WebGLTexture = WebGLObject;
pub type WebGLFramebuffer = WebGLObject;
pub type WebGLRenderbuffer = WebGLObject;
pub type WebGLUniformLocation = WebGLObject;

// ==================== Shader ====================

/// Shader data.
#[derive(Debug, Clone)]
pub struct ShaderData {
    pub shader_type: u32,
    pub source: String,
    pub compiled: bool,
    pub info_log: String,
    pub deleted: bool,
}

impl ShaderData {
    pub fn new(shader_type: u32) -> Self {
        Self {
            shader_type,
            source: String::new(),
            compiled: false,
            info_log: String::new(),
            deleted: false,
        }
    }
}

// ==================== Program ====================

/// Program data.
#[derive(Debug, Clone)]
pub struct ProgramData {
    pub vertex_shader: Option<WebGLShader>,
    pub fragment_shader: Option<WebGLShader>,
    pub linked: bool,
    pub info_log: String,
    pub deleted: bool,
    pub attributes: HashMap<String, AttributeInfo>,
    pub uniforms: HashMap<String, UniformInfo>,
}

impl ProgramData {
    pub fn new() -> Self {
        Self {
            vertex_shader: None,
            fragment_shader: None,
            linked: false,
            info_log: String::new(),
            deleted: false,
            attributes: HashMap::new(),
            uniforms: HashMap::new(),
        }
    }
}

impl Default for ProgramData {
    fn default() -> Self {
        Self::new()
    }
}

/// Attribute info.
#[derive(Debug, Clone)]
pub struct AttributeInfo {
    pub name: String,
    pub location: i32,
    pub size: i32,
    pub type_: u32,
}

/// Uniform info.
#[derive(Debug, Clone)]
pub struct UniformInfo {
    pub name: String,
    pub location: WebGLUniformLocation,
    pub size: i32,
    pub type_: u32,
}

// ==================== Buffer ====================

/// Buffer data.
#[derive(Debug, Clone)]
pub struct BufferData {
    pub target: u32,
    pub usage: u32,
    pub data: Vec<u8>,
    pub deleted: bool,
}

impl BufferData {
    pub fn new() -> Self {
        Self {
            target: 0,
            usage: constants::STATIC_DRAW,
            data: Vec::new(),
            deleted: false,
        }
    }
}

impl Default for BufferData {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Texture ====================

/// Texture data.
#[derive(Debug, Clone)]
pub struct TextureData {
    pub target: u32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub internal_format: u32,
    pub data: Vec<u8>,
    pub mag_filter: u32,
    pub min_filter: u32,
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub deleted: bool,
}

impl TextureData {
    pub fn new() -> Self {
        Self {
            target: constants::TEXTURE_2D,
            width: 0,
            height: 0,
            format: constants::RGBA,
            internal_format: constants::RGBA,
            data: Vec::new(),
            mag_filter: constants::LINEAR,
            min_filter: constants::NEAREST_MIPMAP_LINEAR,
            wrap_s: constants::REPEAT,
            wrap_t: constants::REPEAT,
            deleted: false,
        }
    }
}

impl Default for TextureData {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Framebuffer ====================

/// Framebuffer data.
#[derive(Debug, Clone)]
pub struct FramebufferData {
    pub color_attachment: Option<WebGLTexture>,
    pub depth_attachment: Option<WebGLRenderbuffer>,
    pub stencil_attachment: Option<WebGLRenderbuffer>,
    pub deleted: bool,
}

impl FramebufferData {
    pub fn new() -> Self {
        Self {
            color_attachment: None,
            depth_attachment: None,
            stencil_attachment: None,
            deleted: false,
        }
    }
}

impl Default for FramebufferData {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Renderbuffer ====================

/// Renderbuffer data.
#[derive(Debug, Clone)]
pub struct RenderbufferData {
    pub internal_format: u32,
    pub width: u32,
    pub height: u32,
    pub deleted: bool,
}

impl RenderbufferData {
    pub fn new() -> Self {
        Self {
            internal_format: 0,
            width: 0,
            height: 0,
            deleted: false,
        }
    }
}

impl Default for RenderbufferData {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== Vertex Attribute ====================

/// Vertex attribute pointer.
#[derive(Debug, Clone, Default)]
pub struct VertexAttribPointer {
    pub enabled: bool,
    pub size: i32,
    pub type_: u32,
    pub normalized: bool,
    pub stride: i32,
    pub offset: i32,
    pub buffer: Option<WebGLBuffer>,
}

// ==================== Context State ====================

/// WebGL context state.
#[derive(Debug, Clone)]
pub struct WebGLState {
    pub clear_color: [f32; 4],
    pub clear_depth: f32,
    pub clear_stencil: i32,
    pub viewport: [i32; 4],
    pub scissor: [i32; 4],
    pub blend_enabled: bool,
    pub blend_src_rgb: u32,
    pub blend_dst_rgb: u32,
    pub blend_src_alpha: u32,
    pub blend_dst_alpha: u32,
    pub blend_equation_rgb: u32,
    pub blend_equation_alpha: u32,
    pub cull_face_enabled: bool,
    pub cull_face_mode: u32,
    pub depth_test_enabled: bool,
    pub depth_func: u32,
    pub depth_mask: bool,
    pub stencil_test_enabled: bool,
    pub scissor_test_enabled: bool,
    pub front_face: u32,
    pub line_width: f32,
    pub polygon_offset_fill: bool,
    pub polygon_offset_factor: f32,
    pub polygon_offset_units: f32,
    pub current_program: Option<WebGLProgram>,
    pub current_array_buffer: Option<WebGLBuffer>,
    pub current_element_buffer: Option<WebGLBuffer>,
    pub current_framebuffer: Option<WebGLFramebuffer>,
    pub current_renderbuffer: Option<WebGLRenderbuffer>,
    pub active_texture: u32,
    pub texture_units: Vec<Option<WebGLTexture>>,
    pub vertex_attribs: Vec<VertexAttribPointer>,
}

impl Default for WebGLState {
    fn default() -> Self {
        Self {
            clear_color: [0.0, 0.0, 0.0, 0.0],
            clear_depth: 1.0,
            clear_stencil: 0,
            viewport: [0, 0, 0, 0],
            scissor: [0, 0, 0, 0],
            blend_enabled: false,
            blend_src_rgb: constants::ONE,
            blend_dst_rgb: constants::ZERO,
            blend_src_alpha: constants::ONE,
            blend_dst_alpha: constants::ZERO,
            blend_equation_rgb: constants::FUNC_ADD,
            blend_equation_alpha: constants::FUNC_ADD,
            cull_face_enabled: false,
            cull_face_mode: constants::BACK,
            depth_test_enabled: false,
            depth_func: constants::LESS,
            depth_mask: true,
            stencil_test_enabled: false,
            scissor_test_enabled: false,
            front_face: 0x0901, // CCW
            line_width: 1.0,
            polygon_offset_fill: false,
            polygon_offset_factor: 0.0,
            polygon_offset_units: 0.0,
            current_program: None,
            current_array_buffer: None,
            current_element_buffer: None,
            current_framebuffer: None,
            current_renderbuffer: None,
            active_texture: 0,
            texture_units: vec![None; 32],
            vertex_attribs: (0..16).map(|_| VertexAttribPointer::default()).collect(),
        }
    }
}

// ==================== Draw Call ====================

/// A recorded draw call.
#[derive(Debug, Clone)]
pub enum DrawCall {
    Clear {
        mask: u32,
        color: [f32; 4],
        depth: f32,
        stencil: i32,
    },
    DrawArrays {
        mode: u32,
        first: i32,
        count: i32,
        program: WebGLProgram,
        state: Box<DrawState>,
    },
    DrawElements {
        mode: u32,
        count: i32,
        type_: u32,
        offset: i32,
        program: WebGLProgram,
        state: Box<DrawState>,
    },
}

/// State needed for a draw call.
#[derive(Debug, Clone)]
pub struct DrawState {
    pub viewport: [i32; 4],
    pub blend_enabled: bool,
    pub blend_func: (u32, u32, u32, u32),
    pub depth_test_enabled: bool,
    pub depth_func: u32,
    pub cull_face_enabled: bool,
    pub cull_face_mode: u32,
    pub vertex_attribs: Vec<VertexAttribPointer>,
    pub uniforms: HashMap<WebGLUniformLocation, UniformValue>,
}

/// Uniform value.
#[derive(Debug, Clone)]
pub enum UniformValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Int(i32),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    Mat2([f32; 4]),
    Mat3([f32; 9]),
    Mat4([f32; 16]),
    Sampler(i32),
}

// ==================== WebGL Context ====================

/// The WebGL rendering context.
#[derive(Debug)]
pub struct WebGLRenderingContext {
    pub width: u32,
    pub height: u32,
    state: WebGLState,
    shaders: HashMap<WebGLShader, ShaderData>,
    programs: HashMap<WebGLProgram, ProgramData>,
    buffers: HashMap<WebGLBuffer, BufferData>,
    textures: HashMap<WebGLTexture, TextureData>,
    framebuffers: HashMap<WebGLFramebuffer, FramebufferData>,
    renderbuffers: HashMap<WebGLRenderbuffer, RenderbufferData>,
    uniform_values: HashMap<WebGLUniformLocation, UniformValue>,
    draw_calls: Vec<DrawCall>,
    last_error: u32,
}

impl WebGLRenderingContext {
    /// Create a new WebGL context.
    pub fn new(width: u32, height: u32) -> Self {
        let mut state = WebGLState::default();
        state.viewport = [0, 0, width as i32, height as i32];
        state.scissor = [0, 0, width as i32, height as i32];

        Self {
            width,
            height,
            state,
            shaders: HashMap::new(),
            programs: HashMap::new(),
            buffers: HashMap::new(),
            textures: HashMap::new(),
            framebuffers: HashMap::new(),
            renderbuffers: HashMap::new(),
            uniform_values: HashMap::new(),
            draw_calls: Vec::new(),
            last_error: constants::NO_ERROR,
        }
    }

    /// Resize the context.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.state.viewport = [0, 0, width as i32, height as i32];
    }

    /// Get and clear the error.
    pub fn get_error(&mut self) -> u32 {
        let err = self.last_error;
        self.last_error = constants::NO_ERROR;
        err
    }

    // ==================== State ====================

    /// Clear buffers.
    pub fn clear(&mut self, mask: u32) {
        self.draw_calls.push(DrawCall::Clear {
            mask,
            color: self.state.clear_color,
            depth: self.state.clear_depth,
            stencil: self.state.clear_stencil,
        });
    }

    /// Set clear color.
    pub fn clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.state.clear_color = [r, g, b, a];
    }

    /// Set clear depth.
    pub fn clear_depth(&mut self, depth: f32) {
        self.state.clear_depth = depth;
    }

    /// Set clear stencil.
    pub fn clear_stencil(&mut self, stencil: i32) {
        self.state.clear_stencil = stencil;
    }

    /// Set viewport.
    pub fn viewport(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.state.viewport = [x, y, width, height];
    }

    /// Set scissor.
    pub fn scissor(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.state.scissor = [x, y, width, height];
    }

    /// Enable a capability.
    pub fn enable(&mut self, cap: u32) {
        match cap {
            constants::BLEND => self.state.blend_enabled = true,
            constants::CULL_FACE => self.state.cull_face_enabled = true,
            constants::DEPTH_TEST => self.state.depth_test_enabled = true,
            constants::SCISSOR_TEST => self.state.scissor_test_enabled = true,
            constants::STENCIL_TEST => self.state.stencil_test_enabled = true,
            constants::POLYGON_OFFSET_FILL => self.state.polygon_offset_fill = true,
            _ => self.last_error = constants::INVALID_ENUM,
        }
    }

    /// Disable a capability.
    pub fn disable(&mut self, cap: u32) {
        match cap {
            constants::BLEND => self.state.blend_enabled = false,
            constants::CULL_FACE => self.state.cull_face_enabled = false,
            constants::DEPTH_TEST => self.state.depth_test_enabled = false,
            constants::SCISSOR_TEST => self.state.scissor_test_enabled = false,
            constants::STENCIL_TEST => self.state.stencil_test_enabled = false,
            constants::POLYGON_OFFSET_FILL => self.state.polygon_offset_fill = false,
            _ => self.last_error = constants::INVALID_ENUM,
        }
    }

    /// Check if capability is enabled.
    pub fn is_enabled(&self, cap: u32) -> bool {
        match cap {
            constants::BLEND => self.state.blend_enabled,
            constants::CULL_FACE => self.state.cull_face_enabled,
            constants::DEPTH_TEST => self.state.depth_test_enabled,
            constants::SCISSOR_TEST => self.state.scissor_test_enabled,
            constants::STENCIL_TEST => self.state.stencil_test_enabled,
            constants::POLYGON_OFFSET_FILL => self.state.polygon_offset_fill,
            _ => false,
        }
    }

    /// Set blend function.
    pub fn blend_func(&mut self, src: u32, dst: u32) {
        self.state.blend_src_rgb = src;
        self.state.blend_dst_rgb = dst;
        self.state.blend_src_alpha = src;
        self.state.blend_dst_alpha = dst;
    }

    /// Set separate blend function.
    pub fn blend_func_separate(&mut self, src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32) {
        self.state.blend_src_rgb = src_rgb;
        self.state.blend_dst_rgb = dst_rgb;
        self.state.blend_src_alpha = src_alpha;
        self.state.blend_dst_alpha = dst_alpha;
    }

    /// Set blend equation.
    pub fn blend_equation(&mut self, mode: u32) {
        self.state.blend_equation_rgb = mode;
        self.state.blend_equation_alpha = mode;
    }

    /// Set depth function.
    pub fn depth_func(&mut self, func: u32) {
        self.state.depth_func = func;
    }

    /// Set depth mask.
    pub fn depth_mask(&mut self, flag: bool) {
        self.state.depth_mask = flag;
    }

    /// Set cull face.
    pub fn cull_face(&mut self, mode: u32) {
        self.state.cull_face_mode = mode;
    }

    /// Set front face.
    pub fn front_face(&mut self, mode: u32) {
        self.state.front_face = mode;
    }

    /// Set line width.
    pub fn line_width(&mut self, width: f32) {
        self.state.line_width = width;
    }

    // ==================== Shaders ====================

    /// Create a shader.
    pub fn create_shader(&mut self, shader_type: u32) -> WebGLShader {
        let shader = WebGLObject::new();
        self.shaders.insert(shader, ShaderData::new(shader_type));
        shader
    }

    /// Set shader source.
    pub fn shader_source(&mut self, shader: WebGLShader, source: &str) {
        if let Some(data) = self.shaders.get_mut(&shader) {
            data.source = source.to_string();
        }
    }

    /// Compile shader.
    pub fn compile_shader(&mut self, shader: WebGLShader) {
        if let Some(data) = self.shaders.get_mut(&shader) {
            // In a real implementation, this would compile GLSL
            // For now, we just mark it as compiled
            data.compiled = true;
            data.info_log.clear();
        }
    }

    /// Get shader parameter.
    pub fn get_shader_parameter(&self, shader: WebGLShader, pname: u32) -> Option<i32> {
        let data = self.shaders.get(&shader)?;
        match pname {
            constants::COMPILE_STATUS => Some(if data.compiled { 1 } else { 0 }),
            constants::DELETE_STATUS => Some(if data.deleted { 1 } else { 0 }),
            _ => None,
        }
    }

    /// Get shader info log.
    pub fn get_shader_info_log(&self, shader: WebGLShader) -> String {
        self.shaders
            .get(&shader)
            .map(|d| d.info_log.clone())
            .unwrap_or_default()
    }

    /// Delete shader.
    pub fn delete_shader(&mut self, shader: WebGLShader) {
        if let Some(data) = self.shaders.get_mut(&shader) {
            data.deleted = true;
        }
    }

    // ==================== Programs ====================

    /// Create a program.
    pub fn create_program(&mut self) -> WebGLProgram {
        let program = WebGLObject::new();
        self.programs.insert(program, ProgramData::new());
        program
    }

    /// Attach shader to program.
    pub fn attach_shader(&mut self, program: WebGLProgram, shader: WebGLShader) {
        if let Some(program_data) = self.programs.get_mut(&program) {
            if let Some(shader_data) = self.shaders.get(&shader) {
                match shader_data.shader_type {
                    constants::VERTEX_SHADER => program_data.vertex_shader = Some(shader),
                    constants::FRAGMENT_SHADER => program_data.fragment_shader = Some(shader),
                    _ => {}
                }
            }
        }
    }

    /// Link program.
    pub fn link_program(&mut self, program: WebGLProgram) {
        if let Some(data) = self.programs.get_mut(&program) {
            // Check both shaders are attached and compiled
            let vs_ok = data.vertex_shader.and_then(|s| self.shaders.get(&s)).is_some_and(|d| d.compiled);
            let fs_ok = data.fragment_shader.and_then(|s| self.shaders.get(&s)).is_some_and(|d| d.compiled);

            data.linked = vs_ok && fs_ok;
            if !data.linked {
                data.info_log = "Failed to link: missing or uncompiled shaders".to_string();
            }
        }
    }

    /// Get program parameter.
    pub fn get_program_parameter(&self, program: WebGLProgram, pname: u32) -> Option<i32> {
        let data = self.programs.get(&program)?;
        match pname {
            constants::LINK_STATUS => Some(if data.linked { 1 } else { 0 }),
            constants::DELETE_STATUS => Some(if data.deleted { 1 } else { 0 }),
            _ => None,
        }
    }

    /// Get program info log.
    pub fn get_program_info_log(&self, program: WebGLProgram) -> String {
        self.programs
            .get(&program)
            .map(|d| d.info_log.clone())
            .unwrap_or_default()
    }

    /// Use program.
    pub fn use_program(&mut self, program: Option<WebGLProgram>) {
        self.state.current_program = program;
    }

    /// Delete program.
    pub fn delete_program(&mut self, program: WebGLProgram) {
        if let Some(data) = self.programs.get_mut(&program) {
            data.deleted = true;
        }
    }

    /// Get uniform location.
    pub fn get_uniform_location(&mut self, program: WebGLProgram, name: &str) -> Option<WebGLUniformLocation> {
        if let Some(data) = self.programs.get_mut(&program) {
            if let Some(info) = data.uniforms.get(name) {
                return Some(info.location);
            }
            // Create new uniform location
            let loc = WebGLObject::new();
            data.uniforms.insert(
                name.to_string(),
                UniformInfo {
                    name: name.to_string(),
                    location: loc,
                    size: 1,
                    type_: constants::FLOAT,
                },
            );
            Some(loc)
        } else {
            None
        }
    }

    /// Get attribute location.
    pub fn get_attrib_location(&mut self, program: WebGLProgram, name: &str) -> i32 {
        if let Some(data) = self.programs.get_mut(&program) {
            if let Some(info) = data.attributes.get(name) {
                return info.location;
            }
            // Create new attribute
            let loc = data.attributes.len() as i32;
            data.attributes.insert(
                name.to_string(),
                AttributeInfo {
                    name: name.to_string(),
                    location: loc,
                    size: 4,
                    type_: constants::FLOAT,
                },
            );
            loc
        } else {
            -1
        }
    }

    // ==================== Uniforms ====================

    /// Set uniform float.
    pub fn uniform1f(&mut self, location: WebGLUniformLocation, x: f32) {
        self.uniform_values.insert(location, UniformValue::Float(x));
    }

    /// Set uniform vec2.
    pub fn uniform2f(&mut self, location: WebGLUniformLocation, x: f32, y: f32) {
        self.uniform_values.insert(location, UniformValue::Vec2([x, y]));
    }

    /// Set uniform vec3.
    pub fn uniform3f(&mut self, location: WebGLUniformLocation, x: f32, y: f32, z: f32) {
        self.uniform_values.insert(location, UniformValue::Vec3([x, y, z]));
    }

    /// Set uniform vec4.
    pub fn uniform4f(&mut self, location: WebGLUniformLocation, x: f32, y: f32, z: f32, w: f32) {
        self.uniform_values.insert(location, UniformValue::Vec4([x, y, z, w]));
    }

    /// Set uniform int.
    pub fn uniform1i(&mut self, location: WebGLUniformLocation, x: i32) {
        self.uniform_values.insert(location, UniformValue::Int(x));
    }

    /// Set uniform mat4.
    pub fn uniform_matrix4fv(&mut self, location: WebGLUniformLocation, _transpose: bool, value: &[f32; 16]) {
        self.uniform_values.insert(location, UniformValue::Mat4(*value));
    }

    // ==================== Buffers ====================

    /// Create a buffer.
    pub fn create_buffer(&mut self) -> WebGLBuffer {
        let buffer = WebGLObject::new();
        self.buffers.insert(buffer, BufferData::new());
        buffer
    }

    /// Bind a buffer.
    pub fn bind_buffer(&mut self, target: u32, buffer: Option<WebGLBuffer>) {
        match target {
            constants::ARRAY_BUFFER => self.state.current_array_buffer = buffer,
            constants::ELEMENT_ARRAY_BUFFER => self.state.current_element_buffer = buffer,
            _ => self.last_error = constants::INVALID_ENUM,
        }
    }

    /// Upload buffer data.
    pub fn buffer_data(&mut self, target: u32, data: &[u8], usage: u32) {
        let buffer = match target {
            constants::ARRAY_BUFFER => self.state.current_array_buffer,
            constants::ELEMENT_ARRAY_BUFFER => self.state.current_element_buffer,
            _ => None,
        };

        if let Some(buf) = buffer {
            if let Some(buf_data) = self.buffers.get_mut(&buf) {
                buf_data.target = target;
                buf_data.usage = usage;
                buf_data.data = data.to_vec();
            }
        }
    }

    /// Delete a buffer.
    pub fn delete_buffer(&mut self, buffer: WebGLBuffer) {
        if let Some(data) = self.buffers.get_mut(&buffer) {
            data.deleted = true;
        }
    }

    // ==================== Vertex Attributes ====================

    /// Set vertex attribute pointer.
    pub fn vertex_attrib_pointer(
        &mut self,
        index: u32,
        size: i32,
        type_: u32,
        normalized: bool,
        stride: i32,
        offset: i32,
    ) {
        if (index as usize) < self.state.vertex_attribs.len() {
            self.state.vertex_attribs[index as usize] = VertexAttribPointer {
                enabled: self.state.vertex_attribs[index as usize].enabled,
                size,
                type_,
                normalized,
                stride,
                offset,
                buffer: self.state.current_array_buffer,
            };
        }
    }

    /// Enable vertex attribute array.
    pub fn enable_vertex_attrib_array(&mut self, index: u32) {
        if (index as usize) < self.state.vertex_attribs.len() {
            self.state.vertex_attribs[index as usize].enabled = true;
        }
    }

    /// Disable vertex attribute array.
    pub fn disable_vertex_attrib_array(&mut self, index: u32) {
        if (index as usize) < self.state.vertex_attribs.len() {
            self.state.vertex_attribs[index as usize].enabled = false;
        }
    }

    // ==================== Textures ====================

    /// Create a texture.
    pub fn create_texture(&mut self) -> WebGLTexture {
        let texture = WebGLObject::new();
        self.textures.insert(texture, TextureData::new());
        texture
    }

    /// Bind a texture.
    pub fn bind_texture(&mut self, target: u32, texture: Option<WebGLTexture>) {
        if target == constants::TEXTURE_2D || target == constants::TEXTURE_CUBE_MAP {
            let unit = self.state.active_texture as usize;
            if unit < self.state.texture_units.len() {
                self.state.texture_units[unit] = texture;
            }
        }
    }

    /// Set active texture unit.
    pub fn active_texture(&mut self, texture: u32) {
        self.state.active_texture = texture - 0x84C0; // GL_TEXTURE0
    }

    /// Set texture parameter.
    pub fn tex_parameteri(&mut self, target: u32, pname: u32, param: i32) {
        let unit = self.state.active_texture as usize;
        if let Some(Some(tex)) = self.state.texture_units.get(unit) {
            if let Some(tex_data) = self.textures.get_mut(tex) {
                if tex_data.target == target || tex_data.target == 0 {
                    tex_data.target = target;
                    match pname {
                        constants::TEXTURE_MAG_FILTER => tex_data.mag_filter = param as u32,
                        constants::TEXTURE_MIN_FILTER => tex_data.min_filter = param as u32,
                        constants::TEXTURE_WRAP_S => tex_data.wrap_s = param as u32,
                        constants::TEXTURE_WRAP_T => tex_data.wrap_t = param as u32,
                        _ => {}
                    }
                }
            }
        }
    }

    /// Upload texture image.
    pub fn tex_image_2d(
        &mut self,
        target: u32,
        _level: i32,
        internal_format: i32,
        width: i32,
        height: i32,
        _border: i32,
        format: u32,
        _type_: u32,
        data: Option<&[u8]>,
    ) {
        let unit = self.state.active_texture as usize;
        if let Some(Some(tex)) = self.state.texture_units.get(unit) {
            if let Some(tex_data) = self.textures.get_mut(tex) {
                tex_data.target = target;
                tex_data.width = width as u32;
                tex_data.height = height as u32;
                tex_data.internal_format = internal_format as u32;
                tex_data.format = format;
                if let Some(d) = data {
                    tex_data.data = d.to_vec();
                } else {
                    tex_data.data = vec![0; (width * height * 4) as usize];
                }
            }
        }
    }

    /// Delete a texture.
    pub fn delete_texture(&mut self, texture: WebGLTexture) {
        if let Some(data) = self.textures.get_mut(&texture) {
            data.deleted = true;
        }
    }

    // ==================== Drawing ====================

    /// Draw arrays.
    pub fn draw_arrays(&mut self, mode: u32, first: i32, count: i32) {
        if let Some(program) = self.state.current_program {
            let state = DrawState {
                viewport: self.state.viewport,
                blend_enabled: self.state.blend_enabled,
                blend_func: (
                    self.state.blend_src_rgb,
                    self.state.blend_dst_rgb,
                    self.state.blend_src_alpha,
                    self.state.blend_dst_alpha,
                ),
                depth_test_enabled: self.state.depth_test_enabled,
                depth_func: self.state.depth_func,
                cull_face_enabled: self.state.cull_face_enabled,
                cull_face_mode: self.state.cull_face_mode,
                vertex_attribs: self.state.vertex_attribs.clone(),
                uniforms: self.uniform_values.clone(),
            };

            self.draw_calls.push(DrawCall::DrawArrays {
                mode,
                first,
                count,
                program,
                state: Box::new(state),
            });
        }
    }

    /// Draw elements.
    pub fn draw_elements(&mut self, mode: u32, count: i32, type_: u32, offset: i32) {
        if let Some(program) = self.state.current_program {
            let state = DrawState {
                viewport: self.state.viewport,
                blend_enabled: self.state.blend_enabled,
                blend_func: (
                    self.state.blend_src_rgb,
                    self.state.blend_dst_rgb,
                    self.state.blend_src_alpha,
                    self.state.blend_dst_alpha,
                ),
                depth_test_enabled: self.state.depth_test_enabled,
                depth_func: self.state.depth_func,
                cull_face_enabled: self.state.cull_face_enabled,
                cull_face_mode: self.state.cull_face_mode,
                vertex_attribs: self.state.vertex_attribs.clone(),
                uniforms: self.uniform_values.clone(),
            };

            self.draw_calls.push(DrawCall::DrawElements {
                mode,
                count,
                type_,
                offset,
                program,
                state: Box::new(state),
            });
        }
    }

    // ==================== Framebuffers ====================

    /// Create framebuffer.
    pub fn create_framebuffer(&mut self) -> WebGLFramebuffer {
        let fb = WebGLObject::new();
        self.framebuffers.insert(fb, FramebufferData::new());
        fb
    }

    /// Bind framebuffer.
    pub fn bind_framebuffer(&mut self, _target: u32, framebuffer: Option<WebGLFramebuffer>) {
        self.state.current_framebuffer = framebuffer;
    }

    /// Create renderbuffer.
    pub fn create_renderbuffer(&mut self) -> WebGLRenderbuffer {
        let rb = WebGLObject::new();
        self.renderbuffers.insert(rb, RenderbufferData::new());
        rb
    }

    /// Bind renderbuffer.
    pub fn bind_renderbuffer(&mut self, _target: u32, renderbuffer: Option<WebGLRenderbuffer>) {
        self.state.current_renderbuffer = renderbuffer;
    }

    // ==================== Output ====================

    /// Get draw calls and clear them.
    pub fn take_draw_calls(&mut self) -> Vec<DrawCall> {
        std::mem::take(&mut self.draw_calls)
    }

    /// Get buffer data.
    pub fn get_buffer_data(&self, buffer: WebGLBuffer) -> Option<&BufferData> {
        self.buffers.get(&buffer)
    }

    /// Get texture data.
    pub fn get_texture_data(&self, texture: WebGLTexture) -> Option<&TextureData> {
        self.textures.get(&texture)
    }

    /// Get shader data.
    pub fn get_shader_data(&self, shader: WebGLShader) -> Option<&ShaderData> {
        self.shaders.get(&shader)
    }

    /// Get program data.
    pub fn get_program_data(&self, program: WebGLProgram) -> Option<&ProgramData> {
        self.programs.get(&program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = WebGLRenderingContext::new(800, 600);
        assert_eq!(ctx.width, 800);
        assert_eq!(ctx.height, 600);
        assert_eq!(ctx.state.viewport, [0, 0, 800, 600]);
    }

    #[test]
    fn test_clear_color() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        ctx.clear_color(1.0, 0.5, 0.25, 1.0);
        assert_eq!(ctx.state.clear_color, [1.0, 0.5, 0.25, 1.0]);
    }

    #[test]
    fn test_create_shader() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        let shader = ctx.create_shader(constants::VERTEX_SHADER);
        assert!(shader.is_valid());
        
        ctx.shader_source(shader, "void main() {}");
        ctx.compile_shader(shader);
        
        assert_eq!(ctx.get_shader_parameter(shader, constants::COMPILE_STATUS), Some(1));
    }

    #[test]
    fn test_create_program() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        
        let vs = ctx.create_shader(constants::VERTEX_SHADER);
        ctx.shader_source(vs, "void main() {}");
        ctx.compile_shader(vs);
        
        let fs = ctx.create_shader(constants::FRAGMENT_SHADER);
        ctx.shader_source(fs, "void main() {}");
        ctx.compile_shader(fs);
        
        let program = ctx.create_program();
        ctx.attach_shader(program, vs);
        ctx.attach_shader(program, fs);
        ctx.link_program(program);
        
        assert_eq!(ctx.get_program_parameter(program, constants::LINK_STATUS), Some(1));
    }

    #[test]
    fn test_create_buffer() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        let buffer = ctx.create_buffer();
        ctx.bind_buffer(constants::ARRAY_BUFFER, Some(buffer));
        
        let data = [1.0_f32, 2.0, 3.0, 4.0];
        let bytes = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4)
        };
        ctx.buffer_data(constants::ARRAY_BUFFER, bytes, constants::STATIC_DRAW);
        
        let buf_data = ctx.get_buffer_data(buffer).unwrap();
        assert_eq!(buf_data.data.len(), 16);
    }

    #[test]
    fn test_enable_disable() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        
        assert!(!ctx.is_enabled(constants::BLEND));
        ctx.enable(constants::BLEND);
        assert!(ctx.is_enabled(constants::BLEND));
        ctx.disable(constants::BLEND);
        assert!(!ctx.is_enabled(constants::BLEND));
    }

    #[test]
    fn test_draw_arrays() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        
        // Create minimal program
        let vs = ctx.create_shader(constants::VERTEX_SHADER);
        ctx.compile_shader(vs);
        let fs = ctx.create_shader(constants::FRAGMENT_SHADER);
        ctx.compile_shader(fs);
        let program = ctx.create_program();
        ctx.attach_shader(program, vs);
        ctx.attach_shader(program, fs);
        ctx.link_program(program);
        ctx.use_program(Some(program));
        
        // Draw
        ctx.draw_arrays(constants::TRIANGLES, 0, 3);
        
        let calls = ctx.take_draw_calls();
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn test_uniforms() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        let program = ctx.create_program();
        
        let loc = ctx.get_uniform_location(program, "u_color").unwrap();
        ctx.uniform4f(loc, 1.0, 0.0, 0.0, 1.0);
        
        assert!(ctx.uniform_values.contains_key(&loc));
    }

    #[test]
    fn test_vertex_attribs() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        let buffer = ctx.create_buffer();
        ctx.bind_buffer(constants::ARRAY_BUFFER, Some(buffer));
        
        ctx.vertex_attrib_pointer(0, 3, constants::FLOAT, false, 12, 0);
        ctx.enable_vertex_attrib_array(0);
        
        assert!(ctx.state.vertex_attribs[0].enabled);
        assert_eq!(ctx.state.vertex_attribs[0].size, 3);
    }

    #[test]
    fn test_texture() {
        let mut ctx = WebGLRenderingContext::new(100, 100);
        let texture = ctx.create_texture();
        
        ctx.bind_texture(constants::TEXTURE_2D, Some(texture));
        ctx.tex_parameteri(constants::TEXTURE_2D, constants::TEXTURE_MIN_FILTER, constants::LINEAR as i32);
        
        let tex_data = ctx.get_texture_data(texture).unwrap();
        assert_eq!(tex_data.min_filter, constants::LINEAR);
    }
}

