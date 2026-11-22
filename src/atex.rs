//! `.atex.bin` textures: levels encoded with imgio in the GPU format of the build's target,
//! written to assetc's own container, and read back for the Inspector; see
//! docs/spec/atex.bin.md.

use imgio::{AstcBlock, AstcKind, EncodeOptions};
use serde::Deserialize;

use crate::chunks::{self, Bytes};
use crate::texture::{ColorSpace, Image};

pub const FORMAT: &str = "ATEX";
pub const VERSION: u32 = 1;

/// The GPU format every texture of a build is written in, `[textures] format`; see
/// docs/features/textures.md#format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Bc,
    Astc,
    Rgba,
    Rgb,
}

/// How hard the block encoders try, `[textures] quality`; see
/// docs/features/textures.md#quality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Fastest,
    Fast,
    #[default]
    Default,
    High,
    Best,
}

impl Quality {
    fn imgio(self) -> imgio::Quality {
        match self {
            Self::Fastest => imgio::Quality::Fastest,
            Self::Fast => imgio::Quality::Fast,
            Self::Default => imgio::Quality::Default,
            Self::High => imgio::Quality::High,
            Self::Best => imgio::Quality::Best,
        }
    }
}

/// What a build writes its textures as: the target's format and quality.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Target {
    pub format: Format,
    pub quality: Quality,
}

impl Target {
    /// Uncompressed RGBA8, as lookup textures are always written.
    pub const RGBA: Self = Self {
        format: Format::Rgba,
        quality: Quality::Default,
    };

    pub fn of(textures: &crate::config::Textures) -> Self {
        Self {
            format: textures.format,
            quality: textures.quality,
        }
    }

    /// Lookup textures stay `rgba`: block compression would change the texels they are read for.
    pub fn for_usage(self, usage: Usage) -> Self {
        match usage {
            Usage::Gradient | Usage::Palette | Usage::Lut => Self::RGBA,
            _ => self,
        }
    }

    /// The format's word in a build line (`bc7`, `astc 4x4`, `rgba8`, …).
    pub fn token(self, kind: Kind) -> &'static str {
        match (self.format, kind) {
            (Format::Bc, Kind::Rgba) => "bc7",
            (Format::Bc, Kind::R) => "bc4",
            (Format::Bc, Kind::Rg) => "bc5",
            (Format::Bc, Kind::Hdr) => "bc6h",
            (Format::Astc, Kind::Hdr) => "astc 4x4 hdr",
            (Format::Astc, _) => "astc 4x4",
            (_, Kind::R) => "r8",
            (_, Kind::Rg) => "rg8",
            (Format::Rgba, Kind::Rgba) => "rgba8",
            (Format::Rgb, Kind::Rgba) => "rgb8",
            (Format::Rgba, Kind::Hdr) => "rgba16f",
            (Format::Rgb, Kind::Hdr) => "rgb16f",
        }
    }
}

/// What the images hold: 8-bit RGBA, one or two 8-bit channels, or half-float RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rgba,
    R,
    Rg,
    Hdr,
}

/// What a texture is for; see docs/spec/atex.bin.md#usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    Color,
    Data,
    Normal,
    Orm,
    Height,
    Noise,
    Gradient,
    Palette,
    Atlas,
    Font,
    Lut,
    Environment,
}

impl Usage {
    /// In the order of their values.
    pub const ALL: [Self; 12] = [
        Self::Color,
        Self::Data,
        Self::Normal,
        Self::Orm,
        Self::Height,
        Self::Noise,
        Self::Gradient,
        Self::Palette,
        Self::Atlas,
        Self::Font,
        Self::Lut,
        Self::Environment,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Data => "data",
            Self::Normal => "normal",
            Self::Orm => "orm",
            Self::Height => "height",
            Self::Noise => "noise",
            Self::Gradient => "gradient",
            Self::Palette => "palette",
            Self::Atlas => "atlas",
            Self::Font => "font",
            Self::Lut => "lut",
            Self::Environment => "environment",
        }
    }

    /// Its `usage` value in the file.
    pub fn value(self) -> u32 {
        Self::ALL.iter().position(|u| *u == self).unwrap_or(0) as u32
    }

    /// `color` for sRGB textures, `data` for linear ones.
    pub fn of(color: ColorSpace) -> Self {
        match color {
            ColorSpace::Srgb => Self::Color,
            ColorSpace::Linear => Self::Data,
        }
    }
}

