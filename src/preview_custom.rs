//! The 3D preview of custom materials: the shader's fragment SPIR-V, turned into WGSL with its
//! resources moved to fixed places of a Bevy material, on the preview shape. See
//! docs/features/custom-materials.md#preview.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, CompareFunction, Extent3d, Face, RenderPipelineDescriptor,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use bevy::shader::{Shader, ShaderRef};

use crate::shader::{Blend, Cull, DepthTest, Dimension, Fallback, State};

/// Texture places of the material, by dimension: bindings `3 + 2k` (sampler `4 + 2k`).
const SLOTS_2D: usize = 10;
const SLOTS_CUBE: usize = 3;
const SLOTS_3D: usize = 3;
const FIRST_CUBE: u32 = 3 + 2 * SLOTS_2D as u32;
const FIRST_3D: u32 = FIRST_CUBE + 2 * SLOTS_CUBE as u32;

/// What the UI wants previewed this frame; see docs/features/custom-materials.md#preview.
#[derive(Clone)]
pub struct Want {
    /// Changes whenever the stage or the textures do.
    pub key: u64,
    pub fragment: Arc<Vec<u8>>,
    pub entry: String,
    pub shader: Arc<crate::shader::Shader>,
    /// The GPU block, as edited.
    pub params: Vec<u8>,
    pub state: State,
    /// Each texture's image, or `None` for its fallback.
    pub images: Arc<Vec<Option<crate::texture::Image>>>,
}

/// The stage turned into WGSL, and where its blocks keep their members.
struct Built {
    frame: HashMap<String, u32>,
    object: HashMap<String, u32>,
    material: Handle<CustomPreviewMaterial>,
}

#[derive(Resource, Default)]
pub struct CustomPreview {
    built: Option<(u64, Result<Built, String>)>,
    started: f32,
}

#[derive(Component)]
pub struct CustomShape;

/// Every fragment shader made, by its index in the pipeline key.
static SHADERS: Mutex<Vec<(Handle<Shader>, String)>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CustomKey {
    shader: u32,
    cull: u8,
    depth_write: bool,
    depth_test: u8,
}

/// The material every custom preview uses: the shader's resources at fixed bindings.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(CustomKey)]
pub struct CustomPreviewMaterial {
    #[uniform(0)]
    params: [Vec4; 1024],
    #[uniform(1)]
    frame: [Vec4; 16],
    #[uniform(2)]
    object: [Vec4; 8],
    #[uniform(35)]
    transforms: [Vec4; 12],
    #[texture(3)]
    #[sampler(4)]
    t0: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    t1: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    t2: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    t3: Handle<Image>,
    #[texture(11)]
    #[sampler(12)]
    t4: Handle<Image>,
    #[texture(13)]
    #[sampler(14)]
    t5: Handle<Image>,
    #[texture(15)]
    #[sampler(16)]
    t6: Handle<Image>,
    #[texture(17)]
    #[sampler(18)]
    t7: Handle<Image>,
    #[texture(19)]
    #[sampler(20)]
    t8: Handle<Image>,
    #[texture(21)]
    #[sampler(22)]
    t9: Handle<Image>,
    #[texture(23, dimension = "cube")]
    #[sampler(24)]
    c0: Handle<Image>,
    #[texture(25, dimension = "cube")]
    #[sampler(26)]
    c1: Handle<Image>,
    #[texture(27, dimension = "cube")]
    #[sampler(28)]
    c2: Handle<Image>,
    #[texture(29, dimension = "3d")]
    #[sampler(30)]
    v0: Handle<Image>,
    #[texture(31, dimension = "3d")]
    #[sampler(32)]
    v1: Handle<Image>,
    #[texture(33, dimension = "3d")]
    #[sampler(34)]
    v2: Handle<Image>,
    key: CustomKey,
    blend: Blend,
    alpha_cutoff: f32,
}

impl From<&CustomPreviewMaterial> for CustomKey {
    fn from(m: &CustomPreviewMaterial) -> Self {
        m.key
    }
}

