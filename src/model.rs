//! glTF models: inspection and compilation to `.amesh`, `.amat`, `.aanim` and textures; see
//! docs/features/models.md.

use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use bevy::asset::RenderAssetUsages;
use bevy::gltf::gltf::{self, Document, Gltf, Node, Primitive, mesh::Mode};
use bevy::math::{Mat3, Mat4, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};

use crate::aanim;
use crate::amesh::{self, Joint, Vertex};
use crate::hash::fnv1a64;
use crate::material::{AlphaMode, HeightMode, Material, TextureIds};
use crate::texture::ColorSpace;

/// Whether `file` is a model assetc reads (`*.gltf`, `*.glb`, any case).
pub fn is_model(file: &Path) -> bool {
    file.extension().is_some_and(|ext| {
        ["gltf", "glb"]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

fn parse(data: &[u8]) -> Result<Gltf, String> {
    Gltf::from_slice(data).map_err(|e| format!("not a valid glTF file: {e}"))
}

/// Inspector rows for the glTF in `data`; see docs/features/models.md#inspector.
pub fn rows(data: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let gltf = parse(data)?;
    let asset = &gltf.as_json().asset;
    let mut rows = Vec::new();
    if let Some(generator) = &asset.generator {
        rows.push(("Generator", generator.clone()));
    }
    rows.push(("Version", asset.version.clone()));
    rows.push(("Scenes", gltf.scenes().len().to_string()));
    rows.push(("Nodes", gltf.nodes().len().to_string()));

    let (mut primitives, mut vertices, mut triangles) = (0, 0, 0);
    for primitive in gltf.meshes().flat_map(|m| m.primitives()) {
        primitives += 1;
        let count = primitive
            .get(&gltf::Semantic::Positions)
            .map_or(0, |a| a.count());
        vertices += count;
        let points = primitive.indices().map_or(count, |a| a.count());
        triangles += match primitive.mode() {
            Mode::Triangles => points / 3,
            Mode::TriangleStrip | Mode::TriangleFan => points.saturating_sub(2),
            _ => 0,
        };
    }
    rows.push((
        "Meshes",
        format!("{} ({primitives} primitives)", gltf.meshes().len()),
    ));
    rows.push(("Vertices", vertices.to_string()));
    rows.push(("Triangles", triangles.to_string()));
    rows.push(("Materials", gltf.materials().len().to_string()));
    rows.push(("Textures", gltf.textures().len().to_string()));
    rows.push(("Skins", gltf.skins().len().to_string()));
    rows.push(("Animations", gltf.animations().len().to_string()));
    Ok(rows)
}

/// Each animation's name (`Animation <n>` without one) and duration in seconds, in file order;
/// see docs/features/models.md#animations-tab.
pub fn animations(data: &[u8]) -> Result<Vec<(String, f32)>, String> {
    let gltf = parse(data)?;
    Ok(gltf
        .animations()
        .map(|animation| {
            let name = animation.name().map_or_else(
                || format!("Animation {}", animation.index() + 1),
                str::to_owned,
            );
            let duration = animation
                .channels()
                .filter_map(|c| c.sampler().input().max()?.as_array()?.first()?.as_f64())
                .fold(0.0, f64::max);
            (name, duration as f32)
        })
        .collect())
}

/// The first skin's named joints, in skin order; see docs/features/locators.md#locator.
pub fn joint_names(data: &[u8]) -> Result<Vec<String>, String> {
    let gltf = parse(data)?;
    Ok(gltf
        .skins()
        .next()
        .map(|skin| {
            skin.joints()
                .filter_map(|j| j.name().filter(|n| !n.is_empty()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default())
}

/// The locator a node stands for: `SOCKET_<name>`, or `extras.locator` (`true` for the
/// node's name, or a name); see docs/features/locators.md#from-the-file.
fn node_locator_name(node: &Node<'_>) -> Option<String> {
    let name = node.name().filter(|n| !n.is_empty());
    if let Some(rest) = name.and_then(|n| n.strip_prefix("SOCKET_")) {
        return (!rest.is_empty()).then(|| rest.to_owned());
    }
    let extras = node.extras().as_ref()?;
    let value: gltf::json::Value = gltf::json::deserialize::from_str(extras.get()).ok()?;
    match value.get("locator")? {
        gltf::json::Value::Bool(true) => name.map(str::to_owned),
        gltf::json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// The locators placed in the glTF itself, in scene order; see
/// docs/features/locators.md#from-the-file.
pub fn file_locators(data: &[u8]) -> Result<Vec<crate::amod::Locator>, String> {
    let gltf = parse(data)?;
    let document = &gltf.document;
    let joints: Vec<(usize, &str)> = document
        .skins()
        .next()
        .map(|skin| {
            skin.joints()
                .filter_map(|j| Some((j.index(), j.name().filter(|n| !n.is_empty())?)))
                .collect()
        })
        .unwrap_or_default();
    /// `to_parent`: the node's parent's transform in the space of `joint` (or the root).
    fn visit(
        node: Node<'_>,
        joint: Option<&str>,
        to_parent: Mat4,
        joints: &[(usize, &str)],
        out: &mut Vec<crate::amod::Locator>,
    ) {
        let transform = to_parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(name) = node_locator_name(&node)
            && out.iter().all(|l| l.name != name)
        {
            let (scale, rotation, translation) = transform.to_scale_rotation_translation();
            out.push(crate::amod::Locator {
                joint: joint.map(str::to_owned),
                translation: translation.to_array(),
                rotation: rotation.normalize().to_array(),
                scale: scale.to_array(),
                ..crate::amod::Locator::new(name)
            });
        }
        // Below a named joint, children hang on it.
        let (joint, to_parent) = match joints.iter().find(|(i, _)| *i == node.index()) {
            Some((_, name)) => (Some(*name), Mat4::IDENTITY),
            None => (joint, transform),
        };
        for child in node.children() {
            visit(child, joint, to_parent, joints, out);
        }
    }
    let mut out = Vec::new();
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for node in scene.nodes() {
            visit(node, None, Mat4::IDENTITY, &joints, &mut out);
        }
    }
    Ok(out)
}

/// The sidecar's locators with their joints as skeleton indices; see
/// docs/features/locators.md#build. `joints` are the skeleton's node names, if it has one.
fn resolve_locators(
    locators: &[crate::amod::Locator],
    joints: Option<&[Option<&str>]>,
) -> Result<Vec<amesh::Locator>, String> {
    locators
        .iter()
        .map(|l| {
            let fail = |problem: String| format!("locator {:?}: {problem}", l.name);
            let joint = match (&l.joint, joints) {
                (None, _) => -1,
                (Some(name), None) => return Err(fail(format!("no skeleton for joint {name:?}"))),
                (Some(name), Some(joints)) => joints
                    .iter()
                    .position(|j| *j == Some(name.as_str()))
                    .ok_or_else(|| fail(format!("no joint {name:?}")))?
                    as i32,
            };
            let rotation = bevy::math::Quat::from_array(l.rotation);
            if rotation.length_squared() == 0.0 {
                return Err(fail("zero rotation".into()));
            }
            Ok(amesh::Locator {
                name: l.name.clone(),
                tags: l.tags.clone(),
                joint,
                translation: l.translation,
                rotation: rotation.normalize().to_array(),
                scale: l.scale,
            })
        })
        .collect()
}

/// A name usable as a file name: the glTF name when non-empty, unique among `names` and free
/// of path separators; otherwise `<prefix>_<index>`. See docs/features/models.md#names.
fn leaves(names: &[Option<&str>], prefix: &str) -> Vec<String> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| match name {
            Some(n)
                if !n.is_empty()
                    && !n.contains(['/', '\\'])
                    && *n != "."
                    && *n != ".."
                    && names.iter().filter(|m| **m == Some(*n)).count() == 1 =>
            {
                (*n).to_owned()
            }
            _ => format!("{prefix}_{i}"),
        })
        .collect()
}

/// A mesh node of the scene with its world transform.
struct Instance<'a> {
    node: Node<'a>,
    world: Mat4,
    /// Bound to the file's first skin: left in mesh space.
    skinned: bool,
}

/// Mesh nodes of the default (or first) scene, depth-first, children in file order.
fn instances(document: &Document) -> Vec<Instance<'_>> {
    fn visit<'a>(node: Node<'a>, parent: Mat4, out: &mut Vec<Instance<'a>>) {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if node.mesh().is_some() {
            out.push(Instance {
                skinned: node.skin().is_some_and(|s| s.index() == 0),
                node: node.clone(),
                world,
            });
        }
        for child in node.children() {
            visit(child, world, out);
        }
    }
    let mut out = Vec::new();
    if let Some(scene) = document
        .default_scene()
        .or_else(|| document.scenes().next())
    {
        for node in scene.nodes() {
            visit(node, Mat4::IDENTITY, &mut out);
        }
    }
    out
}

fn is_triangles(primitive: &Primitive<'_>) -> bool {
    matches!(
        primitive.mode(),
        Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan
    )
}

/// The assets a model compiles to, from its JSON alone; see docs/features/models.md#build.
#[derive(Debug, PartialEq)]
pub struct Outputs {
    /// Used materials in first-use order: glTF index and name.
    pub materials: Vec<(usize, String)>,
    /// Images used as base color, normal or emissive textures: index, name, color space.
    pub images: Vec<(usize, String, ColorSpace)>,
    /// Names of the used materials that get an ORM texture.
    pub orm: Vec<String>,
    /// Skinned, with animations: an `.aanim` too.
    pub animation: bool,
}

pub fn outputs(data: &[u8]) -> Result<Outputs, String> {
    analyze(&parse(data)?.document)
}

fn analyze(document: &Document) -> Result<Outputs, String> {
    let names: Vec<_> = document.materials().map(|m| m.name()).collect();
    let material_names = leaves(&names, "material");
    let names: Vec<_> = document.images().map(|i| i.name()).collect();
    let image_names = leaves(&names, "image");
    let instances = instances(document);
    let mut used = Vec::new();
    let mut triangles = false;
    for instance in &instances {
        for primitive in instance
            .node
            .mesh()
            .into_iter()
            .flat_map(|m| m.primitives())
        {
            if !is_triangles(&primitive) {
                continue;
            }
            triangles = true;
            if let Some(i) = primitive.material().index()
                && !used.contains(&i)
            {
                used.push(i);
            }
        }
    }
    if !triangles {
        return Err("no triangles in the default scene".into());
    }
    let mut images: Vec<(usize, String, ColorSpace)> = Vec::new();
    let mut orm = Vec::new();
    for &i in &used {
        let m = document.materials().nth(i).expect("material index");
        let pbr = m.pbr_metallic_roughness();
        let textures = [
            pbr.base_color_texture()
                .map(|t| (t.texture(), ColorSpace::Srgb)),
            m.normal_texture()
                .map(|t| (t.texture(), ColorSpace::Linear)),
            m.emissive_texture()
                .map(|t| (t.texture(), ColorSpace::Srgb)),
        ];
        // The first use of an image decides its color space.
        for (texture, color) in textures.into_iter().flatten() {
            let image = texture.source().index();
            if !images.iter().any(|(j, _, _)| *j == image) {
                images.push((image, image_names[image].clone(), color));
            }
        }
        if pbr.metallic_roughness_texture().is_some() || m.occlusion_texture().is_some() {
            orm.push(material_names[i].clone());
        }
    }
    Ok(Outputs {
        materials: used
            .iter()
            .map(|&i| (i, material_names[i].clone()))
            .collect(),
        images,
        orm,
        animation: instances.iter().any(|i| i.skinned) && document.animations().len() > 0,
    })
}

/// A compiled model, ready to write; see docs/features/models.md#build.
pub struct Compiled {
    pub mesh: Vec<u8>,
    pub animation: Option<Vec<u8>>,
    /// By name: `<P>/<name>.amat`.
    pub materials: Vec<(String, Material)>,
    /// By name: `<P>/<name>.ktx2`.
    pub images: Vec<(String, image::RgbaImage, ColorSpace)>,
    /// By material name: `<P>/<name>.orm.ktx2`, linear.
    pub orm: Vec<(String, image::RgbaImage)>,
    /// For build output.
    pub detail: String,
}

/// Bytes of a buffer or image URI: a data URI or a file next to the model.
fn load_uri(uri: &str, dir: &Path) -> Result<Vec<u8>, String> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (_, data) = rest
            .split_once(";base64,")
            .ok_or_else(|| "unsupported data URI (not base64)".to_owned())?;
        return base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("bad data URI: {e}"));
    }
    let path = dir.join(uri.replace("%20", " "));
    std::fs::read(&path).map_err(|e| format!("{uri}: {e}"))
}

