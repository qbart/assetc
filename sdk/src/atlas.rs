//! `.aatlas.bin` atlas metadata: where each frame is in its texture; see
//! docs/spec/aatlas.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;

const FILE: &str = "aatlas.bin";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Packing {
    /// Frames in cells of one size, row by row (flipbooks).
    #[default]
    Grid,
    /// Frames trimmed and packed tightly.
    Pack,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// FNV-1a-64 of `name`.
    pub name_hash: u64,
    pub name: String,
    /// The frame's pixels in the atlas, px: left.
    pub x: u32,
    /// Top.
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The image file's size.
    pub source_width: u32,
    pub source_height: u32,
    /// Where the frame's pixels start in the source (trimmed, or `0`).
    pub offset_x: u32,
    pub offset_y: u32,
    /// `u0 v0 u1 v1`: the frame's rectangle as 0–1 texture coordinates, top-left origin.
    pub uv: [f32; 4],
}

/// An atlas's frames and how they were laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Atlas {
    /// px.
    pub width: u32,
    pub height: u32,
    pub packing: Packing,
    /// Grid: cells per row; `0` for pack.
    pub columns: u32,
    /// Grid; `0` for pack.
    pub cell_width: u32,
    pub cell_height: u32,
    pub padding: u32,
    /// Asset ID of the atlas texture.
    pub texture: u64,
    /// In name order.
    pub frames: Vec<Frame>,
}

impl Atlas {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "AATLAS", &[1], FILE)?;
        let desc = Bytes::new(file.require("DESC", FILE)?, FILE);
        let strings = Bytes::new(file.require("STRINGS", FILE)?, FILE);
        let packing = match desc.u32(12)? {
            0 => Packing::Grid,
            1 => Packing::Pack,
            other => return Err(desc.err(format!("unknown packing {other}"))),
        };
        let frames = Bytes::new(file.require("FRAMES", FILE)?, FILE)
            .records_n(64, desc.u32(8)? as usize)?
            .map(|r| {
                Ok(Frame {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    x: r.u32(16)?,
                    y: r.u32(20)?,
                    width: r.u32(24)?,
                    height: r.u32(28)?,
                    source_width: r.u32(32)?,
                    source_height: r.u32(36)?,
                    offset_x: r.u32(40)?,
                    offset_y: r.u32(44)?,
                    uv: r.f32s(48)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            width: desc.u32(0)?,
            height: desc.u32(4)?,
            packing,
            columns: desc.u32(16)?,
            cell_width: desc.u32(20)?,
            cell_height: desc.u32(24)?,
            padding: desc.u32(28)?,
            texture: desc.u64(32)?,
            frames,
        })
    }

    /// The frame named `name`.
    pub fn frame(&self, name: &str) -> Option<&Frame> {
        let hash = fnv1a64(name.as_bytes());
        self.frames
            .iter()
            .find(|f| f.name_hash == hash && f.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;

    fn sample() -> Vec<u8> {
        let mut desc = Vec::new();
        for v in [64u32, 32, 2, 0, 2, 32, 32, 1] {
            desc.extend(v.to_le_bytes());
        }
        desc.extend(0xabcdu64.to_le_bytes());
        desc.extend(0u64.to_le_bytes());
        let mut frames = Vec::new();
        for (i, name) in ["a", "bb"].iter().enumerate() {
            let i = i as u32;
            frames.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            for v in [i, name.len() as u32, 32 * i, 0, 32, 32, 30, 30, 1, 1] {
                frames.extend(v.to_le_bytes());
            }
            for v in [0.5 * i as f32, 0.0, 0.5 + 0.5 * i as f32, 1.0] {
                frames.extend(v.to_le_bytes());
            }
        }
        file(
            "AATLAS",
            1,
            &[("DESC", &desc), ("FRAMES", &frames), ("STRINGS", b"abb")],
        )
    }

    #[test]
    fn decodes_frames() {
        let atlas = Atlas::decode(&sample()).unwrap();
        assert_eq!((atlas.width, atlas.height), (64, 32));
        assert_eq!(
            (atlas.packing, atlas.columns, atlas.padding),
            (Packing::Grid, 2, 1)
        );
        assert_eq!(atlas.texture, 0xabcd);
        let bb = atlas.frame("bb").unwrap();
        assert_eq!(
            (bb.x, bb.y, bb.width, bb.source_width, bb.offset_x),
            (32, 0, 32, 30, 1)
        );
        assert_eq!(bb.uv, [0.5, 0.0, 1.0, 1.0]);
        assert!(atlas.frame("c").is_none());
    }

    #[test]
    fn rejects_truncated_frames() {
        let mut bytes = sample();
        // The FRAMES entry's size, shrunk by one byte.
        let at = 32 + 32 + 24;
        let size = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) - 1;
        bytes[at..at + 8].copy_from_slice(&size.to_le_bytes());
        assert!(Atlas::decode(&bytes).is_err());
    }
}