impl Material for CustomPreviewMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://assetc/preview_custom.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        match self.blend {
            Blend::Opaque => AlphaMode::Opaque,
            Blend::Mask => AlphaMode::Mask(self.alpha_cutoff),
            Blend::Blend => AlphaMode::Blend,
            Blend::Add => AlphaMode::Add,
            Blend::Multiply => AlphaMode::Multiply,
        }
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let data = key.bind_group_data;
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_TANGENT.at_shader_location(4),
        ])?];
        if let Some((shader, entry)) = SHADERS.lock().unwrap().get(data.shader as usize).cloned()
            && let Some(fragment) = descriptor.fragment.as_mut()
        {
            fragment.shader = shader;
            fragment.entry_point = Some(entry.into());
            fragment.shader_defs.clear();
        }
        descriptor.primitive.cull_mode = match data.cull {
            0 => Some(Face::Back),
            1 => Some(Face::Front),
            _ => None,
        };
        // Bevy's depth is reversed: nearer is greater.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(data.depth_write);
            depth.depth_compare = Some(match data.depth_test {
                0 => CompareFunction::Greater,
                1 => CompareFunction::GreaterEqual,
                2 => CompareFunction::Equal,
                _ => CompareFunction::Always,
            });
        }
        Ok(())
    }
}

/// The WGSL of `bytes`, its resources moved to the material's bindings; see
/// docs/features/custom-materials.md#preview.
/// Where a block keeps its members, by name.
type Offsets = HashMap<String, u32>;

fn to_wgsl(
    bytes: &[u8],
    shader: &crate::shader::Shader,
) -> Result<(String, Offsets, Offsets), String> {
    let options = naga::front::spv::Options {
        adjust_coordinate_space: false,
        ..Default::default()
    };
    let mut module =
        naga::front::spv::parse_u8_slice(bytes, &options).map_err(|e| e.to_string())?;
    // Each texture's place among those of its dimension.
    let mut places = Vec::new();
    let mut counts = [0usize; 3];
    for t in &shader.textures {
        let (i, first, max) = match t.dimension() {
            Dimension::D2 => (0, 3, SLOTS_2D),
            Dimension::Cube => (1, FIRST_CUBE, SLOTS_CUBE),
            Dimension::D3 => (2, FIRST_3D, SLOTS_3D),
        };
        if counts[i] == max {
            return Err(format!(
                "the preview shows at most {SLOTS_2D} 2D, {SLOTS_CUBE} cube and {SLOTS_3D} 3D textures"
            ));
        }
        places.push(first + 2 * counts[i] as u32);
        counts[i] += 1;
    }
    let mut frame = HashMap::new();
    let mut object = HashMap::new();
    let handles: Vec<_> = module.global_variables.iter().map(|(h, _)| h).collect();
    for h in handles {
        let var = &module.global_variables[h];
        if var.binding.is_none() {
            continue;
        }
        let names: Vec<String> = var
            .name
            .iter()
            .chain(module.types[var.ty].name.iter())
            .cloned()
            .collect();
        let is = |a: &str| names.iter().any(|n| n.eq_ignore_ascii_case(a));
        // Leaf members by name, through any wrapping struct (naga's SPIR-V wraps blocks).
        fn leaves(
            module: &naga::Module,
            ty: naga::Handle<naga::Type>,
            base: u32,
            out: &mut HashMap<String, u32>,
        ) {
            if let naga::TypeInner::Struct { members, .. } = &module.types[ty].inner {
                for m in members {
                    match (&module.types[m.ty].inner, &m.name) {
                        (naga::TypeInner::Struct { .. }, _) => {
                            leaves(module, m.ty, base + m.offset, out)
                        }
                        (_, Some(name)) => {
                            out.insert(name.clone(), base + m.offset);
                        }
                        _ => {}
                    }
                }
            }
        }
        let members = |module: &naga::Module| {
            let mut out = HashMap::new();
            leaves(module, var.ty, 0, &mut out);
            out
        };
        let binding = if is("params") {
            0
        } else if is("frame") {
            frame = members(&module);
            1
        } else if is("object") {
            object = members(&module);
            2
        } else if let Some(i) = shader.textures.iter().position(|t| names.contains(&t.name)) {
            places[i]
        } else if let Some(i) = shader
            .textures
            .iter()
            .position(|t| names.contains(&format!("{}_sampler", t.name)))
        {
            places[i] + 1
        } else {
            return Err(format!(
                "{} is not part of the interface",
                names.first().cloned().unwrap_or_default()
            ));
        };
        module.global_variables.get_mut(h).binding = Some(naga::ResourceBinding {
            group: bevy::pbr::MATERIAL_BIND_GROUP_INDEX as u32,
            binding,
        });
    }
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|e| e.into_inner().to_string())?;
    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .map_err(|e| e.to_string())?;
    Ok((with_struct_layout(&wgsl, &module), frame, object))
}

