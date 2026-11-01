//! Procedural `.noise` textures, see docs/spec/noise.md.

use std::f64::consts::{FRAC_1_SQRT_2, TAU};

use serde::{Deserialize, Serialize};

use crate::texture::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoiseType {
    White,
    Value,
    #[default]
    Perlin,
    Worley,
}

impl NoiseType {
    pub const ALL: [Self; 4] = [Self::White, Self::Value, Self::Perlin, Self::Worley];

    pub fn name(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Value => "Value",
            Self::Perlin => "Perlin",
            Self::Worley => "Worley",
        }
    }
}

/// The parameters of a `.noise` file; field order is the written order.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Noise {
    #[serde(rename = "type")]
    pub kind: NoiseType,
    pub size: u32,
    pub seed: u32,
    pub frequency: u32,
    pub octaves: u32,
    pub gain: f64,
    pub invert: bool,
}

impl Default for Noise {
    fn default() -> Self {
        Self {
            kind: NoiseType::default(),
            size: 512,
            seed: 0,
            frequency: 4,
            octaves: 4,
            gain: 0.5,
            invert: false,
        }
    }
}

/// Second independent value from the same lattice point.
const SECOND: u32 = 0x5bd1e995;

impl Noise {
    pub fn parse(text: &str) -> Result<Self, String> {
        let noise: Self = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        let check = |ok: bool, what: &str| if ok { Ok(()) } else { Err(what.to_owned()) };
        check((1..=4096).contains(&noise.size), "size must be 1..=4096")?;
        check(
            (1..=256).contains(&noise.frequency),
            "frequency must be 1..=256",
        )?;
        check((1..=8).contains(&noise.octaves), "octaves must be 1..=8")?;
        check(
            noise.gain.is_finite() && (0.0..=1.0).contains(&noise.gain),
            "gain must be 0..=1",
        )?;
        Ok(noise)
    }

    /// The written form: every key, in order.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("noise parameters serialize")
    }

    /// The grayscale image, see docs/spec/noise.md#generated-image.
    pub fn generate(&self) -> Image {
        let size = self.size;
        let mut pixels = Vec::with_capacity((size * size * 4) as usize);
        for py in 0..size {
            for px in 0..size {
                let u = (px as f64 + 0.5) / size as f64;
                let v = (py as f64 + 0.5) / size as f64;
                let value = match self.kind {
                    NoiseType::White => unit(hash(px, py, self.seed)),
                    _ => self.octaves_at(u, v),
                };
                let value = if self.invert { 1.0 - value } else { value };
                let gray = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
                pixels.extend_from_slice(&[gray, gray, gray, 255]);
            }
        }
        Image {
            width: size,
            height: size,
            pixels,
        }
    }

    fn octaves_at(&self, u: f64, v: f64) -> f64 {
        let (mut sum, mut total, mut weight) = (0.0, 0.0, 1.0);
        for o in 0..self.octaves {
            let period = self.frequency << o;
            let seed = self.seed.wrapping_add(o);
            let (x, y) = (u * period as f64, v * period as f64);
            let n = match self.kind {
                NoiseType::Value => value(x, y, period, seed),
                NoiseType::Perlin => perlin(x, y, period, seed),
                NoiseType::Worley => worley(x, y, period, seed),
                NoiseType::White => unreachable!("white has no octaves"),
            };
            sum += weight * n;
            total += weight;
            weight *= self.gain;
        }
        sum / total
    }
}

pub fn hash(x: u32, y: u32, s: u32) -> u64 {
    let key = (s as u64) << 32 | ((x as u64) & 0xFFFF) << 16 | ((y as u64) & 0xFFFF);
    splitmix64(key)
}

