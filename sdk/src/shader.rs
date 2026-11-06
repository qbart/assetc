//! `.ashader.bin` shaders: a custom material's interface (params block layout, textures,
//! state, the resources its stages declare) and its SPIR-V; see docs/spec/ashader.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;

const FILE: &str = "ashader.bin";
const FIELD: usize = 32;
const TEXTURE: usize = 48;
const INPUT: usize = 16;
const BINDING: usize = 32;
const SPIRV_MAGIC: u32 = 0x0723_0203;

/// [`Binding::stages`] bit of the vertex stage.
pub const STAGE_VERTEX: u32 = 1;
/// [`Binding::stages`] bit of the fragment stage.
pub const STAGE_FRAGMENT: u32 = 2;

/// The value of `values` that code `value` stands for; an unknown code is an error.
fn code<T: Copy>(b: &Bytes<'_>, value: u32, values: &[T], what: &str) -> Result<T> {
    values
        .get(value as usize)
        .copied()
        .ok_or_else(|| b.err(format!("unknown {what} {value}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    #[default]
    Opaque,
    /// Pixels with alpha below [`State::alpha_cutoff`] are not drawn.
    Mask,
    Blend,
    Add,
    Multiply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cull {
    #[default]
    Back,
    Front,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthTest {
    Less,
    #[default]
    LessEqual,
    Equal,
    Always,
}

/// Blending, culling and depth; see docs/spec/ashader.bin.md#state-bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State {
    pub blend: Blend,
    pub cull: Cull,
    pub depth_write: bool,
    pub depth_test: DepthTest,
    /// For [`Blend::Mask`].
    pub alpha_cutoff: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            blend: Blend::Opaque,
            cull: Cull::Back,
            depth_write: true,
            depth_test: DepthTest::LessEqual,
            alpha_cutoff: 0.5,
        }
    }
}

impl State {
    /// The state bits at `bits` and the alpha cutoff at `cutoff` of `b`.
    pub(crate) fn read(b: &Bytes<'_>, bits: usize, cutoff: usize) -> Result<Self> {
        use Blend as B;
        use DepthTest as D;
        let v = b.u32(bits)?;
        if v >> 8 != 0 {
            return Err(b.err(format!("reserved state bits set in {v:#x}")));
        }
        let blend = [B::Opaque, B::Mask, B::Blend, B::Add, B::Multiply];
        let depth = [D::Less, D::LessEqual, D::Equal, D::Always];
        Ok(Self {
            blend: code(b, v & 7, &blend, "blend")?,
            cull: code(
                b,
                v >> 3 & 3,
                &[Cull::Back, Cull::Front, Cull::None],
                "cull",
            )?,
            depth_write: v & 1 << 5 != 0,
            depth_test: code(b, v >> 6 & 3, &depth, "depth test")?,
            alpha_cutoff: b.f32(cutoff)?,
        })
    }

    /// The state bits as the file stores them.
    pub fn bits(&self) -> u32 {
        self.blend as u32
            | (self.cull as u32) << 3
            | u32::from(self.depth_write) << 5
            | (self.depth_test as u32) << 6
    }
}

/// A value's type in the GPU block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    U32,
    I32,
    F32,
    /// `f32` × 3, 16-byte aligned.
    Vec3,
    /// `f32` × 4.
    Vec4,
}

impl FieldType {
    /// Bytes it takes in the block.
    pub fn size(self) -> u32 {
        match self {
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::Vec3 => 12,
            Self::Vec4 => 16,
        }
    }
}

/// The schema type a block value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSource {
    /// `u32`, `0` or `1`.
    Bool,
    Integer,
    Float,
    /// `u32`, the value's index in the enum.
    Enum,
    /// `vec3`, linear.
    Rgb,
    /// `vec4`, linear.
    Rgba,
}

/// One GPU value of the params block.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// FNV-1a-64 of `path`.
    pub path_hash: u64,
    /// Nested struct fields joined with `.`: `foam.amount`.
    pub path: String,
    /// Bytes into the block.
    pub offset: u32,
    pub ty: FieldType,
    pub source: FieldSource,
}

/// What a texture holds; [`Usage::key`] is its KTX2 `assetc.usage` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    D2,
    D3,
    Cube,
}

/// What to bind when a material leaves a texture empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    White,
    Black,
    Gray,
    /// `0.5, 0.5, 1`.
    Normal,
    Transparent,
}

