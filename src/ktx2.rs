//! KTX2 writer for the subset in docs/spec/ktx2.md, and a header reader for inspecting files.

use crate::texture::{ColorSpace, Image};

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;
const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;
const VK_FORMAT_R16G16B16A16_SFLOAT: u32 = 97;
const LEVEL_INDEX_OFFSET: usize = 80;
const LEVEL_INDEX_ENTRY: usize = 24;
const DFD_SIZE: u32 = 92;

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
pub fn encode_rgba8(color: ColorSpace, usage: Usage, levels: &[Image]) -> Vec<u8> {
    for level in levels {
        assert_eq!(
            level.pixels.len(),
            level.width as usize * level.height as usize * 4
        );
    }
    let data: Vec<&[u8]> = levels.iter().map(|l| l.pixels.as_slice()).collect();
    encode(color, usage, [levels[0].width, levels[0].height, 0], &data)
}

/// Encodes one RGBA8 3D texture level, `size` = width, height, depth; `pixels` are depth
/// slices of rows, top to bottom. See docs/spec/ktx2.md#volume.
pub fn encode_rgba8_volume(
    color: ColorSpace,
    usage: Usage,
    size: [u32; 3],
    pixels: &[u8],
) -> Vec<u8> {
    assert_eq!(
        pixels.len(),
        size.iter().map(|&s| s as usize).product::<usize>() * 4
    );
    encode(color, usage, size, &[pixels])
}

/// `size` is level 0's width, height and depth (0 for 2D).
fn encode(color: ColorSpace, usage: Usage, size: [u32; 3], levels: &[&[u8]]) -> Vec<u8> {
    let format = match color {
        ColorSpace::Srgb => VK_FORMAT_R8G8B8A8_SRGB,
        ColorSpace::Linear => VK_FORMAT_R8G8B8A8_UNORM,
    };
    let kvd = key_value_data(usage);
    let dfd_offset = LEVEL_INDEX_OFFSET + LEVEL_INDEX_ENTRY * levels.len();
    let kvd_offset = dfd_offset + DFD_SIZE as usize;
    let data_offset = (kvd_offset + kvd.len()).next_multiple_of(4);

    // Images are stored smallest level first; the index lists level 0 first.
    let mut offsets = vec![0; levels.len()];
    let mut offset = data_offset;
    for (i, level) in levels.iter().enumerate().rev() {
        offsets[i] = offset;
        offset += level.len();
    }

    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(&IDENTIFIER);
    let level_count = levels.len() as u32;
    let [width, height, depth] = size;
    for v in [format, 1, width, height, depth, 0, 1, level_count, 0] {
        put_u32(&mut out, v);
    }
    put_u32(&mut out, dfd_offset as u32);
    put_u32(&mut out, DFD_SIZE);
    put_u32(&mut out, kvd_offset as u32);
    put_u32(&mut out, kvd.len() as u32);
    put_u64(&mut out, 0);
    put_u64(&mut out, 0);
    debug_assert_eq!(out.len(), LEVEL_INDEX_OFFSET);

    for (level, offset) in levels.iter().zip(&offsets) {
        put_u64(&mut out, *offset as u64);
        put_u64(&mut out, level.len() as u64);
        put_u64(&mut out, level.len() as u64);
    }
    debug_assert_eq!(out.len(), dfd_offset);

    write_dfd(&mut out, color);
    out.extend_from_slice(&kvd);
    out.resize(data_offset, 0);
    for level in levels.iter().rev() {
        out.extend_from_slice(level);
    }
    out
}

fn write_dfd(out: &mut Vec<u8>, color: ColorSpace) {
    // Alpha is flagged linear only when the color channels are sRGB.
    let (transfer, alpha) = match color {
        ColorSpace::Srgb => (2, 0x1F),
        ColorSpace::Linear => (1, 0x0F),
    };
    put_u32(out, DFD_SIZE);
    put_u32(out, 0); // vendorId 0, descriptorType 0
    put_u16(out, 2); // versionNumber
    put_u16(out, 88); // descriptorBlockSize
    out.extend_from_slice(&[1, 1, transfer, 0]); // RGBSDA, BT.709, transfer, straight alpha
    out.extend_from_slice(&[0; 4]); // texelBlockDimension
    out.extend_from_slice(&[4, 0, 0, 0, 0, 0, 0, 0]); // bytesPlane
    for (channel, channel_type) in [0x00u8, 0x01, 0x02, alpha].into_iter().enumerate() {
        put_u16(out, channel as u16 * 8);
        out.push(7); // bitLength - 1
        out.push(channel_type);
        out.extend_from_slice(&[0; 4]); // samplePosition
        put_u32(out, 0);
        put_u32(out, 255);
    }
}

