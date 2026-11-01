//! 3D color LUTs in the `.cube` format, see docs/features/lut.md.

use crate::texture::Image;

#[derive(Debug, Clone, PartialEq)]
pub struct Lut {
    pub title: Option<String>,
    /// Entries per axis.
    pub size: usize,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    /// `size³` output colors, red changing fastest, then green, then blue.
    pub table: Vec<[f32; 3]>,
}

impl Lut {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut title = None;
        let mut size = None;
        let mut domain_min = [0.0; 3];
        let mut domain_max = [1.0; 3];
        let mut table = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let error = |message: &str| format!("line {}: {message}", number + 1);
            let (keyword, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            match keyword {
                "TITLE" => title = Some(rest.trim_matches('"').to_owned()),
                "LUT_3D_SIZE" => {
                    let n: usize = rest.parse().map_err(|_| error("bad LUT_3D_SIZE"))?;
                    if !(2..=256).contains(&n) {
                        return Err(error("LUT_3D_SIZE must be 2..=256"));
                    }
                    size = Some(n);
                }
                "DOMAIN_MIN" => domain_min = triple(rest).ok_or_else(|| error("bad DOMAIN_MIN"))?,
                "DOMAIN_MAX" => domain_max = triple(rest).ok_or_else(|| error("bad DOMAIN_MAX"))?,
                "LUT_1D_SIZE" => return Err(error("1D LUTs are unsupported")),
                _ => match triple(line) {
                    Some(rgb) => table.push(rgb),
                    None => return Err(error(&format!("unsupported line {keyword:?}"))),
                },
            }
        }
        let size = size.ok_or("missing LUT_3D_SIZE")?;
        if table.len() != size.pow(3) {
            return Err(format!(
                "expected {} entries for size {size}, found {}",
                size.pow(3),
                table.len()
            ));
        }
        if (0..3).any(|c| domain_max[c] <= domain_min[c]) {
            return Err("DOMAIN_MAX must exceed DOMAIN_MIN".into());
        }
        Ok(Self {
            title,
            size,
            domain_min,
            domain_max,
            table,
        })
    }

    fn entry(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        self.table[r + self.size * (g + self.size * b)]
    }

    /// Trilinearly interpolated output for `rgb`, clamped to `0..1`.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let last = (self.size - 1) as f32;
        let pos: [f32; 3] = std::array::from_fn(|c| {
            let t = (rgb[c] - self.domain_min[c]) / (self.domain_max[c] - self.domain_min[c]);
            (t * last).clamp(0.0, last)
        });
        let lo = pos.map(|p| (p.floor() as usize).min(self.size - 2));
        let f: [f32; 3] = std::array::from_fn(|c| pos[c] - lo[c] as f32);
        let mut out = [0.0; 3];
        for corner in 0..8 {
            let d = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
            let weight: f32 = (0..3)
                .map(|c| if d[c] == 1 { f[c] } else { 1.0 - f[c] })
                .product();
            if weight == 0.0 {
                continue;
            }
            let e = self.entry(lo[0] + d[0], lo[1] + d[1], lo[2] + d[2]);
            for c in 0..3 {
                out[c] += weight * e[c];
            }
        }
        out.map(|v| v.clamp(0.0, 1.0))
    }

    /// `image` graded on its stored values; alpha kept.
    pub fn grade(&self, image: &Image) -> Image {
        let pixels = image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let [r, g, b] = self.apply([p[0], p[1], p[2]].map(|v| v as f32 / 255.0));
                [to_u8(r), to_u8(g), to_u8(b), p[3]]
            })
            .collect();
        Image {
            width: image.width,
            height: image.height,
            pixels,
        }
    }
}

fn triple(text: &str) -> Option<[f32; 3]> {
    let mut parts = text.split_whitespace().map(|p| p.parse::<f32>().ok());
    let rgb = [parts.next()??, parts.next()??, parts.next()??];
    (parts.next().is_none() && rgb.iter().all(|v| v.is_finite())).then_some(rgb)
}

fn to_u8(v: f32) -> u8 {
    (v * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Identity LUT of `size`, optionally with a transform applied to each entry.
    fn cube(size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> String {
        let mut text = format!("TITLE \"test\"\n# comment\nLUT_3D_SIZE {size}\n\n");
        let last = (size - 1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let [r, g, b] = f([r as f32 / last, g as f32 / last, b as f32 / last]);
                    text += &format!("{r} {g} {b}\n");
                }
            }
        }
        text
    }

    #[test]
    fn parses_header_and_table() {
        let lut = Lut::parse(&cube(3, |c| c)).unwrap();
        assert_eq!(lut.title.as_deref(), Some("test"));
        assert_eq!(lut.size, 3);
        assert_eq!(lut.table.len(), 27);
        assert_eq!(lut.domain_max, [1.0; 3]);
        // Red changes fastest.
        assert_eq!(lut.table[1], [0.5, 0.0, 0.0]);
        assert_eq!(lut.table[3], [0.0, 0.5, 0.0]);
    }

    #[test]
    fn identity_and_interpolation() {
        let identity = Lut::parse(&cube(5, |c| c)).unwrap();
        for rgb in [
            [0.0, 0.0, 0.0],
            [0.3, 0.6, 0.9],
            [1.0, 1.0, 1.0],
            [0.1, 0.5, 0.77],
        ] {
            let out = identity.apply(rgb);
            for c in 0..3 {
                assert!((out[c] - rgb[c]).abs() < 1e-5, "{rgb:?} -> {out:?}");
            }
        }
        let invert = Lut::parse(&cube(2, |c| c.map(|v| 1.0 - v))).unwrap();
        assert_eq!(invert.apply([0.25, 0.5, 1.0]), [0.75, 0.5, 0.0]);
        // Outside the domain clamps.
        assert_eq!(invert.apply([-1.0, 2.0, 0.0]), [1.0, 0.0, 1.0]);
    }

    #[test]
    fn grade_keeps_alpha() {
        let invert = Lut::parse(&cube(2, |c| c.map(|v| 1.0 - v))).unwrap();
        let image = Image {
            width: 1,
            height: 1,
            pixels: vec![0, 255, 51, 7],
        };
        assert_eq!(invert.grade(&image).pixels, [255, 0, 204, 7]);
    }

    #[test]
    fn rejects_bad_files() {
        let bad_domain = format!("DOMAIN_MAX 0 1 1\n{}", cube(2, |c| c));
        for (text, expected) in [
            ("LUT_1D_SIZE 4\n", "1D LUTs"),
            ("0 0 0\n", "missing LUT_3D_SIZE"),
            ("LUT_3D_SIZE 2\n0 0 0\n", "expected 8 entries"),
            ("LUT_3D_SIZE 1\n", "2..=256"),
            ("LUT_3D_SIZE 2\nFOO 1\n", "unsupported line"),
            (bad_domain.as_str(), "must exceed"),
        ] {
            let error = Lut::parse(text).unwrap_err();
            assert!(error.contains(expected), "{text:?}: {error}");
        }
    }
}
