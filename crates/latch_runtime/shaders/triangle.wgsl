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
    // Per-instance position (integer game units - 10 µm precision)
    @location(1) instance_position: vec2<i32>,
    // Per-instance velocity (i16 normalized to -1.0 to 1.0)
    @location(2) instance_velocity: vec2<f32>,  // Snorm16x2 auto-normalizes to f32
    // Per-instance color (u8 normalized to 0.0-1.0)
    @location(3) instance_color: vec4<f32>,     // Unorm8x4 auto-normalizes to f32
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
    
    // Convert integer position to NDC (floating-point)
    // UNITS_PER_NDC = 1,000,000 (1 NDC = 10 meters)
    let position_ndc = vec2<f32>(instance.instance_position) / 1000000.0;
    
    // Velocity comes in as Snorm16x2 (normalized -1.0 to 1.0)
    // Convert back to NDC units: velocity is in game units, position is in NDC
    // vel_ndc = vel_normalized * 32767 / UNITS_PER_NDC
    let vel_scale = 32767.0 / 1000000.0; // i16 max / UNITS_PER_NDC
    let velocity_ndc = instance.instance_velocity * vel_scale;
    
    // GPU-side interpolation: position + velocity * alpha
    let interpolated_pos = position_ndc + velocity_ndc * uniforms.interpolation_alpha;
    
    // Offset base vertex by interpolated instance position
    let world_pos = vertex.position + interpolated_pos;
    out.clip_position = vec4<f32>(world_pos, 0.0, 1.0);
    
    // Color is already normalized by Unorm8x4 (0.0-1.0 range)
    out.color = instance.instance_color.rgb;
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
