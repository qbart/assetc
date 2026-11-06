//! Shader interfaces of custom materials: `.ashader` files, their params' GPU block, the
//! resources their SPIR-V stages declare, and `.ashader.bin`. See
//! docs/features/custom-materials.md, docs/spec/ashader.md and docs/spec/ashader.bin.md.

use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::amesh::Strings;
use crate::aschema::{Body, Schema, Ty, parse_type};
use crate::chunks::{Bytes, Writer};
use crate::hash::fnv1a64;

pub const FORMAT: &str = "ASHADER";
pub const VERSION: u32 = 1;
/// The largest GPU block; see docs/features/custom-materials.md#params.
pub const MAX_BLOCK: u32 = 16 * 1024;
pub const MAX_TEXTURES: usize = 16;

pub fn is_shader(file: &Path) -> bool {
    file.extension().is_some_and(|ext| ext == "ashader")
}

pub fn is_spirv(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("spv"))
}

/// A `.ashader` file; see docs/spec/ashader.md#schema.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Shader {
    pub doc: String,
    pub params: Option<ParamsRef>,
    pub state: State,
    pub stages: Stages,
    pub textures: Vec<Texture>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamsRef {
    /// Relative to the shader's folder.
    pub schema: String,
    #[serde(rename = "type")]
    pub ty: String,
}

