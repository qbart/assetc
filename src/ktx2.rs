//! KTX2 writing and reading through ktx-rs, in the formats of docs/spec/ktx2.md.

use ktx::basisu::gpu_texture::unpack_bc7;
use ktx::dfd::khr_df::{
    KHR_DF_MODEL_UASTC_HDR_4x4, KHR_DF_MODEL_UASTC_LDR_4x4, KHR_DF_TRANSFER_SRGB,
};
use ktx::encode_params::{
    AstcParams, BasisParams, KTX_ASTC_PARAMS_SIZE, KTX_BASIS_CODEC_UASTC_HDR_4X4,
    KTX_BASIS_CODEC_UASTC_LDR_4X4, KTX_BASIS_PARAMS_SIZE, KTX_PACK_ASTC_BLOCK_DIMENSION_4X4,
    KTX_PACK_ASTC_ENCODER_MODE_LDR, KTX_PACK_ASTC_QUALITY_LEVEL_MEDIUM,
    KTX_PACK_UASTC_LEVEL_DEFAULT,
};
use ktx::texture::KtxTexture;
use ktx::vkformat::vk_format_string;
use ktx::{
    ErrorCode, KTX_FACESLICE_WHOLE_LEVEL, KTX_TEXTURE_CREATE_LOAD_IMAGE_DATA_BIT, SupercmpScheme,
    Texture2, TextureCreateInfo, TextureCreateStorage, TranscodeFmt, VkFormat,
};
use serde::Deserialize;

use crate::texture::{ColorSpace, Image};

/// The DFD color models of UASTC LDR and HDR 4 × 4.
const UASTC_LDR: u32 = KHR_DF_MODEL_UASTC_LDR_4x4;
const UASTC_HDR: u32 = KHR_DF_MODEL_UASTC_HDR_4x4;

/// zstd level of `uastc` textures; see docs/features/textures.md#format.
const ZSTD_LEVEL: u32 = 18;

/// The GPU format every texture of a build is written in, `[textures] format`; see
/// docs/features/textures.md#format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Bc,
    Astc,
    Uastc,
    Rgba,
    Rgb,
}

impl Format {
    /// Lookup textures stay `rgba`: block compression would change the texels they are read for.
    pub fn for_usage(self, usage: Usage) -> Self {
        match usage {
            Usage::Gradient | Usage::Palette | Usage::Lut => Self::Rgba,
            _ => self,
        }
    }

    /// The format's word in a build line (`bc7`, `astc 4x4`, `rgba8`, …).
    pub fn token(self, kind: Kind) -> &'static str {
        match (self, kind) {
            (Self::Bc, Kind::Rgba) => "bc7",
            (Self::Bc, Kind::R) => "bc4",
            (Self::Bc, Kind::Rg) => "bc5",
            (Self::Bc, Kind::Hdr) => "bc6h",
            (Self::Astc, Kind::Hdr) => "astc 4x4 hdr",
            (Self::Astc, _) => "astc 4x4",
            (Self::Uastc, Kind::Hdr) => "uastc hdr",
            (Self::Uastc, _) => "uastc",
            (_, Kind::R) => "r8",
            (_, Kind::Rg) => "rg8",
            (Self::Rgba, Kind::Rgba) => "rgba8",
            (Self::Rgb, Kind::Rgba) => "rgb8",
            (Self::Rgba, Kind::Hdr) => "rgba16f",
            (Self::Rgb, Kind::Hdr) => "rgb16f",
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

/// What a texture is for, its `assetc.usage` key; see docs/spec/ktx2.md#keyvalue-data.
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

    /// `color` for sRGB textures, `data` for linear ones.
    pub fn of(color: ColorSpace) -> Self {
        match color {
            ColorSpace::Srgb => Self::Color,
            ColorSpace::Linear => Self::Data,
        }
    }
}

/// Encodes RGBA8 mip levels (largest first, each half the previous) as a KTX2 file.
pub fn encode_rgba8(
    format: Format,
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
    let size = [levels[0].width, levels[0].height, 0];
    let format = format.for_usage(usage);
    encode(format, Kind::Rgba, color, usage, (size, 1), &data)
}

/// Encodes the first `channels` (1 R or 2 RG) of RGBA8 mip levels as a linear KTX2 file; see
/// docs/spec/ktx2.md#one-and-two-channels.
pub fn encode_unorm(
    format: Format,
    channels: usize,
    usage: Usage,
    levels: &[Image],
) -> Result<Vec<u8>, String> {
    assert!(matches!(channels, 1 | 2));
    let data: Vec<Vec<u8>> = levels
        .iter()
        .map(|level| {
            level
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|texel| texel[..channels].iter().copied())
                .collect()
        })
        .collect();
    let data: Vec<&[u8]> = data.iter().map(Vec::as_slice).collect();
    let size = [levels[0].width, levels[0].height, 0];
    let kind = if channels == 1 { Kind::R } else { Kind::Rg };
    encode(format, kind, ColorSpace::Linear, usage, (size, 1), &data)
}

