//! `.ktx2` textures as assetc writes them; see docs/spec/ktx2.md.

use crate::Result;
use crate::read::Bytes;

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

/// One mip level: every layer, each layer's faces (cubemaps: +X, −X, +Y, −Y, +Z, −Z) or depth
/// slices back to back, rows top to bottom.
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
    /// Array layers; `1` for a texture that is not an array (`layerCount` `0`).
    pub layers: u32,
    /// Level 0 (largest) first.
    pub levels: Vec<Level>,
    /// What the texture is for (`noise`, `gradient`, `normal`, `color`, …): the `assetc.usage`
    /// key/value entry, `None` without it; see docs/spec/ktx2.md#keyvalue-data.
    pub usage: Option<String>,
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
        let layers = b.u32(32)?.max(1);
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
            let expected = [w, h, d, faces, layers]
                .into_iter()
                .try_fold(format.bytes_per_texel(), |n, v| n.checked_mul(v as usize))
                .ok_or_else(|| b.err(format!("level {i} is too big")))?;
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
        let kvd = b.slice(b.u32(56)? as usize, b.u32(60)? as usize)?;
        Ok(Self {
            format,
            width,
            height,
            depth,
            faces,
            layers,
            levels,
            usage: key_value(Bytes::new(kvd, "ktx2"), "assetc.usage")?,
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

/// The value of `key` in key/value data `kvd`: entries of `keyAndValueByteLength` u32, then
/// `key\0value\0`, each padded to a multiple of 4. A value's trailing NUL is dropped.
fn key_value(kvd: Bytes<'_>, key: &str) -> Result<Option<String>> {
    let mut at = 0;
    while at < kvd.bytes.len() {
        let len = kvd.u32(at)? as usize;
        let entry = kvd.slice(at + 4, len)?;
        let split = entry.iter().position(|&c| c == 0).unwrap_or(entry.len());
        if &entry[..split] == key.as_bytes() {
            let value = entry.get(split + 1..).unwrap_or_default();
            let value = value.strip_suffix(&[0]).unwrap_or(value);
            return std::str::from_utf8(value)
                .map(|v| Some(v.to_owned()))
                .map_err(|_| kvd.err(format!("{key} is not UTF-8")));
        }
        at = (at + 4 + len).next_multiple_of(4);
    }
    Ok(None)
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

    /// A one-level `w` × `h` RGBA8 texture: identifier, header, index, level index, key/value
    /// data `kvd` (no descriptor).
    fn ktx2_with(format: u32, w: u32, h: u32, layers: u32, kvd: &[u8], pixels: &[u8]) -> Vec<u8> {
        let mut out = IDENTIFIER.to_vec();
        for v in [format, 1, w, h, 0, layers, 1, 1, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let kvd_at = 80 + 24;
        for v in [0, 0, kvd_at, kvd.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&[0; 16]);
        let at = kvd_at as usize + kvd.len();
        out.extend_from_slice(&(at as u64).to_le_bytes());
        out.extend_from_slice(&(pixels.len() as u64).to_le_bytes());
        out.extend_from_slice(&(pixels.len() as u64).to_le_bytes());
        out.extend_from_slice(kvd);
        out.extend_from_slice(pixels);
        out
    }

    fn ktx2(format: u32, w: u32, h: u32, pixels: &[u8]) -> Vec<u8> {
        ktx2_with(format, w, h, 0, &[], pixels)
    }

    /// Key/value data with `entries`, each padded to a multiple of 4.
    fn kvd(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (key, value) in entries {
            let len = key.len() + 1 + value.len() + 1;
            out.extend_from_slice(&(len as u32).to_le_bytes());
            out.extend_from_slice(format!("{key}\0{value}\0").as_bytes());
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
        }
        out
    }

    #[test]
    fn reads_levels_and_checks_sizes() {
        let t = Texture::decode(&ktx2(43, 2, 1, &[1, 2, 3, 4, 5, 6, 7, 8])).unwrap();
        assert_eq!(
            (t.format, t.width, t.height, t.depth, t.layers),
            (Format::Rgba8Srgb, 2, 1, 1, 1)
        );
        assert_eq!(t.levels[0].data, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(t.usage, None);
        assert!(Texture::decode(&ktx2(43, 2, 2, &[0; 8])).is_err());
        assert!(Texture::decode(&ktx2(99, 1, 1, &[0; 4])).is_err());
    }

    #[test]
    fn reads_layers_and_usage() {
        let data = kvd(&[("KTXwriter", "assetc 0.1.0"), ("assetc.usage", "gradient")]);
        let t = Texture::decode(&ktx2_with(43, 1, 1, 2, &data, &[0; 8])).unwrap();
        assert_eq!((t.layers, t.usage.as_deref()), (2, Some("gradient")));
        assert!(Texture::decode(&ktx2_with(43, 1, 1, 2, &data, &[0; 4])).is_err());
        let other = kvd(&[("KTXwriter", "x"), ("zz", "y")]);
        let t = Texture::decode(&ktx2_with(37, 1, 1, 0, &other, &[0; 4])).unwrap();
        assert_eq!(t.usage, None);
        // An entry running past the key/value data.
        let mut bad = kvd(&[("assetc.usage", "data")]);
        bad[0] = 200;
        assert!(Texture::decode(&ktx2_with(37, 1, 1, 0, &bad, &[0; 4])).is_err());
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xc000), -2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0), 0.0);
    }
}
