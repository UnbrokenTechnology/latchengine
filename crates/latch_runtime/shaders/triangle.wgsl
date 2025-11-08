// GPU-side interpolation shader
// 
// Uploads happen only at physics tick rate (60Hz)
// GPU interpolates positions between ticks for smooth rendering
//
// This drastically reduces CPU->GPU bandwidth compared to
// uploading every render frame

struct VertexInput {
    // Base triangle vertex position (3 vertices defining shape)
    @location(0) position: vec2<f32>,
}

struct InstanceInput {
    // Per-instance position (updated at physics rate)
    @location(1) instance_position: vec2<f32>,
    // Per-instance velocity (for interpolation)
    @location(2) instance_velocity: vec2<f32>,
    // Per-instance color (static)
    @location(3) instance_color: vec3<f32>,
}

struct Uniforms {
    // Interpolation factor [0, 1] between current and next tick
    interpolation_alpha: f32,
    // Delta time for physics step
    dt: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    
    // GPU-side interpolation: position + velocity * alpha * dt
    // This gives us smooth motion without uploading every frame
    let interpolated_pos = instance.instance_position + 
                          instance.instance_velocity * uniforms.interpolation_alpha * uniforms.dt;
    
    // Offset base vertex by interpolated instance position
    let world_pos = vertex.position + interpolated_pos;
    out.clip_position = vec4<f32>(world_pos, 0.0, 1.0);
    out.color = instance.instance_color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