/// `wgsl` with each member of a struct type given its std140 place: naga writes no layout
/// attributes, and WGSL lays a nested struct out tighter than std140 does.
fn with_struct_layout(wgsl: &str, module: &naga::Module) -> String {
    let spans: HashMap<&str, u32> = module
        .types
        .iter()
        .filter_map(|(_, t)| match (&t.name, &t.inner) {
            (Some(name), naga::TypeInner::Struct { span, .. }) => Some((name.as_str(), *span)),
            _ => None,
        })
        .collect();
    let mut out = String::with_capacity(wgsl.len());
    for line in wgsl.lines() {
        let member = line
            .trim()
            .strip_suffix(',')
            .and_then(|m| m.split_once(": "))
            .filter(|(name, _)| !name.contains(' ') && !name.starts_with('@'));
        match member.and_then(|(_, ty)| spans.get(ty)) {
            Some(span) => {
                let indent = &line[..line.len() - line.trim_start().len()];
                out += &format!(
                    "{indent}@align(16) @size({}) {}\n",
                    span.div_ceil(16) * 16,
                    line.trim_start()
                );
            }
            None => {
                out += line;
                out.push('\n');
            }
        }
    }
    out
}

fn texture(image: &crate::texture::Image, srgb: bool) -> Image {
    let format = if srgb {
        TextureFormat::Rgba8UnormSrgb
    } else {
        TextureFormat::Rgba8Unorm
    };
    let mut out = Image::new(
        Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image.pixels.clone(),
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    out.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    out
}

/// A one-texel image of `fallback` in `dimension`.
fn fallback(fallback: Fallback, dimension: Dimension) -> Image {
    let texel = fallback.texel();
    let (layers, dim) = match dimension {
        Dimension::D2 => (1, TextureDimension::D2),
        Dimension::Cube => (6, TextureDimension::D2),
        Dimension::D3 => (1, TextureDimension::D3),
    };
    let mut image = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: layers,
        },
        dim,
        texel.repeat(layers as usize),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    if dimension == Dimension::Cube {
        image.texture_view_descriptor = Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::Cube),
            ..default()
        });
    }
    image
}

