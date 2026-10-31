//! KTX2 writer for the subset in docs/spec/ktx2.md.

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;
const LEVEL_INDEX_OFFSET: usize = 80;
const DFD_OFFSET: usize = LEVEL_INDEX_OFFSET + 24;
const DFD_SIZE: u32 = 92;

/// Encodes an sRGB RGBA8 image (rows top to bottom) as a single-level KTX2 file.
pub fn encode_rgba8_srgb(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    assert_eq!(pixels.len(), width as usize * height as usize * 4);

    let kvd = key_value_data();
    let kvd_offset = DFD_OFFSET + DFD_SIZE as usize;
    let level_offset = (kvd_offset + kvd.len()).next_multiple_of(4);

    let mut out = Vec::with_capacity(level_offset + pixels.len());
    out.extend_from_slice(&IDENTIFIER);
    for v in [VK_FORMAT_R8G8B8A8_SRGB, 1, width, height, 0, 0, 1, 1, 0] {
        put_u32(&mut out, v);
    }
    put_u32(&mut out, DFD_OFFSET as u32);
    put_u32(&mut out, DFD_SIZE);
    put_u32(&mut out, kvd_offset as u32);
    put_u32(&mut out, kvd.len() as u32);
    put_u64(&mut out, 0);
    put_u64(&mut out, 0);
    debug_assert_eq!(out.len(), LEVEL_INDEX_OFFSET);

    put_u64(&mut out, level_offset as u64);
    put_u64(&mut out, pixels.len() as u64);
    put_u64(&mut out, pixels.len() as u64);
    debug_assert_eq!(out.len(), DFD_OFFSET);

    write_dfd(&mut out);
    out.extend_from_slice(&kvd);
    out.resize(level_offset, 0);
    out.extend_from_slice(pixels);
    out
}

fn write_dfd(out: &mut Vec<u8>) {
    put_u32(out, DFD_SIZE);
    put_u32(out, 0); // vendorId 0, descriptorType 0
    put_u16(out, 2); // versionNumber
    put_u16(out, 88); // descriptorBlockSize
    out.extend_from_slice(&[1, 1, 2, 0]); // RGBSDA, BT.709, sRGB, straight alpha
    out.extend_from_slice(&[0; 4]); // texelBlockDimension
    out.extend_from_slice(&[4, 0, 0, 0, 0, 0, 0, 0]); // bytesPlane
    for (channel, channel_type) in [0x00u8, 0x01, 0x02, 0x1F].into_iter().enumerate() {
        put_u16(out, channel as u16 * 8);
        out.push(7); // bitLength - 1
        out.push(channel_type);
        out.extend_from_slice(&[0; 4]); // samplePosition
        put_u32(out, 0);
        put_u32(out, 255);
    }
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
    use ::ktx2::{Format, Reader, dfd};

    fn sample() -> (u32, u32, Vec<u8>) {
        let (w, h) = (3, 2);
        let pixels = (0..w * h * 4).map(|i| i as u8).collect();
        (w, h, pixels)
    }

    #[test]
    fn reader_accepts_header_and_level() {
        let (w, h, pixels) = sample();
        let file = encode_rgba8_srgb(w, h, &pixels);
        let reader = Reader::new(&file[..]).unwrap();
        let header = reader.header();
        assert_eq!(header.format, Some(Format::R8G8B8A8_SRGB));
        assert_eq!(header.type_size, 1);
        assert_eq!((header.pixel_width, header.pixel_height), (w, h));
        assert_eq!(header.pixel_depth, 0);
        assert_eq!(header.layer_count, 0);
        assert_eq!(header.face_count, 1);
        assert_eq!(header.level_count, 1);
        assert_eq!(header.supercompression_scheme, None);

        let levels: Vec<_> = reader.levels().collect();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].data, &pixels[..]);
        assert_eq!(levels[0].uncompressed_byte_length, pixels.len() as u64);
    }

    #[test]
    fn dfd_matches_reference_for_format() {
        let (w, h, pixels) = sample();
        let file = encode_rgba8_srgb(w, h, &pixels);
        let reader = Reader::new(&file[..]).unwrap();
        let (expected, _) = dfd::Basic::from_format(Format::R8G8B8A8_SRGB).unwrap();
        match reader.dfd_blocks() {
            [dfd::Block::Basic(basic)] => assert_eq!(basic, &expected),
            other => panic!("unexpected dfd blocks: {other:?}"),
        }
    }

    #[test]
    fn writer_key_and_alignment() {
        let (w, h, pixels) = sample();
        let file = encode_rgba8_srgb(w, h, &pixels);
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
        assert_eq!(file.len() as u64, level_offset + pixels.len() as u64);
    }
}
