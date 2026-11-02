//! `.amesh` meshes: submeshes with meshlets, bounds, levels of detail and skin; see
//! docs/spec/amesh.md.

use std::collections::HashMap;

use meshoptimizer::Meshlet;

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

    fn simplify_options(&self) -> u32 {
        [
            (self.lod_permissive, meshoptimizer::SIMPLIFY_PERMISSIVE),
            (self.lod_lock_border, meshoptimizer::SIMPLIFY_LOCK_BORDER),
            (self.lod_regularize, meshoptimizer::SIMPLIFY_REGULARIZE),
            (self.lod_prune, meshoptimizer::SIMPLIFY_PRUNE),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |options, (_, flag)| options | flag)
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
    /// Per triangle of `indices`, its regions as bits; empty without regions.
    pub regions: Vec<u32>,
    /// Each of [`Extras::color_sets`], one color per vertex.
    pub colors: Vec<Vec<[u8; 4]>>,
}

/// A shape ready to write; see docs/spec/amesh.md#shapes.
pub struct CompiledShape {
    pub name: String,
    pub kind: String,
    /// Index into the skeleton; `-1` is mesh space.
    pub joint: i32,
    /// `type`: 0 box, 1 sphere, 2 capsule, 3 hull.
    pub shape: u32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub size: [f32; 3],
    /// A hull's points, in the parent's space.
    pub points: Vec<[f32; 3]>,
    pub properties: crate::enrich::Properties,
}

/// A path ready to write; see docs/spec/amesh.md#paths.
pub struct CompiledPath {
    pub name: String,
    pub kind: String,
    /// Index into the skeleton; `-1` is mesh space.
    pub joint: i32,
    pub closed: bool,
    pub tension: f32,
    pub alpha: f32,
    pub length: f32,
    /// In the parent's space.
    pub points: Vec<[f32; 3]>,
    pub samples: Vec<crate::path::Sample>,
    pub properties: crate::enrich::Properties,
}

/// What a model carries besides its geometry: locators, properties, shapes, regions, paths
/// (name and tags; their triangles are in [`Primitive::regions`]).
#[derive(Default)]
pub struct Extras {
    pub locators: Vec<Locator>,
    pub properties: crate::enrich::Properties,
    pub shapes: Vec<CompiledShape>,
    pub regions: Vec<(String, Vec<String>)>,
    pub paths: Vec<CompiledPath>,
    /// The name and `source` of the color sets every primitive has; see
    /// docs/spec/amesh.md#color-sets.
    pub color_sets: Vec<(String, u32)>,
}

/// The `STRINGS` chunk being built: text back to back.
#[derive(Default)]
pub(crate) struct Strings(pub Vec<u8>);

impl Strings {
    /// Adds `text`; returns its offset and length.
    pub fn add(&mut self, text: &str) -> (u32, u32) {
        let at = self.0.len() as u32;
        self.0.extend_from_slice(text.as_bytes());
        (at, text.len() as u32)
    }
}

/// Writes `properties` as [`Property`](docs/spec/amesh.md#property-32-bytes) records.
pub(crate) fn put_properties(
    out: &mut Bytes,
    strings: &mut Strings,
    properties: &crate::enrich::Properties,
) {
    use crate::enrich::PropValue;
    for (key, value) in properties {
        let (at, len) = strings.add(key);
        out.u64(crate::hash::fnv1a64(key.as_bytes()))
            .u32(at)
            .u32(len)
            .u32(value.type_index() as u32)
            .u32(0);
        match value {
            PropValue::Bool(b) => out.u64(u64::from(*b)),
            PropValue::Int(i) => out.u64(*i as u64),
            PropValue::Float(f) => out.u64(f.to_bits()),
            PropValue::Text(t) => {
                let (at, len) = strings.add(t);
                out.u32(at).u32(len)
            }
        };
    }
}