/// A `VkFormat` assetc writes, with its block and imgio's name for it; see
/// docs/spec/atex.bin.md#formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VkFormat {
    pub value: u32,
    /// Without `VK_FORMAT_`.
    pub name: &'static str,
    /// Texels across and down a block: 4 for the block formats, 1 for plain texels.
    pub block: u32,
    /// Bytes of a block (or texel).
    pub bytes: u32,
    imgio: imgio::Format,
}

const fn vk(
    value: u32,
    name: &'static str,
    block: u32,
    bytes: u32,
    imgio: imgio::Format,
) -> VkFormat {
    VkFormat {
        value,
        name,
        block,
        bytes,
        imgio,
    }
}

const ASTC_SRGB: imgio::Format = imgio::Format::Astc(AstcBlock::B4x4, AstcKind::Srgb);
const ASTC_UNORM: imgio::Format = imgio::Format::Astc(AstcBlock::B4x4, AstcKind::Unorm);
const ASTC_FLOAT: imgio::Format = imgio::Format::Astc(AstcBlock::B4x4, AstcKind::Float);

pub const R8_UNORM: VkFormat = vk(9, "R8_UNORM", 1, 1, imgio::Format::R8Unorm);
pub const R8G8_UNORM: VkFormat = vk(16, "R8G8_UNORM", 1, 2, imgio::Format::Rg8Unorm);
pub const R8G8B8_UNORM: VkFormat = vk(23, "R8G8B8_UNORM", 1, 3, imgio::Format::Rgb8Unorm);
pub const R8G8B8_SRGB: VkFormat = vk(29, "R8G8B8_SRGB", 1, 3, imgio::Format::Rgb8Srgb);
pub const R8G8B8A8_UNORM: VkFormat = vk(37, "R8G8B8A8_UNORM", 1, 4, imgio::Format::Rgba8Unorm);
pub const R8G8B8A8_SRGB: VkFormat = vk(43, "R8G8B8A8_SRGB", 1, 4, imgio::Format::Rgba8Srgb);
pub const R16G16B16_SFLOAT: VkFormat = vk(90, "R16G16B16_SFLOAT", 1, 6, imgio::Format::Rgb16Float);
pub const R16G16B16A16_SFLOAT: VkFormat =
    vk(97, "R16G16B16A16_SFLOAT", 1, 8, imgio::Format::Rgba16Float);
pub const BC4_UNORM: VkFormat = vk(139, "BC4_UNORM_BLOCK", 4, 8, imgio::Format::Bc4Unorm);
pub const BC5_UNORM: VkFormat = vk(141, "BC5_UNORM_BLOCK", 4, 16, imgio::Format::Bc5Unorm);
pub const BC6H_UFLOAT: VkFormat = vk(143, "BC6H_UFLOAT_BLOCK", 4, 16, imgio::Format::Bc6hUfloat);
pub const BC7_UNORM: VkFormat = vk(145, "BC7_UNORM_BLOCK", 4, 16, imgio::Format::Bc7Unorm);
pub const BC7_SRGB: VkFormat = vk(146, "BC7_SRGB_BLOCK", 4, 16, imgio::Format::Bc7Srgb);
pub const ASTC_4X4_UNORM: VkFormat = vk(157, "ASTC_4x4_UNORM_BLOCK", 4, 16, ASTC_UNORM);
pub const ASTC_4X4_SRGB: VkFormat = vk(158, "ASTC_4x4_SRGB_BLOCK", 4, 16, ASTC_SRGB);
pub const ASTC_4X4_SFLOAT: VkFormat = vk(1_000_066_000, "ASTC_4x4_SFLOAT_BLOCK", 4, 16, ASTC_FLOAT);

/// Every format assetc writes.
pub const VK_FORMATS: [VkFormat; 16] = [
    R8_UNORM,
    R8G8_UNORM,
    R8G8B8_UNORM,
    R8G8B8_SRGB,
    R8G8B8A8_UNORM,
    R8G8B8A8_SRGB,
    R16G16B16_SFLOAT,
    R16G16B16A16_SFLOAT,
    BC4_UNORM,
    BC5_UNORM,
    BC6H_UFLOAT,
    BC7_UNORM,
    BC7_SRGB,
    ASTC_4X4_UNORM,
    ASTC_4X4_SRGB,
    ASTC_4X4_SFLOAT,
];

impl VkFormat {
    pub fn of(value: u32) -> Option<Self> {
        VK_FORMATS.into_iter().find(|f| f.value == value)
    }