/// Render state; each key unset takes its default, see docs/spec/ashader.md#state.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct State {
    pub blend: Option<Blend>,
    pub alpha_cutoff: Option<f32>,
    pub cull: Option<Cull>,
    pub depth_write: Option<bool>,
    pub depth_test: Option<DepthTest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    Opaque,
    Mask,
    Blend,
    Add,
    Multiply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cull {
    Back,
    Front,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepthTest {
    Less,
    LessEqual,
    Equal,
    Always,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Stages {
    pub fragment: Option<String>,
    pub fragment_entry: Option<String>,
    pub vertex: Option<String>,
    pub vertex_entry: Option<String>,
}

/// A texture the shader samples; see docs/spec/ashader.md#textures.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Texture {
    pub name: String,
    pub usage: Usage,
    #[serde(default)]
    pub dimension: Option<Dimension>,
    #[serde(default)]
    pub color_space: Option<ColorKey>,
    #[serde(default)]
    pub default: Option<Fallback>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub bare: bool,
    #[serde(default)]
    pub mipmaps: Option<bool>,
    #[serde(default)]
    pub pack: Option<Pack>,
    #[serde(default)]
    pub fill: Option<[u8; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Usage {
    Color,
    Data,
    Normal,
    Height,
    Noise,
    Gradient,
    Palette,
    Lut,
    Environment,
    Atlas,
}

impl Usage {
    pub const ALL: [Self; 10] = [
        Self::Color,
        Self::Data,
        Self::Normal,
        Self::Height,
        Self::Noise,
        Self::Gradient,
        Self::Palette,
        Self::Lut,
        Self::Environment,
        Self::Atlas,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Data => "data",
            Self::Normal => "normal",
            Self::Height => "height",
            Self::Noise => "noise",
            Self::Gradient => "gradient",
            Self::Palette => "palette",
            Self::Lut => "lut",
            Self::Environment => "environment",
            Self::Atlas => "atlas",
        }
    }

    /// Filled by an image the material's build compiles (else by another asset's texture).
    pub fn takes_images(self) -> bool {
        matches!(self, Self::Color | Self::Data | Self::Normal | Self::Height)
    }

    /// Whether `path` (a file or folder of the input) can fill a texture of this usage; see
    /// docs/features/custom-materials.md#textures.
    pub fn accepts(self, path: &Path) -> bool {
        let ext = |e: &str| path.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
        match self {
            Self::Color | Self::Data | Self::Normal | Self::Height => {
                ["png", "jpg", "jpeg", "tga"].into_iter().any(ext)
            }
            Self::Noise => ext("anoise"),
            Self::Gradient => ext("agradient"),
            Self::Palette => ext("apalette"),
            Self::Lut => ext("cube"),
            Self::Environment => ext("exr"),
            Self::Atlas => crate::atlas::is_atlas(path),
        }
    }

    /// The KTX2 usage key of an image compiled for it.
    pub fn ktx(self) -> crate::ktx2::Usage {
        match self {
            Self::Color => crate::ktx2::Usage::Color,
            Self::Normal => crate::ktx2::Usage::Normal,
            Self::Height => crate::ktx2::Usage::Height,
            _ => crate::ktx2::Usage::Data,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Dimension {
    #[serde(rename = "2d")]
    D2,
    #[serde(rename = "3d")]
    D3,
    #[serde(rename = "cube")]
    Cube,
}

impl Dimension {
    pub fn key(self) -> &'static str {
        match self {
            Self::D2 => "2d",
            Self::D3 => "3d",
            Self::Cube => "cube",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorKey {
    Srgb,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    White,
    Black,
    Gray,
    Normal,
    Transparent,
}

impl Fallback {
    pub fn key(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
            Self::Gray => "gray",
            Self::Normal => "normal",
            Self::Transparent => "transparent",
        }
    }

    /// The fallback's RGBA8 texel.
    pub fn texel(self) -> [u8; 4] {
        match self {
            Self::White => [255, 255, 255, 255],
            Self::Black => [0, 0, 0, 255],
            Self::Gray => [128, 128, 128, 255],
            Self::Normal => [128, 128, 255, 255],
            Self::Transparent => [0, 0, 0, 0],
        }
    }
}

/// A packed texture's channels: each an input name, with the input's channel after a `.`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Pack {
    pub r: Option<String>,
    pub g: Option<String>,
    pub b: Option<String>,
    pub a: Option<String>,
}

impl Pack {
    pub fn channels(&self) -> [Option<&str>; 4] {
        [&self.r, &self.g, &self.b, &self.a].map(|c| c.as_deref())
    }
}

/// A packed channel's input and the input's channel (`0`–`3`).
pub fn pack_input(text: &str) -> Result<(&str, u8), String> {
    let (name, channel) = match text.split_once('.') {
        Some((name, c)) => (
            name,
            match c {
                "r" => 0,
                "g" => 1,
                "b" => 2,
                "a" => 3,
                _ => return Err(format!("pack {text:?}: the channel must be r, g, b or a")),
            },
        ),
        None => (text, 0),
    };
    if !is_name(name) {
        return Err(format!("pack {text:?}: {name:?} is not a valid input name"));
    }
    Ok((name, channel))
}

/// A texture or input name: `[a-z][a-z0-9_]*`, at most 32 bytes.
pub fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Texture {
    pub fn new(name: &str, usage: Usage) -> Self {
        Self {
            name: name.to_owned(),
            usage,
            dimension: None,
            color_space: None,
            default: None,
            required: false,
            bare: false,
            mipmaps: None,
            pack: None,
            fill: None,
        }
    }

    pub fn dimension(&self) -> Dimension {
        self.dimension.unwrap_or(match self.usage {
            Usage::Environment => Dimension::Cube,
            _ => Dimension::D2,
        })
    }

    /// Whether an image compiled for it is sRGB.
    pub fn srgb(&self) -> bool {
        match self.color_space {
            Some(c) => c == ColorKey::Srgb,
            None => self.usage == Usage::Color && self.pack.is_none(),
        }
    }

    pub fn fallback(&self) -> Fallback {
        self.default.unwrap_or(match self.usage {
            Usage::Normal => Fallback::Normal,
            _ => Fallback::White,
        })
    }

    pub fn mipmaps(&self) -> bool {
        self.mipmaps.unwrap_or(true)
    }

    pub fn fill(&self) -> [u8; 4] {
        self.fill.unwrap_or([255; 4])
    }
}

impl Blend {
    pub const ALL: [Self; 5] = [
        Self::Opaque,
        Self::Mask,
        Self::Blend,
        Self::Add,
        Self::Multiply,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Mask => "mask",
            Self::Blend => "blend",
            Self::Add => "add",
            Self::Multiply => "multiply",
        }
    }
}

impl Cull {
    pub const ALL: [Self; 3] = [Self::Back, Self::Front, Self::None];

    pub fn key(self) -> &'static str {
        match self {
            Self::Back => "back",
            Self::Front => "front",
            Self::None => "none",
        }
    }
}

impl DepthTest {
    pub const ALL: [Self; 4] = [Self::Less, Self::LessEqual, Self::Equal, Self::Always];

    pub fn key(self) -> &'static str {
        match self {
            Self::Less => "less",
            Self::LessEqual => "less_equal",
            Self::Equal => "equal",
            Self::Always => "always",
        }
    }
}

impl State {
    /// `over`'s keys where it sets them, else these.
    pub fn with(self, over: State) -> State {
        State {
            blend: over.blend.or(self.blend),
            alpha_cutoff: over.alpha_cutoff.or(self.alpha_cutoff),
            cull: over.cull.or(self.cull),
            depth_write: over.depth_write.or(self.depth_write),
            depth_test: over.depth_test.or(self.depth_test),
        }
    }

    pub fn blend(&self) -> Blend {
        self.blend.unwrap_or(Blend::Opaque)
    }

    pub fn alpha_cutoff(&self) -> f32 {
        self.alpha_cutoff.unwrap_or(0.5)
    }

    pub fn cull(&self) -> Cull {
        self.cull.unwrap_or(Cull::Back)
    }

    pub fn depth_write(&self) -> bool {
        self.depth_write.unwrap_or(!matches!(
            self.blend(),
            Blend::Blend | Blend::Add | Blend::Multiply
        ))
    }

    pub fn depth_test(&self) -> DepthTest {
        self.depth_test.unwrap_or(DepthTest::LessEqual)
    }

    /// The state bits of docs/spec/ashader.bin.md#state-bits.
    pub fn bits(&self) -> u32 {
        let blend = Blend::ALL.iter().position(|b| *b == self.blend()).unwrap() as u32;
        let cull = Cull::ALL.iter().position(|c| *c == self.cull()).unwrap() as u32;
        let depth = DepthTest::ALL
            .iter()
            .position(|d| *d == self.depth_test())
            .unwrap() as u32;
        blend | cull << 3 | u32::from(self.depth_write()) << 5 | depth << 6
    }

    /// Range problems of the keys set.
    pub fn check(&self) -> Result<(), String> {
        match self.alpha_cutoff {
            Some(c) if !(0.0..=1.0).contains(&c) => {
                Err(format!("alpha_cutoff {c} is not between 0 and 1"))
            }
            _ => Ok(()),
        }
    }

    /// The keys set, as `key = value` TOML lines in spec order.
    pub fn toml_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(b) = self.blend {
            out.push(format!("blend = \"{}\"", b.key()));
        }
        if let Some(c) = self.alpha_cutoff {
            out.push(format!("alpha_cutoff = {}", crate::material::float(c)));
        }
        if let Some(c) = self.cull {
            out.push(format!("cull = \"{}\"", c.key()));
        }
        if let Some(d) = self.depth_write {
            out.push(format!("depth_write = {d}"));
        }
        if let Some(d) = self.depth_test {
            out.push(format!("depth_test = \"{}\"", d.key()));
        }
        out
    }
}

impl Shader {
    pub fn parse(text: &str) -> Result<Self, String> {
        let shader: Shader = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        shader.check()?;
        Ok(shader)
    }

    /// The rules of docs/spec/ashader.md that serde does not check.
    fn check(&self) -> Result<(), String> {
        self.state.check()?;
        if self.textures.len() > MAX_TEXTURES {
            return Err(format!(
                "{} textures; at most {MAX_TEXTURES}",
                self.textures.len()
            ));
        }
        let mut names: Vec<&str> = Vec::new();
        for t in &self.textures {
            let at = |m: String| format!("texture {}: {m}", t.name);
            if !is_name(&t.name) {
                return Err(format!("texture name {:?} is not [a-z][a-z0-9_]*", t.name));
            }
            if names.contains(&t.name.as_str()) {
                return Err(format!("{} is named twice", t.name));
            }
            names.push(&t.name);
            let dim = t.dimension();
            let fits = match t.usage {
                Usage::Noise | Usage::Gradient | Usage::Palette | Usage::Atlas => {
                    dim == Dimension::D2
                }
                Usage::Environment => dim == Dimension::Cube,
                Usage::Lut => dim != Dimension::Cube,
                // Images are 2D; packs are checked below.
                _ => dim == Dimension::D2,
            };
            if !fits {
                return Err(at(format!(
                    "a {} texture cannot be {}",
                    t.usage.key(),
                    dim.key()
                )));
            }
            if t.color_space.is_some() && !t.usage.takes_images() {
                return Err(at(format!(
                    "color_space is only for images; a {} keeps its own",
                    t.usage.key()
                )));
            }
            if t.bare && t.pack.is_some() {
                return Err(at("a packed texture cannot be bare".into()));
            }
            if t.fill.is_some() && t.pack.is_none() {
                return Err(at("fill is only for packed textures".into()));
            }
            if let Some(pack) = &t.pack {
                if t.usage != Usage::Data || t.srgb() {
                    return Err(at("a packed texture must be linear data".into()));
                }
                if pack.channels().iter().all(Option::is_none) {
                    return Err(at("pack names no input".into()));
                }
            }
        }
        if self.textures.iter().filter(|t| t.bare).count() > 1 {
            return Err("only one texture can be bare".into());
        }
        for input in self.inputs() {
            let (name, _) = pack_input(input)?;
            if self.textures.iter().any(|t| t.name == name) {
                return Err(format!("input {name} is named like a texture"));
            }
        }
        Ok(())
    }

    /// Every pack channel's text, in texture and channel order.
    fn inputs(&self) -> impl Iterator<Item = &str> {
        self.textures
            .iter()
            .filter_map(|t| t.pack.as_ref())
            .flat_map(|p| p.channels().into_iter().flatten())
    }

    /// Input names, each once, in order of first use.
    pub fn input_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for text in self.inputs() {
            if let Ok((name, _)) = pack_input(text)
                && !out.iter().any(|n| n == name)
            {
                out.push(name.to_owned());
            }
        }
        out
    }

    /// The names a material's `[textures]` and by-name files fill: unpacked textures and
    /// inputs, in order. With the usage that decides which files they take.
    pub fn fillable(&self) -> Vec<(String, Usage)> {
        let mut out = Vec::new();
        for t in &self.textures {
            match &t.pack {
                None => out.push((t.name.clone(), t.usage)),
                Some(pack) => {
                    for text in pack.channels().into_iter().flatten() {
                        if let Ok((name, _)) = pack_input(text)
                            && !out.iter().any(|(n, _)| n == name)
                        {
                            out.push((name.to_owned(), Usage::Data));
                        }
                    }
                }
            }
        }
        out
    }

    /// The file, rewritten as docs/spec/ashader.md#written-by-assetc says.
    pub fn to_toml(&self) -> String {
        let q = |s: &str| toml::Value::String(s.to_owned()).to_string();
        let mut head = Vec::new();
        if !self.doc.is_empty() {
            head.push(format!("doc = {}", q(&self.doc)));
        }
        if let Some(p) = &self.params {
            head.push(format!(
                "params = {{ schema = {}, type = {} }}",
                q(&p.schema),
                q(&p.ty)
            ));
        }
        let mut tables: Vec<String> = Vec::new();
        if !head.is_empty() {
            tables.push(head.join("\n"));
        }
        let state = self.state.toml_lines();
        if !state.is_empty() {
            tables.push(format!("[state]\n{}", state.join("\n")));
        }
        let s = &self.stages;
        let stages: Vec<String> = [
            ("fragment", &s.fragment),
            ("fragment_entry", &s.fragment_entry),
            ("vertex", &s.vertex),
            ("vertex_entry", &s.vertex_entry),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k} = {}", q(v))))
        .collect();
        if !stages.is_empty() {
            tables.push(format!("[stages]\n{}", stages.join("\n")));
        }
        for t in &self.textures {
            let mut lines = vec![
                "[[textures]]".to_owned(),
                format!("name = {}", q(&t.name)),
                format!("usage = \"{}\"", t.usage.key()),
            ];
            if let Some(d) = t.dimension {
                lines.push(format!("dimension = \"{}\"", d.key()));
            }
            if let Some(c) = t.color_space {
                let key = if c == ColorKey::Srgb {
                    "srgb"
                } else {
                    "linear"
                };
                lines.push(format!("color_space = \"{key}\""));
            }
            if let Some(f) = t.default {
                lines.push(format!("default = \"{}\"", f.key()));
            }
            if t.required {
                lines.push("required = true".into());
            }
            if t.bare {
                lines.push("bare = true".into());
            }
            if let Some(m) = t.mipmaps {
                lines.push(format!("mipmaps = {m}"));
            }
            if let Some(p) = &t.pack {
                let parts: Vec<String> = ["r", "g", "b", "a"]
                    .into_iter()
                    .zip(p.channels())
                    .filter_map(|(k, v)| v.map(|v| format!("{k} = {}", q(v))))
                    .collect();
                lines.push(format!("pack = {{ {} }}", parts.join(", ")));
            }
            if let Some(f) = t.fill {
                lines.push(format!("fill = [{}, {}, {}, {}]", f[0], f[1], f[2], f[3]));
            }
            tables.push(lines.join("\n"));
        }
        let mut out = tables.join("\n\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

// The GPU block; see docs/features/custom-materials.md#params.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuType {
    U32,
    I32,
    F32,
    Vec3,
    Vec4,
}

impl GpuType {
    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn glsl(self) -> &'static str {
        match self {
            Self::U32 => "uint",
            Self::I32 => "int",
            Self::F32 => "float",
            Self::Vec3 => "vec3",
            Self::Vec4 => "vec4",
        }
    }

    pub fn wgsl(self) -> &'static str {
        match self {
            Self::U32 => "u32",
            Self::I32 => "i32",
            Self::F32 => "f32",
            Self::Vec3 => "vec3<f32>",
            Self::Vec4 => "vec4<f32>",
        }
    }

    fn size(self) -> u32 {
        match self {
            Self::Vec3 => 12,
            Self::Vec4 => 16,
            _ => 4,
        }
    }

    fn align(self) -> u32 {
        match self {
            Self::Vec3 | Self::Vec4 => 16,
            _ => 4,
        }
    }
}