/// Encodes one RGBA8 3D texture level, `size` = width, height, depth; `pixels` are depth
/// slices of rows, top to bottom. Always `rgba`; see docs/spec/ktx2.md#volume.
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
    encode(Format::Rgba, Kind::Rgba, color, usage, (size, 1), &[pixels])
}

/// Encodes an HDR cubemap's levels (largest first); see docs/spec/ktx2.md#cubemaps.
pub fn encode_cubemap(format: Format, levels: &[crate::hdr::Level]) -> Result<Vec<u8>, String> {
    let size = levels[0].size as u32;
    let data: Vec<Vec<u8>> = levels
        .iter()
        .map(|level| {
            let mut bytes = Vec::with_capacity(6 * level.size * level.size * 8);
            for face in &level.faces {
                for texel in face {
                    for c in texel {
                        bytes.extend_from_slice(&half::f16::from_f32(*c).to_le_bytes());
                    }
                }
            }
            bytes
        })
        .collect();
    let data: Vec<&[u8]> = data.iter().map(Vec::as_slice).collect();
    let usage = Usage::Environment;
    encode(
        format,
        Kind::Hdr,
        ColorSpace::Linear,
        usage,
        ([size, size, 0], 6),
        &data,
    )
}

fn error(e: ErrorCode) -> String {
    format!("KTX2: {}", ktx::ktx_error_string(e))
}

fn threads() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
}