/// Compiles the model in `file`, whose asset path is `asset_path` (`P`), with the locators of
/// its sidecar.
pub fn compile(
    file: &Path,
    asset_path: &str,
    options: &amesh::MeshOptions,
    locators: &[crate::amod::Locator],
) -> Result<Compiled, String> {
    let data = std::fs::read(file).map_err(|e| e.to_string())?;
    let gltf = parse(&data)?;
    let document = &gltf.document;
    let outputs = analyze(document)?;
    let dir = file.parent().unwrap_or(Path::new("."));
    let buffers: Vec<Vec<u8>> = document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => gltf
                .blob
                .clone()
                .ok_or_else(|| "missing the GLB binary chunk".to_owned()),
            gltf::buffer::Source::Uri(uri) => load_uri(uri, dir),
        })
        .collect::<Result<_, _>>()?;
    let read = |buffer: gltf::Buffer<'_>| buffers.get(buffer.index()).map(Vec::as_slice);

    // Geometry.
    let slots: HashMap<usize, u32> = outputs
        .materials
        .iter()
        .enumerate()
        .map(|(slot, (i, _))| (*i, slot as u32))
        .collect();
    let instances = instances(document);
    let skinned = instances.iter().any(|i| i.skinned);
    let mut primitives = Vec::new();
    let mut tangents = true;
    for instance in &instances {
        for primitive in instance
            .node
            .mesh()
            .into_iter()
            .flat_map(|m| m.primitives())
        {
            if !is_triangles(&primitive) {
                continue;
            }
            let (vertices, indices, has_tangents) = read_primitive(&primitive, &read, instance)?;
            tangents &= has_tangents;
            primitives.push(amesh::Primitive {
                vertices,
                indices,
                material: primitive
                    .material()
                    .index()
                    .and_then(|i| slots.get(&i).copied()),
            });
        }
    }
    if skinned {
        for p in &mut primitives {
            for v in &mut p.vertices {
                let sum: f32 = v.weights.iter().sum();
                if sum > 0.0 {
                    v.weights = v.weights.map(|w| w / sum);
                } else {
                    (v.joints, v.weights) = ([0; 4], [1.0, 0.0, 0.0, 0.0]);
                }
            }
        }
    }
    let skeleton = skinned
        .then(|| document.skins().next())
        .flatten()
        .map(|skin| skeleton(document, &skin, &read));
    let skin_joints: Option<Vec<Option<&str>>> = skeleton.as_ref().and_then(|_| {
        let skin = document.skins().next()?;
        Some(skin.joints().map(|j| j.name()).collect())
    });
    let locators = resolve_locators(locators, skin_joints.as_deref()).map_err(|e| {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        format!("{name}.amod: {e}")
    })?;
    let material_ids: Vec<u64> = outputs
        .materials
        .iter()
        .map(|(_, name)| fnv1a64(format!("{asset_path}/{name}.amat").as_bytes()))
        .collect();
    let (mesh, summary) = amesh::encode(
        &primitives,
        &material_ids,
        skeleton.as_deref(),
        &locators,
        tangents,
        options,
    );

    // Animations, for the skeleton.
    let mut clip_count = 0;
    let animation = match (&skeleton, outputs.animation) {
        (Some(joints), true) => {
            let clips = clips(document, &read)?;
            clip_count = clips.len();
            let mesh_id = fnv1a64(format!("{asset_path}.amesh").as_bytes());
            Some(aanim::encode(&clips, joints.len() as u32, mesh_id))
        }
        _ => None,
    };

    // Images, decoded once.
    let mut decoded: HashMap<usize, image::RgbaImage> = HashMap::new();
    let mut decode = |index: usize| -> Result<image::RgbaImage, String> {
        if let Some(image) = decoded.get(&index) {
            return Ok(image.clone());
        }
        let image = document.images().nth(index).expect("image index");
        let bytes = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let buffer = read(view.buffer()).ok_or("missing buffer")?;
                buffer
                    .get(view.offset()..view.offset() + view.length())
                    .ok_or("image view out of range")?
                    .to_vec()
            }
            gltf::image::Source::Uri { uri, .. } => load_uri(uri, dir)?,
        };
        let rgba = image::load_from_memory(&bytes)
            .map_err(|e| format!("image {index}: {e}"))?
            .to_rgba8();
        decoded.insert(index, rgba.clone());
        Ok(rgba)
    };
    let mut images = Vec::new();
    for (index, name, color) in &outputs.images {
        images.push((name.clone(), decode(*index)?, *color));
    }

    // Materials and their ORM textures.
    let texture_id = |name: &str| fnv1a64(format!("{asset_path}/{name}").as_bytes());
    let image_name = |texture: gltf::Texture<'_>| {
        let index = texture.source().index();
        outputs
            .images
            .iter()
            .find(|(i, _, _)| *i == index)
            .map(|(_, name, _)| name.clone())
    };
    let mut materials = Vec::new();
    let mut orm = Vec::new();
    for (index, name) in &outputs.materials {
        let m = document.materials().nth(*index).expect("material index");
        let pbr = m.pbr_metallic_roughness();
        let id_of = |texture: Option<gltf::Texture<'_>>| {
            texture
                .and_then(image_name)
                .map_or(0, |name| texture_id(&name))
        };
        let occlusion = m.occlusion_texture();
        let metal_rough = pbr.metallic_roughness_texture();
        let mut orm_id = 0;
        if occlusion.is_some() || metal_rough.is_some() {
            let occlusion_image = occlusion
                .as_ref()
                .map(|t| decode(t.texture().source().index()))
                .transpose()?;
            let mr_image = metal_rough
                .as_ref()
                .map(|t| decode(t.texture().source().index()))
                .transpose()?;
            orm.push((name.clone(), pack_orm(occlusion_image, mr_image, name)?));
            orm_id = texture_id(&format!("{name}.orm"));
        }
        let alpha_mode = match m.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask,
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        };
        materials.push((
            name.clone(),
            Material {
                base_color: pbr.base_color_factor(),
                emissive: m.emissive_factor(),
                emissive_strength: m.emissive_strength().unwrap_or(1.0),
                metallic: pbr.metallic_factor(),
                roughness: pbr.roughness_factor(),
                occlusion_strength: occlusion.as_ref().map_or(1.0, |t| t.strength()),
                normal_scale: m.normal_texture().map_or(1.0, |t| t.scale()),
                height_scale: 0.0,
                height_midlevel: 0.5,
                alpha_cutoff: m.alpha_cutoff().unwrap_or(0.5),
                alpha_mode,
                double_sided: m.double_sided(),
                height_mode: HeightMode::Parallax,
                textures: TextureIds {
                    base_color: id_of(pbr.base_color_texture().map(|t| t.texture())),
                    normal: id_of(m.normal_texture().map(|t| t.texture())),
                    orm: orm_id,
                    emissive: id_of(m.emissive_texture().map(|t| t.texture())),
                    height: 0,
                },
            },
        ));
    }

    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut detail = vec![
        plural(summary.vertices, "vertex", "vertices"),
        plural(summary.triangles, "triangle", "triangles"),
        plural(summary.submeshes, "submesh", "submeshes"),
        plural(summary.meshlets, "meshlet", "meshlets"),
    ];
    if let Some(joints) = &skeleton {
        detail.push(plural(joints.len(), "joint", "joints"));
    }
    if animation.is_some() {
        detail.push(plural(clip_count, "clip", "clips"));
    }
    detail.push(plural(materials.len(), "material", "materials"));
    detail.push(plural(images.len() + orm.len(), "texture", "textures"));
    Ok(Compiled {
        mesh,
        animation,
        materials,
        images,
        orm,
        detail: detail.join(", "),
    })
}