    /// Bytes of a `w` × `h` image.
    pub fn image_bytes(self, w: u32, h: u32) -> usize {
        w.div_ceil(self.block) as usize * h.div_ceil(self.block) as usize * self.bytes as usize
    }

    pub fn is_srgb(self) -> bool {
        self.name.contains("SRGB")
    }
}

/// No channel, in a swizzle.
const NONE: u8 = 255;

/// The format, swizzle and the RGBA8 (or RGBA16F) texels to encode one source texel as, for
/// `kind` in `format`; see docs/spec/atex.bin.md#formats.
fn layout(format: Format, kind: Kind, srgb: bool) -> (VkFormat, [u8; 4]) {
    let pick = |s: VkFormat, u: VkFormat| if srgb { s } else { u };
    match (kind, format) {
        (Kind::Rgba, Format::Bc) => (pick(BC7_SRGB, BC7_UNORM), [0, 1, 2, 3]),
        (Kind::Rgba, Format::Astc) => (pick(ASTC_4X4_SRGB, ASTC_4X4_UNORM), [0, 1, 2, 3]),
        (Kind::Rgba, Format::Rgba) => (pick(R8G8B8A8_SRGB, R8G8B8A8_UNORM), [0, 1, 2, 3]),
        (Kind::Rgba, Format::Rgb) => (pick(R8G8B8_SRGB, R8G8B8_UNORM), [0, 1, 2, NONE]),
        (Kind::R, Format::Bc) => (BC4_UNORM, [0, NONE, NONE, NONE]),
        (Kind::R, Format::Astc) => (ASTC_4X4_UNORM, [0, NONE, NONE, NONE]),
        (Kind::R, _) => (R8_UNORM, [0, NONE, NONE, NONE]),
        (Kind::Rg, Format::Bc) => (BC5_UNORM, [0, 1, NONE, NONE]),
        (Kind::Rg, Format::Astc) => (ASTC_4X4_UNORM, [0, 3, NONE, NONE]),
        (Kind::Rg, _) => (R8G8_UNORM, [0, 1, NONE, NONE]),
        (Kind::Hdr, Format::Bc) => (BC6H_UFLOAT, [0, 1, 2, NONE]),
        (Kind::Hdr, Format::Astc) => (ASTC_4X4_SFLOAT, [0, 1, 2, 3]),
        (Kind::Hdr, Format::Rgba) => (R16G16B16A16_SFLOAT, [0, 1, 2, 3]),
        (Kind::Hdr, Format::Rgb) => (R16G16B16_SFLOAT, [0, 1, 2, NONE]),
    }
}

/// Encodes RGBA8 mip levels (largest first, each half the previous) as a texture.
pub fn encode_rgba8(
    target: Target,
    color: ColorSpace,
    usage: Usage,
    levels: &[Image],
) -> Result<Vec<u8>, String> {
    for level in levels {
        assert_eq!(
            level.pixels.len(),
            level.width as usize * level.height as usize * 4
        );
    }
    let data: Vec<&[u8]> = levels.iter().map(|l| l.pixels.as_slice()).collect();
    let size = [levels[0].width, levels[0].height, 1];
    encode(
        target.for_usage(usage),
        Kind::Rgba,
        color,
        usage,
        (size, 1),
        &data,
    )
}

/// Encodes the first `channels` (1 R or 2 RG) of RGBA8 mip levels as a linear texture; see
/// docs/spec/atex.bin.md#swizzle.
pub fn encode_unorm(
    target: Target,
    channels: usize,
    usage: Usage,
    levels: &[Image],
) -> Result<Vec<u8>, String> {
    assert!(matches!(channels, 1 | 2));
    let data: Vec<&[u8]> = levels.iter().map(|l| l.pixels.as_slice()).collect();
    let size = [levels[0].width, levels[0].height, 1];
    let kind = if channels == 1 { Kind::R } else { Kind::Rg };
    encode(target, kind, ColorSpace::Linear, usage, (size, 1), &data)
}

/// Encodes one RGBA8 3D texture level, `size` = width, height, depth; `pixels` are depth
/// slices of rows, top to bottom. Always `rgba`; see docs/spec/atex.bin.md#volume.
pub fn encode_rgba8_volume(
    color: ColorSpace,
    usage: Usage,
    size: [u32; 3],
    pixels: &[u8],
) -> Result<Vec<u8>, String> {
    assert_eq!(
        pixels.len(),
        size.iter().map(|&s| s as usize).product::<usize>() * 4
    );
    encode(Target::RGBA, Kind::Rgba, color, usage, (size, 1), &[pixels])
}

