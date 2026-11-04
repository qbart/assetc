//! `.afont.bin` SDF fonts: metrics, glyphs, kerning, and laying out text; see
//! docs/spec/afont.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::read::Bytes;

const FILE: &str = "afont.bin";

/// One glyph; metrics in em, y up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Unicode scalar; `0` is the font's `.notdef`, for characters it lacks.
    pub codepoint: u32,
    /// Pen advance after the glyph.
    pub advance: f32,
    /// Left, bottom, right, top, from the pen on the baseline; empty for a glyph without an
    /// outline (space).
    pub plane: [f32; 4],
    /// Left, top, right, bottom: normalized atlas coordinates, top-left origin.
    pub uv: [f32; 4],
}

impl Glyph {
    /// Whether the glyph has nothing to draw and only advances the pen.
    pub fn is_empty(&self) -> bool {
        self.plane[0] >= self.plane[2] || self.plane[1] >= self.plane[3]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KerningPair {
    /// Codepoint.
    pub left: u32,
    /// Codepoint.
    pub right: u32,
    /// em, added to `left`'s advance before `right`.
    pub advance: f32,
}

/// A glyph placed by [`Font::layout`]: one quad to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub character: char,
    /// Byte offset of `character` in the laid out text.
    pub index: usize,
    /// The pen on the baseline, in the units of the size; y up, the first baseline at `0`.
    pub origin: [f32; 2],
    /// The quad: left, bottom, right, top, in the units of the size, y up.
    pub rect: [f32; 4],
    /// Left, top, right, bottom in the atlas, top-left origin (the glyph's `uv`).
    pub uv: [f32; 4],
}

/// A signed-distance-field font; metrics are in em (multiply by the size to draw at).
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Asset ID of the SDF atlas texture.
    pub atlas: u64,
    /// Above the baseline.
    pub ascent: f32,
    /// Below the baseline (negative).
    pub descent: f32,
    pub line_gap: f32,
    /// Atlas texels the stored distance spans from 0 to 1.
    pub distance_range: f32,
    /// Normalized atlas value on the outline.
    pub edge_value: f32,
    /// Atlas pixels per em the glyphs were drawn at.
    pub em_size: f32,
    /// Texels.
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// `0`; bit 0 is reserved for a multi-channel atlas.
    pub flags: u32,
    /// By codepoint ascending; glyph 0 is the fallback (codepoint 0).
    pub glyphs: Vec<Glyph>,
    /// By (left, right) ascending.
    pub kerning: Vec<KerningPair>,
}