/// Writes `tags` as [`Tag`](docs/spec/amesh.md#tag-16-bytes) records.
fn put_tags(out: &mut Bytes, strings: &mut Strings, tags: &[String]) {
    for tag in tags {
        let (at, len) = strings.add(tag);
        out.u64(crate::hash::fnv1a64(tag.as_bytes()))
            .u32(at)
            .u32(len);
    }
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

/// A locator ready to write; see docs/spec/amesh.md#locators.
pub struct Locator {
    pub name: String,
    pub tags: Vec<String>,
    /// Index into the skeleton; `-1` is mesh space.
    pub joint: i32,
    pub translation: [f32; 3],
    /// Unit quaternion `x y z w`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
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
    extras: &Extras,
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
    // Each color set's colors, parallel to `vertices`.
    let mut colors: Vec<Vec<[u8; 4]>> = vec![Vec::new(); extras.color_sets.len()];
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
    let locators = &extras.locators;
    // Per triangle of `indices`, its regions; see docs/spec/amesh.md#regions.
    let mut triangle_regions: Vec<u32> = Vec::new();

    for primitive in primitives.iter().filter(|p| !p.indices.is_empty()) {
        submesh_count += 1;
        let vertex_count = primitive.vertices.len();
        let index_count = primitive.indices.len();
        let mut local = vec![0u32; index_count];
        meshoptimizer::optimize_vertex_cache(
            &mut local,
            &primitive.indices,
            index_count,
            vertex_count,
        )
        .expect("vertex cache");
        let positions: Vec<f32> = primitive.vertices.iter().flat_map(|v| v.position).collect();
        let base = vertices.len() as u32;
        let first_index = indices.len() as u32;
        let first_meshlet = meshlet_count;

        // Meshlets, each then reordered for locality; see docs/spec/assetc.toml.md#meshes.
        let bound = meshoptimizer::build_meshlets_bound(index_count, max_vertices, max_triangles)
            .expect("meshlet limits");
        let mut built = vec![Meshlet::default(); bound];
        let mut built_vertices = vec![0u32; index_count];
        let mut built_triangles = vec![0u8; index_count];
        let count = meshoptimizer::build_meshlets(
            &mut built,
            &mut built_vertices,
            &mut built_triangles,
            &local,
            index_count,
            &positions,
            vertex_count,
            12,
            max_vertices,
            max_triangles,
            options.meshlet_cone_weight as f32,
        )
        .expect("meshlets");
        for meshlet in &built[..count] {
            let (vertex_count_m, triangle_count) = (
                meshlet.vertex_count as usize,
                meshlet.triangle_count as usize,
            );
            let at = meshlet.vertex_offset as usize;
            let own_vertices = &mut built_vertices[at..at + vertex_count_m];
            let at = meshlet.triangle_offset as usize;
            let own_triangles = &mut built_triangles[at..at + triangle_count * 3];
            meshoptimizer::optimize_meshlet(
                own_vertices,
                own_triangles,
                triangle_count,
                vertex_count_m,
            )
            .expect("meshlet order");
            meshlets
                .u32(meshlet_vertex_count)
                .u32(vertex_count_m as u32)
                .u32(meshlet_triangle_count)
                .u32(triangle_count as u32);
            for v in own_vertices.iter() {
                meshlet_vertices.u32(v + base);
            }
            meshlet_triangles.0.extend_from_slice(own_triangles);
            let b = meshoptimizer::compute_meshlet_bounds(
                own_vertices,
                own_triangles,
                triangle_count,
                &positions,
                vertex_count,
                12,
            )
            .expect("meshlet bounds");
            meshlet_bounds
                .f32s(&b.center)
                .f32s(&[b.radius])
                .f32s(&b.cone_axis)
                .f32s(&[b.cone_cutoff]);
            meshlet_vertex_count += vertex_count_m as u32;
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
        let mut previous = local.len();
        for &ratio in &options.lod_ratios {
            let target = ((local.len() as f64 * ratio / 3.0).round() as usize * 3).max(3);
            let mut error = options.lod_error as f32;
            let mut level = vec![0u32; index_count];
            let mut level_count = 0;
            for _ in 0..=options.lod_attempts {
                level_count = meshoptimizer::simplify_with_attributes(
                    &mut level,
                    &local,
                    index_count,
                    &positions,
                    vertex_count,
                    12,
                    &attributes,
                    5 * 4,
                    &weights,
                    5,
                    None,
                    target,
                    error,
                    simplify_options,
                    None,
                )
                .expect("simplify");
                if level_count < previous {
                    break;
                }
                error += options.lod_error_step as f32;
            }
            if level_count >= previous && options.lod_sloppy_fallback {
                level_count = meshoptimizer::simplify_sloppy(
                    &mut level,
                    &local,
                    index_count,
                    &positions,
                    vertex_count,
                    12,
                    None,
                    target,
                    1.0,
                    None,
                )
                .expect("simplify sloppy");
            }
            level.truncate(level_count);
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
        if !extras.regions.is_empty() {
            // The vertex cache order moves triangles: find each one's source by its corners.
            let mut sources: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
            for (t, triangle) in primitive
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .enumerate()
                .rev()
            {
                let mut key = *triangle;
                key.sort_unstable();
                sources.entry(key).or_default().push(t);
            }
            for triangle in local.as_chunks::<3>().0 {
                let mut key = *triangle;
                key.sort_unstable();
                let source = sources.get_mut(&key).and_then(Vec::pop);
                triangle_regions.push(
                    source
                        .and_then(|t| primitive.regions.get(t).copied())
                        .unwrap_or(0),
                );
            }
        }
        indices.extend(local.iter().map(|i| i + base));
        vertices.extend_from_slice(&primitive.vertices);
        for (s, set) in colors.iter_mut().enumerate() {
            match primitive.colors.get(s) {
                Some(own) if own.len() == primitive.vertices.len() => set.extend_from_slice(own),
                _ => set.extend(std::iter::repeat_n([255; 4], primitive.vertices.len())),
            }
        }
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
    if !locators.is_empty() {
        let (mut records, mut names, mut tags) = (Bytes::default(), Vec::new(), Bytes::default());
        // Names first, then the tags' text; see docs/spec/amesh.md#locators.
        let name_offsets: Vec<u32> = locators
            .iter()
            .map(|l| {
                let at = names.len() as u32;
                names.extend_from_slice(l.name.as_bytes());
                at
            })
            .collect();
        let mut tag_count = 0u16;
        for (l, at) in locators.iter().zip(name_offsets) {
            records
                .u64(crate::hash::fnv1a64(l.name.as_bytes()))
                .u32(at)
                .u32(l.name.len() as u32)
                .i32(l.joint)
                .f32s(&l.translation)
                .f32s(&l.rotation)
                .f32s(&l.scale)
                .u16(if l.tags.is_empty() { 0 } else { tag_count })
                .u16(l.tags.len() as u16);
            for tag in &l.tags {
                tags.u64(crate::hash::fnv1a64(tag.as_bytes()))
                    .u32(names.len() as u32)
                    .u32(tag.len() as u32);
                names.extend_from_slice(tag.as_bytes());
                tag_count += 1;
            }
        }
        writer
            .chunk("LOCATORS", records.take())
            .chunk("LOCNAMES", names);
        if tag_count > 0 {
            writer.chunk("LOCTAGS", tags.take());
        }
    }
    // Color sets, properties, shapes, regions and paths, their text in STRINGS.
    let mut strings = Strings::default();
    if !colors.is_empty() {
        let mut sets = Bytes::default();
        let mut data = Bytes::default();
        for ((name, source), set) in extras.color_sets.iter().zip(&colors) {
            let (at, len) = strings.add(name);
            sets.u64(crate::hash::fnv1a64(name.as_bytes()))
                .u32(at)
                .u32(len)
                .u32(0)
                .u32(*source)
                .u64(0);
            for c in set {
                data.0.extend_from_slice(c);
            }
        }
        writer
            .chunk("COLORSET", sets.take())
            .chunk("COLORS", data.take());
    }
    let mut props = Bytes::default();
    put_properties(&mut props, &mut strings, &extras.properties);
    let (mut shapes, mut shape_points, mut shape_props) =
        (Bytes::default(), Bytes::default(), Bytes::default());
    let (mut point_count, mut property_count) = (0u32, 0u32);
    for shape in &extras.shapes {
        let (name_at, name_len) = strings.add(&shape.name);
        let (kind_at, kind_len) = strings.add(&shape.kind);
        shapes
            .u64(crate::hash::fnv1a64(shape.name.as_bytes()))
            .u32(name_at)
            .u32(name_len)
            .u64(crate::hash::fnv1a64(shape.kind.as_bytes()))
            .u32(kind_at)
            .u32(kind_len)
            .i32(shape.joint)
            .u32(shape.shape)
            .f32s(&shape.translation)
            .f32s(&shape.rotation)
            .f32s(&shape.size)
            .u32(if shape.points.is_empty() {
                0
            } else {
                point_count
            })
            .u32(shape.points.len() as u32)
            .u32(if shape.properties.is_empty() {
                0
            } else {
                property_count
            })
            .u32(shape.properties.len() as u32);
        for point in &shape.points {
            shape_points.f32s(point);
        }
        put_properties(&mut shape_props, &mut strings, &shape.properties);
        point_count += shape.points.len() as u32;
        property_count += shape.properties.len() as u32;
    }
    let (mut regions, mut region_tags) = (Bytes::default(), Bytes::default());
    let mut tag_count = 0u16;
    for (i, (name, tags)) in extras.regions.iter().enumerate() {
        let (at, len) = strings.add(name);
        let triangles = triangle_regions
            .iter()
            .filter(|m| *m & (1 << i) != 0)
            .count();
        regions
            .u64(crate::hash::fnv1a64(name.as_bytes()))
            .u32(at)
            .u32(len)
            .u16(if tags.is_empty() { 0 } else { tag_count })
            .u16(tags.len() as u16)
            .u32(triangles as u32)
            .u64(0);
        put_tags(&mut region_tags, &mut strings, tags);
        tag_count += tags.len() as u16;
    }
    let (mut paths, mut path_points, mut path_samples, mut path_props) = (
        Bytes::default(),
        Bytes::default(),
        Bytes::default(),
        Bytes::default(),
    );
    let (mut path_point_count, mut path_sample_count, mut path_property_count) = (0u32, 0u32, 0u32);
    for path in &extras.paths {
        let (name_at, name_len) = strings.add(&path.name);
        let (kind_at, kind_len) = strings.add(&path.kind);
        paths
            .u64(crate::hash::fnv1a64(path.name.as_bytes()))
            .u32(name_at)
            .u32(name_len)
            .u64(crate::hash::fnv1a64(path.kind.as_bytes()))
            .u32(kind_at)
            .u32(kind_len)
            .i32(path.joint)
            .u32(u32::from(path.closed))
            .f32s(&[path.tension, path.alpha, path.length])
            .u32(path_point_count)
            .u32(path.points.len() as u32)
            .u32(path_sample_count)
            .u32(path.samples.len() as u32)
            .u32(if path.properties.is_empty() {
                0
            } else {
                path_property_count
            })
            .u32(path.properties.len() as u32)
            .u32(0);
        for point in &path.points {
            path_points.f32s(point);
        }
        for sample in &path.samples {
            path_samples
                .f32s(&sample.position)
                .f32s(&[sample.distance])
                .f32s(&sample.tangent)
                .f32s(&[sample.u]);
        }
        put_properties(&mut path_props, &mut strings, &path.properties);
        path_point_count += path.points.len() as u32;
        path_sample_count += path.samples.len() as u32;
        path_property_count += path.properties.len() as u32;
    }
    let has_props = !extras.properties.is_empty();
    let has_shapes = !extras.shapes.is_empty();
    let has_regions = !extras.regions.is_empty();
    let has_paths = !extras.paths.is_empty();
    if !colors.is_empty() || has_props || has_shapes || has_regions || has_paths {
        writer.chunk("STRINGS", strings.0);
    }
    if has_props {
        writer.chunk("PROPS", props.take());
    }
    if has_shapes {
        writer.chunk("SHAPES", shapes.take());
        if point_count > 0 {
            writer.chunk("SHAPEPTS", shape_points.take());
        }
        if property_count > 0 {
            writer.chunk("SHAPEPRP", shape_props.take());
        }
    }
    if has_regions {
        writer.chunk("REGIONS", regions.take());
        if tag_count > 0 {
            writer.chunk("REGTAGS", region_tags.take());
        }
        let mut masks = Bytes::default();
        for mask in &triangle_regions {
            masks.u32(*mask);
        }
        writer.chunk("TRIREGN", masks.take());
    }
    if has_paths {
        writer.chunk("PATHS", paths.take());
        writer.chunk("PATHPTS", path_points.take());
        writer.chunk("PATHSMP", path_samples.take());
        if path_property_count > 0 {
            writer.chunk("PATHPRP", path_props.take());
        }
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
    /// Each of [`Preview::color_sets`], linear, one color per corner.
    pub colors: Vec<Vec<[f32; 4]>>,
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
    /// Each color set's name and `source`, in file order; see
    /// docs/features/vertex-colors.md#compiled-meshes.
    pub color_sets: Vec<(String, u32)>,
    /// Each locator's name and transform in mesh space, joints at their bind pose; see
    /// docs/features/locators.md#compiled-mesh-preview.
    pub locators: Vec<(String, bevy::math::Affine3A)>,
}

/// The locators of a compiled mesh, placed in mesh space.
fn preview_locators(reader: &chunks::Reader<'_>) -> Vec<(String, bevy::math::Affine3A)> {
    use bevy::math::{Affine3A, Mat4, Quat, Vec3};
    let (Some(records), names) = (reader.chunk("LOCATORS"), reader.chunk("LOCNAMES")) else {
        return Vec::new();
    };
    let skeleton = reader.chunk("SKELETON").unwrap_or_default();
    let floats = |p: &[u8], at: usize, n: usize| -> Option<Vec<f32>> {
        (0..n)
            .map(|k| chunks::f32_at(p, 0, 0, at + k * 4))
            .collect()
    };
    records
        .as_chunks::<64>()
        .0
        .iter()
        .filter_map(|r| {
            let (at, len) = (
                chunks::u32_at(r, 0, 64, 8)? as usize,
                chunks::u32_at(r, 0, 64, 12)? as usize,
            );
            let name = names
                .and_then(|n| n.get(at..at + len))
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default();
            let t = floats(r, 20, 3)?;
            let q = floats(r, 32, 4)?;
            let s = floats(r, 48, 3)?;
            let local = Affine3A::from_scale_rotation_translation(
                Vec3::from_slice(&s),
                Quat::from_slice(&q).normalize(),
                Vec3::from_slice(&t),
            );
            let joint = chunks::u32_at(r, 0, 64, 16)? as i32;
            let parent = usize::try_from(joint)
                .ok()
                .and_then(|j| floats(skeleton.get(j * 112..j * 112 + 64)?, 0, 16))
                .map_or(Affine3A::IDENTITY, |m| {
                    Affine3A::from_mat4(Mat4::from_cols_slice(&m).inverse())
                });
            Some((name, parent * local))
        })
        .collect()
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
    // Every color set's colors; see docs/features/vertex-colors.md#compiled-meshes.
    let vertex_count = chunks::u32_at(desc, 0, 32, 0).unwrap_or(0) as usize;
    let color_sets = color_sets(&reader);
    let set_colors: Vec<&[u8]> = (0..color_sets.len())
        .map(|s| {
            reader
                .chunk("COLORS")
                .and_then(|c| c.get(s * vertex_count * 4..(s + 1) * vertex_count * 4))
                .unwrap_or_default()
        })
        .collect();
    let corner = |level: &mut Level, i: usize| -> Option<()> {
        level.colors.resize(set_colors.len(), Vec::new());
        for (colors, out) in set_colors.iter().zip(&mut level.colors) {
            let c = colors.get(i * 4..i * 4 + 4)?;
            out.push([0, 1, 2, 3].map(|k| f32::from(c[k]) / 255.0));
        }
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
        color_sets,
        locators: preview_locators(&reader),
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
    let locators = reader.chunk("LOCATORS").map_or(0, |l| l.len() / 64);
    let count =
        |chunk: &str, stride: usize| match reader.chunk(chunk).map_or(0, |c| c.len() / stride) {
            0 => "none".to_owned(),
            n => n.to_string(),
        };
    let (properties, shapes, regions, paths) = (
        count("PROPS", 32),
        count("SHAPES", 96),
        count("REGIONS", 32),
        count("PATHS", 80),
    );
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
        ("Color sets", {
            let names = color_set_names(&reader);
            if names.is_empty() {
                "none".into()
            } else {
                names.join(", ")
            }
        }),
        (
            "Locators",
            if locators > 0 {
                locators.to_string()
            } else {
                "none".into()
            },
        ),
        ("Properties", properties),
        ("Shapes", shapes),
        ("Regions", regions),
        ("Paths", paths),
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

/// The names of a mesh's color sets, in order; see docs/spec/amesh.md#color-sets.
pub fn color_set_names(reader: &chunks::Reader<'_>) -> Vec<String> {
    color_sets(reader)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// A mesh's color sets: name and `source`, in order.
fn color_sets(reader: &chunks::Reader<'_>) -> Vec<(String, u32)> {
    let (Some(sets), strings) = (reader.chunk("COLORSET"), reader.chunk("STRINGS")) else {
        return Vec::new();
    };
    (0..sets.len() / 32)
        .map(|s| {
            let at = chunks::u32_at(sets, s, 32, 8).unwrap_or(0) as usize;
            let len = chunks::u32_at(sets, s, 32, 12).unwrap_or(0) as usize;
            let name = strings
                .and_then(|t| t.get(at..at + len))
                .map(|t| String::from_utf8_lossy(t).into_owned())
                .unwrap_or_default();
            (name, chunks::u32_at(sets, s, 32, 20).unwrap_or(0))
        })
        .collect()
}

/// `COLORSET.source` as shown: `glTF`, `assetc` or the number.
pub fn source_name(source: u32) -> String {
    match source {
        0 => "glTF".into(),
        1 => "assetc".into(),
        other => other.to_string(),
    }
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
        "COLORSET" => "Color sets",
        "COLORS" => "Vertex colors",
        "SKELETON" => "Skeleton",
        "LOCATORS" => "Locators",
        "LOCNAMES" => "Locator names",
        "LOCTAGS" => "Locator tags",
        "STRINGS" => "Strings",
        "PROPS" => "Properties",
        "SHAPES" => "Shapes",
        "SHAPEPTS" => "Hull points",
        "SHAPEPRP" => "Shape properties",
        "REGIONS" => "Regions",
        "REGTAGS" => "Region tags",
        "TRIREGN" => "Triangle regions",
        "PATHS" => "Paths",
        "PATHPTS" => "Path points",
        "PATHSMP" => "Path samples",
        "PATHPRP" => "Path properties",
        _ => "Unknown chunk",
    }
}

/// The shape of a chunk's table: its columns (after `#` for arrays; `Field`, `Value` for
/// records) and row count.
pub struct Table {
    pub columns: Vec<String>,
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
    let array = |columns: Vec<&str>, stride: usize| Table {
        columns: columns.into_iter().map(str::to_owned).collect(),
        rows: payload.len() / stride,
        numbered: true,
    };
    Some(match name {
        "DESC" => Table {
            columns: ["Field", "Value"].map(String::from).into(),
            rows: DESC_FIELDS.len(),
            numbered: false,
        },
        "BOUNDS" => Table {
            columns: ["Field", "Value"].map(String::from).into(),
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
            columns: ["Submesh", "Level", "First index", "Index count"]
                .map(String::from)
                .into(),
            rows: payload.len().saturating_sub(8) / 8,
            numbered: true,
        },
        "SKIN" => array(vec!["Joints", "Weights"], 24),
        "COLORSET" => array(vec!["Name", "Hash", "Format", "Source"], 32),
        // One row per vertex, one column per set.
        "COLORS" => {
            let names = color_set_names(&reader);
            Table {
                rows: payload.len() / 4 / names.len().max(1),
                columns: names,
                numbered: true,
            }
        }
        "SKELETON" => array(vec!["Parent", "Translation", "Rotation", "Scale"], 112),
        "LOCATORS" => array(
            vec![
                "Name",
                "Hash",
                "Joint",
                "Translation",
                "Rotation",
                "Scale",
                "Tags",
            ],
            64,
        ),
        "LOCTAGS" | "REGTAGS" => array(vec!["Name", "Hash"], 16),
        "PROPS" | "SHAPEPRP" | "PATHPRP" => array(vec!["Key", "Type", "Value"], 32),
        "PATHS" => array(
            vec![
                "Name",
                "Kind",
                "Joint",
                "Closed",
                "Tension",
                "Alpha",
                "Length",
                "Points",
                "Samples",
                "Properties",
            ],
            80,
        ),
        "PATHSMP" => array(vec!["Position", "Distance", "Tangent", "U"], 32),
        "SHAPES" => array(
            vec![
                "Name",
                "Kind",
                "Joint",
                "Type",
                "Translation",
                "Rotation",
                "Size",
                "Points",
                "Properties",
            ],
            96,
        ),
        "SHAPEPTS" | "PATHPTS" => array(vec!["Point"], 12),
        "REGIONS" => array(vec!["Name", "Tags", "Triangles"], 32),
        "TRIREGN" => array(vec!["Regions"], 4),
        _ => Table {
            columns: ["Offset", "Bytes"].map(String::from).into(),
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
        "COLORSET" => {
            let strings = reader.chunk("STRINGS").unwrap_or_default();
            match column {
                0 => {
                    let (at, len) = (u32_at(row, 32, 8) as usize, u32_at(row, 32, 12) as usize);
                    strings
                        .get(at..at + len)
                        .map(|t| String::from_utf8_lossy(t).into_owned())
                        .unwrap_or_default()
                }
                1 => format!(
                    "{:#018x}",
                    p.get(row * 32..row * 32 + 8)
                        .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()))
                ),
                2 => match u32_at(row, 32, 16) {
                    0 => "RGBA8".into(),
                    other => other.to_string(),
                },
                _ => source_name(u32_at(row, 32, 20)),
            }
        }
        "COLORS" => {
            // Sets one after another, each `vertices` long.
            let vertices = table.rows;
            let at = (column * vertices + row) * 4;
            p.get(at..at + 4)
                .map(|c| format!("{}, {}, {}, {}", c[0], c[1], c[2], c[3]))
                .unwrap_or_default()
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
        "PROPS" | "SHAPEPRP" | "SHAPES" | "SHAPEPTS" | "REGIONS" | "REGTAGS" | "TRIREGN"
        | "PATHS" | "PATHPTS" | "PATHSMP" | "PATHPRP" => {
            enrichment_cell(&reader, p, name, row, column)
        }
        "LOCATORS" | "LOCTAGS" => {
            let stride = if name == "LOCATORS" { 64 } else { 16 };
            let names = reader.chunk("LOCNAMES").unwrap_or_default();
            // The text at an (offset, length) pair of a record of `chunk`.
            let text = |chunk: &[u8], stride: usize, row: usize| {
                let at = chunks::u32_at(chunk, row, stride, 8).unwrap_or(0) as usize;
                let len = chunks::u32_at(chunk, row, stride, 12).unwrap_or(0) as usize;
                names
                    .get(at..at + len)
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default()
            };
            match column {
                0 => text(p, stride, row),
                1 => format!(
                    "{:#018x}",
                    p.get(row * stride..row * stride + 8)
                        .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()))
                ),
                _ if name == "LOCTAGS" => String::new(),
                2 => match u32_at(row, 64, 16) as i32 {
                    -1 => "—".into(),
                    joint => joint.to_string(),
                },
                3 => vector(&floats(row, 64, 20, 3)),
                4 => vector(&floats(row, 64, 32, 4)),
                5 => vector(&floats(row, 64, 48, 3)),
                _ => {
                    let u16_at = |at: usize| {
                        p.get(row * 64 + at..row * 64 + at + 2)
                            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
                            as usize
                    };
                    let tags = reader.chunk("LOCTAGS").unwrap_or_default();
                    (u16_at(60)..u16_at(60) + u16_at(62))
                        .map(|t| text(tags, 16, t))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            }
        }
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

/// A cell of the properties, shapes, regions and paths chunks; see docs/features/models.md#chunks-view.
fn enrichment_cell(
    reader: &chunks::Reader<'_>,
    p: &[u8],
    name: &str,
    row: usize,
    column: usize,
) -> String {
    let strings = reader.chunk("STRINGS").unwrap_or_default();
    let u32_at = |chunk: &[u8], stride: usize, row: usize, at: usize| {
        chunks::u32_at(chunk, row, stride, at).unwrap_or(0) as usize
    };
    let text = |at: usize, len: usize| {
        strings
            .get(at..at + len)
            .map(|t| String::from_utf8_lossy(t).into_owned())
            .unwrap_or_default()
    };
    let hash = |stride: usize| {
        p.get(row * stride..row * stride + 8)
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()))
    };
    // A tag or property's name, at offset 8 of its record.
    let named = |chunk: &[u8], stride: usize, row: usize| {
        text(
            u32_at(chunk, stride, row, 8),
            u32_at(chunk, stride, row, 12),
        )
    };
    let floats_of = |stride: usize, at: usize, n: usize| -> Vec<f32> {
        (0..n)
            .map(|k| chunks::f32_at(p, row, stride, at + k * 4).unwrap_or(0.0))
            .collect()
    };
    let floats = |at: usize, n: usize| floats_of(96, at, n);
    match (name, column) {
        ("REGTAGS", 0) => named(p, 16, row),
        ("REGTAGS", _) => format!("{:#018x}", hash(16)),
        ("PROPS" | "SHAPEPRP" | "PATHPRP", 0) => named(p, 32, row),
        ("PROPS" | "SHAPEPRP" | "PATHPRP", 1) => crate::enrich::PropValue::TYPES
            .get(u32_at(p, 32, row, 16))
            .copied()
            .unwrap_or("?")
            .to_owned(),
        ("PROPS" | "SHAPEPRP" | "PATHPRP", _) => {
            let raw = p
                .get(row * 32 + 24..row * 32 + 32)
                .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()));
            match u32_at(p, 32, row, 16) {
                0 => (raw != 0).to_string(),
                1 => (raw as i64).to_string(),
                2 => f64::from_bits(raw).to_string(),
                _ => format!("{:?}", text(u32_at(p, 32, row, 24), u32_at(p, 32, row, 28))),
            }
        }
        ("SHAPES", 0) => named(p, 96, row),
        ("SHAPES", 1) => text(u32_at(p, 96, row, 24), u32_at(p, 96, row, 28)),
        ("SHAPES", 2) => match u32_at(p, 96, row, 32) as i32 {
            -1 => "—".into(),
            joint => joint.to_string(),
        },
        ("SHAPES", 3) => ["box", "sphere", "capsule", "hull"]
            .get(u32_at(p, 96, row, 36))
            .copied()
            .unwrap_or("?")
            .to_owned(),
        ("SHAPES", 4) => vector(&floats(40, 3)),
        ("SHAPES", 5) => vector(&floats(52, 4)),
        ("SHAPES", 6) => vector(&floats(68, 3)),
        ("SHAPES", 7) => u32_at(p, 96, row, 84).to_string(),
        ("SHAPES", _) => u32_at(p, 96, row, 92).to_string(),
        ("PATHS", 0) => named(p, 80, row),
        ("PATHS", 1) => text(u32_at(p, 80, row, 24), u32_at(p, 80, row, 28)),
        ("PATHS", 2) => match u32_at(p, 80, row, 32) as i32 {
            -1 => "—".into(),
            joint => joint.to_string(),
        },
        ("PATHS", 3) => if u32_at(p, 80, row, 36) & 1 != 0 {
            "yes"
        } else {
            "no"
        }
        .into(),
        ("PATHS", 4) => vector(&floats_of(80, 40, 1)),
        ("PATHS", 5) => vector(&floats_of(80, 44, 1)),
        ("PATHS", 6) => vector(&floats_of(80, 48, 1)),
        ("PATHS", 7) => u32_at(p, 80, row, 56).to_string(),
        ("PATHS", 8) => u32_at(p, 80, row, 64).to_string(),
        ("PATHS", _) => u32_at(p, 80, row, 72).to_string(),
        ("PATHSMP", 0) => vector(&floats_of(32, 0, 3)),
        ("PATHSMP", 1) => vector(&floats_of(32, 12, 1)),
        ("PATHSMP", 2) => vector(&floats_of(32, 16, 3)),
        ("PATHSMP", _) => vector(&floats_of(32, 28, 1)),
        ("SHAPEPTS" | "PATHPTS", _) => vector(
            &(0..3)
                .map(|k| chunks::f32_at(p, row, 12, k * 4).unwrap_or(0.0))
                .collect::<Vec<_>>(),
        ),
        ("REGIONS", 0) => named(p, 32, row),
        ("REGIONS", 1) => {
            let tags = reader.chunk("REGTAGS").unwrap_or_default();
            let first = p
                .get(row * 32 + 16..row * 32 + 18)
                .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
            let count = p
                .get(row * 32 + 18..row * 32 + 20)
                .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
            (first..first + count)
                .map(|t| named(tags, 16, t as usize))
                .collect::<Vec<_>>()
                .join(", ")
        }
        ("REGIONS", _) => u32_at(p, 32, row, 20).to_string(),
        (_, _) => {
            // TRIREGN: the names of the bits set.
            let mask = u32_at(p, 4, row, 0);
            let regions = reader.chunk("REGIONS").unwrap_or_default();
            let names: Vec<String> = (0..32)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| named(regions, 32, i))
                .collect();
            if names.is_empty() {
                "—".into()
            } else {
                names.join(", ")
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
            regions: Vec::new(),
            colors: Vec::new(),
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
            &Extras::default(),
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
        let (bytes, summary) = encode(
            &[grid(4)],
            &[9],
            None,
            &Extras::default(),
            true,
            &MeshOptions::default(),
        );
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
            &Extras::default(),
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
        let (bytes, summary) = encode(&[grid(16)], &[], None, &Extras::default(), true, &options);
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
    fn properties_shapes_and_regions() {
        use crate::enrich::{PropValue, Properties};
        let properties: Properties = [
            ("mass".to_owned(), PropValue::Float(80.5)),
            ("name".to_owned(), PropValue::Text("Hero".into())),
        ]
        .into();
        let mut part = grid(4);
        // Region 0 on the first two triangles, region 1 on every other one.
        let triangles = part.indices.len() / 3;
        part.regions = (0..triangles)
            .map(|t| u32::from(t < 2) | (u32::from(t % 2 == 0) << 1))
            .collect();
        let corners: Vec<[u32; 3]> = part.indices.as_chunks::<3>().0.to_vec();
        let expected: Vec<u32> = part.regions.clone();
        let extras = Extras {
            properties: properties.clone(),
            shapes: vec![
                CompiledShape {
                    name: "head".into(),
                    kind: "hitbox".into(),
                    joint: -1,
                    shape: 2,
                    translation: [0.0, 1.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    size: [0.5, 1.0, 0.0],
                    points: Vec::new(),
                    properties: [("damage".to_owned(), PropValue::Float(2.0))].into(),
                },
                CompiledShape {
                    name: "body".into(),
                    kind: String::new(),
                    joint: -1,
                    shape: 3,
                    translation: [0.0; 3],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    size: [0.0; 3],
                    points: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    properties: Properties::new(),
                },
            ],
            regions: vec![
                ("metal".into(), vec!["surface".into()]),
                ("breakable".into(), Vec::new()),
            ],
            paths: {
                let square = vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [1.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0],
                ];
                let curve = crate::path::Curve::new(&square, true, 1.0, 0.5).unwrap();
                vec![CompiledPath {
                    name: "patrol".into(),
                    kind: "route".into(),
                    joint: -1,
                    closed: true,
                    tension: 1.0,
                    alpha: 0.5,
                    length: curve.length(),
                    points: square,
                    samples: curve.samples(),
                    properties: [("speed".to_owned(), PropValue::Float(2.0))].into(),
                }]
            },
            ..Default::default()
        };
        let (bytes, _) = encode(&[part], &[0], None, &extras, true, &MeshOptions::default());
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        assert_eq!(reader.require("PROPS").unwrap().len(), 64);
        assert_eq!(cell(&bytes, "PROPS", 0, 1), "mass");
        assert_eq!(cell(&bytes, "PROPS", 0, 3), "80.5");
        assert_eq!(cell(&bytes, "PROPS", 1, 3), "\"Hero\"");
        assert_eq!(cell(&bytes, "SHAPES", 0, 2), "hitbox");
        assert_eq!(cell(&bytes, "SHAPES", 0, 4), "capsule");
        assert_eq!(cell(&bytes, "SHAPES", 1, 8), "4");
        assert_eq!(cell(&bytes, "SHAPEPRP", 0, 1), "damage");
        assert_eq!(reader.require("SHAPEPTS").unwrap().len(), 4 * 12);
        assert_eq!(cell(&bytes, "REGIONS", 0, 2), "surface");
        assert_eq!(cell(&bytes, "REGIONS", 0, 3), "2");
        assert!(rows(&bytes).unwrap().contains(&("Regions", "2".into())));
        assert!(rows(&bytes).unwrap().contains(&("Paths", "1".into())));
        assert_eq!(cell(&bytes, "PATHS", 0, 1), "patrol");
        assert_eq!(cell(&bytes, "PATHS", 0, 2), "route");
        assert_eq!(cell(&bytes, "PATHS", 0, 4), "yes");
        assert_eq!(cell(&bytes, "PATHS", 0, 7), "4.000");
        assert_eq!(cell(&bytes, "PATHS", 0, 8), "4");
        assert_eq!(cell(&bytes, "PATHS", 0, 9), "65");
        assert_eq!(cell(&bytes, "PATHPTS", 2, 1), "1.000, 0.000, 1.000");
        // Halfway round the square: the opposite corner.
        assert_eq!(cell(&bytes, "PATHSMP", 32, 1), "1.000, 0.000, 1.000");
        assert_eq!(cell(&bytes, "PATHSMP", 32, 2), "2.000");
        assert_eq!(cell(&bytes, "PATHPRP", 0, 1), "speed");
        // Each written triangle keeps its source triangle's regions, wherever it moved.
        let masks = reader.require("TRIREGN").unwrap();
        let indices = reader.require("INDICES").unwrap();
        for t in 0..triangles {
            let corner = |k: usize| {
                u32::from(u16::from_le_bytes([
                    indices[t * 6 + k * 2],
                    indices[t * 6 + k * 2 + 1],
                ]))
            };
            let mut key = [corner(0), corner(1), corner(2)];
            key.sort_unstable();
            let source = corners
                .iter()
                .position(|c| {
                    let mut c = *c;
                    c.sort_unstable();
                    c == key
                })
                .unwrap();
            assert_eq!(chunks::u32_at(masks, t, 4, 0), Some(expected[source]));
        }
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
            &Extras {
                locators: vec![Locator {
                    name: "hand".into(),
                    tags: vec!["weapon".into(), "right".into()],
                    joint: 0,
                    translation: [1.0, 2.0, 3.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0; 3],
                }],
                ..Default::default()
            },
            false,
            &MeshOptions::default(),
        );
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        assert_eq!(reader.require("SKIN").unwrap().len(), summary.vertices * 24);
        assert_eq!(reader.require("SKELETON").unwrap().len(), 112);
        assert_eq!(reader.require("DESC").unwrap()[23], 0b110);
        assert!(rows(&bytes).unwrap().contains(&("Skin", "1 joint".into())));

        // Locators: a record and its name; see docs/spec/amesh.md#locators.
        let locators = reader.require("LOCATORS").unwrap();
        assert_eq!(locators.len(), 64);
        assert_eq!(&locators[..8], crate::hash::fnv1a64(b"hand").to_le_bytes());
        assert_eq!(chunks::u32_at(locators, 0, 64, 16), Some(0));
        assert_eq!(chunks::f32_at(locators, 0, 64, 28), Some(3.0));
        assert_eq!(reader.require("LOCNAMES").unwrap(), b"handweaponright");
        assert_eq!(&locators[60..64], [0, 0, 2, 0]);
        assert_eq!(cell(&bytes, "LOCATORS", 0, 7), "weapon, right");
        assert_eq!(cell(&bytes, "LOCTAGS", 1, 1), "right");
        assert_eq!(
            cell(&bytes, "LOCTAGS", 0, 2),
            format!("{:#018x}", crate::hash::fnv1a64(b"weapon"))
        );
        assert!(rows(&bytes).unwrap().contains(&("Locators", "1".into())));
        assert_eq!(cell(&bytes, "LOCATORS", 0, 1), "hand");
        assert_eq!(cell(&bytes, "LOCATORS", 0, 3), "0");
        assert_eq!(cell(&bytes, "LOCATORS", 0, 4), "1.000, 2.000, 3.000");
        // In the preview, placed by the joint's bind pose (identity here).
        let locators = preview(&bytes).unwrap().locators;
        assert_eq!(locators.len(), 1);
        assert_eq!(locators[0].0, "hand");
        assert_eq!(locators[0].1.translation.to_array(), [1.0, 2.0, 3.0]);
    }
}