/// `levels` hold each level's faces (or depth slices) back to back, in `kind`'s layout; `size` is
/// level 0's width, height and depth (0 for 2D).
fn encode(
    format: Format,
    kind: Kind,
    color: ColorSpace,
    usage: Usage,
    (size, faces): ([u32; 3], u32),
    levels: &[&[u8]],
) -> Result<Vec<u8>, String> {
    let srgb = color == ColorSpace::Srgb;
    // `rgb` drops alpha: every 4th byte of RGBA8, every 4th half of RGBA16F.
    let drop_alpha = |bytes: usize| -> Vec<Vec<u8>> {
        levels
            .iter()
            .map(|level| {
                level
                    .chunks_exact(4 * bytes)
                    .flat_map(|texel| texel[..3 * bytes].iter().copied())
                    .collect()
            })
            .collect()
    };
    let (vk_format, rgb) = match (kind, format) {
        (Kind::Rgba, Format::Rgb) if srgb => (VkFormat::R8G8B8_SRGB, Some(drop_alpha(1))),
        (Kind::Rgba, Format::Rgb) => (VkFormat::R8G8B8_UNORM, Some(drop_alpha(1))),
        (Kind::Hdr, Format::Rgb) => (VkFormat::R16G16B16_SFLOAT, Some(drop_alpha(2))),
        (Kind::Rgba, _) if srgb => (VkFormat::R8G8B8A8_SRGB, None),
        (Kind::Rgba, _) => (VkFormat::R8G8B8A8_UNORM, None),
        (Kind::R, _) => (VkFormat::R8_UNORM, None),
        (Kind::Rg, _) => (VkFormat::R8G8_UNORM, None),
        (Kind::Hdr, _) => (VkFormat::R16G16B16A16_SFLOAT, None),
    };
    let [width, height, depth] = size;
    let info = TextureCreateInfo {
        gl_internalformat: 0,
        vk_format,
        dfd: None,
        base_width: width,
        base_height: height,
        base_depth: depth.max(1),
        num_dimensions: if depth > 0 { 3 } else { 2 },
        num_levels: levels.len() as u32,
        num_layers: 1,
        num_faces: faces,
        is_array: false,
        generate_mipmaps: false,
    };
    let mut texture = Texture2::create(&info, TextureCreateStorage::AllocStorage).map_err(error)?;
    for (i, level) in levels.iter().enumerate() {
        let level = rgb.as_ref().map_or(*level, |rgb| rgb[i].as_slice());
        texture
            .set_image_from_memory(i as u32, 0, KTX_FACESLICE_WHOLE_LEVEL, level)
            .map_err(error)?;
    }
    let writer = format!("assetc {}\0", env!("CARGO_PKG_VERSION"));
    let kv = &mut texture.kv_data_head;
    kv.add_kv_pair("KTXwriter", writer.as_bytes())
        .map_err(error)?;
    kv.add_kv_pair(USAGE_KEY, format!("{}\0", usage.key()).as_bytes())
        .map_err(error)?;

    let hdr = kind == Kind::Hdr;
    match format {
        Format::Rgba | Format::Rgb => {}
        Format::Astc if hdr => {
            uastc(&mut texture, kind)?;
            texture
                .transcode_basis(TranscodeFmt::ASTC_HDR_4x4_RGBA, 0)
                .map_err(error)?;
        }
        Format::Astc => {
            let params = AstcParams {
                struct_size: KTX_ASTC_PARAMS_SIZE,
                thread_count: threads(),
                block_dimension: KTX_PACK_ASTC_BLOCK_DIMENSION_4X4,
                mode: KTX_PACK_ASTC_ENCODER_MODE_LDR,
                quality_level: KTX_PACK_ASTC_QUALITY_LEVEL_MEDIUM,
                ..Default::default()
            };
            texture.compress_astc_ex(&params).map_err(error)?;
        }
        Format::Bc => {
            uastc(&mut texture, kind)?;
            let target = match kind {
                Kind::Rgba => TranscodeFmt::BC7_RGBA,
                Kind::R => TranscodeFmt::BC4_R,
                Kind::Rg => TranscodeFmt::BC5_RG,
                Kind::Hdr => TranscodeFmt::BC6HU_RGB,
            };
            texture.transcode_basis(target, 0).map_err(error)?;
        }
        Format::Uastc => {
            uastc(&mut texture, kind)?;
            texture.deflate_zstd(ZSTD_LEVEL).map_err(error)?;
        }
    }
    texture.write_to_memory().map_err(error)
}

/// Encodes to UASTC 4 × 4 (HDR for half floats); RG as `R R R G`, so that BC5 (R and alpha of
/// UASTC) gets R and G.
fn uastc(texture: &mut Texture2, kind: Kind) -> Result<(), String> {
    let params = BasisParams {
        struct_size: KTX_BASIS_PARAMS_SIZE,
        thread_count: threads(),
        codec: match kind {
            Kind::Hdr => KTX_BASIS_CODEC_UASTC_HDR_4X4,
            _ => KTX_BASIS_CODEC_UASTC_LDR_4X4,
        },
        uastc_flags: KTX_PACK_UASTC_LEVEL_DEFAULT,
        uastc_hdr_level: 2,
        uastc_hdr_quality: 1,
        input_swizzle: if kind == Kind::Rg { *b"rrrg" } else { [0; 4] },
        ..Default::default()
    };
    texture.compress_basis_ex(&params).map_err(error)
}

/// What the Inspector shows of a `.ktx2` file.
#[derive(Debug, PartialEq, Eq)]
pub struct Info {
    pub vk_format: u32,
    pub width: u32,
    pub height: u32,
    /// 0 for 2D textures.
    pub depth: u32,
    pub levels: u32,
    /// `6` for a cubemap.
    pub faces: u32,
    /// `layerCount`: 0 when not an array.
    pub layers: u32,
    /// Whether the data format descriptor's transfer function is sRGB.
    pub srgb: bool,
    /// `KTXwriter` and `assetc.usage`, when there.
    pub writer: Option<String>,
    pub usage: Option<String>,
    /// Level 0 as 8-bit RGBA, for the formats the preview decodes; a 3D texture's slices side
    /// by side.
    pub level0: Option<Image>,
    /// Levels 1 and up, decoded like level 0 (empty when level 0 is not).
    pub mips: Vec<Image>,
    format: String,
}

