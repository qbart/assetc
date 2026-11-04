//! Texture settings and mip generation, see docs/features/textures.md.

use std::fmt;
use std::sync::LazyLock;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
    Linear,
}

impl fmt::Display for ColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Srgb => "srgb",
            Self::Linear => "linear",
        })
    }
}

/// `[textures]` with its patterns compiled.
pub struct Settings {
    linear: GlobSet,
    no_mipmaps: GlobSet,
    volume_luts: GlobSet,
}

impl Settings {
    pub fn new(textures: &config::Textures) -> Result<Self, globset::Error> {
        Ok(Self {
            linear: glob_set(&textures.linear)?,
            no_mipmaps: glob_set(&textures.no_mipmaps)?,
            volume_luts: glob_set(&textures.volume_luts)?,
        })
    }

    /// `source` is the input-relative, `/`-separated path with extension.
    pub fn color_space(&self, source: &str) -> ColorSpace {
        if self.linear.is_match(source) {
            ColorSpace::Linear
        } else {
            ColorSpace::Srgb
        }
    }

    pub fn mipmaps(&self, source: &str) -> bool {
        !self.no_mipmaps.is_match(source)
    }

    /// Whether a color LUT compiles as a 3D texture rather than a strip.
    pub fn volume_lut(&self, source: &str) -> bool {
        self.volume_luts.is_match(source)
    }
}

pub fn glob_set(patterns: &[String]) -> Result<GlobSet, globset::Error> {
    let mut set = GlobSetBuilder::new();
    for pattern in patterns {
        set.add(GlobBuilder::new(pattern).literal_separator(true).build()?);
    }
    set.build()
}

/// An RGBA8 image, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Levels in a full chain down to 1×1: `floor(log2(max(width, height))) + 1`.
pub fn level_count(width: u32, height: u32) -> usize {
    (u32::BITS - width.max(height).max(1).leading_zeros()) as usize
}

/// `base` followed by every smaller level down to 1×1.
pub fn mip_chain(base: Image, color: ColorSpace) -> Vec<Image> {
    let mut levels = vec![base];
    loop {
        let prev = levels.last().unwrap();
        if prev.width == 1 && prev.height == 1 {
            return levels;
        }
        let next = downsample(prev, color);
        levels.push(next);
    }
}

fn downsample(src: &Image, color: ColorSpace) -> Image {
    let (sw, sh) = (src.width as usize, src.height as usize);
    let (dw, dh) = ((sw / 2).max(1), (sh / 2).max(1));
    let mut pixels = Vec::with_capacity(dw * dh * 4);
    for y in 0..dh {
        let (y0, y1) = (y * sh / dh, (y + 1) * sh / dh);
        for x in 0..dw {
            let (x0, x1) = (x * sw / dw, (x + 1) * sw / dw);
            let mut sum = [0.0f32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &src.pixels[(sy * sw + sx) * 4..][..4];
                    for c in 0..4 {
                        sum[c] += match (color, c) {
                            (ColorSpace::Srgb, 0..3) => SRGB_TO_LINEAR[p[c] as usize],
                            _ => p[c] as f32 / 255.0,
                        };
                    }
                }
            }
            let n = ((x1 - x0) * (y1 - y0)) as f32;
            for (c, sum) in sum.into_iter().enumerate() {
                let v = sum / n;
                let v = match (color, c) {
                    (ColorSpace::Srgb, 0..3) => linear_to_srgb(v),
                    _ => v,
                };
                pixels.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    Image {
        width: dw as u32,
        height: dh as u32,
        pixels,
    }
}

static SRGB_TO_LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let v = i as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
});