impl Font {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "AFONT", &[1], FILE)?;
        let desc = Bytes::new(file.require("DESC", FILE)?, FILE);
        let glyph_count = desc.u32(0)? as usize;
        if glyph_count == 0 {
            return Err(desc.err("no glyphs (glyph 0 is required)"));
        }
        let glyphs = Bytes::new(file.require("GLYPHS", FILE)?, FILE)
            .records_n(40, glyph_count)?
            .map(|r| {
                Ok(Glyph {
                    codepoint: r.u32(0)?,
                    advance: r.f32(4)?,
                    plane: r.f32s(8)?,
                    uv: r.f32s(24)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let kerning = Bytes::new(file.require("KERNING", FILE)?, FILE)
            .records_n(12, desc.u32(4)? as usize)?
            .map(|r| {
                Ok(KerningPair {
                    left: r.u32(0)?,
                    right: r.u32(4)?,
                    advance: r.f32(8)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            atlas: desc.u64(8)?,
            ascent: desc.f32(16)?,
            descent: desc.f32(20)?,
            line_gap: desc.f32(24)?,
            distance_range: desc.f32(28)?,
            edge_value: desc.f32(32)?,
            em_size: desc.f32(36)?,
            atlas_width: desc.u32(40)?,
            atlas_height: desc.u32(44)?,
            flags: desc.u32(48)?,
            glyphs,
            kerning,
        })
    }

    /// Baseline to baseline, em: `ascent − descent + line_gap`.
    pub fn line_advance(&self) -> f32 {
        self.ascent - self.descent + self.line_gap
    }

    /// The glyph of `c`, or glyph 0 (`.notdef`) when the font lacks it.
    pub fn glyph(&self, c: char) -> &Glyph {
        self.glyphs
            .binary_search_by_key(&(c as u32), |g| g.codepoint)
            .map_or(&self.glyphs[0], |i| &self.glyphs[i])
    }

    /// The kerning between `left` and `right`, em; `0` without a pair.
    pub fn kerning(&self, left: char, right: char) -> f32 {
        self.kerning
            .binary_search_by_key(&(left as u32, right as u32), |k| (k.left, k.right))
            .map_or(0.0, |i| self.kerning[i].advance)
    }

    /// Places `text` at `size` (units per em): the pen advances by each glyph's advance plus
    /// kerning, `\n` starts a new line one [`line_advance`](Self::line_advance) lower.
    /// Glyphs without an outline (spaces) only advance the pen and are left out.
    pub fn layout(&self, text: &str, size: f32) -> Vec<PlacedGlyph> {
        let mut placed = Vec::with_capacity(text.len());
        let (mut x, mut y) = (0.0f32, 0.0f32);
        let mut previous = None;
        for (index, c) in text.char_indices() {
            match c {
                '\n' => {
                    (x, y, previous) = (0.0, y - self.line_advance() * size, None);
                    continue;
                }
                '\r' => continue,
                _ => {}
            }
            if let Some(p) = previous {
                x += self.kerning(p, c) * size;
            }
            let glyph = self.glyph(c);
            if !glyph.is_empty() {
                let [left, bottom, right, top] = glyph.plane;
                placed.push(PlacedGlyph {
                    character: c,
                    index,
                    origin: [x, y],
                    rect: [
                        x + left * size,
                        y + bottom * size,
                        x + right * size,
                        y + top * size,
                    ],
                    uv: glyph.uv,
                });
            }
            x += glyph.advance * size;
            previous = Some(c);
        }
        placed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;

    fn glyph(codepoint: u32, advance: f32, plane: [f32; 4], uv: [f32; 4]) -> Vec<u8> {
        let mut out = codepoint.to_le_bytes().to_vec();
        for v in [advance].iter().chain(&plane).chain(&uv) {
            out.extend(v.to_le_bytes());
        }
        out
    }

    fn sample(drop_kerning: bool) -> Vec<u8> {
        let mut desc = Vec::new();
        desc.extend(4u32.to_le_bytes());
        desc.extend(1u32.to_le_bytes());
        desc.extend(0x1234u64.to_le_bytes());
        for v in [0.8f32, -0.2, 0.1, 11.95, 0.5, 48.0] {
            desc.extend(v.to_le_bytes());
        }
        for v in [256u32, 128, 0, 0] {
            desc.extend(v.to_le_bytes());
        }
        let box_ = [0.1, 0.0, 0.4, 0.7];
        let mut glyphs = glyph(0, 0.5, box_, [0.0, 0.0, 0.1, 0.1]);
        glyphs.extend(glyph(' ' as u32, 0.25, [0.0; 4], [0.0; 4]));
        glyphs.extend(glyph('A' as u32, 0.5, box_, [0.1, 0.0, 0.2, 0.1]));
        glyphs.extend(glyph('V' as u32, 0.5, box_, [0.2, 0.0, 0.3, 0.1]));
        let mut kerning = Vec::new();
        kerning.extend(('A' as u32).to_le_bytes());
        kerning.extend(('V' as u32).to_le_bytes());
        kerning.extend((-0.125f32).to_le_bytes());
        let mut chunks: Vec<(&str, &[u8])> = vec![("DESC", &desc), ("GLYPHS", &glyphs)];
        if !drop_kerning {
            chunks.push(("KERNING", &kerning));
        }
        file("AFONT", 1, &chunks)
    }

    #[test]
    fn decodes_metrics_glyphs_and_kerning() {
        let font = Font::decode(&sample(false)).unwrap();
        assert_eq!(font.atlas, 0x1234);
        assert_eq!((font.ascent, font.descent, font.em_size), (0.8, -0.2, 48.0));
        assert_eq!((font.atlas_width, font.atlas_height), (256, 128));
        assert_eq!(font.glyphs.len(), 4);
        assert_eq!(font.glyph('A').uv, [0.1, 0.0, 0.2, 0.1]);
        assert_eq!(font.glyph('?').codepoint, 0);
        assert!(font.glyph(' ').is_empty());
        assert_eq!(font.kerning('A', 'V'), -0.125);
        assert_eq!(font.kerning('V', 'A'), 0.0);
        assert!((font.line_advance() - 1.1).abs() < 1e-6);
    }

    #[test]
    fn layout_advances_and_kerns() {
        let font = Font::decode(&sample(false)).unwrap();
        let placed = font.layout("A V\nAV", 16.0);
        let origins: Vec<[f32; 2]> = placed.iter().map(|p| p.origin).collect();
        let line = -font.line_advance() * 16.0;
        // A, then a space (advance 0.25 em), V; then A and V kerned by −0.125 em.
        assert_eq!(origins, [[0.0, 0.0], [12.0, 0.0], [0.0, line], [6.0, line]]);
        assert_eq!(placed[1].character, 'V');
        assert_eq!(placed[1].index, 2);
        assert_eq!(placed[0].rect, [1.6, 0.0, 6.4, 0.7 * 16.0]);
        assert_eq!(placed[3].uv, [0.2, 0.0, 0.3, 0.1]);
    }

    #[test]
    fn rejects_missing_kerning() {
        assert!(Font::decode(&sample(true)).is_err());
    }
}