/// The schema type a GPU value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Bool,
    Integer,
    Float,
    Enum,
    Rgb,
    Rgba,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpuField {
    /// Nested struct fields joined with `.`: `foam.amount`.
    pub path: String,
    pub offset: u32,
    pub ty: GpuType,
    pub source: Source,
    /// An enum's values, for its index.
    pub values: Vec<String>,
}

/// A nested struct of the block, for declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuStruct {
    pub path: String,
    pub ty: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    /// Bytes, a multiple of 16; `0` without GPU fields.
    pub size: u32,
    pub fields: Vec<GpuField>,
    pub structs: Vec<GpuStruct>,
    /// Paths of the fields left out: CPU only.
    pub cpu_only: Vec<String>,
}

fn round_up(v: u32, to: u32) -> u32 {
    v.div_ceil(to) * to
}

/// The std140 GPU block of struct `si`; see docs/features/custom-materials.md#params.
pub fn layout(schema: &Schema, si: usize) -> Result<Block, String> {
    let mut block = Block::default();
    let size = layout_struct(schema, si, 0, "", &mut block, 0)?;
    block.size = if block.fields.is_empty() {
        0
    } else {
        round_up(size, 16)
    };
    if block.size > MAX_BLOCK {
        return Err(format!(
            "the params' GPU block is {} bytes; at most {MAX_BLOCK}",
            block.size
        ));
    }
    Ok(block)
}

