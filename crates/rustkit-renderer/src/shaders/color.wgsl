// Color shader for solid color rendering

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Convert from pixel coords to clip space (-1 to 1)
    let x = in.position.x * 2.0 / uniforms.viewport_size.x - 1.0;
    let y = 1.0 - in.position.y * 2.0 / uniforms.viewport_size.y;
    
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.color = in.color;
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}