/// R from the occlusion image, G and B from the metallic-roughness image (255 without one).
fn pack_orm(
    occlusion: Option<image::RgbaImage>,
    metal_rough: Option<image::RgbaImage>,
    material: &str,
) -> Result<image::RgbaImage, String> {
    let size = match (&occlusion, &metal_rough) {
        (Some(o), Some(m)) if o.dimensions() != m.dimensions() => {
            return Err(format!(
                "material {material}: occlusion is {:?} but metallic-roughness is {:?}",
                o.dimensions(),
                m.dimensions()
            ));
        }
        (Some(o), _) => o.dimensions(),
        (_, Some(m)) => m.dimensions(),
        (None, None) => (1, 1),
    };
    Ok(image::RgbaImage::from_fn(size.0, size.1, |x, y| {
        let r = occlusion.as_ref().map_or(255, |o| o.get_pixel(x, y)[0]);
        let [g, b] = metal_rough.as_ref().map_or([255, 255], |m| {
            let p = m.get_pixel(x, y);
            [p[1], p[2]]
        });
        image::Rgba([r, g, b, 255])
    }))
}

type Read<'s> = dyn Fn(gltf::Buffer<'_>) -> Option<&'s [u8]> + 's;

/// One primitive's vertices and triangle-list indices; static ones baked into world space.
/// Also says whether it has (or got generated) tangents.
fn read_primitive<'s>(
    primitive: &Primitive<'_>,
    read: &Read<'s>,
    instance: &Instance<'_>,
) -> Result<(Vec<Vertex>, Vec<u32>, bool), String> {
    let reader = primitive.reader(read);
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or("primitive without POSITION")?
        .collect();
    let n = positions.len() as u32;
    let raw: Vec<u32> = reader
        .read_indices()
        .map_or_else(|| (0..n).collect(), |i| i.into_u32().collect());
    if raw.iter().any(|&i| i >= n) {
        return Err("index out of range".into());
    }
    let mut indices = match primitive.mode() {
        Mode::TriangleStrip => (2..raw.len())
            .flat_map(|i| {
                if i % 2 == 0 {
                    [raw[i - 2], raw[i - 1], raw[i]]
                } else {
                    [raw[i - 1], raw[i - 2], raw[i]]
                }
            })
            .collect(),
        Mode::TriangleFan => (2..raw.len())
            .flat_map(|i| [raw[i - 1], raw[i], raw[0]])
            .collect(),
        _ => raw[..raw.len() / 3 * 3].to_vec(),
    };
    let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(Iterator::collect);
    let uvs: Option<Vec<[f32; 2]>> = reader.read_tex_coords(0).map(|t| t.into_f32().collect());
    let tangents: Option<Vec<[f32; 4]>> = reader.read_tangents().map(Iterator::collect);
    let joints: Option<Vec<[u16; 4]>> = reader.read_joints(0).map(|j| j.into_u16().collect());
    let weights: Option<Vec<[f32; 4]>> = reader.read_weights(0).map(|w| w.into_f32().collect());
    let mut vertices: Vec<Vertex> = (0..positions.len())
        .map(|i| Vertex {
            position: positions[i],
            normal: normals.as_ref().map_or([0.0; 3], |v| v[i]),
            tangent: tangents.as_ref().map_or([0.0; 4], |v| v[i]),
            uv: uvs.as_ref().map_or([0.0; 2], |v| v[i]),
            joints: joints.as_ref().map_or([0; 4], |v| v[i]),
            weights: weights.as_ref().map_or([0.0; 4], |v| v[i]),
        })
        .collect();

    // Flat normals: every triangle gets its own vertices.
    if normals.is_none() {
        let mut flat = Vec::with_capacity(indices.len());
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(vertices[triangle[k] as usize].position));
            let normal = (b - a).cross(c - a).normalize_or_zero().to_array();
            for &i in triangle {
                flat.push(Vertex {
                    normal,
                    ..vertices[i as usize]
                });
            }
        }
        vertices = flat;
        indices = (0..vertices.len() as u32).collect();
    }

    let mut has_tangents = tangents.is_some();
    if !has_tangents
        && uvs.is_some()
        && !indices.is_empty()
        && let Some(generated) = generate_tangents(&vertices, &indices)
    {
        for (v, t) in vertices.iter_mut().zip(generated) {
            v.tangent = t;
        }
        has_tangents = true;
    }

    if !instance.skinned {
        let world = instance.world;
        let linear = Mat3::from_mat4(world);
        let normal_matrix = linear.inverse().transpose();
        let mirror = if linear.determinant() < 0.0 {
            -1.0
        } else {
            1.0
        };
        for v in &mut vertices {
            v.position = world.transform_point3(Vec3::from(v.position)).to_array();
            v.normal = (normal_matrix * Vec3::from(v.normal))
                .normalize_or_zero()
                .to_array();
            let t = (linear * Vec3::from_slice(&v.tangent[..3])).normalize_or_zero();
            v.tangent = [t.x, t.y, t.z, v.tangent[3] * mirror];
        }
        if mirror < 0.0 {
            // Keep triangles front-facing after a mirroring transform.
            for triangle in indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
    }
    Ok((vertices, indices, has_tangents))
}