fn splitmix64(z: u64) -> u64 {
    let z = z.wrapping_add(0x9E3779B97F4A7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn unit(h: u64) -> f64 {
    (h >> 40) as f64 / (1u64 << 24) as f64
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Lattice index `i` wrapped to the period.
fn wrap(i: i64, period: u32) -> u32 {
    i.rem_euclid(period as i64) as u32
}

/// Interpolates `corner(i, j, dx, dy)` over the cell containing `(x, y)` with the fade.
fn interpolate(x: f64, y: f64, corner: impl Fn(i64, i64, f64, f64) -> f64) -> f64 {
    let (i, j) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - i as f64, y - j as f64);
    let at = |di: i64, dj: i64| corner(i + di, j + dj, fx - di as f64, fy - dj as f64);
    let (tx, ty) = (fade(fx), fade(fy));
    lerp(
        lerp(at(0, 0), at(1, 0), tx),
        lerp(at(0, 1), at(1, 1), tx),
        ty,
    )
}

fn value(x: f64, y: f64, period: u32, seed: u32) -> f64 {
    interpolate(x, y, |i, j, _, _| {
        unit(hash(wrap(i, period), wrap(j, period), seed))
    })
}

fn perlin(x: f64, y: f64, period: u32, seed: u32) -> f64 {
    let n = interpolate(x, y, |i, j, dx, dy| {
        let angle = TAU * unit(hash(wrap(i, period), wrap(j, period), seed));
        angle.cos() * dx + angle.sin() * dy
    });
    (0.5 + n * FRAC_1_SQRT_2).clamp(0.0, 1.0)
}

fn worley(x: f64, y: f64, period: u32, seed: u32) -> f64 {
    let (i, j) = (x.floor() as i64, y.floor() as i64);
    let mut nearest = f64::MAX;
    for dj in -1..=1 {
        for di in -1..=1 {
            let (ci, cj) = (i + di, j + dj);
            let (wi, wj) = (wrap(ci, period), wrap(cj, period));
            let fx = ci as f64 + unit(hash(wi, wj, seed));
            let fy = cj as f64 + unit(hash(wi, wj, seed ^ SECOND));
            nearest = nearest.min((fx - x).hypot(fy - y));
        }
    }
    nearest.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(kind: NoiseType) -> Noise {
        Noise {
            kind,
            size: 32,
            ..Default::default()
        }
    }

    fn gray(image: &Image, x: u32, y: u32) -> u8 {
        image.pixels[((y * image.width + x) * 4) as usize]
    }

    #[test]
    fn hash_reference_values() {
        // splitmix64(0) is the well-known first output of SplitMix64 seeded with 0.
        assert_eq!(hash(0, 0, 0), 0xE220A8397B1DCDAF);
        assert_ne!(hash(1, 0, 0), hash(0, 1, 0));
        assert_ne!(hash(0, 0, 1), hash(0, 0, 0));
    }

    #[test]
    fn written_form_round_trips() {
        let text = Noise::default().to_toml();
        assert_eq!(
            text,
            "type = \"perlin\"\nsize = 512\nseed = 0\nfrequency = 4\noctaves = 4\ngain = 0.5\ninvert = false\n"
        );
        assert_eq!(Noise::parse(&text).unwrap(), Noise::default());
        assert_eq!(Noise::parse("").unwrap(), Noise::default());
    }

    #[test]
    fn rejects_bad_parameters() {
        for bad in [
            "size = 0",
            "size = 5000",
            "frequency = 0",
            "octaves = 9",
            "gain = 1.5",
            "type = \"simplex\"",
            "colour = 1",
        ] {
            assert!(Noise::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn generation_is_deterministic_gray_and_in_range() {
        for kind in NoiseType::ALL {
            let a = noise(kind).generate();
            assert_eq!(a, noise(kind).generate(), "{kind:?}");
            assert_eq!((a.width, a.height), (32, 32));
            for p in a.pixels.as_chunks::<4>().0 {
                assert!(p[0] == p[1] && p[1] == p[2] && p[3] == 255);
            }
            let distinct: std::collections::HashSet<_> = a.pixels.iter().collect();
            assert!(distinct.len() > 8, "{kind:?} looks flat");
        }
        let other_seed = Noise {
            seed: 7,
            ..noise(NoiseType::Perlin)
        };
        assert_ne!(other_seed.generate(), noise(NoiseType::Perlin).generate());
    }

    #[test]
    fn tiles_seamlessly() {
        // A 2× larger image with the same frequency samples the same lattice twice as
        // densely; the octave lattices wrap, so value at u and u + 1 match. Check the
        // generated edges continue each other: column 0 is close to column size-1.
        for kind in [NoiseType::Value, NoiseType::Perlin, NoiseType::Worley] {
            let n = Noise {
                size: 256,
                octaves: 1,
                ..noise(kind)
            };
            let image = n.generate();
            for y in (0..256).step_by(17) {
                let (left, right) = (gray(&image, 0, y), gray(&image, 255, y));
                assert!(
                    left.abs_diff(right) < 24,
                    "{kind:?} y={y}: {left} vs {right}"
                );
            }
        }
    }

    #[test]
    fn invert_flips() {
        let n = noise(NoiseType::Value);
        let inverted = Noise {
            invert: true,
            ..n.clone()
        }
        .generate();
        let plain = n.generate();
        for (a, b) in plain.pixels.iter().zip(&inverted.pixels).step_by(4) {
            assert!((*a as i32 + *b as i32 - 255).abs() <= 1);
        }
    }
}
