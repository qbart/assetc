//! `.agradient.bin` gradients: their stops by name, beside the image baked from them; see
//! docs/spec/agradient.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;
use crate::structs::Structs;

const FILE: &str = "agradient.bin";

/// How cells are placed on the image's grid (gradients and palettes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// Left to right, then top to bottom.
    #[default]
    Rows,
    /// Top to bottom, then left to right.
    Columns,
}

impl Layout {
    pub(crate) fn read(b: &Bytes<'_>, at: usize) -> Result<Self> {
        match b.u32(at)? {
            0 => Ok(Self::Rows),
            1 => Ok(Self::Columns),
            other => Err(b.err(format!("unknown layout {other}"))),
        }
    }
}

/// How the image dithers a gradient's cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dither {
    #[default]
    None,
    /// Blended, rounded with an 8 × 8 Bayer matrix.
    Crosshatch,
    /// Every pixel is one of the two stops' colors.
    Pixel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    /// Position, 0–1.
    pub at: f32,
    /// The color as written: `R G B A`, sRGB, straight alpha.
    pub rgba8: [u8; 4],
    /// The same color as linear floats (R, G and B decoded from sRGB, A / 255).
    pub linear: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// FNV-1a-64 of `name` (of `""` for none).
    pub name_hash: u64,
    pub name: String,
    /// At least two, sorted by `at`; the authored stops (modifiers change only the image).
    pub stops: Vec<Stop>,
    pub dither: Dither,
    /// `u0 v0 u1 v1`: its cell as 0–1 texture coordinates, top-left origin; the first stop is
    /// at the bottom (`v1`).
    pub uv: [f32; 4],
}