/// Encodes an HDR cubemap's levels (largest first); see docs/spec/atex.bin.md#cubemaps.
pub fn encode_cubemap(target: Target, levels: &[crate::hdr::Level]) -> Result<Vec<u8>, String> {
    let size = levels[0].size as u32;
    let data: Vec<Vec<u8>> = levels
        .iter()
        .map(|level| {
            let mut bytes = Vec::with_capacity(6 * level.size * level.size * 8);
            for face in &level.faces {
                for texel in face {
                    for c in texel {
                        // Beyond the half-float range: its largest finite value.
                        let c = c.clamp(-65504.0, 65504.0);
                        bytes.extend_from_slice(&imgio::half::from_f32(c).to_le_bytes());
                    }
                }
            }
            bytes
        })
        .collect();
    let data: Vec<&[u8]> = data.iter().map(Vec::as_slice).collect();
    let usage = Usage::Environment;
    encode(
        target,
        Kind::Hdr,
        ColorSpace::Linear,
        usage,
        ([size, size, 1], 6),
        &data,
    )
}

/// `levels` hold each level's faces (or depth slices) back to back, as RGBA8 texels (RGBA16F
/// for HDR); `size` is level 0's width, height and depth.
fn encode(
    target: Target,
    kind: Kind,
    color: ColorSpace,
    usage: Usage,
    (size, faces): ([u32; 3], u32),
    levels: &[&[u8]],
) -> Result<Vec<u8>, String> {
    let srgb = color == ColorSpace::Srgb && matches!(kind, Kind::Rgba);
    let (vk, swizzle) = layout(target.format, kind, srgb);
    let source_format = match kind {
        Kind::Hdr => imgio::Format::Rgba16Float,
        _ if srgb => imgio::Format::Rgba8Srgb,
        _ => imgio::Format::Rgba8Unorm,
    };
    let texel = if kind == Kind::Hdr { 8 } else { 4 };
    let options = EncodeOptions {
        quality: target.quality.imgio(),
        ..Default::default()
    };
    let mut data = Vec::new();
    let mut records = Bytes(Vec::new());
    for (i, level) in levels.iter().enumerate() {
        let dim = |s: u32| (s >> i).max(1);
        let (w, h, d) = (dim(size[0]), dim(size[1]), dim(size[2]));
        let image = w as usize * h as usize * texel;
        assert_eq!(level.len(), image * d as usize * faces as usize);
        let offset = data.len();
        for texels in level.chunks_exact(image) {
            // Where the channels go: R as `R R R 1`; RG as `R G 0 1` (BC5) or `R R R G` (ASTC).
            let arranged: Vec<u8>;
            let texels = match kind {
                Kind::R => {
                    arranged = texels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|t| [t[0], t[0], t[0], 255])
                        .collect();
                    &arranged
                }
                Kind::Rg if target.format == Format::Astc => {
                    arranged = texels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|t| [t[0], t[0], t[0], t[1]])
                        .collect();
                    &arranged
                }
                Kind::Rg => {
                    arranged = texels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|t| [t[0], t[1], 0, 255])
                        .collect();
                    &arranged
                }
                _ => texels,
            };
            let surface =
                imgio::Surface::new(source_format, w, h, texels).map_err(|e| e.to_string())?;
            let encoded = if vk.block > 1 {
                imgio::encode_to_buffer(&surface, vk.imgio, &options)
            } else {
                imgio::convert::convert_to_buffer(&surface, vk.imgio)
            }
            .map_err(|e| format!("encoding {}: {e}", vk.name))?;
            data.extend_from_slice(&encoded);
        }
        let length = data.len() - offset;
        debug_assert_eq!(length, vk.image_bytes(w, h) * d as usize * faces as usize);
        records
            .u64(offset as u64)
            .u64(length as u64)
            .u32(w)
            .u32(h)
            .u32(d)
            .u32(0);
        data.resize(data.len().next_multiple_of(16), 0);
    }
    let mut desc = Bytes(Vec::new());
    desc.u32(vk.value)
        .u32(size[0])
        .u32(size[1])
        .u32(size[2])
        .u32(faces)
        .u32(1)
        .u32(levels.len() as u32)
        .u32(usage.value())
        .u32(u32::from(vk.is_srgb()));
    desc.0.extend_from_slice(&swizzle);
    desc.0
        .extend_from_slice(&[vk.block as u8, vk.block as u8, 1, 0]);
    desc.u32(vk.bytes).u64(0).u64(0);
    // The last level's padding is not part of the file.
    let used = records.0.rchunks_exact(32).next().map_or(0, |r| {
        let at = |i: usize| u64::from_le_bytes(r[i..i + 8].try_into().unwrap()) as usize;
        at(0) + at(8)
    });
    data.truncate(used);
    let mut writer = chunks::Writer::new(FORMAT, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("LEVELS", records.take())
        .chunk("DATA", data);
    Ok(writer.finish())
}

