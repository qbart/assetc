//! `.apalette.bin` palettes: their colors by name, beside the image baked from them; see
//! docs/spec/apalette.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;
use crate::structs::Structs;

pub use crate::gradient::Layout;

const FILE: &str = "apalette.bin";

#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    /// FNV-1a-64 of `name` (of `""` for none).
    pub name_hash: u64,
    pub name: String,
    /// As written: `R G B A`, sRGB, straight alpha.
    pub rgba8: [u8; 4],
    /// The same color as linear floats (R, G and B decoded from sRGB, A / 255).
    pub linear: [f32; 4],
    /// `u0 v0 u1 v1`: its cell as 0–1 texture coordinates, top-left origin.
    pub uv: [f32; 4],
}

/// An `.apalette` file's colors and the grid of cells its image puts them in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Palette {
    /// The image's size, px.
    pub width: u32,
    pub height: u32,
    /// px.
    pub cell_width: u32,
    pub cell_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub layout: Layout,
    /// Asset ID of the image (the file's `.ktx2`).
    pub texture: u64,
    /// In the file's (cell) order.
    pub colors: Vec<Color>,
    /// Attached structs: [`Scope::Asset`](crate::structs::Scope::Asset) the file,
    /// [`Scope::Item`](crate::structs::Scope::Item) each color, by index.
    pub structs: Structs,
}

impl Palette {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "APALETTE", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let desc = require("DESC")?;
        let count = desc.u32(0)? as usize;
        let strings = require("STRINGS")?;
        let bytes8 = require("RGBA8")?.records_n(4, count)?;
        let floats = require("RGBAF")?.records_n(16, count)?;
        let colors = require("COLORS")?
            .records_n(32, count)?
            .zip(bytes8)
            .zip(floats)
            .map(|((r, rgba8), linear)| {
                Ok(Color {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    rgba8: rgba8.array(0)?,
                    linear: linear.f32s(0)?,
                    uv: r.f32s(16)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            width: desc.u32(8)?,
            height: desc.u32(12)?,
            cell_width: desc.u32(16)?,
            cell_height: desc.u32(20)?,
            columns: desc.u32(24)?,
            rows: desc.u32(28)?,
            layout: Layout::read(&desc, 32)?,
            texture: desc.u64(40)?,
            colors,
            structs: Structs::from_chunks(&file, FILE)?,
        })
    }

    /// The index of the color named `name`.
    pub fn color_index(&self, name: &str) -> Option<usize> {
        let hash = fnv1a64(name.as_bytes());
        self.colors
            .iter()
            .position(|c| c.name_hash == hash && c.name == name)
    }

    /// The color named `name`.
    pub fn color(&self, name: &str) -> Option<&Color> {
        self.color_index(name).map(|i| &self.colors[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::data::Value;
    use crate::graph::tests::text;
    use crate::structs::Scope;

    fn sample(rgbaf_len: Option<usize>) -> Vec<u8> {
        let mut desc = Vec::new();
        for v in [2u32, 0, 16, 8, 8, 8, 2, 1, 0, 0] {
            desc.extend(v.to_le_bytes());
        }
        desc.extend(0x9a1eu64.to_le_bytes());
        let mut strings = Vec::new();
        let (mut colors, mut rgba8, mut rgbaf) = (Vec::new(), Vec::new(), Vec::new());
        for (i, (name, c)) in [("grass", [0u8, 255, 0, 255]), ("", [255, 255, 255, 128])]
            .into_iter()
            .enumerate()
        {
            colors.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            colors.extend(text(&mut strings, name));
            let u0 = 0.5 * i as f32;
            for v in [u0, 0.0, u0 + 0.5, 1.0] {
                colors.extend(v.to_le_bytes());
            }
            rgba8.extend(c);
            for v in [0.0f32, 1.0, 0.0, f32::from(c[3]) / 255.0] {
                rgbaf.extend(v.to_le_bytes());
            }
        }
        let rgbaf = &rgbaf[..rgbaf_len.unwrap_or(rgbaf.len())];
        let structs = crate::structs::tests::chunks(1);
        let mut chunks: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc),
            ("COLORS", &colors),
            ("RGBA8", &rgba8),
            ("RGBAF", rgbaf),
            ("STRINGS", &strings),
        ];
        chunks.extend(structs.iter().map(|(n, p)| (*n, &p[..])));
        file("APALETTE", 1, &chunks)
    }

    #[test]
    fn decodes_colors_and_structs() {
        let palette = Palette::decode(&sample(None)).unwrap();
        assert_eq!((palette.width, palette.columns, palette.rows), (16, 2, 1));
        assert_eq!((palette.layout, palette.texture), (Layout::Rows, 0x9a1e));
        let grass = palette.color("grass").unwrap();
        assert_eq!(grass.rgba8, [0, 255, 0, 255]);
        assert_eq!(grass.linear, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(grass.uv, [0.0, 0.0, 0.5, 1.0]);
        let unnamed = &palette.colors[1];
        assert_eq!((unnamed.name.as_str(), unnamed.rgba8[3]), ("", 128));
        assert_eq!(palette.color_index(""), Some(1));
        let step = palette.structs.get(Scope::Item, 1, "Step").unwrap();
        assert_eq!(step.get("speed"), Some(&Value::Float(2.5)));
    }

    #[test]
    fn rejects_truncated_colors() {
        assert!(Palette::decode(&sample(Some(20))).is_err());
    }
}