/// Lays struct `si` out from `base`; returns where its last GPU value ends.
fn layout_struct(
    schema: &Schema,
    si: usize,
    base: u32,
    prefix: &str,
    block: &mut Block,
    depth: usize,
) -> Result<u32, String> {
    if depth > 16 {
        return Err(format!("{} nests too deeply", schema.types[si].name));
    }
    let Body::Struct { fields, .. } = &schema.types[si].body else {
        return Err(format!("{} is not a struct", schema.types[si].name));
    };
    let mut at = base;
    for f in fields {
        let path = if prefix.is_empty() {
            f.name.clone()
        } else {
            format!("{prefix}.{}", f.name)
        };
        let ty = parse_type(&f.ty).map_err(|e| format!("{path}: {e}"))?;
        let scalar = match &ty {
            Ty::Prim("bool") => Some((GpuType::U32, Source::Bool)),
            Ty::Prim("i8" | "i16" | "i32") => Some((GpuType::I32, Source::Integer)),
            Ty::Prim("u8" | "u16" | "u32") => Some((GpuType::U32, Source::Integer)),
            Ty::Prim("f32" | "f64") => Some((GpuType::F32, Source::Float)),
            Ty::Prim("rgb") => Some((GpuType::Vec3, Source::Rgb)),
            Ty::Prim("rgba") => Some((GpuType::Vec4, Source::Rgba)),
            _ => None,
        };
        if let Some((gpu, source)) = scalar {
            at = round_up(at, gpu.align());
            block.fields.push(GpuField {
                path,
                offset: at,
                ty: gpu,
                source,
                values: Vec::new(),
            });
            at += gpu.size();
            continue;
        }
        let named = match &ty {
            Ty::Named(n) => schema.type_index(n),
            _ => None,
        };
        match named.map(|i| (i, &schema.types[i].body)) {
            Some((_, Body::Enum { values })) => {
                at = round_up(at, 4);
                block.fields.push(GpuField {
                    path,
                    offset: at,
                    ty: GpuType::U32,
                    source: Source::Enum,
                    values: values.clone(),
                });
                at += 4;
            }
            Some((inner, Body::Struct { .. })) => {
                // A nested struct starts and ends on 16 bytes; one without GPU values is left out.
                let start = round_up(at, 16);
                let before = block.fields.len();
                let end = layout_struct(schema, inner, start, &path, block, depth + 1)?;
                if block.fields.len() == before {
                    block.cpu_only.push(path);
                } else {
                    let size = round_up(end - start, 16);
                    block.structs.push(GpuStruct {
                        path,
                        ty: schema.types[inner].name.clone(),
                        offset: start,
                        size,
                    });
                    at = start + size;
                }
            }
            None => block.cpu_only.push(path),
        }
    }
    Ok(at)
}

/// The block's bytes for `values` (the params struct's, resolved).
pub fn pack(block: &Block, values: &Map<String, Value>) -> Vec<u8> {
    let mut out = vec![0u8; block.size as usize];
    for f in &block.fields {
        let mut v = Some(values);
        let mut value = None;
        let parts: Vec<&str> = f.path.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            let here = v.and_then(|m| m.get(*part));
            if i + 1 == parts.len() {
                value = here;
            } else {
                v = here.and_then(Value::as_object);
            }
        }
        let bytes = gpu_bytes(f, value.unwrap_or(&Value::Null));
        let at = f.offset as usize;
        out[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    out
}

/// A value's bytes in the block; `null` and anything unexpected as the type's implicit default.
fn gpu_bytes(f: &GpuField, v: &Value) -> Vec<u8> {
    match f.source {
        Source::Bool => u32::from(v.as_bool().unwrap_or(false))
            .to_le_bytes()
            .to_vec(),
        Source::Integer if f.ty == GpuType::I32 => {
            (v.as_i64().unwrap_or(0) as i32).to_le_bytes().to_vec()
        }
        Source::Integer => (v.as_u64().unwrap_or(0) as u32).to_le_bytes().to_vec(),
        Source::Float => (v.as_f64().unwrap_or(0.0) as f32).to_le_bytes().to_vec(),
        Source::Enum => {
            let index = v
                .as_str()
                .and_then(|s| f.values.iter().position(|e| e == s))
                .unwrap_or(0) as u32;
            index.to_le_bytes().to_vec()
        }
        Source::Rgb | Source::Rgba => {
            let rgba = v.as_str().and_then(hex_color).unwrap_or([0, 0, 0, 255]);
            let lin = |c: u8| crate::texture::srgb_to_linear(c as f32 / 255.0);
            let mut out = Vec::new();
            for c in &rgba[..3] {
                out.extend_from_slice(&lin(*c).to_le_bytes());
            }
            if f.source == Source::Rgba {
                out.extend_from_slice(&(rgba[3] as f32 / 255.0).to_le_bytes());
            }
            out
        }
    }
}

/// `#rrggbb` or `#rrggbbaa` (alpha 255 without).
pub fn hex_color(text: &str) -> Option<[u8; 4]> {
    let hex = text.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if hex.len() == 8 { byte(6)? } else { 255 },
    ])
}

// The interface in SPIR-V; see docs/features/custom-materials.md#the-interface-in-spir-v.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Params,
    Frame,
    Object,
    Texture,
    Sampler,
}

impl ResourceKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Params => "params",
            Self::Frame => "frame",
            Self::Object => "object",
            Self::Texture => "texture",
            Self::Sampler => "sampler",
        }
    }
}

/// A resource of the interface that a stage declares.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub kind: ResourceKind,
    /// The texture's index, for textures and samplers.
    pub texture: usize,
    pub set: u32,
    pub binding: u32,
    /// Bits: `1` vertex, `2` fragment.
    pub stages: u32,
    /// A block's size as declared.
    pub size: u32,
    /// The name it was found by.
    pub name: String,
}

pub const VERTEX: u32 = 1;
pub const FRAGMENT: u32 = 2;

/// `Frame`'s members and their types; see docs/features/custom-materials.md.
pub const FRAME: [(&str, &str); 7] = [
    ("view_proj", "mat4"),
    ("view", "mat4"),
    ("camera_position", "vec4"),
    ("light_direction", "vec4"),
    ("light_color", "vec4"),
    ("time", "vec4"),
    ("viewport", "vec4"),
];
pub const OBJECT: [(&str, &str); 2] = [("model", "mat4"), ("normal", "mat4")];

/// A stage's SPIR-V, parsed.
pub struct Stage {
    pub bit: u32,
    pub file: String,
    pub module: naga::Module,
}

/// Parses `bytes` as SPIR-V holding entry point `entry` of the stage `bit`.
pub fn parse_stage(bytes: &[u8], bit: u32, entry: &str, file: &str) -> Result<Stage, String> {
    if bytes.len() < 4 || bytes[..4] != 0x0723_0203u32.to_le_bytes() {
        return Err(format!("{file}: not SPIR-V"));
    }
    let options = naga::front::spv::Options {
        adjust_coordinate_space: false,
        ..Default::default()
    };
    let module =
        naga::front::spv::parse_u8_slice(bytes, &options).map_err(|e| format!("{file}: {e}"))?;
    let stage = if bit == VERTEX {
        naga::ShaderStage::Vertex
    } else {
        naga::ShaderStage::Fragment
    };
    if !module
        .entry_points
        .iter()
        .any(|e| e.name == entry && e.stage == stage)
    {
        let kind = if bit == VERTEX { "vertex" } else { "fragment" };
        return Err(format!("{file}: has no {kind} entry point {entry:?}"));
    }
    Ok(Stage {
        bit,
        file: file.to_owned(),
        module,
    })
}

/// A global's name: the variable's, else its type's.
fn names(module: &naga::Module, var: &naga::GlobalVariable) -> Vec<String> {
    var.name
        .iter()
        .chain(module.types[var.ty].name.iter())
        .filter(|n| !n.is_empty())
        .cloned()
        .collect()
}

