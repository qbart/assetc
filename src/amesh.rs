//! `.amesh` meshes: submeshes with meshlets, bounds, levels of detail and skin; see
//! docs/spec/amesh.md.

use meshopt::{SimplifyOptions, VertexDataAdapter};

use serde::Deserialize;

use crate::chunks::{self, Bytes, Writer};
use crate::config;
use crate::texture::glob_set;

pub const FORMAT: &str = "AMESH";
pub const VERSION: u32 = 1;
const NO_MATERIAL: u32 = 0xFFFF_FFFF;
const VERTEX_STRIDE: u16 = 28;

/// Declares the meshoptimizer settings once: [`MeshOptions`] (resolved values) and
/// [`MeshPatch`] (the same keys, each optional, for overrides and `.amod` sidecars).
macro_rules! mesh_options {
    ($($name:ident: $ty:ty = $default:expr,)*) => {
        /// Meshoptimizer settings of a mesh; see docs/spec/assetc.toml.md#meshes.
        #[derive(Clone, Debug, PartialEq, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct MeshOptions {
            $(pub $name: $ty,)*
        }

        impl Default for MeshOptions {
            fn default() -> Self {
                Self {
                    $($name: $default,)*
                }
            }
        }

        /// Settings that replace a mesh's resolved ones, key by key.
        #[derive(Clone, Debug, Default, PartialEq, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct MeshPatch {
            $(pub $name: Option<$ty>,)*
        }

        impl MeshPatch {
            pub fn apply(&self, options: &mut MeshOptions) {
                $(if let Some(value) = &self.$name {
                    options.$name = value.clone();
                })*
            }

            pub fn is_empty(&self) -> bool {
                true $(&& self.$name.is_none())*
            }

            /// `key = value` lines of the keys that are set, in declaration order.
            pub fn to_toml(&self) -> String {
                let mut out = String::new();
                $(if let Some(value) = &self.$name {
                    let value = toml::Value::try_from(value).expect("TOML value");
                    out += &format!("{} = {value}\n", stringify!($name));
                })*
                out
            }
        }
    };
}

mesh_options! {
    meshlet_max_vertices: u32 = 64,
    meshlet_max_triangles: u32 = 124,
    meshlet_cone_weight: f64 = 0.25,
    lod_ratios: Vec<f64> = vec![0.5, 0.25],
    lod_error: f64 = 0.05,
    lod_error_step: f64 = 0.05,
    lod_attempts: u32 = 3,
    lod_sloppy_fallback: bool = true,
    lod_uv_weight: f64 = 1.0,
    lod_normal_weight: f64 = 0.5,
    lod_permissive: bool = true,
    lod_lock_border: bool = false,
    lod_regularize: bool = false,
    lod_prune: bool = false,
}

impl MeshOptions {
    /// The ranges assetc and meshoptimizer accept; see docs/spec/assetc.toml.md#meshes.
    fn check(self) -> Result<Self, String> {
        let within = |name: &str, v: f64, max: f64| {
            if (0.0..=max).contains(&v) {
                Ok(())
            } else {
                Err(format!("{name} {v} is outside 0–{max}"))
            }
        };
        let v = self.meshlet_max_vertices;
        if !(3..=256).contains(&v) {
            return Err(format!("meshlet_max_vertices {v} is outside 3–256"));
        }
        let t = self.meshlet_max_triangles;
        if !(4..=512).contains(&t) || !t.is_multiple_of(4) {
            return Err(format!(
                "meshlet_max_triangles {t} must be 4–512 and a multiple of 4"
            ));
        }
        within("meshlet_cone_weight", self.meshlet_cone_weight, 1.0)?;
        if self.lod_ratios.len() > 8 {
            return Err("lod_ratios has more than 8 levels".into());
        }
        let mut previous = 1.0;
        for &r in &self.lod_ratios {
            if !(r > 0.0 && r < previous) {
                return Err(format!(
                    "lod_ratios must be decreasing and between 0 and 1 (exclusive), got {r}"
                ));
            }
            previous = r;
        }
        within("lod_error", self.lod_error, 1.0)?;
        within("lod_error_step", self.lod_error_step, 1.0)?;
        if self.lod_attempts > 10 {
            return Err(format!(
                "lod_attempts {} is outside 0–10",
                self.lod_attempts
            ));
        }
        within("lod_uv_weight", self.lod_uv_weight, 100.0)?;
        within("lod_normal_weight", self.lod_normal_weight, 100.0)?;
        Ok(self)
    }

    fn simplify_options(&self) -> SimplifyOptions {
        let mut options = SimplifyOptions::None;
        for (on, flag) in [
            (self.lod_permissive, SimplifyOptions::Permissive),
            (self.lod_lock_border, SimplifyOptions::LockBorder),
            (self.lod_regularize, SimplifyOptions::Regularize),
            (self.lod_prune, SimplifyOptions::Prune),
        ] {
            if on {
                options |= flag;
            }
        }
        options
    }
}

