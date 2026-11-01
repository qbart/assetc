// Color vision deficiency simulation over the 3D view; see docs/features/canvas.md#color-vision.

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Rows of the matrix applied to linear RGB.
struct VisionFilter {
    r: vec4<f32>,
    g: vec4<f32>,
    b: vec4<f32>,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> vision: VisionFilter;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(screen_texture, screen_sampler, in.uv);
    let rgb = vec3<f32>(
        dot(vision.r.xyz, color.rgb),
        dot(vision.g.xyz, color.rgb),
        dot(vision.b.xyz, color.rgb),
    );
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), color.a);
}