/// MikkTSpace tangents, through Bevy's mesh.
fn generate_tangents(vertices: &[Vertex], indices: &[u32]) -> Option<Vec<[f32; 4]>> {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vertices.iter().map(|v| v.position).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vertices.iter().map(|v| v.normal).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vertices.iter().map(|v| v.uv).collect::<Vec<_>>(),
    )
    .with_inserted_indices(Indices::U32(indices.to_vec()));
    mesh.generate_tangents().ok()?;
    match mesh.attribute(Mesh::ATTRIBUTE_TANGENT)? {
        VertexAttributeValues::Float32x4(tangents) => Some(tangents.clone()),
        _ => None,
    }
}

/// The first skin's joints: bind pose, inverse bind matrices and parents.
fn skeleton(document: &Document, skin: &gltf::Skin<'_>, read: &Read<'_>) -> Vec<Joint> {
    let mut parents = HashMap::new();
    for node in document.nodes() {
        for child in node.children() {
            parents.insert(child.index(), node.index());
        }
    }
    let joints: Vec<Node<'_>> = skin.joints().collect();
    let index_of = |node: usize| joints.iter().position(|j| j.index() == node);
    let inverse_binds: Vec<[[f32; 4]; 4]> = skin
        .reader(read)
        .read_inverse_bind_matrices()
        .map(Iterator::collect)
        .unwrap_or_default();
    joints
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let (translation, rotation, scale) = node.transform().decomposed();
            let inverse_bind = inverse_binds
                .get(i)
                .map_or(Mat4::IDENTITY, Mat4::from_cols_array_2d);
            Joint {
                inverse_bind: inverse_bind.to_cols_array(),
                translation,
                rotation,
                scale,
                parent: parents
                    .get(&node.index())
                    .and_then(|&p| index_of(p))
                    .map_or(-1, |p| p as i32),
            }
        })
        .collect()
}