/// `[meshes]` resolved: defaults and per-model overrides.
pub struct MeshSettings {
    defaults: MeshOptions,
    overrides: Vec<(globset::GlobSet, MeshPatch)>,
}

impl MeshSettings {
    /// Checks the defaults, every override (applied to them) and every glob.
    pub fn new(meshes: &config::Meshes) -> Result<Self, String> {
        let defaults = meshes.defaults.clone().check()?;
        let mut overrides = Vec::new();
        for (i, o) in meshes.overrides.iter().enumerate() {
            let at = |e: String| format!("override {}: {e}", i + 1);
            if o.patterns.is_empty() {
                return Err(at("match is empty".into()));
            }
            let set = glob_set(&o.patterns).map_err(|e| at(e.to_string()))?;
            let mut options = defaults.clone();
            o.patch.apply(&mut options);
            options.check().map_err(at)?;
            overrides.push((set, o.patch.clone()));
        }
        Ok(Self {
            defaults,
            overrides,
        })
    }

    /// Settings for the model at `source` (input-relative, with extension) from
    /// `assetc.toml`; the last matching override that sets a key wins.
    pub fn options(&self, source: &str) -> MeshOptions {
        let mut options = self.defaults.clone();
        for (set, patch) in &self.overrides {
            if set.is_match(source) {
                patch.apply(&mut options);
            }
        }
        options
    }

    /// [`Self::options`] with the model's sidecar `[mesh]`, which wins; see
    /// docs/spec/amod.md#mesh.
    pub fn options_with(
        &self,
        source: &str,
        amod: Option<&crate::amod::Amod>,
    ) -> Result<MeshOptions, String> {
        let mut options = self.options(source);
        if let Some(amod) = amod {
            amod.mesh.apply(&mut options);
        }
        options.check()
    }
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub center: [f32; 3],
    pub radius: f32,
}

impl Bounds {
    /// Reads a 40-byte `Bounds` record at `at`.
    fn read(bytes: &[u8], at: usize) -> Option<Self> {
        let f = |k: usize| chunks::f32_at(bytes, 0, 4, at + k * 4);
        let three = |k: usize| Some([f(k)?, f(k + 1)?, f(k + 2)?]);
        Some(Self {
            min: three(0)?,
            max: three(3)?,
            center: three(6)?,
            radius: f(9)?,
        })
    }
}

