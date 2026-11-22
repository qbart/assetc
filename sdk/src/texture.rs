//! `.atex.bin` textures: every mip level in the GPU format of the build's target, ready to
//! upload; see docs/spec/atex.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::read::Bytes;

const FILE: &str = "atex.bin";

/// The texel formats assetc writes; see docs/spec/atex.bin.md#formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `VK_FORMAT_R8G8B8A8_SRGB` (43): color textures, format `rgba`; lookup textures.
    Rgba8Srgb,
    /// `VK_FORMAT_R8G8B8A8_UNORM` (37): linear textures, format `rgba`; LUTs.
    Rgba8Unorm,
    /// `VK_FORMAT_R8G8B8_SRGB` (29): color textures, format `rgb`.
    Rgb8Srgb,
    /// `VK_FORMAT_R8G8B8_UNORM` (23): linear textures, format `rgb`.
    Rgb8Unorm,
    /// `VK_FORMAT_R16G16B16A16_SFLOAT` (97): HDR environment cubemaps, IEEE half floats.
    Rgba16Float,
    /// `VK_FORMAT_R16G16B16_SFLOAT` (90): HDR environment cubemaps, format `rgb`.
    Rgb16Float,
    /// `VK_FORMAT_R8_UNORM` (9): texture paints with one channel.
    R8Unorm,
    /// `VK_FORMAT_R8G8_UNORM` (16): texture paints with two channels.
    Rg8Unorm,
    /// `VK_FORMAT_BC7_SRGB_BLOCK` (146): color textures, format `bc`.
    Bc7Srgb,
    /// `VK_FORMAT_BC7_UNORM_BLOCK` (145): linear textures, format `bc`.
    Bc7Unorm,
    /// `VK_FORMAT_BC4_UNORM_BLOCK` (139): one-channel texture paints, format `bc`.
    Bc4Unorm,
    /// `VK_FORMAT_BC5_UNORM_BLOCK` (141): two-channel texture paints, format `bc`.
    Bc5Unorm,
    /// `VK_FORMAT_BC6H_UFLOAT_BLOCK` (143): HDR cubemaps, format `bc`.
    Bc6hUfloat,
    /// `VK_FORMAT_ASTC_4x4_SRGB_BLOCK` (158): color textures, format `astc`.
    Astc4x4Srgb,
    /// `VK_FORMAT_ASTC_4x4_UNORM_BLOCK` (157): linear textures, format `astc`.
    Astc4x4Unorm,
    /// `VK_FORMAT_ASTC_4x4_SFLOAT_BLOCK` (1000066000): HDR cubemaps, format `astc`.
    Astc4x4Float,
}

impl Format {
    pub const ALL: [Self; 16] = [
        Self::Rgba8Srgb,
        Self::Rgba8Unorm,
        Self::Rgb8Srgb,
        Self::Rgb8Unorm,
        Self::Rgba16Float,
        Self::Rgb16Float,
        Self::R8Unorm,
        Self::Rg8Unorm,
        Self::Bc7Srgb,
        Self::Bc7Unorm,
        Self::Bc4Unorm,
        Self::Bc5Unorm,
        Self::Bc6hUfloat,
        Self::Astc4x4Srgb,
        Self::Astc4x4Unorm,
        Self::Astc4x4Float,
    ];

    /// Its `VkFormat` value, as the file stores it.
    pub fn vk_format(self) -> u32 {
        match self {
            Self::Rgba8Srgb => 43,
            Self::Rgba8Unorm => 37,
            Self::Rgb8Srgb => 29,
            Self::Rgb8Unorm => 23,
            Self::Rgba16Float => 97,
            Self::Rgb16Float => 90,
            Self::R8Unorm => 9,
            Self::Rg8Unorm => 16,
            Self::Bc7Srgb => 146,
            Self::Bc7Unorm => 145,
            Self::Bc4Unorm => 139,
            Self::Bc5Unorm => 141,
            Self::Bc6hUfloat => 143,
            Self::Astc4x4Srgb => 158,
            Self::Astc4x4Unorm => 157,
            Self::Astc4x4Float => 1_000_066_000,
        }
    }