impl Info {
    /// What the container holds: `2D`, `Cubemap`, `3D` or an array of either.
    pub fn type_name(&self) -> String {
        let base = match (self.faces, self.depth) {
            (6, _) => "Cubemap",
            (_, d) if d > 0 => "3D",
            _ => "2D",
        };
        match self.layers {
            0 => base.to_owned(),
            n => format!("{base} array of {n}"),
        }
    }

    /// The `vkFormat` name without `VK_FORMAT_`, or `UASTC (zstd)` and the like.
    pub fn format_name(&self) -> String {
        self.format.clone()
    }
}

/// The DFD's color model (the low byte of its fourth word).
fn color_model(texture: &Texture2) -> u32 {
    texture.p_dfd.get(3).map_or(0, |w| w & 0xFF)
}

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

pub fn read(bytes: &[u8]) -> Result<Info, String> {
    if !bytes.starts_with(&IDENTIFIER) {
        return Err("not a KTX2 file".into());
    }
    let mut texture = Texture2::create_from_memory(bytes, KTX_TEXTURE_CREATE_LOAD_IMAGE_DATA_BIT)
        .map_err(error)?;
    let model = color_model(&texture);
    // Loading inflates the levels, so the scheme comes from the header.
    let scheme = bytes
        .get(44..48)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let zstd = scheme == Some(SupercmpScheme::ZSTD.0);
    let compression = if zstd { " (zstd)" } else { "" };
    let format = match model {
        UASTC_LDR => format!("UASTC{compression}"),
        UASTC_HDR => format!("UASTC HDR 4x4{compression}"),
        _ => match vk_format_string(texture.vk_format).strip_prefix("VK_FORMAT_") {
            Some(name) => format!("{name}{compression}"),
            None => format!("vkFormat {}{compression}", texture.vk_format.0),
        },
    };
    let value = |texture: &Texture2, key: &str| {
        let value = texture.kv_data_head.find_value(key).ok()?;
        let value = value.strip_suffix(&[0]).unwrap_or(value);
        Some(String::from_utf8_lossy(value).into_owned())
    };
    let writer = value(&texture, "KTXwriter");
    let usage = value(&texture, USAGE_KEY);
    let srgb = texture.get_transfer_function_e() == KHR_DF_TRANSFER_SRGB;
    let (width, height) = (texture.base_width, texture.base_height);
    let depth = if texture.num_dimensions == 3 {
        texture.base_depth
    } else {
        0
    };
    let (faces, levels) = (texture.num_faces, texture.num_levels);
    let layers = if texture.is_array {
        texture.num_layers
    } else {
        0
    };
    let vk_format = texture.vk_format.0;
    let mut images = if faces == 1 && layers == 0 {
        decode(&mut texture, model).unwrap_or_default()
    } else {
        Vec::new()
    };
    let level0 = (!images.is_empty()).then(|| images.remove(0));
    // Volumes show level 0 only.
    let mips = if depth == 0 { images } else { Vec::new() };
    Ok(Info {
        vk_format,
        width,
        height,
        depth,
        levels: levels.max(1),
        faces: faces.max(1),
        layers,
        srgb,
        writer,
        usage,
        level0,
        mips,
        format,
    })
}

/// How the preview reads a level's data.
enum Texels {
    /// Uncompressed, this many 8-bit channels.
    Plain(usize),
    Bc7,
    Bc4,
    Bc5,
}

