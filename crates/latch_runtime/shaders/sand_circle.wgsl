// Sand particle circle shader

struct VertexInput {
    @location(0) position : vec2<f32>,
    @location(1) uv : vec2<f32>,
}

struct InstanceInput {
    @location(2) instance_position : vec2<i32>,
    @location(3) instance_velocity : vec2<f32>,
    @location(4) instance_color : vec4<f32>,
    @location(5) instance_radius : f32,
}

struct Uniforms {
    interpolation_alpha : f32,
    dt : f32,
}

@group(0) @binding(0)
var<uniform> uniforms : Uniforms;

struct VertexOutput {
    @builtin(position) clip_position : vec4<f32>,
    @location(0) color : vec3<f32>,
    @location(1) uv : vec2<f32>,
}

const UNITS_PER_NDC : f32 = 200000.0; // 10 meters per NDC
const VELOCITY_SCALE : f32 = 32767.0 / UNITS_PER_NDC;

@vertex
fn vs_main(vertex : VertexInput, instance : InstanceInput) -> VertexOutput {
    var out : VertexOutput;
    let position_ndc = vec2<f32>(instance.instance_position) / UNITS_PER_NDC;
    let velocity_ndc = instance.instance_velocity * VELOCITY_SCALE;
    let interpolated = position_ndc + velocity_ndc * uniforms.interpolation_alpha;

    let radius_ndc = instance.instance_radius / UNITS_PER_NDC;
    let scaled = vertex.position * radius_ndc;
    out.clip_position = vec4<f32>(interpolated + scaled, 0.0, 1.0);
    out.color = instance.instance_color.rgb;
    out.uv = vertex.uv;
    return out;
}

@fragment
fn fs_main(input : VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(input.uv);
    if dist > 1.0 {
        discard;
    }
    return vec4<f32>(input.color, 1.0);
}
