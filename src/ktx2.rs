//! KTX2 writer for the subset in docs/spec/ktx2.md, and a header reader for inspecting files.

use crate::texture::{ColorSpace, Image};

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;
const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;
const LEVEL_INDEX_OFFSET: usize = 80;
const LEVEL_INDEX_ENTRY: usize = 24;
const DFD_SIZE: u32 = 92;

/// Encodes RGBA8 mip levels (largest first, each half the previous) as a KTX2 file.
pub fn encode_rgba8(color: ColorSpace, levels: &[Image]) -> Vec<u8> {
    let base = &levels[0];
    for level in levels {
        assert_eq!(
            level.pixels.len(),
            level.width as usize * level.height as usize * 4
        );
    }

    let format = match color {
        ColorSpace::Srgb => VK_FORMAT_R8G8B8A8_SRGB,
        ColorSpace::Linear => VK_FORMAT_R8G8B8A8_UNORM,
    };
    let kvd = key_value_data();
    let dfd_offset = LEVEL_INDEX_OFFSET + LEVEL_INDEX_ENTRY * levels.len();
    let kvd_offset = dfd_offset + DFD_SIZE as usize;
    let data_offset = (kvd_offset + kvd.len()).next_multiple_of(4);

    // Images are stored smallest level first; the index lists level 0 first.
    let mut offsets = vec![0; levels.len()];
    let mut offset = data_offset;
    for (i, level) in levels.iter().enumerate().rev() {
        offsets[i] = offset;
        offset += level.pixels.len();
    }

    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(&IDENTIFIER);
    let level_count = levels.len() as u32;
    for v in [format, 1, base.width, base.height, 0, 0, 1, level_count, 0] {
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
        put_u64(&mut out, level.pixels.len() as u64);
        put_u64(&mut out, level.pixels.len() as u64);
    }
    debug_assert_eq!(out.len(), dfd_offset);

    write_dfd(&mut out, color);
    out.extend_from_slice(&kvd);
    out.resize(data_offset, 0);
    for level in levels.iter().rev() {
        out.extend_from_slice(&level.pixels);
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

/// What the Inspector shows of a `.ktx2` file.
#[derive(Debug, PartialEq, Eq)]
pub struct Info {
    pub vk_format: u32,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    /// Level 0, only for the subset assetc writes.
    pub level0: Option<Image>,
    /// Levels 1 and up, decoded like level 0 (empty when level 0 is not).
    pub mips: Vec<Image>,
}

impl Info {
    pub fn format_name(&self) -> String {
        match self.vk_format {
            VK_FORMAT_R8G8B8A8_SRGB => "R8G8B8A8_SRGB".into(),
            VK_FORMAT_R8G8B8A8_UNORM => "R8G8B8A8_UNORM".into(),
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

    let plain_2d = depth == 0 && layers == 0 && faces == 1 && supercompression == 0;
    let rgba8 = matches!(
        vk_format,
        VK_FORMAT_R8G8B8A8_SRGB | VK_FORMAT_R8G8B8A8_UNORM
    );
    // Level `i` is `max(1, size >> i)` on each axis; its entry is in the level index.
    let level = |i: u32| {
        let entry = LEVEL_INDEX_OFFSET + LEVEL_INDEX_ENTRY * i as usize;
        if entry + LEVEL_INDEX_ENTRY > bytes.len() {
            return None;
        }
        let (offset, length) = (u64_at(entry), u64_at(entry + 8));
        let (w, h) = ((width >> i).max(1), (height >> i).max(1));
        let end = offset.checked_add(length)?;
        if length != w as u64 * h as u64 * 4 || end > bytes.len() as u64 {
            return None;
        }
        Some(Image {
            width: w,
            height: h,
            pixels: bytes[offset as usize..end as usize].to_vec(),
        })
    };
    let readable = plain_2d && rgba8;
    let level0 = readable.then(|| level(0)).flatten();
    let mips = if level0.is_some() {
        (1..levels.max(1)).map_while(level).collect()
    } else {
        Vec::new()
    };
    Ok(Info {
        vk_format,
        width,
        height,
        levels: levels.max(1),
        level0,
        mips,
    })
}

fn key_value_data() -> Vec<u8> {
    let key = b"KTXwriter\0";
    let value = format!("assetc {}\0", env!("CARGO_PKG_VERSION"));
    let mut out = Vec::new();
    put_u32(&mut out, (key.len() + value.len()) as u32);
    out.extend_from_slice(key);
    out.extend_from_slice(value.as_bytes());
    out.resize(out.len().next_multiple_of(4), 0);
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
        let file = encode_rgba8(ColorSpace::Srgb, std::slice::from_ref(&image));
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
            let file = encode_rgba8(color, &[sample()]);
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
        let file = encode_rgba8(ColorSpace::Linear, &levels);
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
        let info = read(&encode_rgba8(ColorSpace::Srgb, &levels)).unwrap();
        assert_eq!((info.width, info.height, info.levels), (3, 2, 2));
        assert_eq!(info.format_name(), "R8G8B8A8_SRGB");
        assert_eq!(info.level0, Some(levels[0].clone()));
        assert_eq!(info.mips, levels[1..]);

        let info = read(&encode_rgba8(ColorSpace::Linear, &[sample()])).unwrap();
        assert_eq!(info.format_name(), "R8G8B8A8_UNORM");
    }

    #[test]
    fn read_without_preview_for_other_formats() {
        let mut file = encode_rgba8(ColorSpace::Srgb, &[sample()]);
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
        let file = encode_rgba8(ColorSpace::Srgb, std::slice::from_ref(&image));
        let reader = Reader::new(&file[..]).unwrap();
        let kv: Vec<_> = reader.key_value_data().collect();
        assert_eq!(kv.len(), 1);
        assert_eq!(kv[0].0, "KTXwriter");
        assert_eq!(
            kv[0].1,
            format!("assetc {}\0", env!("CARGO_PKG_VERSION")).as_bytes()
        );
        let level_offset = u64::from_le_bytes(file[80..88].try_into().unwrap());
        assert_eq!(level_offset % 4, 0);
        assert_eq!(file.len() as u64, level_offset + image.pixels.len() as u64);
    }
}