/// An sRGB-encoded channel (0–1) as linear light.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(linear: &[&str], no_mipmaps: &[&str]) -> Settings {
        Settings::new(&config::Textures {
            linear: linear.iter().map(|s| s.to_string()).collect(),
            no_mipmaps: no_mipmaps.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
        .unwrap()
    }

    fn checker(w: u32, h: u32) -> Image {
        let pixels = (0..w * h)
            .flat_map(|i| {
                let v = if (i % w + i / w).is_multiple_of(2) {
                    0
                } else {
                    255
                };
                [v, v, v, v]
            })
            .collect();
        Image {
            width: w,
            height: h,
            pixels,
        }
    }

    #[test]
    fn defaults_are_srgb_with_mipmaps() {
        let s = settings(&[], &[]);
        assert_eq!(s.color_space("a/b.png"), ColorSpace::Srgb);
        assert!(s.mipmaps("a/b.png"));
    }

    #[test]
    fn globs_match_relative_paths() {
        let s = settings(&["**/*_n.png"], &["ui/**"]);
        assert_eq!(s.color_space("rock_n.png"), ColorSpace::Linear);
        assert_eq!(s.color_space("a/b/rock_n.png"), ColorSpace::Linear);
        assert_eq!(s.color_space("rock_n.PNG"), ColorSpace::Srgb);
        assert_eq!(s.color_space("rock.png"), ColorSpace::Srgb);
        assert!(!s.mipmaps("ui/button.png"));
        assert!(!s.mipmaps("ui/a/b.png"));
        assert!(s.mipmaps("textures/ui.png"));
    }

    #[test]
    fn star_does_not_cross_directories() {
        let s = settings(&["*.png"], &[]);
        assert_eq!(s.color_space("a.png"), ColorSpace::Linear);
        assert_eq!(s.color_space("dir/a.png"), ColorSpace::Srgb);
    }

    #[test]
    fn invalid_pattern_is_an_error() {
        let textures = config::Textures {
            linear: vec!["[".into()],
            no_mipmaps: vec![],
            ..Default::default()
        };
        assert!(Settings::new(&textures).is_err());
    }

    #[test]
    fn chain_sizes_go_down_to_one() {
        let sizes: Vec<_> = mip_chain(checker(5, 2), ColorSpace::Linear)
            .iter()
            .map(|l| (l.width, l.height))
            .collect();
        assert_eq!(sizes, [(5, 2), (2, 1), (1, 1)]);
        assert_eq!(mip_chain(checker(1, 1), ColorSpace::Srgb).len(), 1);
        assert_eq!(mip_chain(checker(256, 64), ColorSpace::Srgb).len(), 9);
        for (w, h) in [(1, 1), (5, 2), (256, 64), (3, 7), (1000, 1)] {
            assert_eq!(
                level_count(w, h),
                mip_chain(checker(w, h), ColorSpace::Linear).len()
            );
        }
    }

    #[test]
    fn srgb_averages_in_linear_light() {
        let levels = mip_chain(checker(2, 2), ColorSpace::Srgb);
        // Color 188 (0.5 linear), alpha averaged as stored.
        assert_eq!(levels[1].pixels, [188, 188, 188, 128]);
    }

    #[test]
    fn linear_averages_as_stored() {
        let levels = mip_chain(checker(2, 2), ColorSpace::Linear);
        assert_eq!(levels[1].pixels, [128, 128, 128, 128]);
    }

    #[test]
    fn odd_sizes_fold_last_row_and_column() {
        let base = Image {
            width: 3,
            height: 1,
            pixels: [0u8, 30, 90].iter().flat_map(|&v| [v, v, v, 255]).collect(),
        };
        let levels = mip_chain(base, ColorSpace::Linear);
        assert_eq!(levels[1].pixels, [40, 40, 40, 255]);
    }

    #[test]
    fn srgb_round_trips_uniform_color() {
        let base = Image {
            width: 4,
            height: 4,
            pixels: [10u8, 100, 200, 50].repeat(16),
        };
        for level in mip_chain(base, ColorSpace::Srgb) {
            assert_eq!(&level.pixels[..4], [10, 100, 200, 50]);
        }
    }
}
