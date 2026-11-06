// assetc's vertex stage for previewing custom materials: it feeds the fragment inputs of
// docs/features/custom-materials.md#the-interface-in-spir-v from the material's
// own block of transforms, so it needs none of Bevy's mesh bindings.

// Its own block, apart from the shader's Frame and Object, whose layouts are the shader's.
struct Transforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
}

@group(3) @binding(35) var<uniform> transforms: Transforms;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) tangent: vec4<f32>,
}

struct Out {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_tangent: vec4<f32>,
    @location(4) color: vec4<f32>,
}

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let world = transforms.model * vec4(v.position, 1.0);
    out.clip_position = transforms.view_proj * world;
    out.world_position = world.xyz;
    out.world_normal = normalize((transforms.normal * vec4(v.normal, 0.0)).xyz);
    out.uv = v.uv;
    out.world_tangent = vec4(normalize((transforms.model * vec4(v.tangent.xyz, 0.0)).xyz), v.tangent.w);
    out.color = vec4(1.0);
    return out;
}