    /// The format of a `VkFormat` value, if assetc writes it.
    pub fn from_vk_format(value: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.vk_format() == value)
    }

    /// Texels across a block (`1` for the uncompressed formats, `4` for the block formats) and
    /// the bytes of one block.
    pub fn block(self) -> (u32, usize) {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Unorm => (1, 4),
            Self::Rgb8Srgb | Self::Rgb8Unorm => (1, 3),
            Self::Rgba16Float => (1, 8),
            Self::Rgb16Float => (1, 6),
            Self::R8Unorm => (1, 1),
            Self::Rg8Unorm => (1, 2),
            Self::Bc4Unorm => (4, 8),
            _ => (4, 16),
        }
    }

    /// Bytes of one texel, or of one block for the block formats.
    pub fn bytes_per_texel(self) -> usize {
        self.block().1
    }

    /// Whether the GPU reads 4 × 4 blocks rather than texels.
    pub fn is_block(self) -> bool {
        self.block().0 > 1
    }

    /// Whether the color channels are sRGB-encoded.
    pub fn is_srgb(self) -> bool {
        matches!(
            self,
            Self::Rgba8Srgb | Self::Rgb8Srgb | Self::Bc7Srgb | Self::Astc4x4Srgb
        )
    }
}

/// What a texture is for; see docs/spec/atex.bin.md#usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Usage {
    /// Any other sRGB texture (images, base color, emissive).
    Color,
    /// Any other linear texture; also a value this version does not know.
    #[default]
    Data,
    Normal,
    /// Packed occlusion, roughness, metallic.
    Orm,
    Height,
    Noise,
    Gradient,
    Palette,
    Atlas,
    /// A font's signed distance field atlas.
    Font,
    /// A color LUT, strip or volume.
    Lut,
    /// An HDR environment map's skybox cubemap.
    Environment,
    /// An HDR environment map's specular map: level `i` of `n` prefiltered for roughness
    /// `i / (n − 1)`, see [`Texture::specular_lod`].
    EnvironmentSpecular,
    /// An HDR environment map's irradiance map (irradiance divided by π), see
    /// [`Texture::sh9`].
    EnvironmentIrradiance,
}

impl Usage {
    /// In the order of their values.
    pub const ALL: [Self; 14] = [
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
        Self::EnvironmentSpecular,
        Self::EnvironmentIrradiance,
    ];

    /// Its value in the file, or [`Usage::Data`] for one this version does not know.
    pub fn from_value(value: u32) -> Self {
        Self::ALL.get(value as usize).copied().unwrap_or(Self::Data)
    }

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
            Self::EnvironmentSpecular => "environment_specular",
            Self::EnvironmentIrradiance => "environment_irradiance",
        }
    }
}

/// No channel, in [`Texture::swizzle`].
pub const NO_CHANNEL: u8 = 255;

/// One mip level: every layer, each layer's faces (cubemaps: +X, −X, +Y, −Y, +Z, −Z) or depth
/// slices back to back, rows (of texels or 4 × 4 blocks) top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    /// `1` for 2D textures and cubemaps; `N` for a volume LUT.
    pub depth: u32,
    /// `6` for cubemaps, else `1`.
    pub faces: u32,
    /// Array layers; `1` for a texture that is not an array.
    pub layers: u32,
    /// Level 0 (largest) first.
    pub levels: Vec<Level>,
    /// What the texture is for.
    pub usage: Usage,
    /// Whether the color channels are sRGB-encoded (the `SRGB` flag).
    pub srgb: bool,
    /// For the texture's channel R, G, B, A: the component of a sampled value that holds it
    /// (`0`–`3` for R–A), [`NO_CHANNEL`] past its channels; see
    /// docs/spec/atex.bin.md#swizzle.
    pub swizzle: [u8; 4],
    /// An irradiance map's 9 spherical harmonics coefficients, each `[r, g, b]`, in the order
    /// `(l, m)` = `(0, 0)`, `(1, −1)`, `(1, 0)`, `(1, 1)`, `(2, −2)` … `(2, 2)`; `None` in
    /// other textures. See docs/spec/atex.bin.md#sh9 and [`Texture::irradiance`].
    pub sh9: Option<[[f32; 3]; 9]>,
}

