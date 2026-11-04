//! `.amesh.bin` meshes; see docs/spec/amesh.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::read::Bytes;
use crate::structs::Structs;

const FILE: &str = "amesh.bin";
const VERTEX: usize = 28;
const NO_MATERIAL: u32 = 0xFFFF_FFFF;

/// Axis-aligned box and bounding sphere; see docs/spec/amesh.bin.md#bounds-40-bytes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bounds {
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    /// The box's center.
    pub sphere_center: [f32; 3],
    pub sphere_radius: f32,
}

/// One vertex, normal and tangent unpacked from their octahedral encoding.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    pub position: [f32; 3],
    /// Unit length; a zero normal reads as `+Z`.
    pub normal: [f32; 3],
    /// Unit `x y z`, `w` the handedness (`1` or `-1`).
    pub tangent: [f32; 4],
    /// `TEXCOORD_0`, top-left origin.
    pub uv: [f32; 2],
}

/// A range of indices; see docs/spec/amesh.bin.md#lodtable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexRange {
    pub first_index: u32,
    pub index_count: u32,
}

/// One glTF primitive: indices and meshlets sharing one material.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SubMesh {
    /// Into [`Mesh::indices`].
    pub first_index: u32,
    pub index_count: u32,
    /// Into [`Mesh::meshlets`].
    pub first_meshlet: u32,
    pub meshlet_count: u32,
    /// Index into [`Mesh::materials`], `None` for no material.
    pub material: Option<u32>,
    pub bounds: Bounds,
    /// Reduced levels of detail, ranges into [`Mesh::lod_indices`]; empty without `LODTABLE`.
    /// An `index_count` of `0` means no simpler level was reached.
    pub lods: Vec<IndexRange>,
}

/// See docs/spec/amesh.bin.md#meshlet-16-bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Meshlet {
    /// Into [`Mesh::meshlet_vertices`].
    pub vertex_offset: u32,
    pub vertex_count: u32,
    /// Into [`Mesh::meshlet_triangles`], in triangles.
    pub triangle_offset: u32,
    pub triangle_count: u32,
}

/// Bounding sphere and normal cone of a meshlet.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeshletBounds {
    pub center: [f32; 3],
    pub radius: f32,
    pub cone_axis: [f32; 3],
    pub cone_cutoff: f32,
}

/// A vertex's joints and weights, parallel to [`Mesh::vertices`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkinVertex {
    /// Indices into [`Mesh::skeleton`].
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

/// A skeleton joint; see docs/spec/amesh.bin.md#joint-112-bytes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Joint {
    /// Column-major, mesh space to joint bind space.
    pub inverse_bind: [f32; 16],
    /// The joint's local bind-pose transform.
    pub bind_translation: [f32; 3],
    /// Unit quaternion `x y z w`.
    pub bind_rotation: [f32; 4],
    pub bind_scale: [f32; 3],
    /// Index of the parent joint, `None` for a root.
    pub parent: Option<usize>,
}

/// A tag of a locator or region: its text and FNV-1a-64 hash.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tag {
    pub hash: u64,
    pub text: String,
}

/// A named transform; see docs/spec/amesh.bin.md#locators.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Locator {
    /// FNV-1a-64 of the name; look locators up by it.
    pub name_hash: u64,
    pub name: String,
    /// Parent joint, `None` for mesh space.
    pub joint: Option<usize>,
    pub translation: [f32; 3],
    /// Unit quaternion `x y z w`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub tags: Vec<Tag>,
}

/// Per-vertex RGBA8 colors; see docs/spec/amesh.bin.md#color-sets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ColorSet {
    pub name_hash: u64,
    pub name: String,
    /// `0` the glTF's `COLOR_n`, `1` made by assetc.
    pub source: u32,
    /// Unsigned normalized, linear; parallel to [`Mesh::vertices`].
    pub colors: Vec<[u8; 4]>,
}

/// Extra texture coordinates; see docs/spec/amesh.bin.md#uv-sets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UvSet {
    pub name_hash: u64,
    pub name: String,
    /// Asset ID of the texture the coordinates sample.
    pub texture: u64,
    /// Parallel to [`Mesh::vertices`].
    pub uvs: Vec<[f32; 2]>,
}

use crate::property::properties_of;
pub use crate::property::{Property, PropertyValue};

/// What a [`Shape`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeType {
    /// `size` is the full size along X, Y, Z.
    #[default]
    Box,
    /// `size[0]` is the radius.
    Sphere,
    /// `size[0]` is the radius, `size[1]` the distance between the half-spheres' centers.
    Capsule,
    /// The volume of [`Shape::points`].
    Hull,
}

/// A simple volume; see docs/spec/amesh.bin.md#shapes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape {
    pub name_hash: u64,
    pub name: String,
    /// FNV-1a-64 of the kind (of the empty text when none).
    pub kind_hash: u64,
    pub kind: String,
    /// Parent joint, `None` for mesh space.
    pub joint: Option<usize>,
    pub shape_type: ShapeType,
    pub translation: [f32; 3],
    /// Unit quaternion `x y z w`.
    pub rotation: [f32; 4],
    pub size: [f32; 3],
    /// A hull's points, in the parent's space.
    pub points: Vec<[f32; 3]>,
    pub properties: Vec<Property>,
}