fn build(
    want: &Want,
    shaders: &mut Assets<Shader>,
    images: &mut Assets<Image>,
    materials: &mut Assets<CustomPreviewMaterial>,
) -> Result<Built, String> {
    let (wgsl, frame, object) = to_wgsl(&want.fragment, &want.shader)?;
    let shader = shaders.add(Shader::from_wgsl(wgsl, "assetc://custom_preview"));
    let index = {
        let mut list = SHADERS.lock().unwrap();
        list.push((shader, want.entry.clone()));
        list.len() - 1
    };
    let mut slots = [vec![], vec![], vec![]];
    for (i, t) in want.shader.textures.iter().enumerate() {
        let image = match (t.dimension(), want.images.get(i).cloned().flatten()) {
            (Dimension::D2, Some(image)) => texture(&image, t.srgb()),
            (dimension, _) => fallback(t.fallback(), dimension),
        };
        let slot = match t.dimension() {
            Dimension::D2 => 0,
            Dimension::Cube => 1,
            Dimension::D3 => 2,
        };
        slots[slot].push(images.add(image));
    }
    let mut empty = |d: Dimension| images.add(fallback(Fallback::White, d));
    let mut take = |slot: usize, k: usize, d: Dimension| {
        slots[slot].get(k).cloned().unwrap_or_else(|| empty(d))
    };
    let state = want.state;
    let material = CustomPreviewMaterial {
        params: [Vec4::ZERO; 1024],
        frame: [Vec4::ZERO; 16],
        object: [Vec4::ZERO; 8],
        transforms: [Vec4::ZERO; 12],
        t0: take(0, 0, Dimension::D2),
        t1: take(0, 1, Dimension::D2),
        t2: take(0, 2, Dimension::D2),
        t3: take(0, 3, Dimension::D2),
        t4: take(0, 4, Dimension::D2),
        t5: take(0, 5, Dimension::D2),
        t6: take(0, 6, Dimension::D2),
        t7: take(0, 7, Dimension::D2),
        t8: take(0, 8, Dimension::D2),
        t9: take(0, 9, Dimension::D2),
        c0: take(1, 0, Dimension::Cube),
        c1: take(1, 1, Dimension::Cube),
        c2: take(1, 2, Dimension::Cube),
        v0: take(2, 0, Dimension::D3),
        v1: take(2, 1, Dimension::D3),
        v2: take(2, 2, Dimension::D3),
        key: CustomKey {
            shader: index as u32,
            cull: Cull::ALL.iter().position(|c| *c == state.cull()).unwrap() as u8,
            depth_write: state.depth_write(),
            depth_test: DepthTest::ALL
                .iter()
                .position(|d| *d == state.depth_test())
                .unwrap() as u8,
        },
        blend: state.blend(),
        alpha_cutoff: state.alpha_cutoff(),
    };
    Ok(Built {
        frame,
        object,
        material: materials.add(material),
    })
}

/// Writes `values` into `block` at their members' offsets.
fn put(block: &mut [Vec4], offsets: &HashMap<String, u32>, values: &[(&str, &[f32])]) {
    let floats: &mut [f32] = bytemuck_floats(block);
    for (name, v) in values {
        if let Some(at) = offsets.get(*name) {
            let at = *at as usize / 4;
            if at + v.len() <= floats.len() {
                floats[at..at + v.len()].copy_from_slice(v);
            }
        }
    }
}

fn bytemuck_floats(block: &mut [Vec4]) -> &mut [f32] {
    // SAFETY: `Vec4` is four `f32`s with no padding.
    unsafe { std::slice::from_raw_parts_mut(block.as_mut_ptr().cast::<f32>(), block.len() * 4) }
}