/// What the Inspector shows of a `.atex.bin` file.
#[derive(Debug, PartialEq, Eq)]
pub struct Info {
    pub format: VkFormat,
    pub width: u32,
    pub height: u32,
    /// 1 unless a volume.
    pub depth: u32,
    pub faces: u32,
    pub layers: u32,
    pub levels: u32,
    pub usage: String,
    pub srgb: bool,
    pub swizzle: [u8; 4],
    /// Bytes of every level together.
    pub bytes: u64,
    /// Level 0 as 8-bit RGBA, for the formats the preview decodes; a 3D texture's slices side
    /// by side.
    pub level0: Option<Image>,
    /// Levels 1 and up, decoded like level 0 (empty when level 0 is not).
    pub mips: Vec<Image>,
}

impl Info {
    /// What the file holds: `2D`, `Cubemap`, `3D` or an array of either.
    pub fn type_name(&self) -> String {
        let base = match (self.faces, self.depth) {
            (6, _) => "Cubemap",
            (_, d) if d > 1 => "3D",
            _ => "2D",
        };
        match self.layers {
            0 | 1 => base.to_owned(),
            n => format!("{base} array of {n}"),
        }
    }

    pub fn format_name(&self) -> &'static str {
        self.format.name
    }

    /// The channels and where they are read: `RGBA`, `RG`, `RG (from R, A)`, …
    pub fn channels(&self) -> String {
        const NAMES: [char; 4] = ['R', 'G', 'B', 'A'];
        let used: Vec<u8> = self
            .swizzle
            .iter()
            .copied()
            .take_while(|&c| c != NONE)
            .collect();
        let name: String = NAMES[..used.len()].iter().collect();
        if used.iter().enumerate().all(|(i, &c)| c as usize == i) {
            return name;
        }
        let from: Vec<String> = used
            .iter()
            .map(|&c| NAMES.get(c as usize).map_or("?".into(), |n| n.to_string()))
            .collect();
        format!("{name} (from {})", from.join(", "))
    }

    /// Every level's 8-bit RGBA size against its bytes: `4:1`.
    pub fn compression(&self) -> String {
        let rgba: u64 = (0..self.levels)
            .map(|i| {
                let dim = |s: u32| u64::from((s >> i).max(1));
                dim(self.width) * dim(self.height) * dim(self.depth) * 4
            })
            .sum::<u64>()
            * u64::from(self.faces * self.layers.max(1));
        let ratio = rgba as f64 / self.bytes.max(1) as f64;
        if (ratio - ratio.round()).abs() < 0.05 {
            format!("{}:1", ratio.round())
        } else {
            format!("{ratio:.1}:1")
        }
    }
}

/// Reads a texture's description and decodes what the preview shows.
pub fn read(bytes: &[u8]) -> Result<Info, String> {
    let reader = chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported ATEX version {}", reader.version));
    }
    let desc = reader.require("DESC")?;
    let u = |at| chunks::u32_at(desc, 0, 64, at).ok_or("DESC is too short");
    let value = u(0)?;
    let format = VkFormat::of(value).ok_or_else(|| format!("unsupported VkFormat {value}"))?;
    let (width, height, depth, faces, layers, count) =
        (u(4)?, u(8)?, u(12)?, u(16)?, u(20)?, u(24)?);
    let usage = match Usage::ALL.get(u(28)? as usize) {
        Some(usage) => usage.key().to_owned(),
        None => format!("usage {}", u(28)?),
    };
    let srgb = u(32)? & 1 == 1;
    let swizzle = u(36)?.to_le_bytes();
    let table = reader.require("LEVELS")?;
    let data = reader.require("DATA")?;
    let mut levels = Vec::new();
    for i in 0..count as usize {
        let at = |o| chunks::u32_at(table, i, 32, o).ok_or(format!("LEVELS has no level {i}"));
        let offset = u64::from(at(0)?) | u64::from(at(4)?) << 32;
        let size = u64::from(at(8)?) | u64::from(at(12)?) << 32;
        let (w, h, d) = (at(16)?, at(20)?, at(24)?);
        let expected = format.image_bytes(w, h) as u64 * u64::from(d * faces * layers.max(1));
        if size != expected {
            return Err(format!(
                "level {i} has {size} bytes, {w} × {h} × {d} needs {expected}"
            ));
        }
        let level = usize::try_from(offset)
            .ok()
            .and_then(|o| data.get(o..o.checked_add(size as usize)?))
            .ok_or(format!("level {i} runs past DATA"))?;
        levels.push((w, h, d, level));
    }
    let total = levels.iter().map(|l| l.3.len() as u64).sum();
    let mut images = if faces == 1 && layers <= 1 {
        levels
            .iter()
            .map(|&(w, h, d, level)| preview(format, swizzle, (w, h, d), level))
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let level0 = (!images.is_empty()).then(|| images.remove(0));
    // Volumes show level 0 only.
    let mips = if depth <= 1 { images } else { Vec::new() };
    Ok(Info {
        format,
        width,
        height,
        depth,
        faces,
        layers,
        levels: count,
        usage,
        srgb,
        swizzle,
        bytes: total,
        level0,
        mips,
    })
}