/// The input filling one channel of a packed texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInput {
    pub name: String,
    /// The input's channel: `0` r, `1` g, `2` b, `3` a.
    pub channel: u8,
}

/// How a packed texture is made from its inputs' channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// For R, G, B and A; `None` where no input fills the channel.
    pub inputs: [Option<PackInput>; 4],
    /// R G B A values of the channels no input fills.
    pub fill: [u8; 4],
}

/// One texture the shader samples.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderTexture {
    /// FNV-1a-64 of `name`.
    pub name_hash: u64,
    pub name: String,
    pub usage: Usage,
    pub dimension: Dimension,
    pub default: Fallback,
    pub srgb: bool,
    /// A material without it fails to build.
    pub required: bool,
    /// The material's unsuffixed file fills it by name.
    pub bare: bool,
    pub mipmaps: bool,
    /// `Some` when packed from other inputs' channels.
    pub pack: Option<Pack>,
}

/// A resource of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// The params uniform block.
    Params,
    /// The frame uniform block.
    Frame,
    /// The object uniform block.
    Object,
    Texture,
    Sampler,
}

/// Where the stages declare one resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub kind: BindingKind,
    /// Index into [`Shader::textures`] for [`BindingKind::Texture`] and
    /// [`BindingKind::Sampler`], else `0`.
    pub texture: u32,
    /// Descriptor set (WGSL `@group`).
    pub set: u32,
    /// WGSL `@binding`.
    pub binding: u32,
    /// [`STAGE_VERTEX`] | [`STAGE_FRAGMENT`] bits.
    pub stages: u32,
    /// For uniform blocks, their size in bytes as the stage declares it; else `0`.
    pub size: u32,
    /// The name it was found by (`Params` or `params`, a texture's name, `<name>_sampler`).
    pub name: String,
}

impl Binding {
    pub fn in_vertex(&self) -> bool {
        self.stages & STAGE_VERTEX != 0
    }

    pub fn in_fragment(&self) -> bool {
        self.stages & STAGE_FRAGMENT != 0
    }
}

/// A compiled shader interface.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shader {
    /// Asset ID of the params schema's `.aschema.bin`; `0` without params.
    pub params_schema: u64,
    /// FNV-1a-64 of `params_type`; `0` without params.
    pub params_type_hash: u64,
    /// The params struct's name; `""` without params.
    pub params_type: String,
    /// The GPU block's size, bytes, a multiple of 16; `0` without a block.
    pub block_size: u32,
    pub state: State,
    /// Entry point of `vertex`; `""` without it.
    pub vertex_entry: String,
    /// Entry point of `fragment`; `""` without it.
    pub fragment_entry: String,
    pub doc: String,
    /// The GPU block's values, in offset order.
    pub fields: Vec<Field>,
    /// In the shader's order, which a material's textures follow.
    pub textures: Vec<ShaderTexture>,
    /// The resources the stages declare: params, frame, object, then each texture and its
    /// sampler.
    pub bindings: Vec<Binding>,
    /// The vertex stage's SPIR-V words.
    pub vertex: Option<Vec<u32>>,
    /// The fragment stage's SPIR-V words.
    pub fragment: Option<Vec<u32>>,
}

impl Shader {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ASHADER", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let desc = require("DESC")?;
        let strings = require("STRINGS")?;