/// A naga type in GLSL words, for the checks: `float`, `vec4`, `mat4`, …
fn glsl_type(module: &naga::Module, ty: naga::Handle<naga::Type>) -> String {
    use naga::{ScalarKind, TypeInner, VectorSize};
    let scalar = |s: naga::Scalar| match (s.kind, s.width) {
        (ScalarKind::Float, 4) => "float",
        (ScalarKind::Sint, 4) => "int",
        (ScalarKind::Uint, 4) => "uint",
        (ScalarKind::Bool, _) => "bool",
        _ => "other",
    };
    let n = |v: VectorSize| v as u8;
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => scalar(*s).to_owned(),
        TypeInner::Vector { size, scalar: s } if s.kind == ScalarKind::Float && s.width == 4 => {
            format!("vec{}", n(*size))
        }
        TypeInner::Matrix {
            columns,
            rows,
            scalar: s,
        } if s.width == 4 && columns == rows => format!("mat{}", n(*columns)),
        TypeInner::Struct { .. } => "struct".into(),
        _ => "other".into(),
    }
}

/// A block's members by path (nested structs flattened) with offset and GLSL type.
fn members(
    module: &naga::Module,
    ty: naga::Handle<naga::Type>,
    base: u32,
    prefix: &str,
    out: &mut Vec<(String, u32, String)>,
) {
    let naga::TypeInner::Struct { members: list, .. } = &module.types[ty].inner else {
        return;
    };
    for m in list {
        let name = m.name.clone().unwrap_or_default();
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}.{name}")
        };
        if matches!(module.types[m.ty].inner, naga::TypeInner::Struct { .. }) {
            members(module, m.ty, base + m.offset, &path, out);
        } else {
            out.push((path, base + m.offset, glsl_type(module, m.ty)));
        }
    }
}

fn block_span(module: &naga::Module, ty: naga::Handle<naga::Type>) -> u32 {
    match &module.types[ty].inner {
        naga::TypeInner::Struct { span, .. } => *span,
        _ => 0,
    }
}

/// The interface's resources the stages declare, checked against the shader and its block;
/// or every problem. See docs/features/custom-materials.md#the-interface-in-spir-v.
pub fn reflect(
    shader: &Shader,
    block: &Block,
    stages: &[Stage],
) -> Result<Vec<Resource>, Vec<String>> {
    let mut found: Vec<Resource> = Vec::new();
    let mut problems = Vec::new();
    for stage in stages {
        let module = &stage.module;
        let file = &stage.file;
        for (_, var) in module.global_variables.iter() {
            if !matches!(
                var.space,
                naga::AddressSpace::Uniform
                    | naga::AddressSpace::Handle
                    | naga::AddressSpace::Storage { .. }
                    | naga::AddressSpace::Immediate
            ) {
                continue;
            }
            let names = names(module, var);
            if names.is_empty() {
                problems.push(format!(
                    "{file}: has no names; compile without stripping debug info"
                ));
                continue;
            }
            let shown = names[0].clone();
            let is = |a: &str, b: &str| names.iter().any(|n| n == a || n == b);
            let inner = &module.types[var.ty].inner;
            let (kind, texture, name) = match (&var.space, inner) {
                (naga::AddressSpace::Uniform, _) if is("Params", "params") => {
                    let mut list = Vec::new();
                    members(module, var.ty, 0, "", &mut list);
                    for (path, offset, ty) in list {
                        match block.fields.iter().find(|f| f.path == path) {
                            None => problems.push(format!(
                                "{file}: Params.{path} is not a GPU field of the params"
                            )),
                            Some(f) if f.offset != offset || f.ty.glsl() != ty => {
                                problems.push(format!(
                                    "{file}: Params.{path} is {ty} at offset {offset}; the block has {} at {}",
                                    f.ty.glsl(),
                                    f.offset
                                ))
                            }
                            Some(_) => {}
                        }
                    }
                    (ResourceKind::Params, 0, "Params".to_owned())
                }
                (naga::AddressSpace::Uniform, _)
                    if is("Frame", "frame") || is("Object", "object") =>
                {
                    let (kind, known, label) = if is("Frame", "frame") {
                        (ResourceKind::Frame, &FRAME[..], "Frame")
                    } else {
                        (ResourceKind::Object, &OBJECT[..], "Object")
                    };
                    let mut list = Vec::new();
                    members(module, var.ty, 0, "", &mut list);
                    for (path, _, ty) in list {
                        match known.iter().find(|(n, _)| *n == path) {
                            None => problems
                                .push(format!("{file}: {label}.{path} is not a member of {label}")),
                            Some((_, want)) if *want != ty => problems
                                .push(format!("{file}: {label}.{path} is {ty}; it must be {want}")),
                            Some(_) => {}
                        }
                    }
                    (kind, 0, label.to_owned())
                }
                (naga::AddressSpace::Handle, naga::TypeInner::Image { dim, arrayed, .. }) => {
                    match shader.textures.iter().position(|t| names.contains(&t.name)) {
                        Some(i) => {
                            let t = &shader.textures[i];
                            let declared = match (dim, arrayed) {
                                (naga::ImageDimension::D2, false) => Some(Dimension::D2),
                                (naga::ImageDimension::D3, false) => Some(Dimension::D3),
                                (naga::ImageDimension::Cube, false) => Some(Dimension::Cube),
                                _ => None,
                            };
                            if declared != Some(t.dimension()) {
                                problems.push(format!(
                                    "{file}: texture {} must be {}",
                                    t.name,
                                    t.dimension().key()
                                ));
                            }
                            (ResourceKind::Texture, i, t.name.clone())
                        }
                        None => {
                            problems.push(format!("{file}: {shown} is not part of the interface"));
                            continue;
                        }
                    }
                }
                (naga::AddressSpace::Handle, naga::TypeInner::Sampler { .. }) => {
                    let texture = shader
                        .textures
                        .iter()
                        .position(|t| names.iter().any(|n| *n == format!("{}_sampler", t.name)));
                    match texture {
                        Some(i) => (
                            ResourceKind::Sampler,
                            i,
                            format!("{}_sampler", shader.textures[i].name),
                        ),
                        None => {
                            problems.push(format!("{file}: {shown} is not part of the interface"));
                            continue;
                        }
                    }
                }
                _ => {
                    problems.push(format!("{file}: {shown} is not part of the interface"));
                    continue;
                }
            };
            let Some(at) = &var.binding else {
                problems.push(format!("{file}: {name} has no binding"));
                continue;
            };
            let size = if matches!(
                kind,
                ResourceKind::Params | ResourceKind::Frame | ResourceKind::Object
            ) {
                block_span(module, var.ty)
            } else {
                0
            };
            match found.iter_mut().find(|r| r.kind == kind && r.texture == texture) {
                Some(r) if (r.set, r.binding) != (at.group, at.binding) => problems.push(format!(
                    "{file}: {name} is at set {} binding {}, another stage has it at set {} binding {}",
                    at.group, at.binding, r.set, r.binding
                )),
                Some(r) => {
                    r.stages |= stage.bit;
                    r.size = r.size.max(size);
                }
                None => found.push(Resource {
                    kind,
                    texture,
                    set: at.group,
                    binding: at.binding,
                    stages: stage.bit,
                    size,
                    name,
                }),
            }
        }
    }
    for (i, a) in found.iter().enumerate() {
        if let Some(b) = found[..i]
            .iter()
            .find(|b| (b.set, b.binding) == (a.set, a.binding))
        {
            problems.push(format!(
                "{} and {} are both at set {} binding {}",
                b.name, a.name, a.set, a.binding
            ));
        }
    }
    problems.dedup();
    if !problems.is_empty() {
        return Err(problems);
    }
    let order = |r: &Resource| {
        let k = match r.kind {
            ResourceKind::Params => 0,
            ResourceKind::Frame => 1,
            ResourceKind::Object => 2,
            ResourceKind::Texture => 3,
            ResourceKind::Sampler => 4,
        };
        if k >= 3 { (3, r.texture, k) } else { (k, 0, 0) }
    };
    found.sort_by_key(order);
    Ok(found)
}

