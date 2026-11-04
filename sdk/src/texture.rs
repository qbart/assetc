//! `.ktx2` textures as assetc writes them; see docs/spec/ktx2.md.

use crate::read::Bytes;
use crate::Result;

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];

/// The texel formats assetc writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `VK_FORMAT_R8G8B8A8_SRGB` (43): color textures.
    Rgba8Srgb,
    /// `VK_FORMAT_R8G8B8A8_UNORM` (37): linear textures and LUTs.
    Rgba8Unorm,
    /// `VK_FORMAT_R16G16B16A16_SFLOAT` (97): HDR environment cubemaps, IEEE half floats.
    Rgba16Float,
}

impl Format {
    pub fn bytes_per_texel(self) -> usize {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Unorm => 4,
            Self::Rgba16Float => 8,
        }
    }
}

/// One mip level: every face (cubemaps: +X, −X, +Y, −Y, +Z, −Z) or depth slice back to back,
/// rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texture {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    /// `1` for 2D textures and cubemaps; `N` for a volume LUT.
    pub depth: u32,
    /// `6` for cubemaps, else `1`.
    pub faces: u32,
    /// Level 0 (largest) first.
    pub levels: Vec<Level>,
}

impl Texture {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let b = Bytes::new(bytes, "ktx2");
        if b.slice(0, 12)? != IDENTIFIER {
            return Err(b.err("not a KTX2 file"));
        }
        let format = match b.u32(12)? {
            43 => Format::Rgba8Srgb,
            37 => Format::Rgba8Unorm,
            97 => Format::Rgba16Float,
            other => return Err(b.err(format!("unsupported vkFormat {other}"))),
        };
        let (width, height) = (b.u32(20)?, b.u32(24)?);
        let depth = b.u32(28)?.max(1);
        if b.u32(32)? > 1 {
            return Err(b.err("array textures are not supported"));
        }
        let faces = b.u32(36)?;
        if faces != 1 && faces != 6 {
            return Err(b.err(format!("{faces} faces")));
        }
        if b.u32(44)? != 0 {
            return Err(b.err("supercompressed textures are not supported"));
        }
        let level_count = b.u32(40)?.max(1);
        let mut levels = Vec::with_capacity(level_count.min(32) as usize);
        for i in 0..level_count {
            let at = 80 + 24 * i as usize;
            let offset = b.u64(at)? as usize;
            let length = b.u64(at + 8)? as usize;
            let level = |size: u32| (size >> i).max(1);
            let (w, h, d) = (level(width), level(height), level(depth));
            let expected = w as usize * h as usize * d as usize * faces as usize
                * format.bytes_per_texel();
            if length != expected {
                return Err(b.err(format!(
                    "level {i} has {length} bytes, {w} × {h} × {d} needs {expected}"
                )));
            }
            levels.push(Level {
                width: w,
                height: h,
                depth: d,
                data: b.slice(offset, length)?.to_vec(),
            });
        }
        Ok(Self {
            format,
            width,
            height,
            depth,
            faces,
            levels,
        })
    }

    pub fn is_cubemap(&self) -> bool {
        self.faces == 6
    }

    pub fn is_volume(&self) -> bool {
        self.depth > 1
    }

    pub fn is_srgb(&self) -> bool {
        self.format == Format::Rgba8Srgb
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

    /// A one-level `w` × `h` RGBA8 texture: identifier, header, index and level index only.
    fn ktx2(format: u32, w: u32, h: u32, faces: u32, pixels: &[u8]) -> Vec<u8> {
        let mut out = IDENTIFIER.to_vec();
        for v in [format, 1, w, h, 0, 0, faces, 1, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&[0; 32]);
        let at = 80 + 24;
        out.extend_from_slice(&(at as u64).to_le_bytes());
        out.extend_from_slice(&(pixels.len() as u64).to_le_bytes());
        out.extend_from_slice(&(pixels.len() as u64).to_le_bytes());
        out.extend_from_slice(pixels);
        out
    }

    #[test]
    fn reads_levels_and_checks_sizes() {
        let t = Texture::decode(&ktx2(43, 2, 1, 1, &[1, 2, 3, 4, 5, 6, 7, 8])).unwrap();
        assert_eq!((t.format, t.width, t.height, t.depth), (Format::Rgba8Srgb, 2, 1, 1));
        assert_eq!(t.levels[0].data, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(Texture::decode(&ktx2(43, 2, 2, 1, &[0; 8])).is_err());
        assert!(Texture::decode(&ktx2(99, 1, 1, 1, &[0; 4])).is_err());
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0), 0.0);
    }
}