/// Every level as 8-bit RGBA, a 3D level's slices side by side; `None` for formats the preview
/// does not decode (half floats, BC6H, ASTC HDR).
fn decode(texture: &mut Texture2, model: u32) -> Option<Vec<Image>> {
    let texels = match texture.vk_format {
        VkFormat::R8_UNORM => Texels::Plain(1),
        VkFormat::R8G8_UNORM => Texels::Plain(2),
        VkFormat::R8G8B8_UNORM | VkFormat::R8G8B8_SRGB => Texels::Plain(3),
        VkFormat::R8G8B8A8_UNORM | VkFormat::R8G8B8A8_SRGB => Texels::Plain(4),
        VkFormat::BC7_UNORM_BLOCK | VkFormat::BC7_SRGB_BLOCK => Texels::Bc7,
        VkFormat::BC4_UNORM_BLOCK => Texels::Bc4,
        VkFormat::BC5_UNORM_BLOCK => Texels::Bc5,
        VkFormat::ASTC_4x4_UNORM_BLOCK | VkFormat::ASTC_4x4_SRGB_BLOCK => {
            texture.decode_astc().ok()?;
            Texels::Plain(4)
        }
        VkFormat::UNDEFINED if model == UASTC_LDR => {
            texture.transcode_basis(TranscodeFmt::RGBA32, 0).ok()?;
            Texels::Plain(4)
        }
        _ => return None,
    };
    let data = texture.get_data()?;
    (0..texture.num_levels)
        .map(|i| {
            let (w, h, d) = (
                (texture.base_width >> i).max(1) as usize,
                (texture.base_height >> i).max(1) as usize,
                if texture.num_dimensions == 3 {
                    (texture.base_depth >> i).max(1) as usize
                } else {
                    1
                },
            );
            let offset = texture.get_image_offset(i, 0, 0).ok()?;
            let length = texture.get_level_size(i);
            let level = data.get(offset..offset.checked_add(length)?)?;
            let pixels = match texels {
                Texels::Plain(n) => plain(level, n, (w, h, d))?,
                Texels::Bc7 | Texels::Bc4 | Texels::Bc5 => blocks(level, &texels, (w, h))?,
            };
            Some(Image {
                width: (w * d) as u32,
                height: h as u32,
                pixels,
            })
        })
        .collect()
}

/// Uncompressed texels to RGBA: R shows as gray, RG as red and green, RGB with alpha 255; a 3D
/// level's slices side by side.
fn plain(level: &[u8], n: usize, (w, h, d): (usize, usize, usize)) -> Option<Vec<u8>> {
    let row = w * n;
    if level.len() < row * h * d {
        return None;
    }
    let mut pixels = Vec::with_capacity(w * h * d * 4);
    for y in 0..h {
        for z in 0..d {
            let start = (z * h + y) * row;
            for texel in level[start..start + row].chunks_exact(n) {
                pixels.extend_from_slice(&match *texel {
                    [r] => [r, r, r, 255],
                    [r, g] => [r, g, 0, 255],
                    [r, g, b] => [r, g, b, 255],
                    [r, g, b, a] => [r, g, b, a],
                    _ => return None,
                });
            }
        }
    }
    Some(pixels)
}

/// BC7, BC4 (gray) or BC5 (red and green) 4 × 4 blocks to RGBA.
fn blocks(level: &[u8], texels: &Texels, (w, h): (usize, usize)) -> Option<Vec<u8>> {
    let size = if matches!(texels, Texels::Bc4) { 8 } else { 16 };
    let across = w.div_ceil(4);
    if level.len() < across * h.div_ceil(4) * size {
        return None;
    }
    let mut pixels = vec![0; w * h * 4];
    for (i, block) in level.chunks_exact(size).enumerate() {
        let (bx, by) = (i % across * 4, i / across * 4);
        if by >= h {
            break;
        }
        let texels: [[u8; 4]; 16] = match texels {
            Texels::Bc7 => {
                let mut out = [[0; 4]; 16];
                unpack_bc7(block.try_into().ok()?, &mut out);
                out
            }
            Texels::Bc4 => bc4(block).map(|r| [r, r, r, 255]),
            _ => {
                let (r, g) = (bc4(&block[..8]), bc4(&block[8..]));
                std::array::from_fn(|k| [r[k], g[k], 0, 255])
            }
        };
        for (k, texel) in texels.iter().enumerate() {
            let (x, y) = (bx + k % 4, by + k / 4);
            if x < w && y < h {
                pixels[(y * w + x) * 4..][..4].copy_from_slice(texel);
            }
        }
    }
    Some(pixels)
}