/// Encodes an HDR cubemap's levels (largest first) as half floats; see
/// docs/spec/ktx2.md#cubemaps.
pub fn encode_cubemap_rgba16f(levels: &[crate::hdr::Level]) -> Vec<u8> {
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
    let kvd = key_value_data(Usage::Environment);
    let dfd_offset = LEVEL_INDEX_OFFSET + LEVEL_INDEX_ENTRY * data.len();
    let kvd_offset = dfd_offset + DFD_SIZE as usize;
    let data_offset = (kvd_offset + kvd.len()).next_multiple_of(8);
    let mut offsets = vec![0; data.len()];
    let mut offset = data_offset;
    for (i, level) in data.iter().enumerate().rev() {
        offsets[i] = offset;
        offset += level.len();
    }
    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(&IDENTIFIER);
    for v in [
        VK_FORMAT_R16G16B16A16_SFLOAT,
        2,
        size,
        size,
        0,
        0,
        6,
        data.len() as u32,
        0,
    ] {
        put_u32(&mut out, v);
    }
    put_u32(&mut out, dfd_offset as u32);
    put_u32(&mut out, DFD_SIZE);
    put_u32(&mut out, kvd_offset as u32);
    put_u32(&mut out, kvd.len() as u32);
    put_u64(&mut out, 0);
    put_u64(&mut out, 0);
    for (level, offset) in data.iter().zip(&offsets) {
        put_u64(&mut out, *offset as u64);
        put_u64(&mut out, level.len() as u64);
        put_u64(&mut out, level.len() as u64);
    }
    // The data format descriptor: four signed float halves, linear.
    put_u32(&mut out, DFD_SIZE);
    put_u32(&mut out, 0);
    put_u16(&mut out, 2);
    put_u16(&mut out, 88);
    out.extend_from_slice(&[1, 1, 1, 0]);
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&[8, 0, 0, 0, 0, 0, 0, 0]);
    for (k, channel) in [0x00u8, 0x01, 0x02, 0x0F].into_iter().enumerate() {
        put_u16(&mut out, k as u16 * 16);
        out.push(15);
        out.push(channel | 0x40 | 0x80);
        out.extend_from_slice(&[0; 4]);
        put_u32(&mut out, 0xBF80_0000);
        put_u32(&mut out, 0x3F80_0000);
    }
    out.extend_from_slice(&kvd);
    out.resize(data_offset, 0);
    for level in data.iter().rev() {
        out.extend_from_slice(level);
    }
    out
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
    /// Level 0, only for the subset assetc writes; a 3D texture's slices side by side.
    pub level0: Option<Image>,
    /// Levels 1 and up, decoded like level 0 (empty when level 0 is not).
    pub mips: Vec<Image>,
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

    pub fn format_name(&self) -> String {
        match self.vk_format {
            VK_FORMAT_R8G8B8A8_SRGB => "R8G8B8A8_SRGB".into(),
            VK_FORMAT_R8G8B8A8_UNORM => "R8G8B8A8_UNORM".into(),
            VK_FORMAT_R16G16B16A16_SFLOAT => "R16G16B16A16_SFLOAT".into(),
            n => format!("vkFormat {n}"),
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<Info, String> {
    if bytes.len() < LEVEL_INDEX_OFFSET + LEVEL_INDEX_ENTRY || bytes[..12] != IDENTIFIER {
        return Err("not a KTX2 file".into());
    }
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let [
        vk_format,
        _,
        width,
        height,
        depth,
        layers,
        faces,
        levels,
        supercompression,
    ] = std::array::from_fn(|i| u32_at(12 + i * 4));

    let plain = layers == 0 && faces == 1 && supercompression == 0;
    let rgba8 = matches!(
        vk_format,
        VK_FORMAT_R8G8B8A8_SRGB | VK_FORMAT_R8G8B8A8_UNORM
    );
    // Level `i` is `max(1, size >> i)` on each axis; its entry is in the level index.
    // A 3D level is laid out as its depth slices side by side.
    let level = |i: u32| {
        let entry = LEVEL_INDEX_OFFSET + LEVEL_INDEX_ENTRY * i as usize;
        if entry + LEVEL_INDEX_ENTRY > bytes.len() {
            return None;
        }
        let (offset, length) = (u64_at(entry), u64_at(entry + 8));
        let (w, h, d) = (
            (width >> i).max(1),
            (height >> i).max(1),
            (depth >> i).max(1),
        );
        let end = offset.checked_add(length)?;
        if length != w as u64 * h as u64 * d as u64 * 4 || end > bytes.len() as u64 {
            return None;
        }
        let data = &bytes[offset as usize..end as usize];
        let row = w as usize * 4;
        let mut pixels = Vec::with_capacity(data.len());
        for y in 0..h as usize {
            for z in 0..d as usize {
                let start = (z * h as usize + y) * row;
                pixels.extend_from_slice(&data[start..start + row]);
            }
        }
        Some(Image {
            width: w * d,
            height: h,
            pixels,
        })
    };
    let level0 = (plain && rgba8).then(|| level(0)).flatten();
    // Volumes show level 0 only.
    let mips = if level0.is_some() && depth == 0 {
        (1..levels.max(1)).map_while(level).collect()
    } else {
        Vec::new()
    };
    let dfd = u32_at(48) as usize;
    let srgb = bytes.get(dfd + 14) == Some(&2);
    let pairs = key_values(bytes);
    let value = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    Ok(Info {
        vk_format,
        width,
        height,
        depth,
        levels: levels.max(1),
        faces: faces.max(1),
        layers,
        srgb,
        writer: value("KTXwriter"),
        usage: value(USAGE_KEY),
        level0,
        mips,
    })
}

/// `KTXwriter`, then `assetc.usage`: sorted by key, each padded to 4 bytes.
fn key_value_data(usage: Usage) -> Vec<u8> {
    let writer = format!("assetc {}", env!("CARGO_PKG_VERSION"));
    let mut out = Vec::new();
    for (key, value) in [("KTXwriter", writer.as_str()), (USAGE_KEY, usage.key())] {
        put_u32(&mut out, (key.len() + 1 + value.len() + 1) as u32);
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        out.extend_from_slice(value.as_bytes());
        out.push(0);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}

const USAGE_KEY: &str = "assetc.usage";

/// The key/value pairs of a KTX2 file's key/value data, values without their NUL.
fn key_values(bytes: &[u8]) -> Vec<(String, String)> {
    let u32_at = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
    };
    let (Some(offset), Some(length)) = (u32_at(56), u32_at(60)) else {
        return Vec::new();
    };
    let end = (offset + length).min(bytes.len());
    let mut out = Vec::new();
    let mut at = offset;
    while at + 4 <= end {
        let Some(size) = u32_at(at) else { break };
        let Some(entry) = bytes.get(at + 4..(at + 4 + size).min(end)) else {
            break;
        };
        if let Some(nul) = entry.iter().position(|&b| b == 0) {
            let key = String::from_utf8_lossy(&entry[..nul]).into_owned();
            let value = entry[nul + 1..]
                .strip_suffix(&[0])
                .unwrap_or(&entry[nul + 1..]);
            out.push((key, String::from_utf8_lossy(value).into_owned()));
        }
        at = (at + 4 + size).next_multiple_of(4);
    }
    out
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::mip_chain;
    use ::ktx2::{Format, Reader, dfd};

    fn sample() -> Image {
        let (width, height) = (3, 2);
        let pixels = (0..width * height * 4).map(|i| i as u8).collect();
        Image {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn reader_accepts_header_and_level() {
        let image = sample();
        let file = encode_rgba8(ColorSpace::Srgb, Usage::Color, std::slice::from_ref(&image));
        let reader = Reader::new(&file[..]).unwrap();
        let header = reader.header();
        assert_eq!(header.format, Some(Format::R8G8B8A8_SRGB));
        assert_eq!(header.type_size, 1);
        assert_eq!((header.pixel_width, header.pixel_height), (3, 2));
        assert_eq!(header.pixel_depth, 0);
        assert_eq!(header.layer_count, 0);
        assert_eq!(header.face_count, 1);
        assert_eq!(header.level_count, 1);
        assert_eq!(header.supercompression_scheme, None);

        let levels: Vec<_> = reader.levels().collect();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].data, &image.pixels[..]);
        assert_eq!(
            levels[0].uncompressed_byte_length,
            image.pixels.len() as u64
        );
    }

    #[test]
    fn dfd_matches_reference_for_both_formats() {
        for (color, format) in [
            (ColorSpace::Srgb, Format::R8G8B8A8_SRGB),
            (ColorSpace::Linear, Format::R8G8B8A8_UNORM),
        ] {
            let file = encode_rgba8(color, Usage::Color, &[sample()]);
            let reader = Reader::new(&file[..]).unwrap();
            assert_eq!(reader.header().format, Some(format));
            let (expected, _) = dfd::Basic::from_format(format).unwrap();
            match reader.dfd_blocks() {
                [dfd::Block::Basic(basic)] => assert_eq!(basic, &expected),
                other => panic!("unexpected dfd blocks: {other:?}"),
            }
        }
    }

    #[test]
    fn mip_levels_stored_smallest_first() {
        let levels = mip_chain(sample(), ColorSpace::Linear);
        assert_eq!(levels.len(), 2);
        let file = encode_rgba8(ColorSpace::Linear, Usage::Color, &levels);
        let reader = Reader::new(&file[..]).unwrap();
        assert_eq!(reader.header().level_count, 2);

        let read: Vec<_> = reader.levels().collect();
        assert_eq!(read.len(), 2);
        for (read, level) in read.iter().zip(&levels) {
            assert_eq!(read.data, &level.pixels[..]);
        }
        let offset = |i: usize| {
            let at = LEVEL_INDEX_OFFSET + i * LEVEL_INDEX_ENTRY;
            u64::from_le_bytes(file[at..at + 8].try_into().unwrap())
        };
        assert!(offset(1) < offset(0));
        assert_eq!(offset(1) % 4, 0);
        assert_eq!(file.len() as u64, offset(0) + levels[0].pixels.len() as u64);
    }

    #[test]
    fn read_returns_header_and_level0() {
        let levels = mip_chain(sample(), ColorSpace::Srgb);
        let info = read(&encode_rgba8(ColorSpace::Srgb, Usage::Color, &levels)).unwrap();
        assert_eq!((info.width, info.height, info.levels), (3, 2, 2));
        assert_eq!(info.format_name(), "R8G8B8A8_SRGB");
        assert_eq!(info.level0, Some(levels[0].clone()));
        assert_eq!(info.mips, levels[1..]);

        let info = read(&encode_rgba8(ColorSpace::Linear, Usage::Color, &[sample()])).unwrap();
        assert_eq!(info.format_name(), "R8G8B8A8_UNORM");
    }

    #[test]
    fn volumes_are_3d_and_preview_as_slices() {
        // 2 × 1 × 2: texel (x, 0, z) has red x, blue z.
        let pixels = [
            0, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 255, 255,
        ];
        let file = encode_rgba8_volume(ColorSpace::Linear, Usage::Lut, [2, 1, 2], &pixels);
        let reader = ::ktx2::Reader::new(&file[..]).unwrap();
        let header = reader.header();
        assert_eq!(
            (header.pixel_width, header.pixel_height, header.pixel_depth),
            (2, 1, 2)
        );
        assert_eq!(reader.levels().next().unwrap().data, &pixels[..]);
        let info = read(&file).unwrap();
        assert_eq!(info.depth, 2);
        let level0 = info.level0.unwrap();
        assert_eq!((level0.width, level0.height), (4, 1));
        assert_eq!(level0.pixels, pixels);
    }

    #[test]
    fn read_without_preview_for_other_formats() {
        let mut file = encode_rgba8(ColorSpace::Srgb, Usage::Color, &[sample()]);
        file[12..16].copy_from_slice(&131u32.to_le_bytes()); // BC1
        let info = read(&file).unwrap();
        assert_eq!(info.format_name(), "vkFormat 131");
        assert_eq!(info.level0, None);
    }

    #[test]
    fn read_rejects_non_ktx2() {
        assert!(read(b"not a ktx2 file at all").is_err());
        assert!(read(&[0; 200]).is_err());
    }

    #[test]
    fn writer_key_and_alignment() {
        let image = sample();
        let file = encode_rgba8(
            ColorSpace::Srgb,
            Usage::Normal,
            std::slice::from_ref(&image),
        );
        let reader = Reader::new(&file[..]).unwrap();
        let kv: Vec<_> = reader.key_value_data().collect();
        assert_eq!(kv.len(), 2);
        assert_eq!(kv[0].0, "KTXwriter");
        assert_eq!(
            kv[0].1,
            format!("assetc {}\0", env!("CARGO_PKG_VERSION")).as_bytes()
        );
        assert_eq!(kv[1], ("assetc.usage", &b"normal\0"[..]));
        // Read back with the container's type and color space.
        let info = read(&file).unwrap();
        assert_eq!(info.usage.as_deref(), Some("normal"));
        assert_eq!(
            info.writer,
            Some(format!("assetc {}", env!("CARGO_PKG_VERSION")))
        );
        assert_eq!(info.type_name(), "2D");
        assert!(info.srgb);
        let level_offset = u64::from_le_bytes(file[80..88].try_into().unwrap());
        assert_eq!(level_offset % 4, 0);
        assert_eq!(file.len() as u64, level_offset + image.pixels.len() as u64);
    }
}