// Loading for the build and the GUI.

/// A shader read with everything it names: its params' struct and block, and its stages
/// checked against it.
pub struct Loaded {
    pub shader: Shader,
    /// The params struct's schema (input-relative), its name, the schema and the struct's
    /// index.
    pub params: Option<(String, String, Schema, usize)>,
    pub block: Block,
    pub resources: Vec<Resource>,
    pub vertex: Option<Vec<u8>>,
    pub fragment: Option<Vec<u8>>,
}

/// Reads the shader at `rel` (input-relative) and what it names; the first problem fails it,
/// with how many more there are.
pub fn load(input: &Path, rel: &str) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(input.join(rel)).map_err(|e| e.to_string())?;
    let shader = Shader::parse(&text)?;
    load_parsed(input, rel, shader)
}

/// [`load`] of a shader already parsed (the one being edited).
pub fn load_parsed(input: &Path, rel: &str, shader: Shader) -> Result<Loaded, String> {
    let (dir, _) = crate::plan::split_dir(rel);
    let inside = |path: &str| {
        crate::plan::resolve(dir, path).ok_or_else(|| format!("{path} leaves the input"))
    };
    let params = match &shader.params {
        None => None,
        Some(p) => {
            let schema_rel = inside(&p.schema)?;
            let text = std::fs::read_to_string(input.join(&schema_rel))
                .map_err(|e| format!("params schema {}: {e}", p.schema))?;
            let schema =
                Schema::parse(&text).map_err(|e| format!("params schema {}: {e}", p.schema))?;
            let si = crate::adata::struct_index(&schema, &p.ty)
                .ok_or_else(|| format!("params: {} is not a struct of {}", p.ty, p.schema))?;
            Some((schema_rel, p.ty.clone(), schema, si))
        }
    };
    let block = match &params {
        Some((_, _, schema, si)) => layout(schema, *si)?,
        None => Block::default(),
    };
    let mut stages = Vec::new();
    let mut bytes = [None, None];
    for (bit, path, entry) in [
        (VERTEX, &shader.stages.vertex, &shader.stages.vertex_entry),
        (
            FRAGMENT,
            &shader.stages.fragment,
            &shader.stages.fragment_entry,
        ),
    ] {
        let Some(path) = path else {
            continue;
        };
        let file = inside(path)?;
        let data = std::fs::read(input.join(&file)).map_err(|e| format!("{path}: {e}"))?;
        let entry = entry.as_deref().unwrap_or("main");
        stages.push(parse_stage(&data, bit, entry, path)?);
        bytes[(bit == FRAGMENT) as usize] = Some(data);
    }
    let resources = reflect(&shader, &block, &stages).map_err(|problems| {
        let mut message = problems[0].clone();
        if problems.len() > 1 {
            message += &format!(" (and {} more)", problems.len() - 1);
        }
        message
    })?;
    let [vertex, fragment] = bytes;
    Ok(Loaded {
        shader,
        params,
        block,
        resources,
        vertex,
        fragment,
    })
}

impl Loaded {
    /// `.ashader.bin`.
    pub fn encode(&self) -> Vec<u8> {
        encode(&Compiled {
            shader: &self.shader,
            params: self
                .params
                .as_ref()
                .map(|(rel, ty, ..)| (rel.clone(), ty.clone())),
            block: &self.block,
            resources: &self.resources,
            vertex: self.vertex.as_deref(),
            fragment: self.fragment.as_deref(),
        })
    }