/// A named set of triangles; see docs/spec/amesh.bin.md#regions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Region {
    pub name_hash: u64,
    pub name: String,
    pub tags: Vec<Tag>,
    /// Triangles of [`Mesh::indices`] in it.
    pub triangle_count: u32,
}

/// A point of a path, evenly spaced by distance.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathSample {
    pub position: [f32; 3],
    /// From the start along the curve.
    pub distance: f32,
    /// Unit direction of travel, zero where the curve stops.
    pub tangent: [f32; 3],
    /// The spline parameter.
    pub u: f32,
}

/// A curve; see docs/spec/amesh.bin.md#paths.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path {
    pub name_hash: u64,
    pub name: String,
    pub kind_hash: u64,
    pub kind: String,
    /// Parent joint, `None` for mesh space.
    pub joint: Option<usize>,
    pub closed: bool,
    pub tension: f32,
    pub alpha: f32,
    pub length: f32,
    /// Control points of the cardinal spline.
    pub points: Vec<[f32; 3]>,
    pub samples: Vec<PathSample>,
    pub properties: Vec<Property>,
}

/// A compiled mesh; optional chunks read as empty.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    /// Bit 0 tangents valid, bit 1 skinned, bit 2 levels of detail present.
    pub flags: u8,
    pub meshlet_max_vertices: u32,
    pub meshlet_max_triangles: u32,
    /// Of the whole mesh (bind pose when skinned).
    pub bounds: Bounds,
    pub vertices: Vec<Vertex>,
    /// Triangle lists, global into [`Mesh::vertices`], widened to `u32`.
    pub indices: Vec<u32>,
    pub submeshes: Vec<SubMesh>,
    /// Material asset IDs (`.amat.bin`).
    pub materials: Vec<u64>,
    pub meshlets: Vec<Meshlet>,
    /// Global vertex indices.
    pub meshlet_vertices: Vec<u32>,
    /// Indices into each meshlet's own `meshlet_vertices` range.
    pub meshlet_triangles: Vec<[u8; 3]>,
    /// Parallel to [`Mesh::meshlets`].
    pub meshlet_bounds: Vec<MeshletBounds>,
    /// Reduced levels per submesh (`LODTABLE.lod_count`), `0` without levels of detail.
    pub lod_count: u32,
    /// Index data of the reduced levels, global into [`Mesh::vertices`].
    pub lod_indices: Vec<u32>,
    /// Parallel to [`Mesh::vertices`]; empty unless skinned.
    pub skin: Vec<SkinVertex>,
    pub skeleton: Vec<Joint>,
    pub locators: Vec<Locator>,
    pub color_sets: Vec<ColorSet>,
    pub uv_sets: Vec<UvSet>,
    /// The model's properties, sorted by key.
    pub properties: Vec<Property>,
    pub shapes: Vec<Shape>,
    pub regions: Vec<Region>,
    /// Per triangle of [`Mesh::indices`], its regions as bits (region `i` is `1 << i`).
    pub triangle_regions: Vec<u32>,
    pub paths: Vec<Path>,
    /// Attached structs: [`Scope::Asset`](crate::structs::Scope::Asset) the model.
    pub structs: Structs,
}