/// A meshlet's `MeshletBounds` record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshletBounds {
    pub center: [f32; 3],
    pub radius: f32,
    pub cone_axis: [f32; 3],
    pub cone_cutoff: f32,
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
    options: &MeshOptions,
) -> (Vec<u8>, Summary) {
    let (max_vertices, max_triangles) = (
        options.meshlet_max_vertices as usize,
        options.meshlet_max_triangles as usize,
    );
    let weights = [
        options.lod_uv_weight,
        options.lod_uv_weight,
        options.lod_normal_weight,
        options.lod_normal_weight,
        options.lod_normal_weight,
    ]
    .map(|w| w as f32);
    let simplify_options = options.simplify_options();
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

        let built = meshopt::build_meshlets(
            &local,
            &adapter,
            max_vertices,
            max_triangles,
            options.meshlet_cone_weight as f32,
        );
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
        // UVs and normals weigh in, so surviving vertices keep fitting their faces, and
        // collapses may cross attribute seams (every edge of a flat-shaded mesh is one).
        let attributes: Vec<f32> = primitive
            .vertices
            .iter()
            .flat_map(|v| [v.uv[0], v.uv[1], v.normal[0], v.normal[1], v.normal[2]])
            .collect();
        let unlocked = vec![false; primitive.vertices.len()];
        let mut previous = local.len();
        for &ratio in &options.lod_ratios {
            let target = ((local.len() as f64 * ratio / 3.0).round() as usize * 3).max(3);
            let mut error = options.lod_error as f32;
            let mut level = Vec::new();
            for _ in 0..=options.lod_attempts {
                level = meshopt::simplify_with_attributes_and_locks(
                    &local,
                    &adapter,
                    &attributes,
                    &weights,
                    5 * 4,
                    &unlocked,
                    target,
                    error,
                    simplify_options,
                    None,
                );
                if level.len() < previous {
                    break;
                }
                error += options.lod_error_step as f32;
            }
            if level.len() >= previous && options.lod_sloppy_fallback {
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
    let lods = !options.lod_ratios.is_empty() && submesh_count > 0;
    let flags = u8::from(tangents) | (u8::from(skeleton.is_some()) << 1) | (u8::from(lods) << 2);
    desc.u32(vertices.len() as u32)
        .u32(indices.len() as u32)
        .u32(meshlet_count)
        .u32(submesh_count)
        .u32(materials.len() as u32)
        .u16(VERTEX_STRIDE)
        .u8(if wide { 4 } else { 2 })
        .u8(flags)
        .u32(options.meshlet_max_vertices)
        .u32(options.meshlet_max_triangles);
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
    lod_table
        .u32(options.lod_ratios.len() as u32)
        .u32(submesh_count);
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
        .chunk("MLBOUNDS", meshlet_bounds.take());
    if lods {
        writer
            .chunk("LODIDX", lod_index_bytes.take())
            .chunk("LODTABLE", lod_table.take());
    }
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

/// Inverse of [`octahedral`].
fn octahedral_decode([u, v]: [i16; 2]) -> [f32; 3] {
    let (mut x, mut y) = (f32::from(u) / 32767.0, f32::from(v) / 32767.0);
    let z = 1.0 - x.abs() - y.abs();
    if z < 0.0 {
        let sign = |s: f32| if s >= 0.0 { 1.0 } else { -1.0 };
        (x, y) = ((1.0 - y.abs()) * sign(x), (1.0 - x.abs()) * sign(y));
    }
    let length = (x * x + y * y + z * z).sqrt().max(f32::EPSILON);
    [x / length, y / length, z / length]
}

/// One level of detail of a compiled mesh as triangles: three corners per triangle, and per
/// triangle its meshlet (LOD 0 only), submesh and material slot; see
/// docs/features/models.md#compiled-mesh-preview.
#[derive(Clone, Debug, Default)]
pub struct Level {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// `w` is the handedness.
    pub tangents: Vec<[f32; 4]>,
    /// Empty for reduced levels.
    pub meshlets: Vec<u32>,
    pub submeshes: Vec<u32>,
    pub material_slots: Vec<Option<u32>>,
    /// Every submesh reused the level before (it made no progress).
    pub same_as_previous: bool,
}

impl Level {
    pub fn triangles(&self) -> usize {
        self.submeshes.len()
    }
}

/// A compiled mesh for the canvas: LOD 0 (from its meshlets) and each reduced level.
#[derive(Clone, Debug, Default)]
pub struct Preview {
    pub levels: Vec<Level>,
    /// Bounds of the whole mesh (`BOUNDS`), of each submesh, and of each meshlet.
    pub bounds: Bounds,
    pub submesh_bounds: Vec<Bounds>,
    pub meshlet_bounds: Vec<MeshletBounds>,
    /// Asset IDs of the material slots (`MATERIAL`).
    pub materials: Vec<u64>,
}

impl Preview {
    /// Each meshlet's box, around the vertices of its LOD 0 triangles (empty if it has none).
    pub fn meshlet_boxes(&self) -> Vec<[[f32; 3]; 2]> {
        let mut boxes =
            vec![[[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]]; self.meshlet_bounds.len()];
        let Some(full) = self.levels.first() else {
            return boxes;
        };
        for (t, &m) in full.meshlets.iter().enumerate() {
            let Some([min, max]) = boxes.get_mut(m as usize) else {
                continue;
            };
            for p in &full.positions[t * 3..t * 3 + 3] {
                for c in 0..3 {
                    min[c] = min[c].min(p[c]);
                    max[c] = max[c].max(p[c]);
                }
            }
        }
        boxes
    }
}

pub fn preview(bytes: &[u8]) -> Result<Preview, String> {
    let reader = chunks::Reader::parse(bytes, FORMAT)?;
    let desc = reader.require("DESC")?;
    let vertices = reader.require("VERTICES")?;
    let indices = reader.require("INDICES")?;
    let meshlets = reader.require("MESHLETS")?;
    let meshlet_vertices = reader.require("MLVERTS")?;
    let triangles = reader.require("MLTRIS")?;
    let submeshes = reader.require("SUBMESH")?;
    let bad = || "mesh data out of range".to_owned();
    let wide = *desc.get(22).ok_or_else(bad)? == 4;
    let index = |i: usize| -> Option<u32> {
        if wide {
            chunks::u32_at(indices, i, 4, 0)
        } else {
            let raw = indices.get(i * 2..i * 2 + 2)?;
            Some(u32::from(u16::from_le_bytes(raw.try_into().ok()?)))
        }
    };
    let corner = |level: &mut Level, i: usize| -> Option<()> {
        let base = i.checked_mul(28)?;
        let raw = vertices.get(base..base + 28)?;
        let f = |at: usize| f32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
        let s = |at: usize| i16::from_le_bytes(raw[at..at + 2].try_into().unwrap());
        let [tx, ty, tz] = octahedral_decode([s(16) & !1, s(18)]);
        let w = if s(16) & 1 == 1 { -1.0 } else { 1.0 };
        level.positions.push([f(0), f(4), f(8)]);
        level.normals.push(octahedral_decode([s(12), s(14)]));
        level.uvs.push([f(20), f(24)]);
        level.tangents.push([tx, ty, tz, w]);
        Some(())
    };

    // Submesh ranges: (first index, index count, first meshlet, meshlet count, material).
    let submesh_count = submeshes.len() / 64;
    let mut ranges = Vec::with_capacity(submesh_count);
    for s in 0..submesh_count {
        let field = |at| chunks::u32_at(submeshes, s, 64, at).ok_or_else(bad);
        let material = field(16)?;
        ranges.push((
            field(0)?,
            field(4)?,
            field(8)?,
            field(12)?,
            (material != NO_MATERIAL).then_some(material),
        ));
    }

    // LOD 0, meshlet by meshlet.
    let mut full = Level::default();
    for (s, &(_, _, first_meshlet, meshlet_count, material)) in ranges.iter().enumerate() {
        for m in first_meshlet as usize..(first_meshlet + meshlet_count) as usize {
            let field = |at| {
                chunks::u32_at(meshlets, m, 16, at)
                    .map(|v| v as usize)
                    .ok_or_else(bad)
            };
            let (vertex_offset, triangle_offset, triangle_count) =
                (field(0)?, field(8)?, field(12)?);
            for t in triangle_offset..triangle_offset + triangle_count {
                for &local in triangles.get(t * 3..t * 3 + 3).ok_or_else(bad)? {
                    let global =
                        chunks::u32_at(meshlet_vertices, vertex_offset + local as usize, 4, 0)
                            .ok_or_else(bad)?;
                    corner(&mut full, global as usize).ok_or_else(bad)?;
                }
                full.meshlets.push(m as u32);
                full.submeshes.push(s as u32);
                full.material_slots.push(material);
            }
        }
    }
    let mut levels = vec![full];

    // Reduced levels; a submesh without progress reuses its previous level.
    if let (Some(lod_indices), Some(table)) = (reader.chunk("LODIDX"), reader.chunk("LODTABLE")) {
        let lod_count = chunks::u32_at(table, 0, 8, 0).ok_or_else(bad)? as usize;
        let range = |s: usize, k: usize| {
            let row = 1 + s * lod_count + k;
            Some((
                chunks::u32_at(table, row, 8, 0)?,
                chunks::u32_at(table, row, 8, 4)?,
            ))
        };
        for k in 0..lod_count {
            let mut level = Level {
                same_as_previous: true,
                ..Default::default()
            };
            for (s, &(first_index, index_count, _, _, material)) in ranges.iter().enumerate() {
                // The nearest level at or below this one that has indices.
                let reduced = (0..=k)
                    .rev()
                    .find_map(|j| range(s, j).filter(|r| r.1 > 0).map(|r| (j, r)));
                level.same_as_previous &= reduced.is_none_or(|(j, _)| j != k);
                let corners: Vec<u32> = match reduced {
                    Some((_, (first, count))) => (first..first + count)
                        .map(|i| chunks::u32_at(lod_indices, i as usize, 4, 0))
                        .collect::<Option<_>>()
                        .ok_or_else(bad)?,
                    None => (first_index..first_index + index_count)
                        .map(|i| index(i as usize))
                        .collect::<Option<_>>()
                        .ok_or_else(bad)?,
                };
                for triangle in corners.as_chunks::<3>().0 {
                    for &i in triangle {
                        corner(&mut level, i as usize).ok_or_else(bad)?;
                    }
                    level.submeshes.push(s as u32);
                    level.material_slots.push(material);
                }
            }
            levels.push(level);
        }
    }
    let mesh_bounds = reader.require("BOUNDS")?;
    let meshlet_bounds = reader.require("MLBOUNDS")?;
    Ok(Preview {
        levels,
        bounds: Bounds::read(mesh_bounds, 0).ok_or_else(bad)?,
        submesh_bounds: (0..submesh_count)
            .map(|s| Bounds::read(submeshes, s * 64 + 24))
            .collect::<Option<_>>()
            .ok_or_else(bad)?,
        meshlet_bounds: (0..meshlet_bounds.len() / 32)
            .map(|m| {
                let f = |at| chunks::f32_at(meshlet_bounds, m, 32, at);
                Some(MeshletBounds {
                    center: [f(0)?, f(4)?, f(8)?],
                    radius: f(12)?,
                    cone_axis: [f(16)?, f(20)?, f(24)?],
                    cone_cutoff: f(28)?,
                })
            })
            .collect::<Option<_>>()
            .ok_or_else(bad)?,
        materials: reader
            .require("MATERIAL")?
            .as_chunks::<8>()
            .0
            .iter()
            .map(|id| u64::from_le_bytes(*id))
            .collect(),
    })
}

/// Inspector rows of an `.amesh` file; see docs/features/models.md#inspector.
pub fn rows(bytes: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let reader = chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported {FORMAT} version {}", reader.version));
    }
    let desc = reader.require("DESC")?;
    if desc.len() < 32 {
        return Err("DESC is too short".into());
    }
    let field = |at| chunks::u32_at(desc, 0, 32, at).unwrap_or(0);
    let (stride, width, flags) = (u16::from_le_bytes([desc[20], desc[21]]), desc[22], desc[23]);
    let bounds = reader.require("BOUNDS")?;
    let f = |i: usize| chunks::f32_at(bounds, 0, 40, i * 4).unwrap_or(0.0);
    let joints = reader.chunk("SKELETON").map_or(0, |s| s.len() / 112);
    let lods = reader
        .chunk("LODTABLE")
        .and_then(|t| chunks::u32_at(t, 0, 8, 0))
        .unwrap_or(0);
    let index_type = if width == 4 { "u32" } else { "u16" };
    Ok(vec![
        (
            "Format",
            format!(
                "{FORMAT} v{}, {} chunks",
                reader.version,
                reader.entries().len()
            ),
        ),
        ("Vertices", field(0).to_string()),
        ("Vertex stride", format!("{stride} B")),
        ("Indices", format!("{} ({index_type})", field(4))),
        ("Triangles", (field(4) / 3).to_string()),
        ("Submeshes", field(12).to_string()),
        ("Meshlets", field(8).to_string()),
        (
            "Meshlet limits",
            format!("{} vertices, {} triangles", field(24), field(28)),
        ),
        ("Materials", field(16).to_string()),
        ("Flags", flag_names(flags)),
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

/// `DESC.flags` as `0x07 (tangents, skinned, levels of detail)`.
fn flag_names(flags: u8) -> String {
    let names: Vec<_> = ["tangents", "skinned", "levels of detail"]
        .into_iter()
        .enumerate()
        .filter(|(bit, _)| flags & (1 << bit) != 0)
        .map(|(_, name)| name)
        .collect();
    let names = if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    };
    format!("{flags:#04x} ({names})")
}

/// What a chunk holds, for the Chunks view; see docs/features/models.md#chunks-view.
pub fn describe(name: &str) -> &'static str {
    match name {
        "DESC" => "Mesh descriptor",
        "BOUNDS" => "Mesh bounds",
        "VERTICES" => "Vertices",
        "INDICES" => "Indices",
        "SUBMESH" => "Submeshes",
        "MATERIAL" => "Material asset IDs",
        "MESHLETS" => "Meshlets",
        "MLVERTS" => "Meshlet vertices",
        "MLTRIS" => "Meshlet triangles",
        "MLBOUNDS" => "Meshlet bounds",
        "LODIDX" => "LOD indices",
        "LODTABLE" => "LOD table",
        "SKIN" => "Skin weights",
        "SKELETON" => "Skeleton",
        _ => "Unknown chunk",
    }
}

/// The shape of a chunk's table: its columns (after `#` for arrays; `Field`, `Value` for
/// records) and row count.
pub struct Table {
    pub columns: Vec<&'static str>,
    pub rows: usize,
    /// One row per element, with its index first.
    pub numbered: bool,
}

const DESC_FIELDS: [&str; 10] = [
    "vertex_count",
    "index_count",
    "meshlet_count",
    "submesh_count",
    "material_count",
    "vertex_stride",
    "index_width",
    "flags",
    "meshlet_max_vertices",
    "meshlet_max_triangles",
];
const BOUNDS_FIELDS: [&str; 4] = ["aabb_min", "aabb_max", "sphere_center", "sphere_radius"];

pub fn table(bytes: &[u8], name: &str) -> Option<Table> {
    let reader = chunks::Reader::parse(bytes, FORMAT).ok()?;
    let payload = reader.chunk(name)?;
    let wide = reader.chunk("DESC").and_then(|d| d.get(22)) == Some(&4);
    let array = |columns: Vec<&'static str>, stride: usize| Table {
        columns,
        rows: payload.len() / stride,
        numbered: true,
    };
    Some(match name {
        "DESC" => Table {
            columns: vec!["Field", "Value"],
            rows: DESC_FIELDS.len(),
            numbered: false,
        },
        "BOUNDS" => Table {
            columns: vec!["Field", "Value"],
            rows: BOUNDS_FIELDS.len(),
            numbered: false,
        },
        "VERTICES" => array(vec!["Position", "Normal", "Tangent", "UV"], 28),
        "INDICES" => array(vec!["i0", "i1", "i2"], if wide { 12 } else { 6 }),
        "SUBMESH" => array(
            vec![
                "First index",
                "Index count",
                "First meshlet",
                "Meshlet count",
                "Material",
                "AABB min",
                "AABB max",
            ],
            64,
        ),
        "MATERIAL" => array(vec!["Asset ID"], 8),
        "MESHLETS" => array(
            vec!["Vertex offset", "Vertices", "Triangle offset", "Triangles"],
            16,
        ),
        "MLVERTS" => array(vec!["Vertex"], 4),
        "MLTRIS" => array(vec!["i0", "i1", "i2"], 3),
        "LODIDX" => array(vec!["i0", "i1", "i2"], 12),
        "MLBOUNDS" => array(vec!["Center", "Radius", "Cone axis", "Cone cutoff"], 32),
        // An 8-byte header, then one range per submesh and level.
        "LODTABLE" => Table {
            columns: vec!["Submesh", "Level", "First index", "Index count"],
            rows: payload.len().saturating_sub(8) / 8,
            numbered: true,
        },
        "SKIN" => array(vec!["Joints", "Weights"], 24),
        "SKELETON" => array(vec!["Parent", "Translation", "Rotation", "Scale"], 112),
        _ => Table {
            columns: vec!["Offset", "Bytes"],
            rows: payload.len().div_ceil(16),
            numbered: false,
        },
    })
}

fn vector(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{v:.3}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The text of a cell of a chunk's table (`column` 0 is `#` for numbered tables).
pub fn cell(bytes: &[u8], name: &str, row: usize, column: usize) -> String {
    let Ok(reader) = chunks::Reader::parse(bytes, FORMAT) else {
        return String::new();
    };
    let Some(p) = reader.chunk(name) else {
        return String::new();
    };
    let Some(table) = table(bytes, name) else {
        return String::new();
    };
    if table.numbered && column == 0 {
        return row.to_string();
    }
    let column = if table.numbered { column - 1 } else { column };
    let u32_at =
        |index: usize, stride: usize, at: usize| chunks::u32_at(p, index, stride, at).unwrap_or(0);
    let floats = |index: usize, stride: usize, at: usize, n: usize| -> Vec<f32> {
        (0..n)
            .map(|k| chunks::f32_at(p, index, stride, at + k * 4).unwrap_or(0.0))
            .collect()
    };
    let i16_at = |at: usize| {
        p.get(at..at + 2)
            .map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
    };
    match name {
        "DESC" if column == 0 => DESC_FIELDS[row].to_owned(),
        "DESC" => match row {
            5 => format!(
                "{} B",
                p.get(20..22)
                    .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
            ),
            6 => format!("{} B", p.get(22).copied().unwrap_or(0)),
            7 => flag_names(p.get(23).copied().unwrap_or(0)),
            8 => u32_at(0, 32, 24).to_string(),
            9 => u32_at(0, 32, 28).to_string(),
            _ => u32_at(0, 32, row * 4).to_string(),
        },
        "BOUNDS" if column == 0 => BOUNDS_FIELDS[row].to_owned(),
        "BOUNDS" => match row {
            3 => vector(&floats(0, 40, 36, 1)),
            _ => vector(&floats(0, 40, row * 12, 3)),
        },
        "VERTICES" => {
            let base = row * 28;
            match column {
                0 => vector(&floats(row, 28, 0, 3)),
                1 => vector(&octahedral_decode([i16_at(base + 12), i16_at(base + 14)])),
                2 => {
                    let x = i16_at(base + 16);
                    let [tx, ty, tz] = octahedral_decode([x & !1, i16_at(base + 18)]);
                    let w = if x & 1 == 1 { -1.0 } else { 1.0 };
                    vector(&[tx, ty, tz, w])
                }
                _ => vector(&floats(row, 28, 20, 2)),
            }
        }
        "INDICES" => {
            let wide = reader.chunk("DESC").and_then(|d| d.get(22)) == Some(&4);
            if wide {
                u32_at(row * 3 + column, 4, 0).to_string()
            } else {
                let at = (row * 3 + column) * 2;
                p.get(at..at + 2)
                    .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
                    .to_string()
            }
        }
        "SUBMESH" => match column {
            4 => match u32_at(row, 64, 16) {
                NO_MATERIAL => "—".into(),
                slot => slot.to_string(),
            },
            5 => vector(&floats(row, 64, 24, 3)),
            6 => vector(&floats(row, 64, 36, 3)),
            c => u32_at(row, 64, c * 4).to_string(),
        },
        "MATERIAL" => p
            .get(row * 8..row * 8 + 8)
            .map(|b| format!("{:#018x}", u64::from_le_bytes(b.try_into().unwrap())))
            .unwrap_or_default(),
        "MESHLETS" => u32_at(row, 16, column * 4).to_string(),
        "MLVERTS" => u32_at(row, 4, 0).to_string(),
        "MLTRIS" => p.get(row * 3 + column).map_or(String::new(), u8::to_string),
        "LODIDX" => u32_at(row * 3 + column, 4, 0).to_string(),
        "MLBOUNDS" => match column {
            0 => vector(&floats(row, 32, 0, 3)),
            1 => vector(&floats(row, 32, 12, 1)),
            2 => vector(&floats(row, 32, 16, 3)),
            _ => vector(&floats(row, 32, 28, 1)),
        },
        "LODTABLE" => {
            let lods = u32_at(0, 8, 0).max(1) as usize;
            match column {
                0 => (row / lods).to_string(),
                1 => (row % lods + 1).to_string(),
                c => u32_at(row + 1, 8, (c - 2) * 4).to_string(),
            }
        }
        "SKIN" => {
            if column == 0 {
                (0..4)
                    .map(|k| {
                        p.get(row * 24 + k * 2..row * 24 + k * 2 + 2)
                            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                vector(&floats(row, 24, 8, 4))
            }
        }
        "SKELETON" => match column {
            0 => match u32_at(row, 112, 104) as i32 {
                -1 => "—".into(),
                parent => parent.to_string(),
            },
            1 => vector(&floats(row, 112, 64, 3)),
            2 => vector(&floats(row, 112, 76, 4)),
            _ => vector(&floats(row, 112, 92, 3)),
        },
        _ => {
            let bytes = p.get(row * 16..(row * 16 + 16).min(p.len())).unwrap_or(&[]);
            if column == 0 {
                format!("{:#06x}", row * 16)
            } else {
                bytes
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }
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
    fn previews_meshlet_triangles() {
        let mut second = grid(2);
        second.material = None;
        let (bytes, summary) = encode(
            &[grid(16), second],
            &[7],
            None,
            true,
            &MeshOptions::default(),
        );
        let preview = preview(&bytes).unwrap();
        assert_eq!(preview.materials, [7]);
        let full = &preview.levels[0];
        assert_eq!(full.triangles(), summary.triangles);
        assert_eq!(full.positions.len(), summary.triangles * 3);
        assert_eq!(
            *full.meshlets.last().unwrap() as usize,
            summary.meshlets - 1
        );
        assert_eq!(full.submeshes[0], 0);
        assert_eq!(*full.submeshes.last().unwrap(), 1);
        assert_eq!(full.material_slots[0], Some(0));
        assert_eq!(*full.material_slots.last().unwrap(), None);
        // Bounds: the mesh's hold each submesh's and each meshlet's box.
        assert_eq!(preview.submesh_bounds.len(), 2);
        assert_eq!(preview.meshlet_bounds.len(), summary.meshlets);
        let b = preview.bounds;
        let inside = |[min, max]: [[f32; 3]; 2]| {
            (0..3)
                .all(|c| min[c] >= b.min[c] - 1e-5 && max[c] <= b.max[c] + 1e-5 && min[c] <= max[c])
        };
        assert!(
            preview
                .submesh_bounds
                .iter()
                .all(|s| inside([s.min, s.max]))
        );
        let boxes = preview.meshlet_boxes();
        assert_eq!(boxes.len(), summary.meshlets);
        assert!(boxes.iter().all(|&m| inside(m)));
        // Each meshlet's sphere holds its box's center.
        for (m, [min, max]) in preview.meshlet_bounds.iter().zip(&boxes) {
            let d: f32 = (0..3)
                .map(|c| ((min[c] + max[c]) / 2.0 - m.center[c]).powi(2))
                .sum();
            assert!(d.sqrt() <= m.radius + 1e-4);
        }
        // Normals, tangents and their handedness decode back.
        assert!(full.normals.iter().all(|n| (n[2] - 1.0).abs() < 1e-3));
        assert!(
            full.tangents
                .iter()
                .all(|t| (t[0] - 1.0).abs() < 1e-3 && t[3] == 1.0)
        );
        for n in [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.6, -0.8]] {
            let back = octahedral_decode(octahedral(n));
            assert!(
                (0..3).all(|c| (back[c] - n[c]).abs() < 1e-3),
                "{n:?} {back:?}"
            );
        }

        // Two reduced levels, smaller, without meshlets.
        assert_eq!(preview.levels.len(), 3);
        let lod1 = &preview.levels[1];
        assert!(lod1.triangles() < full.triangles());
        assert!(lod1.meshlets.is_empty());
        assert!(!lod1.same_as_previous);
        // The small grid's submesh is still there in every level.
        assert!(preview.levels.iter().all(|l| l.submeshes.contains(&1)));
    }

    #[test]
    fn chunk_tables_and_info() {
        let (bytes, summary) = encode(&[grid(4)], &[9], None, true, &MeshOptions::default());
        let desc = table(&bytes, "DESC").unwrap();
        assert!(!desc.numbered);
        assert_eq!(desc.rows, 10);
        assert_eq!(cell(&bytes, "DESC", 0, 0), "vertex_count");
        assert_eq!(cell(&bytes, "DESC", 0, 1), "25");
        assert_eq!(cell(&bytes, "DESC", 5, 1), "28 B");
        assert_eq!(
            cell(&bytes, "DESC", 7, 1),
            "0x05 (tangents, levels of detail)"
        );
        let vertices = table(&bytes, "VERTICES").unwrap();
        assert_eq!(vertices.rows, summary.vertices);
        assert_eq!(cell(&bytes, "VERTICES", 1, 0), "1");
        assert_eq!(cell(&bytes, "VERTICES", 1, 2), "0.000, 0.000, 1.000");
        assert_eq!(cell(&bytes, "VERTICES", 1, 3), "1.000, 0.000, 0.000, 1.000");
        assert_eq!(table(&bytes, "INDICES").unwrap().rows, summary.triangles);
        assert_eq!(cell(&bytes, "MATERIAL", 0, 1), "0x0000000000000009");
        assert_eq!(cell(&bytes, "SUBMESH", 0, 5), "0");
        let lods = table(&bytes, "LODTABLE").unwrap();
        assert_eq!(lods.rows, 2);
        assert_eq!(cell(&bytes, "LODTABLE", 1, 2), "2");
        assert_eq!(describe("MLBOUNDS"), "Meshlet bounds");

        let rows = rows(&bytes).unwrap();
        let row = |label: &str| rows.iter().find(|(l, _)| *l == label).unwrap().1.clone();
        assert_eq!(row("Format"), "AMESH v1, 12 chunks");
        assert_eq!(row("Indices"), format!("{} (u16)", summary.triangles * 3));
        assert_eq!(row("Vertex stride"), "28 B");
        assert_eq!(row("Meshlet limits"), "64 vertices, 124 triangles");
        assert_eq!(row("Flags"), "0x05 (tangents, levels of detail)");
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
        let (bytes, summary) = encode(
            &[grid(16), grid(2)],
            &[42],
            None,
            true,
            &MeshOptions::default(),
        );
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
    fn options_come_from_settings_and_overrides() {
        let meshes: config::Meshes = toml::from_str(
            r#"meshlet_max_vertices = 96
lod_ratios = [0.4]

[[overrides]]
match = ["characters/**"]
meshlet_max_vertices = 128
meshlet_max_triangles = 256
lod_prune = true

[[overrides]]
match = ["characters/hero.glb"]
meshlet_max_triangles = 64
"#,
        )
        .unwrap();
        let settings = MeshSettings::new(&meshes).unwrap();
        let limits = |o: MeshOptions| (o.meshlet_max_vertices, o.meshlet_max_triangles);
        assert_eq!(limits(settings.options("props/box.glb")), (96, 124));
        assert_eq!(settings.options("props/box.glb").lod_ratios, [0.4]);
        assert_eq!(limits(settings.options("characters/fox.glb")), (128, 256));
        assert!(settings.options("characters/fox.glb").lod_prune);
        // Later overrides win, key by key.
        assert_eq!(limits(settings.options("characters/hero.glb")), (128, 64));
        // The sidecar wins over everything.
        let amod = crate::amod::Amod::parse("[mesh]\nmeshlet_max_vertices = 32\n").unwrap();
        let options = settings
            .options_with("characters/hero.glb", Some(&amod))
            .unwrap();
        assert_eq!(limits(options), (32, 64));

        // The stored limits are the ones used; no ratios, no LOD chunks.
        let options = MeshOptions {
            meshlet_max_vertices: 16,
            meshlet_max_triangles: 8,
            lod_ratios: Vec::new(),
            ..Default::default()
        };
        let (bytes, summary) = encode(&[grid(16)], &[], None, true, &options);
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        let desc = reader.require("DESC").unwrap();
        assert_eq!(chunks::u32_at(desc, 0, 32, 24), Some(16));
        assert_eq!(chunks::u32_at(desc, 0, 32, 28), Some(8));
        assert_eq!(desc[23], 0b001, "tangents, no LODs");
        assert!(reader.chunk("LODTABLE").is_none());
        let meshlets = reader.require("MESHLETS").unwrap();
        for m in 0..summary.meshlets {
            assert!(chunks::u32_at(meshlets, m, 16, 12).unwrap() <= 8);
        }

        let bad = |text: &str| {
            let meshes: config::Meshes = toml::from_str(text).unwrap();
            MeshSettings::new(&meshes).err().unwrap()
        };
        assert!(bad("meshlet_max_vertices = 300").contains("3–256"));
        assert!(bad("meshlet_max_triangles = 126").contains("multiple of 4"));
        assert!(bad("lod_ratios = [0.25, 0.5]").contains("decreasing"));
        assert!(bad("lod_error = 2.0").contains("lod_error"));
        assert!(bad("[[overrides]]\nmatch = [\"[\"]").starts_with("override 1:"));
        assert!(
            bad("[[overrides]]\nmatch = [\"a\"]\nlod_attempts = 99")
                .starts_with("override 1: lod_attempts")
        );
        assert!(toml::from_str::<config::Meshes>("max = 1").is_err());
        assert!(toml::from_str::<config::Meshes>("[[overrides]]\nlod_prune = true").is_err());
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
        let (bytes, summary) = encode(
            &[grid(2)],
            &[],
            Some(&[joint]),
            false,
            &MeshOptions::default(),
        );
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        assert_eq!(reader.require("SKIN").unwrap().len(), summary.vertices * 24);
        assert_eq!(reader.require("SKELETON").unwrap().len(), 112);
        assert_eq!(reader.require("DESC").unwrap()[23], 0b110);
        assert!(rows(&bytes).unwrap().contains(&("Skin", "1 joint".into())));
    }
}