type ShapeOf<'w, 's> = Query<
    'w,
    's,
    (&'static Mesh3d, &'static Transform, &'static mut Visibility),
    (With<crate::preview3d::PreviewShape>, Without<CustomShape>),
>;
type CustomOf<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Mesh3d,
        &'static mut MeshMaterial3d<CustomPreviewMaterial>,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    With<CustomShape>,
>;

/// Shows the wanted custom material on the preview shape, or hides it; runs after the shape is
/// set up for the frame.
#[allow(clippy::too_many_arguments)]
pub fn sync_custom(
    mut commands: Commands,
    mut preview: ResMut<crate::preview3d::Preview3d>,
    mut state: ResMut<CustomPreview>,
    time: Res<Time>,
    mut shaders: ResMut<Assets<Shader>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<CustomPreviewMaterial>>,
    mut shape: ShapeOf,
    mut custom: CustomOf,
    camera: Query<(&Camera, &GlobalTransform), With<crate::preview3d::PreviewCamera>>,
    light: Query<&GlobalTransform, With<DirectionalLight>>,
) {
    let want = preview.custom.take();
    let mut hide = || {
        for (_, _, _, _, mut v) in custom.iter_mut() {
            *v = Visibility::Hidden;
        }
    };
    let Some(want) = want else {
        preview.custom_error = None;
        hide();
        return;
    };
    if state.built.as_ref().map(|(k, _)| *k) != Some(want.key) {
        let built = build(&want, &mut shaders, &mut images, &mut materials);
        state.built = Some((want.key, built));
        state.started = time.elapsed_secs();
    }
    let built = match &state.built {
        Some((_, Ok(b))) => b,
        Some((_, Err(e))) => {
            preview.custom_error = Some(e.clone());
            hide();
            return;
        }
        None => return,
    };
    preview.custom_error = None;
    let Ok((mesh, transform, mut visibility)) = shape.single_mut() else {
        return;
    };
    *visibility = Visibility::Hidden;
    let model = transform.to_matrix();
    let normal = model.inverse().transpose();
    if custom.is_empty() {
        commands.spawn((
            CustomShape,
            Mesh3d(mesh.0.clone()),
            MeshMaterial3d(built.material.clone()),
            *transform,
            Visibility::Visible,
        ));
    }
    for (_, mut m, mut mat, mut t, mut v) in custom.iter_mut() {
        if m.0 != mesh.0 {
            m.0 = mesh.0.clone();
        }
        if mat.0 != built.material {
            mat.0 = built.material.clone();
        }
        *t = *transform;
        *v = Visibility::Visible;
    }
    let Some(mut material) = materials.get_mut(&built.material) else {
        return;
    };
    let block = bytemuck_floats(&mut material.params);
    let bytes: Vec<f32> = want
        .params
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    let n = bytes.len().min(block.len());
    block[..n].copy_from_slice(&bytes[..n]);
    if let Ok((cam, cam_transform)) = camera.single() {
        let view = cam_transform.to_matrix().inverse();
        let view_proj = cam.clip_from_view() * view;
        let position = cam_transform.translation();
        let towards_light = light
            .iter()
            .next()
            .map_or(Vec3::new(-0.5, 0.6, 1.0).normalize(), |l| {
                -l.forward().as_vec3()
            });
        let size = cam
            .logical_viewport_size()
            .unwrap_or(Vec2::ONE)
            .max(Vec2::ONE);
        let t = time.elapsed_secs() - state.started;
        let frame: [(&str, &[f32]); 7] = [
            ("view_proj", &view_proj.to_cols_array()),
            ("view", &view.to_cols_array()),
            (
                "camera_position",
                &[position.x, position.y, position.z, 1.0],
            ),
            (
                "light_direction",
                &[towards_light.x, towards_light.y, towards_light.z, 0.0],
            ),
            ("light_color", &[1.0, 1.0, 1.0, 1.0]),
            ("time", &[t, time.delta_secs(), 0.0, 0.0]),
            ("viewport", &[size.x, size.y, 1.0 / size.x, 1.0 / size.y]),
        ];
        put(&mut material.frame, &built.frame, &frame);
        let transforms = bytemuck_floats(&mut material.transforms);
        transforms[..16].copy_from_slice(&view_proj.to_cols_array());
        transforms[16..32].copy_from_slice(&model.to_cols_array());
        transforms[32..48].copy_from_slice(&normal.to_cols_array());
    }
    let object: [(&str, &[f32]); 2] = [
        ("model", &model.to_cols_array()),
        ("normal", &normal.to_cols_array()),
    ];
    put(&mut material.object, &built.object, &object);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_stage_becomes_valid_wgsl_at_the_material_bindings() {
        let root = env!("CARGO_MANIFEST_DIR");
        let bytes = std::fs::read(format!("{root}/samples/shaders/water.frag.spv")).unwrap();
        let text =
            std::fs::read_to_string(format!("{root}/samples/shaders/water.ashader")).unwrap();
        let shader = crate::shader::Shader::parse(&text).unwrap();
        let (wgsl, frame, _) = to_wgsl(&bytes, &shader).unwrap();
        assert!(
            wgsl.contains("@group(3) @binding(0) \nvar<uniform> params"),
            "{wgsl}"
        );
        assert!(wgsl.contains("@align(16) @size(16) foam: Foam,"), "{wgsl}");
        assert_eq!(frame["time"], 48);
        // Bevy parses it again: it must still be valid, uniform layout rules included.
        let module = naga::front::wgsl::parse_str(&wgsl).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap();
    }
}