impl Mesh {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "AMESH", &[1], FILE)?;
        let chunk = |name| Bytes::new(file.chunk(name).unwrap_or_default(), FILE);
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));

        let desc = require("DESC")?;
        let vertex_count = desc.u32(0)? as usize;
        let index_count = desc.u32(4)? as usize;
        let meshlet_count = desc.u32(8)? as usize;
        let submesh_count = desc.u32(12)? as usize;
        let material_count = desc.u32(16)? as usize;
        let stride = desc.u16(20)? as usize;
        let index_width = desc.u8(22)?;
        if stride < VERTEX {
            return Err(desc.err(format!("vertex stride {stride} is below {VERTEX}")));
        }
        if !index_count.is_multiple_of(3) {
            return Err(desc.err(format!("index count {index_count} is not a multiple of 3")));
        }

        let vertices = require("VERTICES")?
            .records_n(stride, vertex_count)?
            .map(|v| vertex(&v))
            .collect::<Result<Vec<_>>>()?;
        let raw = require("INDICES")?;
        let indices: Vec<u32> = match index_width {
            2 => raw
                .records_n(2, index_count)?
                .map(|i| i.u16(0).map(u32::from))
                .collect::<Result<_>>()?,
            4 => raw
                .records_n(4, index_count)?
                .map(|i| i.u32(0))
                .collect::<Result<_>>()?,
            w => return Err(raw.err(format!("index width {w} is not 2 or 4"))),
        };
        check_indices(raw, &indices, vertex_count, "INDICES")?;

        let materials = require("MATERIAL")?
            .records_n(8, material_count)?
            .map(|m| m.u64(0))
            .collect::<Result<Vec<_>>>()?;
        let mut submeshes = require("SUBMESH")?
            .records_n(64, submesh_count)?
            .map(|s| {
                let material = s.u32(16)?;
                Ok(SubMesh {
                    first_index: s.u32(0)?,
                    index_count: s.u32(4)?,
                    first_meshlet: s.u32(8)?,
                    meshlet_count: s.u32(12)?,
                    material: (material != NO_MATERIAL).then_some(material),
                    bounds: bounds(&s, 24)?,
                    lods: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for s in &submeshes {
            check_range(
                desc,
                s.first_index,
                s.index_count,
                indices.len(),
                "submesh indices",
            )?;
            check_range(
                desc,
                s.first_meshlet,
                s.meshlet_count,
                meshlet_count,
                "submesh meshlets",
            )?;
            if s.material.is_some_and(|m| m as usize >= materials.len()) {
                return Err(desc.err("submesh material past MATERIAL"));
            }
        }

        let meshlets = require("MESHLETS")?
            .records_n(16, meshlet_count)?
            .map(|m| {
                Ok(Meshlet {
                    vertex_offset: m.u32(0)?,
                    vertex_count: m.u32(4)?,
                    triangle_offset: m.u32(8)?,
                    triangle_count: m.u32(12)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mlverts = require("MLVERTS")?;
        let meshlet_vertices = mlverts
            .records(4)?
            .map(|v| v.u32(0))
            .collect::<Result<Vec<_>>>()?;
        check_indices(mlverts, &meshlet_vertices, vertex_count, "MLVERTS")?;
        let meshlet_triangles = require("MLTRIS")?
            .records(3)?
            .map(|t| t.array::<3>(0))
            .collect::<Result<Vec<_>>>()?;
        for m in &meshlets {
            check_range(
                desc,
                m.vertex_offset,
                m.vertex_count,
                meshlet_vertices.len(),
                "meshlet vertices",
            )?;
            check_range(
                desc,
                m.triangle_offset,
                m.triangle_count,
                meshlet_triangles.len(),
                "meshlet triangles",
            )?;
        }
        let meshlet_bounds = require("MLBOUNDS")?
            .records_n(32, meshlet_count)?
            .map(|b| {
                Ok(MeshletBounds {
                    center: b.f32s(0)?,
                    radius: b.f32(12)?,
                    cone_axis: b.f32s(16)?,
                    cone_cutoff: b.f32(28)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Levels of detail: LODIDX and LODTABLE come together.
        let (mut lod_count, mut lod_indices) = (0, Vec::new());
        if let (Some(idx), Some(table)) = (file.chunk("LODIDX"), file.chunk("LODTABLE")) {
            let (idx, table) = (Bytes::new(idx, FILE), Bytes::new(table, FILE));
            lod_indices = idx
                .records(4)?
                .map(|i| i.u32(0))
                .collect::<Result<Vec<_>>>()?;
            check_indices(idx, &lod_indices, vertex_count, "LODIDX")?;
            lod_count = table.u32(0)?;
            if table.u32(4)? as usize != submeshes.len() {
                return Err(table.err("LODTABLE submesh count differs from DESC"));
            }
            let levels = lod_count as usize;
            let ranges = Bytes::new(table.slice(8, table.bytes.len().saturating_sub(8))?, FILE);
            let mut ranges = ranges.records_n(8, submeshes.len().saturating_mul(levels))?;
            for s in &mut submeshes {
                for r in ranges.by_ref().take(levels) {
                    let range = IndexRange {
                        first_index: r.u32(0)?,
                        index_count: r.u32(4)?,
                    };
                    check_range(
                        table,
                        range.first_index,
                        range.index_count,
                        lod_indices.len(),
                        "LOD range",
                    )?;
                    s.lods.push(range);
                }
            }
        }

        let skeleton = chunk("SKELETON")
            .records(112)?
            .map(|j| {
                Ok(Joint {
                    inverse_bind: j.f32s(0)?,
                    bind_translation: j.f32s(64)?,
                    bind_rotation: j.f32s(76)?,
                    bind_scale: j.f32s(92)?,
                    parent: usize::try_from(j.i32(104)?).ok(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if skeleton
            .iter()
            .any(|j| j.parent.is_some_and(|p| p >= skeleton.len()))
        {
            return Err(desc.err("joint parent past SKELETON"));
        }
        let skin = match file.chunk("SKIN") {
            None => Vec::new(),
            Some(skin) => Bytes::new(skin, FILE)
                .records_n(24, vertex_count)?
                .map(|s| {
                    let joints = [s.u16(0)?, s.u16(2)?, s.u16(4)?, s.u16(6)?];
                    if joints.iter().any(|&j| usize::from(j) >= skeleton.len()) {
                        return Err(s.err("skin joint past SKELETON"));
                    }
                    Ok(SkinVertex {
                        joints,
                        weights: s.f32s(8)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };

        let names = chunk("LOCNAMES");
        let locator_tags = chunk("LOCTAGS")
            .records(16)?
            .map(|t| tag(&t, names))
            .collect::<Result<Vec<_>>>()?;
        let locators = chunk("LOCATORS")
            .records(64)?
            .map(|l| {
                Ok(Locator {
                    name_hash: l.u64(0)?,
                    name: l.text_ref(8, names)?,
                    joint: usize::try_from(l.i32(16)?).ok(),
                    translation: l.f32s(20)?,
                    rotation: l.f32s(32)?,
                    scale: l.f32s(48)?,
                    tags: run(&locator_tags, l.u16(60)?.into(), l.u16(62)?.into(), &l)?.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let strings = chunk("STRINGS");
        let colors = chunk("COLORS");
        let mut color_sets = Vec::new();
        for (s, set) in chunk("COLORSET").records(32)?.enumerate() {
            // Every format's entries are 4 bytes; sets of unknown formats are skipped.
            let data = run_bytes(colors, 4, s.saturating_mul(vertex_count), vertex_count)?;
            if set.u32(16)? != 0 {
                continue;
            }
            color_sets.push(ColorSet {
                name_hash: set.u64(0)?,
                name: set.text_ref(8, strings)?,
                source: set.u32(20)?,
                colors: data
                    .records(4)?
                    .map(|c| c.array::<4>(0))
                    .collect::<Result<_>>()?,
            });
        }
        let uvs = chunk("UVS");
        let uv_sets = chunk("UVSET")
            .records(32)?
            .enumerate()
            .map(|(s, set)| {
                let data = run_bytes(uvs, 8, s.saturating_mul(vertex_count), vertex_count)?;
                Ok(UvSet {
                    name_hash: set.u64(0)?,
                    name: set.text_ref(8, strings)?,
                    texture: set.u64(16)?,
                    uvs: data
                        .records(8)?
                        .map(|uv| uv.f32s(0))
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let properties = properties_of(chunk("PROPS"), strings)?;
        let shape_points = points(chunk("SHAPEPTS"))?;
        let shape_props = properties_of(chunk("SHAPEPRP"), strings)?;
        let shapes = chunk("SHAPES")
            .records(96)?
            .map(|s| {
                let shape_type = match s.u32(36)? {
                    0 => ShapeType::Box,
                    1 => ShapeType::Sphere,
                    2 => ShapeType::Capsule,
                    3 => ShapeType::Hull,
                    t => return Err(s.err(format!("unknown shape type {t}"))),
                };
                Ok(Shape {
                    name_hash: s.u64(0)?,
                    name: s.text_ref(8, strings)?,
                    kind_hash: s.u64(16)?,
                    kind: s.text_ref(24, strings)?,
                    joint: usize::try_from(s.i32(32)?).ok(),
                    shape_type,
                    translation: s.f32s(40)?,
                    rotation: s.f32s(52)?,
                    size: s.f32s(68)?,
                    points: run(&shape_points, s.u32(80)? as usize, s.u32(84)? as usize, &s)?
                        .to_vec(),
                    properties: run(&shape_props, s.u32(88)? as usize, s.u32(92)? as usize, &s)?
                        .to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let region_tags = chunk("REGTAGS")
            .records(16)?
            .map(|t| tag(&t, strings))
            .collect::<Result<Vec<_>>>()?;
        let regions = chunk("REGIONS")
            .records(32)?
            .map(|r| {
                Ok(Region {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    tags: run(&region_tags, r.u16(16)?.into(), r.u16(18)?.into(), &r)?.to_vec(),
                    triangle_count: r.u32(20)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let triangle_regions = match file.chunk("TRIREGN") {
            None => Vec::new(),
            Some(masks) => Bytes::new(masks, FILE)
                .records_n(4, index_count / 3)?
                .map(|m| m.u32(0))
                .collect::<Result<Vec<_>>>()?,
        };

        let path_points = points(chunk("PATHPTS"))?;
        let path_samples = chunk("PATHSMP")
            .records(32)?
            .map(|s| {
                Ok(PathSample {
                    position: s.f32s(0)?,
                    distance: s.f32(12)?,
                    tangent: s.f32s(16)?,
                    u: s.f32(28)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let path_props = properties_of(chunk("PATHPRP"), strings)?;
        let paths = chunk("PATHS")
            .records(80)?
            .map(|p| {
                Ok(Path {
                    name_hash: p.u64(0)?,
                    name: p.text_ref(8, strings)?,
                    kind_hash: p.u64(16)?,
                    kind: p.text_ref(24, strings)?,
                    joint: usize::try_from(p.i32(32)?).ok(),
                    closed: p.u32(36)? & 1 != 0,
                    tension: p.f32(40)?,
                    alpha: p.f32(44)?,
                    length: p.f32(48)?,
                    points: run(&path_points, p.u32(52)? as usize, p.u32(56)? as usize, &p)?
                        .to_vec(),
                    samples: run(&path_samples, p.u32(60)? as usize, p.u32(64)? as usize, &p)?
                        .to_vec(),
                    properties: run(&path_props, p.u32(68)? as usize, p.u32(72)? as usize, &p)?
                        .to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            flags: desc.u8(23)?,
            meshlet_max_vertices: desc.u32(24)?,
            meshlet_max_triangles: desc.u32(28)?,
            bounds: bounds(&require("BOUNDS")?, 0)?,
            vertices,
            indices,
            submeshes,
            materials,
            meshlets,
            meshlet_vertices,
            meshlet_triangles,
            meshlet_bounds,
            lod_count,
            lod_indices,
            skin,
            skeleton,
            locators,
            color_sets,
            uv_sets,
            properties,
            shapes,
            regions,
            triangle_regions,
            paths,
            structs: Structs::from_chunks(&file, FILE)?,
        })
    }

    /// Whether the tangents are valid (`flags` bit 0).
    pub fn has_tangents(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Whether the mesh has a skin and skeleton (`flags` bit 1).
    pub fn is_skinned(&self) -> bool {
        self.flags & 2 != 0
    }

    /// The indices of `submesh` at level of detail `level` (`0` the full mesh, `k` the
    /// `k`-th reduced level), falling back to the nearest coarser level that has indices;
    /// `None` for a submesh or level the mesh does not have.
    pub fn level_indices(&self, submesh: usize, level: usize) -> Option<&[u32]> {
        let s = self.submeshes.get(submesh)?;
        if level > s.lods.len() {
            return None;
        }
        let (all, r) = match s.lods[..level].iter().rev().find(|r| r.index_count > 0) {
            Some(&r) => (&self.lod_indices, r),
            None => (
                &self.indices,
                IndexRange {
                    first_index: s.first_index,
                    index_count: s.index_count,
                },
            ),
        };
        all.get(r.first_index as usize..)?
            .get(..r.index_count as usize)
    }

    /// The locator whose name hashes to `name_hash`.
    pub fn locator(&self, name_hash: u64) -> Option<&Locator> {
        self.locators.iter().find(|l| l.name_hash == name_hash)
    }

    /// The model property whose key hashes to `key_hash`.
    pub fn property(&self, key_hash: u64) -> Option<&Property> {
        self.properties.iter().find(|p| p.key_hash == key_hash)
    }
}

fn vertex(v: &Bytes<'_>) -> Result<Vertex> {
    let tangent = i16s(v, 16)?;
    let [x, y, z] = octahedral([tangent[0] & !1, tangent[1]]);
    Ok(Vertex {
        position: v.f32s(0)?,
        normal: octahedral(i16s(v, 12)?),
        tangent: [x, y, z, if tangent[0] & 1 == 1 { -1.0 } else { 1.0 }],
        uv: v.f32s(20)?,
    })
}

/// Decodes an octahedral SNORM16 pair; see docs/spec/amesh.bin.md#octahedral-encoding.
fn octahedral([u, v]: [i16; 2]) -> [f32; 3] {
    let (mut x, mut y) = (f32::from(u) / 32767.0, f32::from(v) / 32767.0);
    let z = 1.0 - x.abs() - y.abs();
    if z < 0.0 {
        let sign = |s: f32| if s >= 0.0 { 1.0 } else { -1.0 };
        (x, y) = ((1.0 - y.abs()) * sign(x), (1.0 - x.abs()) * sign(y));
    }
    let length = (x * x + y * y + z * z).sqrt().max(f32::EPSILON);
    [x / length, y / length, z / length]
}

/// Two i16s at `at`.
fn i16s(b: &Bytes<'_>, at: usize) -> Result<[i16; 2]> {
    Ok([b.u16(at)? as i16, b.u16(at + 2)? as i16])
}

fn bounds(b: &Bytes<'_>, at: usize) -> Result<Bounds> {
    Ok(Bounds {
        aabb_min: b.f32s(at)?,
        aabb_max: b.f32s(at + 12)?,
        sphere_center: b.f32s(at + 24)?,
        sphere_radius: b.f32(at + 36)?,
    })
}

fn tag(t: &Bytes<'_>, text: Bytes<'_>) -> Result<Tag> {
    Ok(Tag {
        hash: t.u64(0)?,
        text: t.text_ref(8, text)?,
    })
}

fn points(chunk: Bytes<'_>) -> Result<Vec<[f32; 3]>> {
    chunk.records(12)?.map(|p| p.f32s(0)).collect()
}

/// Records `first .. first + count` of `all`, checked; `at` names the file in errors.
pub(crate) fn run<'a, T>(
    all: &'a [T],
    first: usize,
    count: usize,
    at: &Bytes<'_>,
) -> Result<&'a [T]> {
    first
        .checked_add(count)
        .and_then(|end| all.get(first..end))
        .ok_or_else(|| {
            at.err(format!(
                "range {first} + {count} runs past {} records",
                all.len()
            ))
        })
}

/// Records `first .. first + count` of `stride` bytes in `chunk`.
fn run_bytes<'a>(chunk: Bytes<'a>, stride: usize, first: usize, count: usize) -> Result<Bytes<'a>> {
    let at = first.checked_mul(stride);
    let len = count.checked_mul(stride);
    match (at, len) {
        (Some(at), Some(len)) => Ok(Bytes::new(chunk.slice(at, len)?, chunk.file)),
        _ => Err(chunk.err("range overflows")),
    }
}

fn check_range(b: Bytes<'_>, first: u32, count: u32, len: usize, what: &str) -> Result<()> {
    match (first as usize).checked_add(count as usize) {
        Some(end) if end <= len => Ok(()),
        _ => Err(b.err(format!("{what} {first} + {count} run past {len}"))),
    }
}

fn check_indices(b: Bytes<'_>, indices: &[u32], vertex_count: usize, what: &str) -> Result<()> {
    match indices.iter().find(|&&i| i as usize >= vertex_count) {
        Some(i) => Err(b.err(format!("{what}: index {i} past {vertex_count} vertices"))),
        None => Ok(()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::tests::file;

    /// Little-endian bytes being written.
    #[derive(Default)]
    pub struct Out(pub Vec<u8>);

    impl Out {
        pub fn u8(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        pub fn u16(&mut self, v: u16) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        pub fn u32(&mut self, v: u32) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        pub fn i32(&mut self, v: i32) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        pub fn u64(&mut self, v: u64) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        pub fn f32s(&mut self, v: &[f32]) -> &mut Self {
            for f in v {
                self.0.extend_from_slice(&f.to_le_bytes());
            }
            self
        }
    }

    /// Text back to back, as `STRINGS`.
    #[derive(Default)]
    pub struct Strings(pub Vec<u8>);

    impl Strings {
        pub fn add(&mut self, text: &str) -> (u32, u32) {
            let at = self.0.len() as u32;
            self.0.extend_from_slice(text.as_bytes());
            (at, text.len() as u32)
        }
    }

    /// A [`Property`] record: key `key`, a Float, or Text when `text` is given.
    pub fn property(
        out: &mut Out,
        strings: &mut Strings,
        key: &str,
        float: f64,
        text: Option<&str>,
    ) {
        let (at, len) = strings.add(key);
        out.u64(crate::hash::fnv1a64(key.as_bytes()))
            .u32(at)
            .u32(len);
        match text {
            Some(t) => {
                let (at, len) = strings.add(t);
                out.u32(3).u32(0).u32(at).u32(len)
            }
            None => out.u32(2).u32(0).u64(float.to_bits()),
        };
    }

    fn bounds_record(out: &mut Out, r: f32) {
        out.f32s(&[-r, -r, -r, r, r, r, 0.0, 0.0, 0.0, r]);
    }

    /// One triangle with every optional chunk; `skip` names chunks to leave out and
    /// `cut` shortens `VERTICES` by that many bytes.
    fn mesh(skip: &[&str], cut: usize) -> Vec<u8> {
        let mut desc = Out::default();
        desc.u32(3)
            .u32(3)
            .u32(1)
            .u32(1)
            .u32(1)
            .u16(28)
            .u8(2)
            .u8(0b111)
            .u32(64)
            .u32(124);
        let mut bounds = Out::default();
        bounds_record(&mut bounds, 1.0);
        let mut vertices = Out::default();
        // +X normal; tangent +Y with negative w; -Z normal.
        for (p, n, t) in [
            ([0.0, 0.0, 0.0], [32767i16, 0], [1i16, 32767]),
            ([1.0, 0.0, 0.0], [32767, 32767], [0, 32767]),
            ([0.0, 1.0, 0.0], [0, 0], [0, 0]),
        ] {
            vertices.f32s(&p);
            for c in n.into_iter().chain(t) {
                vertices.u16(c as u16);
            }
            vertices.f32s(&[p[0], p[1]]);
        }
        vertices.0.truncate(vertices.0.len() - cut);
        let mut indices = Out::default();
        indices.u16(0).u16(1).u16(2);
        let mut submesh = Out::default();
        submesh.u32(0).u32(3).u32(0).u32(1).u32(0).u32(0);
        bounds_record(&mut submesh, 1.0);
        let mut material = Out::default();
        material.u64(0xABCD);
        let mut meshlets = Out::default();
        meshlets.u32(0).u32(3).u32(0).u32(1);
        let mut mlverts = Out::default();
        mlverts.u32(0).u32(1).u32(2);
        let mut mlbounds = Out::default();
        mlbounds.f32s(&[0.5, 0.5, 0.0, 1.0, 0.0, 0.0, 1.0, 0.5]);
        let mut lodidx = Out::default();
        lodidx.u32(2).u32(1).u32(0);
        let mut lodtable = Out::default();
        lodtable.u32(2).u32(1).u32(0).u32(3).u32(0).u32(0);
        let mut skin = Out::default();
        for _ in 0..3 {
            skin.u16(1).u16(0).u16(0).u16(0).f32s(&[1.0, 0.0, 0.0, 0.0]);
        }
        let mut skeleton = Out::default();
        for parent in [-1, 0] {
            let mut identity = [0.0f32; 16];
            identity[0] = 1.0;
            identity[5] = 1.0;
            identity[10] = 1.0;
            identity[15] = 1.0;
            skeleton
                .f32s(&identity)
                .f32s(&[0.0, 1.0, 0.0])
                .f32s(&[0.0, 0.0, 0.0, 1.0])
                .f32s(&[1.0; 3])
                .i32(parent)
                .u32(0);
        }
        let mut locators = Out::default();
        locators
            .u64(crate::hash::fnv1a64(b"tip"))
            .u32(0)
            .u32(3)
            .i32(1)
            .f32s(&[1.0, 2.0, 3.0])
            .f32s(&[0.0, 0.0, 0.0, 1.0])
            .f32s(&[1.0; 3])
            .u16(0)
            .u16(1);
        let mut loctags = Out::default();
        loctags.u64(crate::hash::fnv1a64(b"attach")).u32(3).u32(6);

        let mut strings = Strings::default();
        let mut colorset = Out::default();
        let (at, len) = strings.add("team");
        colorset
            .u64(crate::hash::fnv1a64(b"team"))
            .u32(at)
            .u32(len)
            .u32(0)
            .u32(1)
            .u64(0);
        let mut colors = Out::default();
        for v in 0..3u8 {
            colors.u8(v).u8(10).u8(20).u8(255);
        }
        let mut uvset = Out::default();
        let (at, len) = strings.add("team_uv");
        uvset.u64(1).u32(at).u32(len).u64(0x77).u64(0);
        let mut uvs = Out::default();
        uvs.f32s(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        let mut props = Out::default();
        property(&mut props, &mut strings, "mass", 6.5, None);
        property(&mut props, &mut strings, "species", 0.0, Some("fox"));
        let mut shapes = Out::default();
        let (name_at, name_len) = strings.add("hull");
        let (kind_at, kind_len) = strings.add("solid");
        shapes
            .u64(2)
            .u32(name_at)
            .u32(name_len)
            .u64(3)
            .u32(kind_at)
            .u32(kind_len)
            .i32(-1)
            .u32(3)
            .f32s(&[0.0; 3])
            .f32s(&[0.0, 0.0, 0.0, 1.0])
            .f32s(&[0.0; 3])
            .u32(0)
            .u32(4)
            .u32(0)
            .u32(1);
        let mut shapepts = Out::default();
        shapepts.f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let mut shapeprp = Out::default();
        property(&mut shapeprp, &mut strings, "friction", 0.25, None);
        let mut regions = Out::default();
        let (at, len) = strings.add("head");
        regions.u64(4).u32(at).u32(len).u16(0).u16(1).u32(1).u64(0);
        let mut regtags = Out::default();
        let (at, len) = strings.add("damage");
        regtags.u64(5).u32(at).u32(len);
        let mut triregn = Out::default();
        triregn.u32(1);
        let mut paths = Out::default();
        let (name_at, name_len) = strings.add("rail");
        paths
            .u64(6)
            .u32(name_at)
            .u32(name_len)
            .u64(7)
            .u32(0)
            .u32(0)
            .i32(0)
            .u32(1)
            .f32s(&[0.5, 0.5, 2.0])
            .u32(0)
            .u32(3)
            .u32(0)
            .u32(2)
            .u32(0)
            .u32(0)
            .u32(0);
        let mut pathpts = Out::default();
        pathpts.f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        let mut pathsmp = Out::default();
        pathsmp
            .f32s(&[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0])
            .f32s(&[0.0, 0.0, 0.0, 2.0, 1.0, 0.0, 0.0, 3.0]);

        let chunks: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc.0),
            ("BOUNDS", &bounds.0),
            ("VERTICES", &vertices.0),
            ("INDICES", &indices.0),
            ("SUBMESH", &submesh.0),
            ("MATERIAL", &material.0),
            ("MESHLETS", &meshlets.0),
            ("MLVERTS", &mlverts.0),
            ("MLTRIS", &[0, 1, 2]),
            ("MLBOUNDS", &mlbounds.0),
            ("LODIDX", &lodidx.0),
            ("LODTABLE", &lodtable.0),
            ("SKIN", &skin.0),
            ("SKELETON", &skeleton.0),
            ("LOCATORS", &locators.0),
            ("LOCNAMES", b"tipattach"),
            ("LOCTAGS", &loctags.0),
            ("COLORSET", &colorset.0),
            ("COLORS", &colors.0),
            ("UVSET", &uvset.0),
            ("UVS", &uvs.0),
            ("STRINGS", &strings.0),
            ("PROPS", &props.0),
            ("SHAPES", &shapes.0),
            ("SHAPEPTS", &shapepts.0),
            ("SHAPEPRP", &shapeprp.0),
            ("REGIONS", &regions.0),
            ("REGTAGS", &regtags.0),
            ("TRIREGN", &triregn.0),
            ("PATHS", &paths.0),
            ("PATHPTS", &pathpts.0),
            ("PATHSMP", &pathsmp.0),
            ("FUTURE", b"ignored"),
        ];
        let chunks: Vec<_> = chunks
            .into_iter()
            .filter(|(n, _)| !skip.contains(n))
            .collect();
        file("AMESH", 1, &chunks)
    }

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn reads_every_chunk() {
        let m = Mesh::decode(&mesh(&[], 0)).unwrap();
        assert!(m.has_tangents() && m.is_skinned());
        assert_eq!((m.meshlet_max_vertices, m.meshlet_max_triangles), (64, 124));
        assert_eq!(m.bounds.sphere_radius, 1.0);
        assert_eq!(m.vertices.len(), 3);
        let [a, b, c] = [m.vertices[0], m.vertices[1], m.vertices[2]];
        assert!(close(&a.normal, &[1.0, 0.0, 0.0]));
        assert!(close(&a.tangent, &[0.0, 1.0, 0.0, -1.0]));
        assert!(close(&b.normal, &[0.0, 0.0, -1.0]));
        assert!(close(&b.tangent, &[0.0, 1.0, 0.0, 1.0]));
        assert!(close(&c.normal, &[0.0, 0.0, 1.0]));
        assert_eq!((b.position, b.uv), ([1.0, 0.0, 0.0], [1.0, 0.0]));
        assert_eq!(m.indices, [0, 1, 2]);
        assert_eq!(m.materials, [0xABCD]);
        let s = &m.submeshes[0];
        assert_eq!(
            (s.index_count, s.meshlet_count, s.material),
            (3, 1, Some(0))
        );
        assert_eq!(m.meshlet_vertices, [0, 1, 2]);
        assert_eq!(m.meshlet_triangles, [[0, 1, 2]]);
        assert_eq!(m.meshlet_bounds[0].cone_cutoff, 0.5);
        assert_eq!(m.lod_count, 2);
        assert_eq!(m.level_indices(0, 0), Some(&[0, 1, 2][..]));
        assert_eq!(m.level_indices(0, 1), Some(&[2, 1, 0][..]));
        // Level 2 reached nothing simpler: level 1 again.
        assert_eq!(m.level_indices(0, 2), Some(&[2, 1, 0][..]));
        assert_eq!(m.level_indices(0, 3), None);
        assert_eq!(m.skin[2].joints, [1, 0, 0, 0]);
        assert_eq!(m.skeleton[0].parent, None);
        assert_eq!(m.skeleton[1].parent, Some(0));
        assert_eq!(m.skeleton[1].bind_translation, [0.0, 1.0, 0.0]);
        let tip = m.locator(crate::hash::fnv1a64(b"tip")).unwrap();
        assert_eq!((tip.name.as_str(), tip.joint), ("tip", Some(1)));
        assert_eq!(tip.translation, [1.0, 2.0, 3.0]);
        assert_eq!(tip.tags[0].text, "attach");
        assert_eq!(m.color_sets[0].name, "team");
        assert_eq!(m.color_sets[0].source, 1);
        assert_eq!(m.color_sets[0].colors[2], [2, 10, 20, 255]);
        assert_eq!(
            (m.uv_sets[0].name.as_str(), m.uv_sets[0].texture),
            ("team_uv", 0x77)
        );
        assert_eq!(m.uv_sets[0].uvs[1], [0.3, 0.4]);
        let mass = m.property(crate::hash::fnv1a64(b"mass")).unwrap();
        assert_eq!(mass.value, PropertyValue::Float(6.5));
        assert_eq!(m.properties[1].value, PropertyValue::Text("fox".into()));
        let hull = &m.shapes[0];
        assert_eq!((hull.name.as_str(), hull.kind.as_str()), ("hull", "solid"));
        assert_eq!(
            (hull.shape_type, hull.joint, hull.points.len()),
            (ShapeType::Hull, None, 4)
        );
        assert_eq!(hull.properties[0].key, "friction");
        assert_eq!(m.regions[0].name, "head");
        assert_eq!(m.regions[0].tags[0].text, "damage");
        assert_eq!(m.triangle_regions, [1]);
        let rail = &m.paths[0];
        assert_eq!((rail.name.as_str(), rail.kind.as_str()), ("rail", ""));
        assert!(rail.closed);
        assert_eq!(
            (rail.joint, rail.length, rail.points.len()),
            (Some(0), 2.0, 3)
        );
        assert_eq!(rail.samples[1].u, 3.0);
        assert!(rail.properties.is_empty());
    }

    #[test]
    fn optional_chunks_read_empty() {
        let optional = [
            "LODIDX", "LODTABLE", "SKIN", "SKELETON", "LOCATORS", "LOCNAMES", "LOCTAGS",
            "COLORSET", "COLORS", "UVSET", "UVS", "STRINGS", "PROPS", "SHAPES", "SHAPEPTS",
            "SHAPEPRP", "REGIONS", "REGTAGS", "TRIREGN", "PATHS", "PATHPTS", "PATHSMP",
        ];
        let m = Mesh::decode(&mesh(&optional, 0)).unwrap();
        assert_eq!(m.vertices.len(), 3);
        assert!(m.submeshes[0].lods.is_empty() && m.lod_indices.is_empty());
        assert_eq!(m.level_indices(0, 0), Some(&[0, 1, 2][..]));
        assert!(m.skin.is_empty() && m.skeleton.is_empty() && m.locators.is_empty());
        assert!(m.color_sets.is_empty() && m.uv_sets.is_empty() && m.properties.is_empty());
        assert!(m.shapes.is_empty() && m.regions.is_empty() && m.paths.is_empty());
    }

    #[test]
    fn rejects_missing_and_short_chunks() {
        assert!(Mesh::decode(&mesh(&["MLBOUNDS"], 0)).is_err());
        assert!(Mesh::decode(&mesh(&[], 1)).is_err());
        // A shape's properties without SHAPEPRP, strings without STRINGS.
        assert!(Mesh::decode(&mesh(&["SHAPEPRP"], 0)).is_err());
        assert!(Mesh::decode(&mesh(&["STRINGS"], 0)).is_err());
        assert!(Mesh::decode(&mesh(&[], 0)[..100]).is_err());
    }
}