/// The 16 values of a BC4 block (also each half of a BC5 block).
fn bc4(block: &[u8]) -> [u8; 16] {
    let (a0, a1) = (block[0] as u32, block[1] as u32);
    let palette: [u32; 8] = std::array::from_fn(|i| match i as u32 {
        0 => a0,
        1 => a1,
        i if a0 > a1 => ((8 - i) * a0 + (i - 1) * a1) / 7,
        i @ 2..=5 => ((6 - i) * a0 + (i - 1) * a1) / 5,
        6 => 0,
        _ => 255,
    });
    let mut bits = [0; 8];
    bits[..6].copy_from_slice(&block[2..8]);
    let bits = u64::from_le_bytes(bits);
    std::array::from_fn(|k| palette[(bits >> (3 * k) & 7) as usize] as u8)
}

const USAGE_KEY: &str = "assetc.usage";

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

    const ALL: [Format; 5] = [
        Format::Bc,
        Format::Astc,
        Format::Uastc,
        Format::Rgba,
        Format::Rgb,
    ];

    /// `ktx validate` passes, the custom `assetc.usage` key (7010) the only warning.
    fn assert_valid(file: &[u8]) {
        let mut issues = Vec::new();
        let rc = ktx::cli::validate::validate_memory(file, false, false, &mut |r| {
            issues.push((r.id, r.message.clone()))
        })
        .unwrap();
        assert_eq!(rc, 0, "{issues:?}");
        assert!(issues.iter().all(|(id, _)| *id == 7010), "{issues:?}");
    }

    fn close(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 3)
    }

    #[test]
    fn rgba_round_trips_with_keys() {
        let levels = mip_chain(sample(), ColorSpace::Srgb);
        let file = encode_rgba8(Format::Rgba, ColorSpace::Srgb, Usage::Normal, &levels).unwrap();
        assert_valid(&file);
        let info = read(&file).unwrap();
        assert_eq!((info.width, info.height, info.levels), (3, 2, 2));
        assert_eq!(info.format_name(), "R8G8B8A8_SRGB");
        assert_eq!(info.type_name(), "2D");
        assert!(info.srgb);
        assert_eq!(info.level0, Some(levels[0].clone()));
        assert_eq!(info.mips, levels[1..]);
        assert_eq!(info.usage.as_deref(), Some("normal"));
        let writer = info.writer.unwrap();
        assert!(
            writer.starts_with(&format!("assetc {} / libktx ", env!("CARGO_PKG_VERSION"))),
            "{writer}"
        );

        let info = read(
            &encode_rgba8(Format::Rgba, ColorSpace::Linear, Usage::Color, &[sample()]).unwrap(),
        )
        .unwrap();
        assert_eq!(info.format_name(), "R8G8B8A8_UNORM");
        assert!(!info.srgb);
    }

    #[test]
    fn every_format_is_valid_and_previews() {
        let texel = [200, 60, 20, 255];
        for format in ALL {
            for color in [ColorSpace::Srgb, ColorSpace::Linear] {
                let levels = mip_chain(flat(16, texel), color);
                let file = encode_rgba8(format, color, Usage::of(color), &levels).unwrap();
                assert_valid(&file);
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
            read(&encode_rgba8(format, color, Usage::Color, &[flat(8, texel)]).unwrap())
                .unwrap()
                .format_name()
        };
        assert_eq!(name(Format::Bc, ColorSpace::Srgb), "BC7_SRGB_BLOCK");
        assert_eq!(name(Format::Bc, ColorSpace::Linear), "BC7_UNORM_BLOCK");
        assert_eq!(name(Format::Astc, ColorSpace::Srgb), "ASTC_4x4_SRGB_BLOCK");
        assert_eq!(name(Format::Uastc, ColorSpace::Srgb), "UASTC (zstd)");
        assert_eq!(name(Format::Rgb, ColorSpace::Srgb), "R8G8B8_SRGB");
        assert_eq!(name(Format::Rgb, ColorSpace::Linear), "R8G8B8_UNORM");
    }

    #[test]
    fn rgb_drops_alpha() {
        let file = encode_rgba8(Format::Rgb, ColorSpace::Linear, Usage::Data, &[sample()]).unwrap();
        let texture =
            Texture2::create_from_memory(&file, KTX_TEXTURE_CREATE_LOAD_IMAGE_DATA_BIT).unwrap();
        assert_eq!(texture.get_data().unwrap()[..6], [0, 1, 2, 4, 5, 6]);
        assert_eq!(
            read(&file).unwrap().level0.unwrap().pixels[..8],
            [0, 1, 2, 255, 4, 5, 6, 255]
        );
    }

    #[test]
    fn one_and_two_channels_in_every_format() {
        let levels = mip_chain(flat(8, [200, 60, 0, 255]), ColorSpace::Linear);
        for (channels, names) in [
            (
                1,
                [
                    "BC4_UNORM_BLOCK",
                    "ASTC_4x4_UNORM_BLOCK",
                    "UASTC (zstd)",
                    "R8_UNORM",
                    "R8_UNORM",
                ],
            ),
            (
                2,
                [
                    "BC5_UNORM_BLOCK",
                    "ASTC_4x4_UNORM_BLOCK",
                    "UASTC (zstd)",
                    "R8G8_UNORM",
                    "R8G8_UNORM",
                ],
            ),
        ] {
            for (format, name) in ALL.into_iter().zip(names) {
                let file = encode_unorm(format, channels, Usage::Data, &levels).unwrap();
                assert_valid(&file);
                let info = read(&file).unwrap();
                assert_eq!(info.format_name(), name);
                let texel = &info.level0.unwrap().pixels[..4];
                // BC and the plain formats keep R and G apart; ASTC and UASTC hold R R R G.
                let expected = match (channels, format) {
                    (1, _) => [200, 200, 200, 255],
                    (_, Format::Astc | Format::Uastc) => [200, 200, 200, 60],
                    _ => [200, 60, 0, 255],
                };
                assert!(close(texel, &expected), "{format:?} {channels}: {texel:?}");
            }
        }
    }

    #[test]
    fn lookup_textures_stay_rgba() {
        for usage in [Usage::Gradient, Usage::Palette, Usage::Lut] {
            let file = encode_rgba8(Format::Bc, ColorSpace::Srgb, usage, &[sample()]).unwrap();
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
            "UASTC HDR 4x4 (zstd)",
            "R16G16B16A16_SFLOAT",
            "R16G16B16_SFLOAT",
        ]) {
            let file = encode_cubemap(format, &levels).unwrap();
            assert_valid(&file);
            let info = read(&file).unwrap();
            assert_eq!(info.format_name(), name);
            assert_eq!(
                (info.faces, info.levels, info.type_name().as_str()),
                (6, 4, "Cubemap")
            );
            assert_eq!(info.usage.as_deref(), Some("environment"));
            assert_eq!(info.level0, None);
        }
    }

    #[test]
    fn mip_levels_stored_smallest_first() {
        let levels = mip_chain(sample(), ColorSpace::Linear);
        let file = encode_rgba8(Format::Rgba, ColorSpace::Linear, Usage::Color, &levels).unwrap();
        let offset = |i: usize| {
            let at = 80 + i * 24;
            u64::from_le_bytes(file[at..at + 8].try_into().unwrap())
        };
        assert!(offset(1) < offset(0));
        assert_eq!(offset(1) % 4, 0);
        assert_eq!(file.len() as u64, offset(0) + levels[0].pixels.len() as u64);
    }

    #[test]
    fn volumes_are_3d_and_preview_as_slices() {
        // 2 × 1 × 2: texel (x, 0, z) has red x, blue z.
        let pixels = [
            0, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255,
        ];
        let file = encode_rgba8_volume(ColorSpace::Linear, Usage::Lut, [2, 1, 2], &pixels).unwrap();
        assert_valid(&file);
        let info = read(&file).unwrap();
        assert_eq!((info.width, info.height, info.depth), (2, 1, 2));
        assert_eq!(info.type_name(), "3D");
        let level0 = info.level0.unwrap();
        assert_eq!((level0.width, level0.height), (4, 1));
        assert_eq!(level0.pixels, pixels);
    }

    #[test]
    fn read_rejects_non_ktx2() {
        assert!(read(b"not a ktx2 file at all").is_err());
        assert!(read(&[0; 200]).is_err());
    }
}