/// Every animation, keeping channels on the first skin's joints.
fn clips(document: &Document, read: &Read<'_>) -> Result<Vec<aanim::Clip>, String> {
    use gltf::animation::{Interpolation, Property, util::ReadOutputs};
    let Some(skin) = document.skins().next() else {
        return Ok(Vec::new());
    };
    let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
    let mut clips = Vec::new();
    for (i, animation) in document.animations().enumerate() {
        let mut channels = Vec::new();
        for channel in animation.channels() {
            let Some(joint) = joints
                .iter()
                .position(|&j| j == channel.target().node().index())
            else {
                continue;
            };
            let (path, components) = match channel.target().property() {
                Property::Translation => (0, 3),
                Property::Rotation => (1, 4),
                Property::Scale => (2, 3),
                Property::MorphTargetWeights => continue,
            };
            let reader = channel.reader(read);
            let times: Vec<f32> = reader
                .read_inputs()
                .ok_or("animation channel without input")?
                .collect();
            let mut values: Vec<f32> = match reader
                .read_outputs()
                .ok_or("animation channel without output")?
            {
                ReadOutputs::Translations(t) => t.flatten().collect(),
                ReadOutputs::Rotations(r) => r.into_f32().flatten().collect(),
                ReadOutputs::Scales(s) => s.flatten().collect(),
                ReadOutputs::MorphTargetWeights(_) => continue,
            };
            let cubic = channel.sampler().interpolation() == Interpolation::CubicSpline;
            if cubic {
                // Keep the value of each in-tangent / value / out-tangent triple.
                values = values
                    .chunks(components)
                    .skip(1)
                    .step_by(3)
                    .flatten()
                    .copied()
                    .collect();
            }
            channels.push(aanim::Channel {
                joint: joint as u32,
                path,
                interpolation: u8::from(channel.sampler().interpolation() != Interpolation::Step),
                components: components as u8,
                times,
                values,
            });
        }
        clips.push(aanim::Clip {
            name: animation
                .name()
                .filter(|n| !n.is_empty())
                .map_or_else(|| format!("Animation {}", i + 1), str::to_owned),
            channels,
        });
    }
    Ok(clips)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOX: &[u8] = include_bytes!("../samples/Fox.glb");

    fn row<'a>(rows: &'a [(&str, String)], label: &str) -> &'a str {
        &rows.iter().find(|(l, _)| *l == label).unwrap().1
    }

    #[test]
    fn models_by_extension() {
        assert!(is_model(Path::new("a/Fox.GLB")));
        assert!(is_model(Path::new("scene.gltf")));
        assert!(!is_model(Path::new("mesh.obj")));
    }

    #[test]
    fn rows_count_the_document() {
        let rows = rows(FOX).unwrap();
        assert_eq!(row(&rows, "Version"), "2.0");
        assert_eq!(row(&rows, "Meshes"), "1 (1 primitives)");
        assert_eq!(row(&rows, "Skins"), "1");
        assert_eq!(row(&rows, "Animations"), "3");
        assert!(row(&rows, "Vertices").parse::<u32>().unwrap() > 0);
        assert!(row(&rows, "Triangles").parse::<u32>().unwrap() > 0);
    }

    #[test]
    fn json_without_buffers_is_enough() {
        let json = r#"{
            "asset": {"version": "2.0", "generator": "hand"},
            "buffers": [{"byteLength": 36, "uri": "missing.bin"}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                           "min": [0, 0, 0], "max": [1, 1, 0]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [{"mesh": 0}, {"mesh": 0}],
            "scenes": [{"nodes": [0, 1]}]
        }"#;
        let rows = rows(json.as_bytes()).unwrap();
        assert_eq!(row(&rows, "Generator"), "hand");
        assert_eq!(row(&rows, "Nodes"), "2");
        // Per mesh, not per instance.
        assert_eq!(row(&rows, "Vertices"), "3");
        assert_eq!(row(&rows, "Triangles"), "1");
    }

    #[test]
    fn animations_have_names_and_durations() {
        let animations = animations(FOX).unwrap();
        let names: Vec<_> = animations.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Survey", "Walk", "Run"]);
        assert!(animations.iter().all(|(_, d)| *d > 0.0));
    }

    /// One triangle without normals, under a node moved by +10 on X, with a named material.
    fn static_triangle() -> String {
        let floats: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let data = base64::engine::general_purpose::STANDARD.encode(floats);
        format!(
            r#"{{
            "asset": {{"version": "2.0"}},
            "buffers": [{{"byteLength": 36, "uri": "data:application/octet-stream;base64,{data}"}}],
            "bufferViews": [{{"buffer": 0, "byteLength": 36}}],
            "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                           "min": [0, 0, 0], "max": [1, 1, 0]}}],
            "materials": [{{"name": "Paint", "pbrMetallicRoughness": {{"baseColorFactor": [1, 0, 0, 1]}}}},
                          {{"name": "unused"}}],
            "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "material": 0}}]}}],
            "nodes": [{{"mesh": 0, "translation": [10, 0, 0]}}],
            "scenes": [{{"nodes": [0]}}]
        }}"#
        )
    }

    #[test]
    fn names_are_unique_and_safe() {
        let names = leaves(
            &[Some("Wood"), None, Some("a/b"), Some("Dup"), Some("Dup")],
            "m",
        );
        assert_eq!(names, ["Wood", "m_1", "m_2", "m_3", "m_4"]);
    }

    #[test]
    fn static_models_bake_transforms_and_flat_normals() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tri.gltf");
        std::fs::write(&file, static_triangle()).unwrap();
        let outputs = outputs(&std::fs::read(&file).unwrap()).unwrap();
        // Only used materials.
        assert_eq!(outputs.materials, [(0, "Paint".to_owned())]);
        assert!(!outputs.animation);

        let compiled = compile(&file, "props/tri", &amesh::MeshOptions::default(), &[]).unwrap();
        assert!(compiled.animation.is_none());
        assert_eq!(compiled.materials[0].0, "Paint");
        assert_eq!(compiled.materials[0].1.base_color, [1.0, 0.0, 0.0, 1.0]);
        let reader = crate::chunks::Reader::parse(&compiled.mesh, "AMESH").unwrap();
        let vertices = reader.require("VERTICES").unwrap();
        assert_eq!(vertices.len(), 3 * 28);
        let xs: Vec<f32> = (0..3)
            .map(|i| crate::chunks::f32_at(vertices, i, 28, 0).unwrap())
            .collect();
        assert!(xs.iter().all(|&x| x >= 10.0), "{xs:?}");
        // Flat normal +Z encodes as (0, 0).
        assert_eq!(&vertices[12..16], &[0, 0, 0, 0]);
        assert_eq!(
            reader.require("MATERIAL").unwrap(),
            fnv1a64(b"props/tri/Paint.amat").to_le_bytes()
        );
        assert!(
            compiled
                .detail
                .starts_with("3 vertices, 1 triangle, 1 submesh, 1 meshlet")
        );
    }

    #[test]
    fn locators_resolve_joints_by_name() {
        use crate::amod::Locator;
        let mut hand = Locator::new("hand".into());
        hand.joint = Some("b".into());
        hand.rotation = [0.0, 0.0, 0.0, 2.0];
        let joints = [Some("a"), None, Some("b"), Some("b")];
        let resolved =
            resolve_locators(&[hand.clone(), Locator::new("root".into())], Some(&joints)).unwrap();
        assert_eq!(resolved[0].joint, 2, "the first joint with the name");
        assert_eq!(resolved[0].rotation, [0.0, 0.0, 0.0, 1.0], "normalized");
        assert_eq!(resolved[1].joint, -1);

        hand.joint = Some("c".into());
        let err = resolve_locators(std::slice::from_ref(&hand), Some(&joints)).err();
        assert_eq!(err.as_deref(), Some(r#"locator "hand": no joint "c""#));
        let err = resolve_locators(std::slice::from_ref(&hand), None).err();
        assert_eq!(
            err.as_deref(),
            Some(r#"locator "hand": no skeleton for joint "c""#)
        );
        let mut zero = Locator::new("z".into());
        zero.rotation = [0.0; 4];
        assert!(resolve_locators(&[zero], None).is_err());

        // The Fox's joints, by name.
        let names = joint_names(FOX).unwrap();
        assert_eq!(names.len(), 24);
        hand.joint = Some(names[5].clone());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Fox.glb");
        std::fs::write(&file, FOX).unwrap();
        let compiled = compile(&file, "Fox", &amesh::MeshOptions::default(), &[hand]).unwrap();
        let reader = crate::chunks::Reader::parse(&compiled.mesh, "AMESH").unwrap();
        let locators = reader.require("LOCATORS").unwrap();
        assert_eq!(crate::chunks::u32_at(locators, 0, 64, 16), Some(5));
        let mut static_hand = Locator::new("hand".into());
        static_hand.joint = Some("x".into());
        let tri = dir.path().join("tri.gltf");
        std::fs::write(&tri, static_triangle()).unwrap();
        let err = compile(&tri, "tri", &amesh::MeshOptions::default(), &[static_hand]).err();
        assert_eq!(
            err.as_deref(),
            Some(r#"tri.gltf.amod: locator "hand": no skeleton for joint "x""#)
        );
    }

    #[test]
    fn invalid_files_fail() {
        assert!(
            rows(b"nope")
                .unwrap_err()
                .starts_with("not a valid glTF file")
        );
    }
}