impl Texture {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ATEX", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let desc = require("DESC")?;
        let value = desc.u32(0)?;
        let format = Format::from_vk_format(value)
            .ok_or_else(|| desc.err(format!("unsupported VkFormat {value}")))?;
        let (width, height, depth) = (desc.u32(4)?, desc.u32(8)?, desc.u32(12)?);
        let (faces, layers, level_count) = (desc.u32(16)?, desc.u32(20)?, desc.u32(24)?);
        if width == 0 || height == 0 || depth == 0 || layers == 0 || level_count == 0 {
            return Err(desc.err("a size or count is 0"));
        }
        if faces != 1 && faces != 6 {
            return Err(desc.err(format!("{faces} faces")));
        }
        let usage = Usage::from_value(desc.u32(28)?);
        let srgb = desc.u32(32)? & 1 == 1;
        let swizzle = desc.array::<4>(36)?;
        let sh9 = match file.chunk("SH9") {
            None => None,
            Some(chunk) if chunk.len() == 27 * 4 => {
                let floats = Bytes::new(chunk, FILE).f32s::<27>(0)?;
                Some(std::array::from_fn(|i| {
                    [floats[3 * i], floats[3 * i + 1], floats[3 * i + 2]]
                }))
            }
            Some(chunk) => {
                return Err(
                    Bytes::new(chunk, FILE).err(format!("SH9 has {} bytes, not 108", chunk.len()))
                );
            }
        };
        let data = require("DATA")?;
        let levels = require("LEVELS")?
            .records_n(32, level_count as usize)?
            .enumerate()
            .map(|(i, r)| {
                let (offset, length) = (r.u64(0)?, r.u64(8)?);
                let (w, h, d) = (r.u32(16)?, r.u32(20)?, r.u32(24)?);
                let size = |s: u32| (s >> i).max(1);
                if (w, h, d) != (size(width), size(height), size(depth)) {
                    return Err(r.err(format!("level {i} is {w} × {h} × {d}")));
                }
                let (block, bytes) = format.block();
                let expected = [w.div_ceil(block), h.div_ceil(block), d, faces, layers]
                    .into_iter()
                    .try_fold(bytes, |n, v| n.checked_mul(v as usize))
                    .ok_or_else(|| r.err(format!("level {i} is too big")))?;
                if length != expected as u64 {
                    return Err(r.err(format!(
                        "level {i} has {length} bytes, {w} × {h} × {d} needs {expected}"
                    )));
                }
                let at = usize::try_from(offset).map_err(|_| r.err("offset too big"))?;
                Ok(Level {
                    width: w,
                    height: h,
                    depth: d,
                    data: data.slice(at, expected)?.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            format,
            width,
            height,
            depth,
            faces,
            layers,
            levels,
            usage,
            srgb,
            swizzle,
            sh9,
        })
    }

    /// The level of a specular map ([`Usage::EnvironmentSpecular`]) holding perceptual
    /// `roughness` (0–1): `roughness × (levels − 1)`, to sample at.
    pub fn specular_lod(&self, roughness: f32) -> f32 {
        roughness.clamp(0.0, 1.0) * (self.levels.len() - 1) as f32
    }

    /// An irradiance map's light along unit `direction` (`+Y` up), from its [`Texture::sh9`]:
    /// the irradiance divided by π, so diffuse ambient light is `albedo × irradiance`.
    pub fn irradiance(&self, direction: [f32; 3]) -> Option<[f32; 3]> {
        let sh = self.sh9.as_ref()?;
        let [x, y, z] = direction;
        let basis = [
            0.282_095,
            0.488_603 * y,
            0.488_603 * z,
            0.488_603 * x,
            1.092_548 * x * y,
            1.092_548 * y * z,
            0.315_392 * (3.0 * z * z - 1.0),
            1.092_548 * x * z,
            0.546_274 * (x * x - y * y),
        ];
        Some(std::array::from_fn(|c| {
            (0..9).map(|i| sh[i][c] * basis[i]).sum()
        }))
    }

    pub fn is_cubemap(&self) -> bool {
        self.faces == 6
    }

    pub fn is_volume(&self) -> bool {
        self.depth > 1
    }

    pub fn is_srgb(&self) -> bool {
        self.srgb
    }

    /// How many channels the texture has: 1 (R), 2 (RG), 3 (RGB) or 4 (RGBA).
    pub fn channels(&self) -> usize {
        self.swizzle
            .iter()
            .take_while(|&&c| c != NO_CHANNEL)
            .count()
    }
}

/// An IEEE half float (as in [`Format::Rgba16Float`]) as `f32`.
pub fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = f32::from(bits & 0x3ff);
    sign * match exponent {
        0 => mantissa * 2f32.powi(-24),
        31 if mantissa == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        e => (1.0 + mantissa / 1024.0) * 2f32.powi(i32::from(e) - 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;

    /// A texture of one or two levels: `levels` are each level's (w, h, bytes).
    fn atex(format: u32, swizzle: [u8; 4], usage: u32, levels: &[(u32, u32, &[u8])]) -> Vec<u8> {
        atex_with(format, swizzle, usage, levels, &[])
    }

    /// [`atex`] with more chunks after `DATA`.
    fn atex_with(
        format: u32,
        swizzle: [u8; 4],
        usage: u32,
        levels: &[(u32, u32, &[u8])],
        extra: &[(&str, &[u8])],
    ) -> Vec<u8> {
        let (w, h) = (levels[0].0, levels[0].1);
        let mut desc = Vec::new();
        for v in [format, w, h, 1, 1, 1, levels.len() as u32, usage, 1] {
            desc.extend(v.to_le_bytes());
        }
        desc.extend(swizzle);
        desc.extend([4, 4, 1, 0]);
        desc.extend(16u32.to_le_bytes());
        desc.extend([0; 16]);
        let (mut table, mut data) = (Vec::new(), Vec::new());
        for (lw, lh, bytes) in levels {
            table.extend((data.len() as u64).to_le_bytes());
            table.extend((bytes.len() as u64).to_le_bytes());
            for v in [*lw, *lh, 1, 0] {
                table.extend(v.to_le_bytes());
            }
            data.extend_from_slice(bytes);
            data.resize(data.len().next_multiple_of(16), 0);
        }
        let mut chunks = vec![("DESC", &desc[..]), ("LEVELS", &table), ("DATA", &data)];
        chunks.extend_from_slice(extra);
        file("ATEX", 1, &chunks)
    }

    #[test]
    fn reads_levels_and_checks_sizes() {
        let t = Texture::decode(&atex(
            43,
            [0, 1, 2, 3],
            0,
            &[(2, 1, &[1, 2, 3, 4, 5, 6, 7, 8]), (1, 1, &[9, 9, 9, 9])],
        ))
        .unwrap();
        assert_eq!(
            (t.format, t.width, t.height, t.depth, t.layers, t.faces),
            (Format::Rgba8Srgb, 2, 1, 1, 1, 1)
        );
        assert_eq!(t.levels[0].data, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(t.levels[1].data, [9, 9, 9, 9]);
        assert_eq!((t.usage, t.channels()), (Usage::Color, 4));
        assert!(t.is_srgb());
        // A level too short for its size, a level of the wrong size, an unknown format.
        assert!(Texture::decode(&atex(43, [0; 4], 0, &[(2, 2, &[0; 8])])).is_err());
        assert!(
            Texture::decode(&atex(43, [0; 4], 0, &[(2, 1, &[0; 8]), (2, 1, &[0; 8])])).is_err()
        );
        assert!(Texture::decode(&atex(99, [0; 4], 0, &[(1, 1, &[0; 4])])).is_err());
    }

    #[test]
    fn block_formats_count_blocks() {
        // 5 × 3 texels are 2 × 1 blocks.
        let bc7 = Texture::decode(&atex(146, [0, 1, 2, 3], 0, &[(5, 3, &[0; 32])])).unwrap();
        assert_eq!(bc7.format, Format::Bc7Srgb);
        assert!(bc7.format.is_block());
        assert!(Texture::decode(&atex(146, [0; 4], 0, &[(5, 3, &[0; 16])])).is_err());
        let rg = [0, 3, NO_CHANNEL, NO_CHANNEL];
        let astc = Texture::decode(&atex(157, rg, 1, &[(1, 1, &[0; 16])])).unwrap();
        assert_eq!(
            (astc.format, astc.channels(), astc.usage),
            (Format::Astc4x4Unorm, 2, Usage::Data)
        );
        let bc4 = Texture::decode(&atex(139, [0, 255, 255, 255], 1, &[(4, 4, &[0; 8])])).unwrap();
        assert_eq!((bc4.format, bc4.channels()), (Format::Bc4Unorm, 1));
    }

    #[test]
    fn formats_and_usages_by_value() {
        for f in Format::ALL {
            assert_eq!(Format::from_vk_format(f.vk_format()), Some(f));
        }
        assert_eq!(Format::from_vk_format(0), None);
        assert_eq!(Usage::from_value(11), Usage::Environment);
        assert_eq!(Usage::from_value(12), Usage::EnvironmentSpecular);
        assert_eq!(Usage::from_value(13), Usage::EnvironmentIrradiance);
        assert_eq!(Usage::EnvironmentIrradiance.key(), "environment_irradiance");
        assert_eq!(Usage::from_value(14), Usage::Data);
        assert_eq!(Usage::from_value(99), Usage::Data);
        assert_eq!(Usage::Orm.key(), "orm");
    }

    #[test]
    fn lighting_maps() {
        // A constant irradiance of (0.5, 1, 2): only the first coefficient.
        let mut floats = [0.0f32; 27];
        floats[..3].copy_from_slice(&[0.5, 1.0, 2.0].map(|c| c / 0.282_095));
        let sh: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        let block = [0u8; 16];
        let t = Texture::decode(&atex_with(
            143,
            [0, 1, 2, 255],
            13,
            &[(4, 4, &block)],
            &[("SH9", &sh)],
        ))
        .unwrap();
        assert_eq!(t.usage, Usage::EnvironmentIrradiance);
        let e = t.irradiance([0.0, 1.0, 0.0]).unwrap();
        assert!(
            (e[0] - 0.5).abs() < 1e-6 && (e[2] - 2.0).abs() < 1e-6,
            "{e:?}"
        );
        let bad = atex_with(
            143,
            [0, 1, 2, 255],
            13,
            &[(4, 4, &block)],
            &[("SH9", &sh[..8])],
        );
        assert!(Texture::decode(&bad).is_err());

        let levels: [(u32, u32, &[u8]); 3] =
            [(16, 16, &[0; 256]), (8, 8, &[0; 64]), (4, 4, &block)];
        let t = Texture::decode(&atex(143, [0, 1, 2, 255], 12, &levels)).unwrap();
        assert_eq!(t.usage, Usage::EnvironmentSpecular);
        assert_eq!(t.sh9, None);
        assert_eq!(t.irradiance([0.0, 1.0, 0.0]), None);
        assert_eq!(
            (
                t.specular_lod(0.0),
                t.specular_lod(0.5),
                t.specular_lod(1.0)
            ),
            (0.0, 1.0, 2.0)
        );
    }

    #[test]
    fn rejects_other_files() {
        assert!(Texture::decode(b"\xABKTX 20\xBB\r\n\x1A\n").is_err());
        assert!(Texture::decode(&file("AMESH", 1, &[])).is_err());
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0), 0.0);
    }
}