    /// The CLI detail: `shader: 64-byte params, 4 textures, fragment`.
    pub fn detail(&self) -> String {
        let params = match self.block.size {
            0 => "no GPU params".to_owned(),
            n => format!("{n}-byte params"),
        };
        let n = self.shader.textures.len();
        let textures = format!("{n} texture{}", if n == 1 { "" } else { "s" });
        let stages: Vec<&str> = [
            (self.vertex.is_some(), "vertex"),
            (self.fragment.is_some(), "fragment"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        let stages = if stages.is_empty() {
            "no stages".to_owned()
        } else {
            stages.join(", ")
        };
        format!("shader: {params}, {textures}, {stages}")
    }
}

// `.ashader.bin`; see docs/spec/ashader.bin.md.

/// A shader compiled: what `.ashader.bin` holds.
pub struct Compiled<'a> {
    pub shader: &'a Shader,
    /// The params struct's schema (input-relative) and name.
    pub params: Option<(String, String)>,
    pub block: &'a Block,
    pub resources: &'a [Resource],
    pub vertex: Option<&'a [u8]>,
    pub fragment: Option<&'a [u8]>,
}

pub fn encode(c: &Compiled) -> Vec<u8> {
    let mut strings = Strings::default();
    let mut text = |out: &mut Bytes, t: &str| {
        let (at, len) = strings.add(t);
        out.u32(at).u32(len);
    };
    let s = &c.shader;
    let mut desc = Bytes::default();
    match &c.params {
        Some((schema, ty)) => {
            desc.u64(crate::structs::schema_id(schema))
                .u64(fnv1a64(ty.as_bytes()));
            text(&mut desc, ty);
        }
        None => {
            desc.u64(0).u64(0);
            text(&mut desc, "");
        }
    }
    desc.u32(c.block.size)
        .u32(s.state.bits())
        .f32s(&[s.state.alpha_cutoff()]);
    let entry = |set: bool, e: &Option<String>| {
        if set {
            e.clone().unwrap_or_else(|| "main".into())
        } else {
            String::new()
        }
    };
    text(
        &mut desc,
        &entry(c.vertex.is_some(), &s.stages.vertex_entry),
    );
    text(
        &mut desc,
        &entry(c.fragment.is_some(), &s.stages.fragment_entry),
    );
    text(&mut desc, &s.doc);
    desc.u32(0);

    let mut fields = Bytes::default();
    for f in &c.block.fields {
        fields.u64(fnv1a64(f.path.as_bytes()));
        text(&mut fields, &f.path);
        fields
            .u32(f.offset)
            .u32(f.ty.code())
            .u32(f.source as u32)
            .u32(0);
    }

    let mut textures = Bytes::default();
    let mut inputs = Bytes::default();
    let mut input_count = 0u32;
    for t in &s.textures {
        textures.u64(fnv1a64(t.name.as_bytes()));
        text(&mut textures, &t.name);
        let usage = Usage::ALL.iter().position(|u| *u == t.usage).unwrap() as u8;
        let dim = match t.dimension() {
            Dimension::D2 => 0,
            Dimension::D3 => 1,
            Dimension::Cube => 2,
        };
        let fallback = [
            Fallback::White,
            Fallback::Black,
            Fallback::Gray,
            Fallback::Normal,
            Fallback::Transparent,
        ]
        .iter()
        .position(|f| *f == t.fallback())
        .unwrap() as u8;
        let flags = u8::from(t.srgb())
            | u8::from(t.required) << 1
            | u8::from(t.bare) << 2
            | u8::from(t.mipmaps()) << 3
            | u8::from(t.pack.is_some()) << 4;
        textures
            .u32(0)
            .u8(usage)
            .u8(dim)
            .u8(fallback)
            .u8(flags)
            .u8(0)
            .u8(0)
            .u8(0)
            .u8(0);
        match &t.pack {
            Some(pack) => {
                textures.u32(input_count).u32(4);
                let fill = t.fill();
                textures.u8(fill[0]).u8(fill[1]).u8(fill[2]).u8(fill[3]);
                for channel in pack.channels() {
                    let (name, from) = channel.and_then(|c| pack_input(c).ok()).unwrap_or(("", 0));
                    text(&mut inputs, name);
                    inputs.u32(from as u32).u32(0);
                    input_count += 1;
                }
            }
            None => {
                textures.u32(0).u32(0).u32(0);
            }
        }
        textures.u64(0);
    }

    let mut resources = Bytes::default();
    for r in c.resources {
        let kind = r.kind as u32;
        resources
            .u32(kind)
            .u32(r.texture as u32)
            .u32(r.set)
            .u32(r.binding)
            .u32(r.stages)
            .u32(r.size);
        text(&mut resources, &r.name);
    }

    let mut w = Writer::new(FORMAT, VERSION);
    w.chunk("DESC", desc.take())
        .chunk("FIELDS", fields.take())
        .chunk("TEXTURES", textures.take())
        .chunk("INPUTS", inputs.take())
        .chunk("BINDINGS", resources.take());
    if let Some(v) = c.vertex {
        w.chunk("SPIRVV", v.to_vec());
    }
    if let Some(f) = c.fragment {
        w.chunk("SPIRVF", f.to_vec());
    }
    w.chunk("STRINGS", strings.0);
    w.finish()
}

/// GLSL declarations of the interface, at sets and bindings of assetc's choosing; see
/// docs/features/custom-materials.md#shader-files.
pub fn glsl(shader: &Shader, block: &Block) -> String {
    let mut out = String::new();
    declare_structs(block, &mut out, true);
    if !block.fields.is_empty() {
        out += "layout(set = 1, binding = 0, std140) uniform Params {\n";
        for line in top_members(block, true) {
            out += &format!("    {line}\n");
        }
        out += "} params;\n";
    }
    for (i, t) in shader.textures.iter().enumerate() {
        let ty = match t.dimension() {
            Dimension::D2 => "texture2D",
            Dimension::D3 => "texture3D",
            Dimension::Cube => "textureCube",
        };
        let b = 1 + 2 * i;
        out += &format!(
            "layout(set = 1, binding = {b}) uniform {ty} {};\nlayout(set = 1, binding = {}) uniform sampler {}_sampler;\n",
            t.name,
            b + 1,
            t.name
        );
    }
    out
}

/// WGSL declarations, as [`glsl`].
pub fn wgsl(shader: &Shader, block: &Block) -> String {
    let mut out = String::new();
    declare_structs(block, &mut out, false);
    if !block.fields.is_empty() {
        out += "struct Params {\n";
        for line in top_members(block, false) {
            out += &format!("    {line}\n");
        }
        out += "}\n@group(1) @binding(0) var<uniform> params: Params;\n";
    }
    for (i, t) in shader.textures.iter().enumerate() {
        let ty = match t.dimension() {
            Dimension::D2 => "texture_2d<f32>",
            Dimension::D3 => "texture_3d<f32>",
            Dimension::Cube => "texture_cube<f32>",
        };
        let b = 1 + 2 * i;
        out += &format!(
            "@group(1) @binding({b}) var {}: {ty};\n@group(1) @binding({}) var {}_sampler: sampler;\n",
            t.name,
            b + 1,
            t.name
        );
    }
    out
}

/// The direct members of the struct at `path` (`""`: the block), as declaration lines.
fn members_of(block: &Block, path: &str, glsl: bool) -> Vec<String> {
    let depth = if path.is_empty() {
        0
    } else {
        path.split('.').count()
    };
    let under = |p: &str| {
        (path.is_empty() || p.starts_with(&format!("{path}."))) && p.split('.').count() == depth + 1
    };
    let mut items: Vec<(u32, String)> = Vec::new();
    for f in block.fields.iter().filter(|f| under(&f.path)) {
        let name = f.path.rsplit('.').next().unwrap();
        items.push((
            f.offset,
            if glsl {
                format!("{} {name};", f.ty.glsl())
            } else {
                format!("{name}: {},", f.ty.wgsl())
            },
        ));
    }
    for s in block.structs.iter().filter(|s| under(&s.path)) {
        let name = s.path.rsplit('.').next().unwrap();
        items.push((
            s.offset,
            if glsl {
                format!("{} {name};", s.ty)
            } else {
                // WGSL lays a nested struct out tighter than std140 unless told its size.
                format!("@align(16) @size({}) {name}: {},", s.size, s.ty)
            },
        ));
    }
    items.sort_by_key(|(o, _)| *o);
    items.into_iter().map(|(_, l)| l).collect()
}

fn top_members(block: &Block, glsl: bool) -> Vec<String> {
    members_of(block, "", glsl)
}

/// Nested struct types, innermost first, each once.
fn declare_structs(block: &Block, out: &mut String, glsl: bool) {
    let mut done: Vec<&str> = Vec::new();
    let mut list: Vec<&GpuStruct> = block.structs.iter().collect();
    list.sort_by_key(|s| std::cmp::Reverse(s.path.split('.').count()));
    for s in list {
        if done.contains(&s.ty.as_str()) {
            continue;
        }
        done.push(&s.ty);
        *out += &format!("struct {} {{\n", s.ty);
        for line in members_of(block, &s.path, glsl) {
            *out += &format!("    {line}\n");
        }
        *out += if glsl { "};\n" } else { "}\n" };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r##"{
      "$dialect": "a/1",
      "types": {
        "Mode": { "kind": "enum", "values": ["calm", "storm"] },
        "Foam": { "kind": "struct", "fields": [
          { "name": "amount", "type": "f32", "default": 0.5 },
          { "name": "scale", "type": "f32" }
        ] },
        "Water": { "kind": "struct", "fields": [
          { "name": "depth_color", "type": "rgb", "default": "#ffffff" },
          { "name": "wave_speed", "type": "f32", "default": 1 },
          { "name": "name", "type": "string" },
          { "name": "mode", "type": "Mode" },
          { "name": "foam", "type": "Foam" },
          { "name": "tint", "type": "rgba" },
          { "name": "big", "type": "u64" },
          { "name": "on", "type": "bool" }
        ] }
      }
    }"##;

    #[test]
    fn std140_layout_and_packing() {
        let schema = Schema::parse(SCHEMA).unwrap();
        let si = schema.type_index("Water").unwrap();
        let block = layout(&schema, si).unwrap();
        let at: Vec<(&str, u32)> = block
            .fields
            .iter()
            .map(|f| (f.path.as_str(), f.offset))
            .collect();
        // vec3 at 0 leaves room for a float at 12; the nested struct starts on 16.
        assert_eq!(
            at,
            [
                ("depth_color", 0),
                ("wave_speed", 12),
                ("mode", 16),
                ("foam.amount", 32),
                ("foam.scale", 36),
                ("tint", 48),
                ("on", 64),
            ]
        );
        assert_eq!(block.size, 80);
        assert_eq!(block.cpu_only, ["name", "big"]);
        let values = crate::adata::resolve(
            &schema,
            si,
            serde_json::json!({ "mode": "storm", "on": true })
                .as_object()
                .unwrap(),
            None,
        );
        let bytes = pack(&block, &values);
        let f32_at = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(f32_at(0), 1.0);
        assert_eq!(f32_at(12), 1.0);
        assert_eq!(u32_at(16), 1);
        assert_eq!(f32_at(32), 0.5);
        assert_eq!(f32_at(60), 1.0, "rgba's default alpha");
        assert_eq!(u32_at(64), 1);
    }

    #[test]
    fn shader_files_parse_check_and_round_trip() {
        let text = r#"doc = "Water"
params = { schema = "game.aschema", type = "Water" }

[state]
blend = "blend"

[stages]
fragment = "water.frag.spv"

[[textures]]
name = "color"
usage = "color"
bare = true

[[textures]]
name = "mask"
usage = "data"
pack = { r = "foam", g = "depth.a" }
fill = [0, 255, 0, 255]
"#;
        let shader = Shader::parse(text).unwrap();
        assert_eq!(shader.to_toml(), text);
        assert_eq!(shader.input_names(), ["foam", "depth"]);
        assert!(!shader.state.depth_write());
        let bad = |t: &str| Shader::parse(t).unwrap_err();
        assert!(bad("[[textures]]\nname = \"Color\"\nusage = \"color\"\n").contains("name"));
        assert!(
            bad("[[textures]]\nname = \"sky\"\nusage = \"environment\"\ndimension = \"2d\"\n")
                .contains("cannot be 2d")
        );
        assert!(
            bad("[[textures]]\nname = \"a\"\nusage = \"data\"\npack = { r = \"a\" }\n")
                .contains("named like a texture")
        );
        assert!(bad("shade = 1\n").contains("unknown field"));
    }

    /// SPIR-V of `wgsl`, for the tests.
    fn spirv(wgsl: &str) -> Vec<u8> {
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let words = naga::back::spv::write_vec(
            &module,
            &info,
            &naga::back::spv::Options {
                flags: naga::back::spv::WriterFlags::DEBUG,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn generated_declarations_compile_and_match() {
        let schema = Schema::parse(SCHEMA).unwrap();
        let block = layout(&schema, schema.type_index("Water").unwrap()).unwrap();
        let shader = Shader::parse("[[textures]]\nname = \"color\"\nusage = \"color\"\n").unwrap();
        let wgsl = wgsl(&shader, &block)
            + "@fragment fn main() -> @location(0) vec4<f32> {
                 return textureSample(color, color_sampler, vec2(0.5))
                   * params.foam.scale * f32(params.mode) * params.tint.a * f32(params.on);
               }";
        let stage = parse_stage(&spirv(&wgsl), FRAGMENT, "main", "gen.spv").unwrap();
        assert!(reflect(&shader, &block, &[stage]).is_ok(), "{wgsl}");
        assert!(glsl(&shader, &block).contains("    Foam foam;\n"));
    }

    #[test]
    fn stages_are_matched_by_name() {
        let schema = Schema::parse(SCHEMA).unwrap();
        let block = layout(&schema, schema.type_index("Water").unwrap()).unwrap();
        let shader = Shader::parse(
            "[[textures]]\nname = \"color\"\nusage = \"color\"\n\n[[textures]]\nname = \"sky\"\nusage = \"environment\"\n",
        )
        .unwrap();
        let good = spirv(
            "struct Params { depth_color: vec3<f32>, wave_speed: f32 }
             struct Frame { time: vec4<f32> }
             @group(3) @binding(7) var<uniform> params: Params;
             @group(0) @binding(0) var<uniform> frame: Frame;
             @group(3) @binding(2) var color: texture_2d<f32>;
             @group(3) @binding(3) var color_sampler: sampler;
             @fragment fn main() -> @location(0) vec4<f32> {
               return textureSample(color, color_sampler, vec2(0.5)) * params.wave_speed * frame.time.x;
             }",
        );
        let stage = parse_stage(&good, FRAGMENT, "main", "a.spv").unwrap();
        let found = reflect(&shader, &block, &[stage]).unwrap();
        let got: Vec<(ResourceKind, u32, u32)> =
            found.iter().map(|r| (r.kind, r.set, r.binding)).collect();
        assert_eq!(
            got,
            [
                (ResourceKind::Params, 3, 7),
                (ResourceKind::Frame, 0, 0),
                (ResourceKind::Texture, 3, 2),
                (ResourceKind::Sampler, 3, 3),
            ]
        );
        assert!(parse_stage(&good, VERTEX, "main", "a.spv").is_err());

        let bad = spirv(
            "struct Params { wave_speed: f32 }
             @group(1) @binding(0) var<uniform> params: Params;
             @group(1) @binding(1) var other: texture_2d<f32>;
             @group(1) @binding(2) var sky: texture_2d<f32>;
             @fragment fn main() -> @location(0) vec4<f32> {
               return textureLoad(other, vec2(0), 0) + textureLoad(sky, vec2(0), 0) * params.wave_speed;
             }",
        );
        let stage = parse_stage(&bad, FRAGMENT, "main", "b.spv").unwrap();
        let problems = reflect(&shader, &block, &[stage]).unwrap_err();
        assert_eq!(
            problems,
            [
                "b.spv: Params.wave_speed is float at offset 0; the block has float at 12",
                "b.spv: other is not part of the interface",
                "b.spv: texture sky must be cube",
            ]
        );
    }
}