/// A level as 8-bit RGBA, a 3D level's slices side by side; `None` for formats the preview
/// does not decode (half floats, BC6H, ASTC HDR). R shows as gray, RG as red and green.
fn preview(
    format: VkFormat,
    swizzle: [u8; 4],
    (w, h, d): (u32, u32, u32),
    level: &[u8],
) -> Option<Image> {
    let (texels, channels) = if format.block > 1 {
        if matches!(format, BC6H_UFLOAT | ASTC_4X4_SFLOAT) {
            return None;
        }
        let decoded = imgio::decode_to_buffer(format.imgio, level, w, h).ok()?;
        let n = format.imgio.decoded().block_bytes();
        (decoded.to_vec(), n)
    } else if format.bytes <= 4 {
        (level.to_vec(), format.bytes as usize)
    } else {
        return None;
    };
    // An R or RG texture shows only its channels, wherever they are stored.
    let used = swizzle.iter().take_while(|&&c| c != NONE).count();
    let (w, h, d) = (w as usize, h as usize, d as usize);
    let row = w * channels;
    if texels.len() < row * h * d {
        return None;
    }
    let mut pixels = Vec::with_capacity(w * h * d * 4);
    for y in 0..h {
        for z in 0..d {
            let start = (z * h + y) * row;
            for t in texels[start..start + row].chunks_exact(channels) {
                let c = |i: usize| t.get(swizzle[i] as usize).copied().unwrap_or(0);
                pixels.extend_from_slice(&match (used, channels) {
                    (1, _) => [c(0), c(0), c(0), 255],
                    (2, _) => [c(0), c(1), 0, 255],
                    (3, _) => [c(0), c(1), c(2), 255],
                    _ => [c(0), c(1), c(2), c(3)],
                });
            }
        }
    }
    Some(Image {
        width: (w * d) as u32,
        height: h as u32,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::mip_chain;

    fn sample() -> Image {
        let (width, height) = (3, 2);
        let pixels = (0..width * height * 4).map(|i| i as u8).collect();
        Image {
            width,
            height,
            pixels,
        }
    }

    fn flat(size: u32, texel: [u8; 4]) -> Image {
        Image {
            width: size,
            height: size,
            pixels: texel.repeat((size * size) as usize),
        }
    }

    const ALL: [Format; 4] = [Format::Bc, Format::Astc, Format::Rgba, Format::Rgb];

    fn target(format: Format) -> Target {
        Target {
            format,
            quality: Quality::Fast,
        }
    }

    fn close(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 3)
    }

    #[test]
    fn rgba_round_trips_with_usage() {
        let levels = mip_chain(sample(), ColorSpace::Srgb);
        let file = encode_rgba8(Target::RGBA, ColorSpace::Srgb, Usage::Normal, &levels).unwrap();
        let info = read(&file).unwrap();
        assert_eq!((info.width, info.height, info.levels), (3, 2, 2));
        assert_eq!(info.format_name(), "R8G8B8A8_SRGB");
        assert_eq!(info.type_name(), "2D");
        assert_eq!(info.channels(), "RGBA");
        assert!(info.srgb);
        assert_eq!(info.level0, Some(levels[0].clone()));
        assert_eq!(info.mips, levels[1..]);
        assert_eq!(info.usage, "normal");
        assert_eq!(info.bytes, 3 * 2 * 4 + 4);
        assert_eq!(info.compression(), "1:1");

        let info = read(
            &encode_rgba8(Target::RGBA, ColorSpace::Linear, Usage::Color, &[sample()]).unwrap(),
        )
        .unwrap();
        assert_eq!(info.format_name(), "R8G8B8A8_UNORM");
        assert!(!info.srgb);
    }

    #[test]
    fn layout_follows_the_spec() {
        let levels = mip_chain(sample(), ColorSpace::Srgb);
        let file =
            encode_rgba8(target(Format::Bc), ColorSpace::Srgb, Usage::Color, &levels).unwrap();
        let reader = chunks::Reader::parse(&file, FORMAT).unwrap();
        assert_eq!(reader.version, 1);
        let names: Vec<_> = reader.entries().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["DESC", "LEVELS", "DATA"]);
        let desc = reader.chunk("DESC").unwrap();
        assert_eq!(desc.len(), 64);
        let u = |at| chunks::u32_at(desc, 0, 64, at).unwrap();
        assert_eq!(
            [u(0), u(4), u(8), u(12), u(16), u(20), u(24), u(28), u(32)],
            [146, 3, 2, 1, 1, 1, 2, 0, 1]
        );
        assert_eq!(desc[36..44], [0, 1, 2, 3, 4, 4, 1, 0]);
        assert_eq!(u(44), 16);
        let levels = reader.chunk("LEVELS").unwrap();
        let at = |i, o| chunks::u32_at(levels, i, 32, o).unwrap();
        // Two levels of one block each; the second at 16.
        assert_eq!([at(0, 0), at(0, 8), at(0, 16), at(0, 20)], [0, 16, 3, 2]);
        assert_eq!([at(1, 0), at(1, 8), at(1, 16), at(1, 20)], [16, 16, 1, 1]);
        assert_eq!(reader.chunk("DATA").unwrap().len(), 32);
    }

    #[test]
    fn every_format_previews_close_to_the_image() {
        let texel = [200, 60, 20, 255];
        for format in ALL {
            for color in [ColorSpace::Srgb, ColorSpace::Linear] {
                let levels = mip_chain(flat(16, texel), color);
                let file = encode_rgba8(target(format), color, Usage::of(color), &levels).unwrap();
                let info = read(&file).unwrap();
                assert_eq!(info.levels, 5, "{format:?}");
                assert_eq!(info.srgb, color == ColorSpace::Srgb, "{format:?}");
                let level0 = info.level0.unwrap();
                assert_eq!((level0.width, level0.height), (16, 16));
                assert!(
                    close(&level0.pixels[..4], &texel),
                    "{format:?} {:?}",
                    &level0.pixels[..4]
                );
                assert_eq!(info.mips.len(), 4);
            }
        }
        let name = |format, color| {
            read(&encode_rgba8(target(format), color, Usage::Color, &[flat(8, texel)]).unwrap())
                .unwrap()
                .format_name()
        };
        assert_eq!(name(Format::Bc, ColorSpace::Srgb), "BC7_SRGB_BLOCK");
        assert_eq!(name(Format::Bc, ColorSpace::Linear), "BC7_UNORM_BLOCK");
        assert_eq!(name(Format::Astc, ColorSpace::Srgb), "ASTC_4x4_SRGB_BLOCK");
        assert_eq!(name(Format::Rgb, ColorSpace::Srgb), "R8G8B8_SRGB");
        assert_eq!(name(Format::Rgb, ColorSpace::Linear), "R8G8B8_UNORM");
    }

    #[test]
    fn bc7_is_a_quarter_of_rgba() {
        let levels = mip_chain(flat(64, [10, 20, 30, 40]), ColorSpace::Srgb);
        let info = read(
            &encode_rgba8(target(Format::Bc), ColorSpace::Srgb, Usage::Color, &levels).unwrap(),
        )
        .unwrap();
        assert_eq!(info.compression(), "4:1");
        assert_eq!(info.bytes, (256 + 64 + 16 + 4 + 1 + 1 + 1) * 16);
    }

    #[test]
    fn rgb_drops_alpha() {
        let file = encode_rgba8(
            target(Format::Rgb),
            ColorSpace::Linear,
            Usage::Data,
            &[sample()],
        )
        .unwrap();
        let reader = chunks::Reader::parse(&file, FORMAT).unwrap();
        assert_eq!(reader.chunk("DATA").unwrap()[..6], [0, 1, 2, 4, 5, 6]);
        let info = read(&file).unwrap();
        assert_eq!(
            info.level0.as_ref().unwrap().pixels[..8],
            [0, 1, 2, 255, 4, 5, 6, 255]
        );
        assert_eq!(info.channels(), "RGB");
    }

    #[test]
    fn one_and_two_channels_in_every_format() {
        let levels = mip_chain(flat(8, [200, 60, 0, 255]), ColorSpace::Linear);
        for (channels, names, shown) in [
            (
                1,
                [
                    "BC4_UNORM_BLOCK",
                    "ASTC_4x4_UNORM_BLOCK",
                    "R8_UNORM",
                    "R8_UNORM",
                ],
                ["R", "R", "R", "R"],
            ),
            (
                2,
                [
                    "BC5_UNORM_BLOCK",
                    "ASTC_4x4_UNORM_BLOCK",
                    "R8G8_UNORM",
                    "R8G8_UNORM",
                ],
                ["RG", "RG (from R, A)", "RG", "RG"],
            ),
        ] {
            for ((format, name), channel_text) in ALL.into_iter().zip(names).zip(shown) {
                let file = encode_unorm(target(format), channels, Usage::Data, &levels).unwrap();
                let info = read(&file).unwrap();
                assert_eq!(info.format_name(), name);
                assert_eq!(info.channels(), channel_text);
                // The preview shows the channels wherever they are stored.
                let texel = &info.level0.unwrap().pixels[..4];
                let expected = match channels {
                    1 => [200, 200, 200, 255],
                    _ => [200, 60, 0, 255],
                };
                assert!(close(texel, &expected), "{format:?} {channels}: {texel:?}");
            }
        }
    }

    #[test]
    fn lookup_textures_stay_rgba() {
        for usage in [Usage::Gradient, Usage::Palette, Usage::Lut] {
            let file =
                encode_rgba8(target(Format::Bc), ColorSpace::Srgb, usage, &[sample()]).unwrap();
            assert_eq!(read(&file).unwrap().format_name(), "R8G8B8A8_SRGB");
        }
    }

    #[test]
    fn cubemaps_in_every_format() {
        let levels: Vec<crate::hdr::Level> = [8usize, 4, 2, 1]
            .into_iter()
            .map(|size| crate::hdr::Level {
                size,
                faces: std::array::from_fn(|f| vec![[2.5, 0.5, f as f32, 1.0]; size * size]),
            })
            .collect();
        for (format, name) in ALL.into_iter().zip([
            "BC6H_UFLOAT_BLOCK",
            "ASTC_4x4_SFLOAT_BLOCK",
            "R16G16B16A16_SFLOAT",
            "R16G16B16_SFLOAT",
        ]) {
            let file = encode_cubemap(target(format), &levels).unwrap();
            let info = read(&file).unwrap();
            assert_eq!(info.format_name(), name);
            assert_eq!(
                (info.faces, info.levels, info.type_name().as_str()),
                (6, 4, "Cubemap")
            );
            assert_eq!(info.usage, "environment");
            assert_eq!(info.level0, None);
        }
        // BC6H keeps values above 1.
        let file = encode_cubemap(target(Format::Bc), &levels).unwrap();
        let reader = chunks::Reader::parse(&file, FORMAT).unwrap();
        let block = &reader.chunk("DATA").unwrap()[..16];
        let decoded = imgio::decode_to_buffer(imgio::Format::Bc6hUfloat, block, 4, 4).unwrap();
        let red = imgio::half::to_f32(u16::from_le_bytes([decoded[0], decoded[1]]));
        assert!((red - 2.5).abs() < 0.1, "{red}");
    }

    #[test]
    fn volumes_are_3d_and_preview_as_slices() {
        // 2 × 1 × 2: texel (x, 0, z) has red x, blue z.
        let pixels = [
            0, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255,
        ];
        let file = encode_rgba8_volume(ColorSpace::Linear, Usage::Lut, [2, 1, 2], &pixels).unwrap();
        let info = read(&file).unwrap();
        assert_eq!((info.width, info.height, info.depth), (2, 1, 2));
        assert_eq!(info.type_name(), "3D");
        let level0 = info.level0.unwrap();
        assert_eq!((level0.width, level0.height), (4, 1));
        assert_eq!(level0.pixels, pixels);
    }

    #[test]
    fn read_rejects_other_files() {
        assert!(read(b"not a texture at all").is_err());
        assert!(read(&[0; 200]).is_err());
        let mut file =
            encode_rgba8(Target::RGBA, ColorSpace::Linear, Usage::Data, &[sample()]).unwrap();
        let n = file.len();
        file.truncate(n - 4);
        assert!(read(&file).is_err());
    }
}
