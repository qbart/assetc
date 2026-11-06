// The source of water.frag.spv, kept for reference: assetc reads only the SPIR-V.
// Compiled with naga (`naga water.frag.wgsl water.frag.spv`); the names are what matter,
// the groups and bindings are free. See docs/features/custom-materials.md.

struct Foam {
    amount: f32,
    scale: f32,
}

// The GPU block of WaterParams, std140: a nested struct starts and ends on 16 bytes.
struct Params {
    deep_color: vec3<f32>,
    wave_speed: f32,
    shallow_color: vec3<f32>,
    wave_scale: f32,
    @align(16) @size(16) foam: Foam,
    style: u32,
    opacity: f32,
}

struct Frame {
    camera_position: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    time: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> params: Params;
@group(1) @binding(1) var color: texture_2d<f32>;
@group(1) @binding(2) var color_sampler: sampler;
@group(1) @binding(3) var ripples: texture_2d<f32>;
@group(1) @binding(4) var ripples_sampler: sampler;
@group(1) @binding(5) var mask: texture_2d<f32>;
@group(1) @binding(6) var mask_sampler: sampler;

@fragment
fn main(
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> @location(0) vec4<f32> {
    let rough = 1.0 + f32(params.style);
    let t = frame.time.x * params.wave_speed;
    let uv1 = uv * params.wave_scale + vec2(t * 0.05, t * 0.03);
    let uv2 = uv * params.wave_scale * 1.7 - vec2(t * 0.04, -t * 0.02);
    let n1 = textureSample(ripples, ripples_sampler, uv1).xyz * 2.0 - 1.0;
    let n2 = textureSample(ripples, ripples_sampler, uv2).xyz * 2.0 - 1.0;
    let bend = (n1.xy + n2.xy) * 0.25 * rough;
    let n = normalize(normalize(world_normal) + vec3(bend.x, 0.0, bend.y));

    let m = textureSample(mask, mask_sampler, uv);
    let base = mix(params.shallow_color, params.deep_color, m.g)
        * textureSample(color, color_sampler, uv).rgb;
    let l = normalize(frame.light_direction.xyz);
    let v = normalize(frame.camera_position.xyz - world_position);
    let h = normalize(l + v);
    let light = frame.light_color.rgb * frame.light_color.w;
    let diffuse = (max(dot(n, l), 0.0) * 0.8 + 0.2) * light;
    let specular = pow(max(dot(n, h), 0.0), 96.0) * light;
    let swirl = textureSample(mask, mask_sampler, uv * params.foam.scale + vec2(t * 0.02)).r;
    let foam = smoothstep(1.0 - params.foam.amount, 1.0, m.r * swirl);
    return vec4(base * diffuse + specular + vec3(foam), max(params.opacity, foam));
}