        let fields = require("FIELDS")?
            .records(FIELD)?
            .map(|r| {
                use FieldSource as S;
                use FieldType as T;
                let types = [T::U32, T::I32, T::F32, T::Vec3, T::Vec4];
                let sources = [S::Bool, S::Integer, S::Float, S::Enum, S::Rgb, S::Rgba];
                Ok(Field {
                    path_hash: r.u64(0)?,
                    path: r.text_ref(8, strings)?,
                    offset: r.u32(16)?,
                    ty: code(&r, r.u32(20)?, &types, "field type")?,
                    source: code(&r, r.u32(24)?, &sources, "field source")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let inputs = require("INPUTS")?
            .records(INPUT)?
            .map(|r| {
                let name = r.text_ref(0, strings)?;
                let channel = code(&r, r.u32(8)?, &[0u8, 1, 2, 3], "channel")?;
                Ok((!name.is_empty()).then_some(PackInput { name, channel }))
            })
            .collect::<Result<Vec<_>>>()?;

        let textures = require("TEXTURES")?
            .records(TEXTURE)?
            .map(|r| {
                use Fallback as F;
                use Usage as U;
                let usages = [
                    U::Color,
                    U::Data,
                    U::Normal,
                    U::Height,
                    U::Noise,
                    U::Gradient,
                    U::Palette,
                    U::Lut,
                    U::Environment,
                    U::Atlas,
                ];
                let dimensions = [Dimension::D2, Dimension::D3, Dimension::Cube];
                let fallbacks = [F::White, F::Black, F::Gray, F::Normal, F::Transparent];
                let flags = r.u8(23)?;
                let pack = if flags & 16 != 0 {
                    let first = r.u32(28)? as usize;
                    let count = r.u32(32)?;
                    let range = (count == 4)
                        .then(|| inputs.get(first..first.checked_add(4)?))
                        .flatten()
                        .ok_or_else(|| {
                            r.err(format!("inputs {first} + {count} run past INPUTS"))
                        })?;
                    Some(Pack {
                        inputs: std::array::from_fn(|i| range[i].clone()),
                        fill: r.array(36)?,
                    })
                } else {
                    None
                };
                Ok(ShaderTexture {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    usage: code(&r, r.u8(20)?.into(), &usages, "usage")?,
                    dimension: code(&r, r.u8(21)?.into(), &dimensions, "dimension")?,
                    default: code(&r, r.u8(22)?.into(), &fallbacks, "default")?,
                    srgb: flags & 1 != 0,
                    required: flags & 2 != 0,
                    bare: flags & 4 != 0,
                    mipmaps: flags & 8 != 0,
                    pack,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let bindings = require("BINDINGS")?
            .records(BINDING)?
            .map(|r| {
                use BindingKind as K;
                let kinds = [K::Params, K::Frame, K::Object, K::Texture, K::Sampler];
                Ok(Binding {
                    kind: code(&r, r.u32(0)?, &kinds, "binding kind")?,
                    texture: r.u32(4)?,
                    set: r.u32(8)?,
                    binding: r.u32(12)?,
                    stages: r.u32(16)?,
                    size: r.u32(20)?,
                    name: r.text_ref(24, strings)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(b) = bindings.iter().find(|b| {
            matches!(b.kind, BindingKind::Texture | BindingKind::Sampler)
                && b.texture as usize >= textures.len()
        }) {
            return Err(desc.err(format!(
                "binding {} names texture {} of {}",
                b.name,
                b.texture,
                textures.len()
            )));
        }

        Ok(Self {
            params_schema: desc.u64(0)?,
            params_type_hash: desc.u64(8)?,
            params_type: desc.text_ref(16, strings)?,
            block_size: desc.u32(24)?,
            state: State::read(&desc, 28, 32)?,
            vertex_entry: desc.text_ref(36, strings)?,
            fragment_entry: desc.text_ref(44, strings)?,
            doc: desc.text_ref(52, strings)?,
            fields,
            textures,
            bindings,
            vertex: spirv(file.chunk("SPIRVV"), "SPIRVV")?,
            fragment: spirv(file.chunk("SPIRVF"), "SPIRVF")?,
        })
    }

    /// Whether the shader has a params struct.
    pub fn has_params(&self) -> bool {
        !self.params_type.is_empty()
    }

    /// The block's value at `path` (`wave_speed`, `foam.amount`).
    pub fn field(&self, path: &str) -> Option<&Field> {
        let hash = fnv1a64(path.as_bytes());
        self.fields
            .iter()
            .find(|f| f.path_hash == hash && f.path == path)
    }

    /// The index of the texture named `name`, as in [`Shader::textures`] and a material's
    /// texture IDs.
    pub fn texture_index(&self, name: &str) -> Option<usize> {
        let hash = fnv1a64(name.as_bytes());
        self.textures
            .iter()
            .position(|t| t.name_hash == hash && t.name == name)
    }

    /// The texture named `name`.
    pub fn texture(&self, name: &str) -> Option<&ShaderTexture> {
        self.texture_index(name).map(|i| &self.textures[i])
    }

    /// The resource the stages declare as `name`.
    pub fn binding(&self, name: &str) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.name == name)
    }

    /// The resource of `kind`; `texture` is the texture's index for
    /// [`BindingKind::Texture`] and [`BindingKind::Sampler`], ignored for the others.
    pub fn binding_for(&self, kind: BindingKind, texture: u32) -> Option<&Binding> {
        let texture = match kind {
            BindingKind::Texture | BindingKind::Sampler => texture,
            _ => 0,
        };
        self.bindings
            .iter()
            .find(|b| b.kind == kind && b.texture == texture)
    }
}

/// A SPIR-V chunk's words; its size must be a multiple of 4 and it must start with the magic.
fn spirv(chunk: Option<&[u8]>, name: &str) -> Result<Option<Vec<u32>>> {
    let Some(bytes) = chunk else {
        return Ok(None);
    };
    let b = Bytes::new(bytes, FILE);
    if !bytes.len().is_multiple_of(4) {
        return Err(b.err(format!("{name} is {} bytes, not whole words", bytes.len())));
    }
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes(w.try_into().expect("4 bytes")))
        .collect();
    if words.first() != Some(&SPIRV_MAGIC) {
        return Err(b.err(format!("{name} is not SPIR-V")));
    }
    Ok(Some(words))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::graph::tests::text;

    /// What [`sample`] writes; overrides break it.
    #[derive(Default)]
    pub struct Breaks {
        pub state: Option<u32>,
        pub fragment: Option<Vec<u8>>,
        pub input_first: Option<u32>,
        pub drop_fields: bool,
    }

    /// A `WaterParams` shader: a 32-byte block (`wave_speed` f32 at 0, `deep` vec3 at 16),
    /// textures `color` (bare sRGB) and `mask` (packed: `foam` → R, `depth.a` → G), bindings
    /// for params, `color` and its sampler, and a fragment stage.
    pub fn sample(breaks: &Breaks) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut desc = 0x5c4eu64.to_le_bytes().to_vec();
        desc.extend(fnv1a64(b"WaterParams").to_le_bytes());
        desc.extend(text(&mut strings, "WaterParams"));
        desc.extend(32u32.to_le_bytes());
        // blend, cull none, no depth write, less_equal.
        let state = breaks.state.unwrap_or(2 | 2 << 3 | 1 << 6);
        desc.extend(state.to_le_bytes());
        desc.extend(0.25f32.to_le_bytes());
        desc.extend(text(&mut strings, ""));
        desc.extend(text(&mut strings, "main"));
        desc.extend(text(&mut strings, "Lake water"));
        desc.extend(0u32.to_le_bytes());

        let mut fields = Vec::new();
        for (path, offset, ty, source) in [("wave_speed", 0u32, 2u32, 2u32), ("deep", 16, 3, 4)] {
            fields.extend(fnv1a64(path.as_bytes()).to_le_bytes());
            fields.extend(text(&mut strings, path));
            for v in [offset, ty, source, 0] {
                fields.extend(v.to_le_bytes());
            }
        }

        let mut textures = Vec::new();
        // name, usage, dimension, default, flags, inputs, count, fill
        for (name, usage, flags, packed) in
            [("color", 0u8, 1 | 4 | 8, false), ("mask", 1, 8 | 16, true)]
        {
            textures.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            textures.extend(text(&mut strings, name));
            textures.extend(0u32.to_le_bytes());
            textures.extend([usage, 0, 0, flags, 0, 0, 0, 0]);
            if packed {
                textures.extend(breaks.input_first.unwrap_or(0).to_le_bytes());
                textures.extend(4u32.to_le_bytes());
                textures.extend([0, 255, 0, 255]);
            } else {
                textures.extend([0; 12]);
            }
            textures.extend([0; 8]);
        }

        let mut inputs = Vec::new();
        for (name, channel) in [("foam", 0u32), ("depth", 3), ("", 0), ("", 0)] {
            inputs.extend(text(&mut strings, name));
            inputs.extend(channel.to_le_bytes());
            inputs.extend(0u32.to_le_bytes());
        }

        let mut bindings = Vec::new();
        for (kind, texture, set, binding, size, name) in [
            (0u32, 0u32, 1u32, 0u32, 32u32, "params"),
            (3, 0, 1, 1, 0, "color"),
            (4, 0, 1, 2, 0, "color_sampler"),
        ] {
            for v in [kind, texture, set, binding, STAGE_FRAGMENT, size] {
                bindings.extend(v.to_le_bytes());
            }
            bindings.extend(text(&mut strings, name));
        }

        let fragment = breaks.fragment.clone().unwrap_or_else(|| {
            [SPIRV_MAGIC, 0x0001_0000, 0, 1, 0]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect()
        });
        let mut chunks: Vec<(&str, &[u8])> = vec![("DESC", &desc)];
        if !breaks.drop_fields {
            chunks.push(("FIELDS", &fields));
        }
        chunks.extend([
            ("TEXTURES", &textures[..]),
            ("INPUTS", &inputs),
            ("BINDINGS", &bindings),
            ("SPIRVF", &fragment),
            ("STRINGS", &strings),
        ]);
        file("ASHADER", 1, &chunks)
    }

    #[test]
    fn decodes_interface_and_stages() {
        let shader = Shader::decode(&sample(&Breaks::default())).unwrap();
        assert!(shader.has_params());
        assert_eq!(shader.params_type, "WaterParams");
        assert_eq!(shader.params_type_hash, fnv1a64(b"WaterParams"));
        assert_eq!((shader.params_schema, shader.block_size), (0x5c4e, 32));
        assert_eq!(shader.doc, "Lake water");
        let state = shader.state;
        assert_eq!((state.blend, state.cull), (Blend::Blend, Cull::None));
        assert_eq!(
            (state.depth_write, state.depth_test),
            (false, DepthTest::LessEqual)
        );
        assert_eq!(state.alpha_cutoff, 0.25);
        assert_eq!(state.bits(), 2 | 2 << 3 | 1 << 6);

        let deep = shader.field("deep").unwrap();
        assert_eq!(
            (deep.offset, deep.ty, deep.source),
            (16, FieldType::Vec3, FieldSource::Rgb)
        );
        assert_eq!(deep.ty.size(), 12);
        assert!(shader.field("nope").is_none());

        let color = shader.texture("color").unwrap();
        assert_eq!(
            (color.usage, color.dimension, color.default),
            (Usage::Color, Dimension::D2, Fallback::White)
        );
        assert!(color.srgb && color.bare && color.mipmaps && !color.required);
        assert!(color.pack.is_none());
        assert_eq!(shader.texture_index("mask"), Some(1));
        let pack = shader.texture("mask").unwrap().pack.as_ref().unwrap();
        assert_eq!(pack.fill, [0, 255, 0, 255]);
        assert_eq!(
            pack.inputs[0]
                .as_ref()
                .map(|i| (i.name.as_str(), i.channel)),
            Some(("foam", 0))
        );
        assert_eq!(
            pack.inputs[1]
                .as_ref()
                .map(|i| (i.name.as_str(), i.channel)),
            Some(("depth", 3))
        );
        assert!(pack.inputs[2].is_none() && pack.inputs[3].is_none());
        assert_eq!(Usage::Data.key(), "data");

        let params = shader.binding_for(BindingKind::Params, 9).unwrap();
        assert_eq!((params.set, params.binding, params.size), (1, 0, 32));
        assert!(params.in_fragment() && !params.in_vertex());
        assert_eq!(shader.binding("color_sampler").unwrap().binding, 2);
        assert_eq!(
            shader.binding_for(BindingKind::Texture, 0).unwrap().name,
            "color"
        );
        assert!(shader.binding_for(BindingKind::Texture, 1).is_none());
        assert!(shader.binding_for(BindingKind::Frame, 0).is_none());

        assert!(shader.vertex.is_none());
        assert_eq!(shader.vertex_entry, "");
        assert_eq!(shader.fragment_entry, "main");
        assert_eq!(shader.fragment.as_ref().map(|w| w[0]), Some(SPIRV_MAGIC));
        assert_eq!(shader.fragment.as_ref().map(Vec::len), Some(5));
    }

    #[test]
    fn rejects_bad_files() {
        let bad = |b: Breaks| Shader::decode(&sample(&b)).is_err();
        assert!(bad(Breaks {
            drop_fields: true,
            ..Breaks::default()
        }));
        assert!(bad(Breaks {
            state: Some(5),
            ..Breaks::default()
        }));
        assert!(bad(Breaks {
            state: Some(1 << 8),
            ..Breaks::default()
        }));
        assert!(bad(Breaks {
            input_first: Some(1),
            ..Breaks::default()
        }));
        assert!(bad(Breaks {
            fragment: Some(vec![3, 2, 0x23, 7, 0]),
            ..Breaks::default()
        }));
        assert!(bad(Breaks {
            fragment: Some(vec![0; 8]),
            ..Breaks::default()
        }));
        let mut short = sample(&Breaks::default());
        short.truncate(short.len() - 1);
        assert!(Shader::decode(&short).is_err());
    }
}
