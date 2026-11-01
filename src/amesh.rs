//! `.amesh` meshes: submeshes with meshlets, bounds, levels of detail and skin; see
//! docs/spec/amesh.md.

use meshopt::{SimplifyOptions, VertexDataAdapter};

use crate::chunks::{self, Bytes, Writer};

pub const FORMAT: &str = "AMESH";
pub const VERSION: u32 = 1;
pub const MAX_VERTICES: usize = 64;
pub const MAX_TRIANGLES: usize = 124;
const CONE_WEIGHT: f32 = 0.25;
/// Reduced levels of detail, as fractions of the full triangle count.
const LOD_RATIOS: [f32; 2] = [0.5, 0.25];
const LOD_ERROR: f32 = 0.05;
const LOD_RETRIES: usize = 3;
const NO_MATERIAL: u32 = 0xFFFF_FFFF;
const VERTEX_STRIDE: u16 = 28;

/// A vertex before packing; `tangent.w` is the handedness.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub uv: [f32; 2],
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

/// One submesh: a triangle list and the index of its material in the mesh's list.
pub struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Option<u32>,
}

pub struct Joint {
    /// Column-major.
    pub inverse_bind: [f32; 16],
    pub translation: [f32; 3],
    /// `x y z w`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub parent: i32,
}

/// What a compiled mesh holds, for build output.
#[derive(Debug, Default, PartialEq)]
pub struct Summary {
    pub vertices: usize,
    pub triangles: usize,
    pub submeshes: usize,
    pub meshlets: usize,
}

#[derive(Clone, Copy)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
    center: [f32; 3],
    radius: f32,
}

fn bounds<'a>(positions: impl Iterator<Item = &'a [f32; 3]> + Clone) -> Bounds {
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in positions.clone() {
        for c in 0..3 {
            min[c] = min[c].min(p[c]);
            max[c] = max[c].max(p[c]);
        }
    }
    if min[0] > max[0] {
        (min, max) = ([0.0; 3], [0.0; 3]);
    }
    let center = std::array::from_fn(|c| (min[c] + max[c]) / 2.0);
    let radius = positions
        .map(|p| (0..3).map(|c| (p[c] - center[c]).powi(2)).sum::<f32>())
        .fold(0.0, f32::max)
        .sqrt();
    Bounds {
        min,
        max,
        center,
        radius,
    }
}

fn put_bounds(out: &mut Bytes, b: &Bounds) {
    out.f32s(&b.min)
        .f32s(&b.max)
        .f32s(&b.center)
        .f32s(&[b.radius]);
}

/// Octahedral encoding of a unit vector as two SNORM16 values, see docs/spec/amesh.md.
pub fn octahedral([x, y, z]: [f32; 3]) -> [i16; 2] {
    let sum = x.abs() + y.abs() + z.abs();
    if sum == 0.0 {
        return [0, 0];
    }
    let (mut u, mut v) = (x / sum, y / sum);
    if z < 0.0 {
        let sign = |s: f32| if s >= 0.0 { 1.0 } else { -1.0 };
        (u, v) = ((1.0 - v.abs()) * sign(u), (1.0 - u.abs()) * sign(v));
    }
    [u, v].map(|c| (c.clamp(-1.0, 1.0) * 32767.0).round() as i16)
}

/// Octahedral tangent with the handedness in bit 0 of x.
pub fn octahedral_tangent([x, y, z, w]: [f32; 4]) -> [i16; 2] {
    let [u, v] = octahedral([x, y, z]);
    let u = (u & !1) | i16::from(w < 0.0);
    [u, v]
}