impl Gradient {
    /// The color at `t` (0–1) as the image has it without dithering: the two stops around `t`
    /// blended in premultiplied alpha on their sRGB bytes, then rounded (see
    /// docs/spec/agradient.md#image). `t` outside 0–1 gives the end stops; no stops give
    /// `0 0 0 0`, one stop its color.
    pub fn sample(&self, t: f32) -> [u8; 4] {
        let stops = &self.stops;
        match stops.len() {
            0 => return [0; 4],
            1 => return stops[0].rgba8,
            _ => {}
        }
        let t = if t.is_nan() { 0.0 } else { f64::from(t) };
        let mut k = 0;
        while k + 2 < stops.len() && t > f64::from(stops[k + 1].at) {
            k += 1;
        }
        let (a, b) = (&stops[k], &stops[k + 1]);
        let span = f64::from(b.at) - f64::from(a.at);
        let s = if span > 0.0 {
            ((t - f64::from(a.at)) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = |v: f64| (v + 0.5).floor().clamp(0.0, 255.0) as u8;
        let (aa, ba) = (f64::from(a.rgba8[3]), f64::from(b.rgba8[3]));
        let alpha = aa + (ba - aa) * s;
        let stored = q(alpha);
        if stored == 0 {
            return [0; 4];
        }
        let channel = |c: usize| {
            let pa = f64::from(a.rgba8[c]) * aa / 255.0;
            let pb = f64::from(b.rgba8[c]) * ba / 255.0;
            q((pa + (pb - pa) * s) * 255.0 / alpha)
        };
        [channel(0), channel(1), channel(2), stored]
    }

    /// [`Gradient::sample`] as linear floats (R, G and B decoded from sRGB, A / 255), the
    /// values a GPU reads from the sRGB image.
    pub fn sample_linear(&self, t: f32) -> [f32; 4] {
        let [r, g, b, a] = self.sample(t);
        [
            srgb_to_linear(r),
            srgb_to_linear(g),
            srgb_to_linear(b),
            f32::from(a) / 255.0,
        ]
    }
}

/// An sRGB-encoded byte as a linear 0–1 float.
pub fn srgb_to_linear(byte: u8) -> f32 {
    let c = f32::from(byte) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// An `.agradient` file's gradients and the grid of cells its image puts them in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GradientSet {
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
    pub gradients: Vec<Gradient>,
    /// Attached structs: [`Scope::Asset`](crate::structs::Scope::Asset) the file,
    /// [`Scope::Item`](crate::structs::Scope::Item) each gradient, by index.
    pub structs: Structs,
}

impl GradientSet {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "AGRAD", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let desc = require("DESC")?;
        let stop_count = desc.u32(4)? as usize;
        let strings = require("STRINGS")?;
        let positions = require("STOPS")?.records_n(4, stop_count)?;
        let bytes8 = require("RGBA8")?.records_n(4, stop_count)?;
        let floats = require("RGBAF")?.records_n(16, stop_count)?;
        let stops = positions
            .zip(bytes8)
            .zip(floats)
            .map(|((at, rgba8), linear)| {
                Ok(Stop {
                    at: at.f32(0)?,
                    rgba8: rgba8.array(0)?,
                    linear: linear.f32s(0)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let gradients = require("GRADIENT")?
            .records_n(48, desc.u32(0)? as usize)?
            .map(|r| {
                let (first, count) = (r.u32(16)? as usize, r.u32(20)? as usize);
                if count < 2 {
                    return Err(r.err(format!("a gradient has {count} stops, not at least 2")));
                }
                let stops = first
                    .checked_add(count)
                    .and_then(|end| stops.get(first..end))
                    .ok_or_else(|| r.err("a gradient's stops run past STOPS"))?
                    .to_vec();
                let dither = match r.u32(24)? {
                    0 => Dither::None,
                    1 => Dither::Crosshatch,
                    2 => Dither::Pixel,
                    other => return Err(r.err(format!("unknown dither {other}"))),
                };
                Ok(Gradient {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    stops,
                    dither,
                    uv: r.f32s(32)?,
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
            gradients,
            structs: Structs::from_chunks(&file, FILE)?,
        })
    }

    /// The index of the gradient named `name`.
    pub fn gradient_index(&self, name: &str) -> Option<usize> {
        let hash = fnv1a64(name.as_bytes());
        self.gradients
            .iter()
            .position(|g| g.name_hash == hash && g.name == name)
    }

    /// The gradient named `name`.
    pub fn gradient(&self, name: &str) -> Option<&Gradient> {
        self.gradient_index(name).map(|i| &self.gradients[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::graph::tests::text;
    use crate::structs::Scope;

    /// `fire`: black (transparent at 0) → red at 0.5 → white; `ice`: two blue stops.
    fn sample(stop_count: u32, structs: bool) -> Vec<u8> {
        let mut desc = Vec::new();
        for v in [2u32, stop_count, 32, 64, 16, 64, 2, 1, 1, 0] {
            desc.extend(v.to_le_bytes());
        }
        desc.extend(0x9a7du64.to_le_bytes());
        let stops: [(f32, [u8; 4]); 5] = [
            (0.0, [0, 0, 0, 0]),
            (0.5, [255, 0, 0, 255]),
            (1.0, [255, 255, 255, 255]),
            (0.0, [0, 0, 255, 255]),
            (1.0, [0, 0, 128, 255]),
        ];
        let (mut at, mut rgba8, mut rgbaf) = (Vec::new(), Vec::new(), Vec::new());
        for (t, c) in stops {
            at.extend(t.to_le_bytes());
            rgba8.extend(c);
            for (i, v) in c.into_iter().enumerate() {
                let f = if i < 3 {
                    srgb_to_linear(v)
                } else {
                    f32::from(v) / 255.0
                };
                rgbaf.extend(f.to_le_bytes());
            }
        }
        let mut strings = Vec::new();
        let mut gradients = Vec::new();
        for (i, (name, first, count, dither)) in [("fire", 0u32, 3u32, 1u32), ("ice", 3, 2, 0)]
            .into_iter()
            .enumerate()
        {
            gradients.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            gradients.extend(text(&mut strings, name));
            for v in [first, count, dither, 0] {
                gradients.extend(v.to_le_bytes());
            }
            let u0 = 0.5 * i as f32;
            for v in [u0, 0.0, u0 + 0.5, 1.0] {
                gradients.extend(v.to_le_bytes());
            }
        }
        let extra = crate::structs::tests::chunks(1);
        let mut chunks: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc),
            ("GRADIENT", &gradients),
            ("STOPS", &at),
            ("RGBA8", &rgba8),
            ("RGBAF", &rgbaf),
            ("STRINGS", &strings),
        ];
        if structs {
            chunks.extend(extra.iter().map(|(n, p)| (*n, &p[..])));
        }
        file("AGRAD", 1, &chunks)
    }

    #[test]
    fn decodes_gradients_and_stops() {
        let set = GradientSet::decode(&sample(5, false)).unwrap();
        assert_eq!(
            (set.width, set.height, set.columns, set.rows),
            (32, 64, 2, 1)
        );
        assert_eq!((set.layout, set.texture), (Layout::Columns, 0x9a7d));
        assert!(set.structs.is_empty());
        let fire = set.gradient("fire").unwrap();
        assert_eq!((fire.stops.len(), fire.dither), (3, Dither::Crosshatch));
        assert_eq!(fire.stops[1].rgba8, [255, 0, 0, 255]);
        assert_eq!(fire.stops[1].linear, [1.0, 0.0, 0.0, 1.0]);
        let ice = &set.gradients[1];
        assert_eq!((ice.name.as_str(), ice.uv), ("ice", [0.5, 0.0, 1.0, 1.0]));
        assert_eq!(set.gradient_index("ice"), Some(1));
        assert!(set.gradient("lava").is_none());
    }

    #[test]
    fn samples_as_the_image_blends() {
        let set = GradientSet::decode(&sample(5, false)).unwrap();
        let (fire, ice) = (&set.gradients[0], &set.gradients[1]);
        assert_eq!(fire.sample(0.0), [0, 0, 0, 0]);
        // Premultiplied: transparent black adds no darkness, only less alpha.
        assert_eq!(fire.sample(0.25), [255, 0, 0, 128]);
        assert_eq!(fire.sample(0.5), [255, 0, 0, 255]);
        assert_eq!(fire.sample(0.75), [255, 128, 128, 255]);
        assert_eq!(fire.sample(1.0), [255, 255, 255, 255]);
        assert_eq!(fire.sample(2.0), [255, 255, 255, 255]);
        assert_eq!(fire.sample(-1.0), [0, 0, 0, 0]);
        assert_eq!(ice.sample(0.5), [0, 0, 192, 255]);
        let linear = ice.sample_linear(0.0);
        assert_eq!(linear, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(fire.sample(f32::NAN), [0, 0, 0, 0]);
    }

    #[test]
    fn reads_structs_and_rejects_bad_stops() {
        let set = GradientSet::decode(&sample(5, true)).unwrap();
        let step = set.structs.get(Scope::Item, 1, "Step").unwrap();
        assert_eq!(step.get("speed").and_then(|v| v.as_float()), Some(2.5));
        // Too few stops for the gradients' ranges.
        assert!(GradientSet::decode(&sample(4, false)).is_err());
        assert!(GradientSet::decode(&sample(6, false)).is_err());
    }
}