/// Encodes the mesh; `materials` are asset IDs, `skeleton` makes it skinned.
pub fn encode(
    primitives: &[Primitive],
    materials: &[u64],
    skeleton: Option<&[Joint]>,
    tangents: bool,
) -> (Vec<u8>, Summary) {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut submeshes = Bytes::default();
    let mut meshlets = Bytes::default();
    let mut meshlet_vertices = Bytes::default();
    let mut meshlet_triangles = Bytes::default();
    let mut meshlet_bounds = Bytes::default();
    let mut lod_indices: Vec<u32> = Vec::new();
    let mut lod_ranges = Bytes::default();
    let (mut meshlet_count, mut meshlet_vertex_count, mut meshlet_triangle_count) = (0, 0, 0);
    let mut submesh_count = 0;

    for primitive in primitives.iter().filter(|p| !p.indices.is_empty()) {
        submesh_count += 1;
        let local = meshopt::optimize_vertex_cache(&primitive.indices, primitive.vertices.len());
        let positions: Vec<u8> = primitive
            .vertices
            .iter()
            .flat_map(|v| v.position.iter().flat_map(|c| c.to_le_bytes()))
            .collect();
        let adapter = VertexDataAdapter::new(&positions, 12, 0).expect("positions");
        let base = vertices.len() as u32;
        let first_index = indices.len() as u32;
        let first_meshlet = meshlet_count;

        let built =
            meshopt::build_meshlets(&local, &adapter, MAX_VERTICES, MAX_TRIANGLES, CONE_WEIGHT);
        for meshlet in built.iter() {
            let triangle_count = meshlet.triangles.len() / 3;
            meshlets
                .u32(meshlet_vertex_count)
                .u32(meshlet.vertices.len() as u32)
                .u32(meshlet_triangle_count)
                .u32(triangle_count as u32);
            for v in meshlet.vertices {
                meshlet_vertices.u32(v + base);
            }
            meshlet_triangles.0.extend_from_slice(meshlet.triangles);
            let b = meshopt::compute_meshlet_bounds(meshlet, &adapter);
            meshlet_bounds
                .f32s(&b.center)
                .f32s(&[b.radius])
                .f32s(&b.cone_axis)
                .f32s(&[b.cone_cutoff]);
            meshlet_vertex_count += meshlet.vertices.len() as u32;
            meshlet_triangle_count += triangle_count as u32;
            meshlet_count += 1;
        }

        // Reduced levels: each from the full mesh, stopping progress counts as no level.
        let mut previous = local.len();
        for ratio in LOD_RATIOS {
            let target = ((local.len() as f32 * ratio / 3.0).round() as usize * 3).max(3);
            let mut error = LOD_ERROR;
            let mut level = Vec::new();
            for _ in 0..=LOD_RETRIES {
                level =
                    meshopt::simplify(&local, &adapter, target, error, SimplifyOptions::None, None);
                if level.len() < previous {
                    break;
                }
                error += LOD_ERROR;
            }
            if level.len() >= previous {
                level = meshopt::simplify_sloppy(&local, &adapter, target, 1.0, None);
            }
            if level.is_empty() || level.len() >= previous {
                lod_ranges.u32(lod_indices.len() as u32).u32(0);
                continue;
            }
            lod_ranges
                .u32(lod_indices.len() as u32)
                .u32(level.len() as u32);
            lod_indices.extend(level.iter().map(|i| i + base));
            previous = level.len();
        }

        let own = bounds(primitive.vertices.iter().map(|v| &v.position));
        submeshes
            .u32(first_index)
            .u32(local.len() as u32)
            .u32(first_meshlet)
            .u32(meshlet_count - first_meshlet)
            .u32(primitive.material.unwrap_or(NO_MATERIAL))
            .u32(0);
        put_bounds(&mut submeshes, &own);
        indices.extend(local.iter().map(|i| i + base));
        vertices.extend_from_slice(&primitive.vertices);
    }

    let wide = vertices.len() > 0xFFFF;
    let mut desc = Bytes::default();
    let flags = u8::from(tangents) | (u8::from(skeleton.is_some()) << 1) | (1 << 2);
    desc.u32(vertices.len() as u32)
        .u32(indices.len() as u32)
        .u32(meshlet_count)
        .u32(submesh_count)
        .u32(materials.len() as u32)
        .u16(VERTEX_STRIDE)
        .u8(if wide { 4 } else { 2 })
        .u8(flags)
        .u32(MAX_VERTICES as u32)
        .u32(MAX_TRIANGLES as u32);
    let mut whole = Bytes::default();
    put_bounds(&mut whole, &bounds(vertices.iter().map(|v| &v.position)));
    let mut packed = Bytes::default();
    for v in &vertices {
        packed.f32s(&v.position);
        for c in octahedral(v.normal) {
            packed.i16(c);
        }
        for c in octahedral_tangent(v.tangent) {
            packed.i16(c);
        }
        packed.f32s(&v.uv);
    }
    let mut index_bytes = Bytes::default();
    for &i in &indices {
        if wide {
            index_bytes.u32(i);
        } else {
            index_bytes.u16(i as u16);
        }
    }
    let mut material_ids = Bytes::default();
    for &id in materials {
        material_ids.u64(id);
    }
    let mut lod_index_bytes = Bytes::default();
    for &i in &lod_indices {
        lod_index_bytes.u32(i);
    }
    let mut lod_table = Bytes::default();
    lod_table.u32(LOD_RATIOS.len() as u32).u32(submesh_count);
    lod_table.0.extend(lod_ranges.take());

    let mut writer = Writer::new(FORMAT, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("BOUNDS", whole.take())
        .chunk("VERTICES", packed.take())
        .chunk("INDICES", index_bytes.take())
        .chunk("SUBMESH", submeshes.take())
        .chunk("MATERIAL", material_ids.take())
        .chunk("MESHLETS", meshlets.take())
        .chunk("MLVERTS", meshlet_vertices.take())
        .chunk("MLTRIS", meshlet_triangles.take())
        .chunk("MLBOUNDS", meshlet_bounds.take())
        .chunk("LODIDX", lod_index_bytes.take())
        .chunk("LODTABLE", lod_table.take());
    if let Some(joints) = skeleton {
        let mut skin = Bytes::default();
        for v in &vertices {
            for j in v.joints {
                skin.u16(j);
            }
            skin.f32s(&v.weights);
        }
        let mut bones = Bytes::default();
        for j in joints {
            bones
                .f32s(&j.inverse_bind)
                .f32s(&j.translation)
                .f32s(&j.rotation)
                .f32s(&j.scale)
                .i32(j.parent)
                .u32(0);
        }
        writer
            .chunk("SKIN", skin.take())
            .chunk("SKELETON", bones.take());
    }
    let summary = Summary {
        vertices: vertices.len(),
        triangles: indices.len() / 3,
        submeshes: submesh_count as usize,
        meshlets: meshlet_count as usize,
    };
    (writer.finish(), summary)
}

/// Inspector rows of an `.amesh` file; see docs/features/models.md#inspector.
pub fn rows(bytes: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let reader = chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported {FORMAT} version {}", reader.version));
    }
    let desc = reader.require("DESC")?;
    let field = |at| chunks::u32_at(desc, 0, 32, at).ok_or("DESC is too short");
    let flags = *desc.get(23).ok_or("DESC is too short")?;
    let bounds = reader.require("BOUNDS")?;
    let f = |i: usize| chunks::f32_at(bounds, 0, 40, i * 4).unwrap_or(0.0);
    let joints = reader.chunk("SKELETON").map_or(0, |s| s.len() / 112);
    let lods = reader
        .chunk("LODTABLE")
        .and_then(|t| chunks::u32_at(t, 0, 8, 0))
        .unwrap_or(0);
    Ok(vec![
        ("Vertices", field(0)?.to_string()),
        ("Triangles", (field(4)? / 3).to_string()),
        ("Submeshes", field(12)?.to_string()),
        ("Meshlets", field(8)?.to_string()),
        ("Materials", field(16)?.to_string()),
        (
            "Levels of detail",
            if lods > 0 {
                lods.to_string()
            } else {
                "none".into()
            },
        ),
        (
            "Skin",
            if flags & 2 != 0 {
                format!("{joints} {}", if joints == 1 { "joint" } else { "joints" })
            } else {
                "none".into()
            },
        ),
        (
            "Bounds",
            format!(
                "{:.2}, {:.2}, {:.2} – {:.2}, {:.2}, {:.2}",
                f(0),
                f(1),
                f(2),
                f(3),
                f(4),
                f(5)
            ),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `n × n` grid of quads in the XY plane, facing +Z.
    fn grid(n: u32) -> Primitive {
        let mut vertices = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                vertices.push(Vertex {
                    position: [x as f32, y as f32, 0.0],
                    normal: [0.0, 0.0, 1.0],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    uv: [x as f32 / n as f32, y as f32 / n as f32],
                    ..Default::default()
                });
            }
        }
        let mut indices = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let a = y * (n + 1) + x;
                indices.extend([a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]);
            }
        }
        Primitive {
            vertices,
            indices,
            material: Some(0),
        }
    }

    #[test]
    fn octahedral_encoding() {
        assert_eq!(octahedral([0.0, 0.0, 1.0]), [0, 0]);
        assert_eq!(octahedral([1.0, 0.0, 0.0]), [32767, 0]);
        assert_eq!(octahedral([0.0, 0.0, -1.0]), [32767, 32767]);
        assert_eq!(octahedral_tangent([1.0, 0.0, 0.0, -1.0])[0] & 1, 1);
        assert_eq!(octahedral_tangent([1.0, 0.0, 0.0, 1.0])[0] & 1, 0);
    }

    #[test]
    fn encodes_chunks_meshlets_and_lods() {
        let (bytes, summary) = encode(&[grid(16), grid(2)], &[42], None, true);
        assert_eq!(summary.submeshes, 2);
        assert_eq!(summary.triangles, 16 * 16 * 2 + 8);
        assert_eq!(summary.vertices, 17 * 17 + 9);
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        let desc = reader.require("DESC").unwrap();
        assert_eq!(desc.len(), 32);
        assert_eq!(desc[22], 2, "u16 indices");
        assert_eq!(desc[23], 0b101, "tangents and LODs, not skinned");
        assert_eq!(
            reader.require("VERTICES").unwrap().len(),
            summary.vertices * 28
        );
        assert_eq!(
            reader.require("INDICES").unwrap().len(),
            summary.triangles * 3 * 2
        );
        assert_eq!(reader.require("MATERIAL").unwrap(), 42u64.to_le_bytes());
        let meshlets = reader.require("MESHLETS").unwrap();
        assert_eq!(meshlets.len(), summary.meshlets * 16);
        assert!(summary.meshlets > 1);
        for m in 0..summary.meshlets {
            assert!(chunks::u32_at(meshlets, m, 16, 4).unwrap() <= 64);
            assert!(chunks::u32_at(meshlets, m, 16, 12).unwrap() <= 124);
        }
        assert_eq!(
            reader.require("MLBOUNDS").unwrap().len(),
            summary.meshlets * 32
        );
        // Submesh 1 starts after submesh 0's indices and meshlets.
        let submesh = reader.require("SUBMESH").unwrap();
        assert_eq!(submesh.len(), 2 * 64);
        assert_eq!(chunks::u32_at(submesh, 1, 64, 0), Some(16 * 16 * 6));
        // The big grid simplifies; LOD ranges are inside LODIDX.
        let table = reader.require("LODTABLE").unwrap();
        assert_eq!(chunks::u32_at(table, 0, 8, 0), Some(2));
        let first = chunks::u32_at(table, 1, 8, 4).unwrap();
        assert!(first > 0 && first < 16 * 16 * 6, "{first}");
        assert!(reader.chunk("SKIN").is_none());

        let rows = rows(&bytes).unwrap();
        assert!(rows.contains(&("Submeshes", "2".into())));
        assert!(rows.contains(&("Skin", "none".into())));
    }

    #[test]
    fn skinned_meshes_carry_skin_and_skeleton() {
        let joint = Joint {
            inverse_bind: std::array::from_fn(|i| if i % 5 == 0 { 1.0 } else { 0.0 }),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            parent: -1,
        };
        let (bytes, summary) = encode(&[grid(2)], &[], Some(&[joint]), false);
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        assert_eq!(reader.require("SKIN").unwrap().len(), summary.vertices * 24);
        assert_eq!(reader.require("SKELETON").unwrap().len(), 112);
        assert_eq!(reader.require("DESC").unwrap()[23], 0b110);
        assert!(rows(&bytes).unwrap().contains(&("Skin", "1 joint".into())));
    }
}
