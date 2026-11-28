//! The procedural noise of [assetc](https://github.com/qbart/assetc): the generators of
//! `.anoise` files, their parameters and color modifiers, giving the pixels assetc builds.
#![doc = include_str!("../README.md")]

use std::f64::consts::{FRAC_1_SQRT_2, TAU};

// Images, parameters and color modifiers are assetc-colors'.
pub use assetc_colors::Image;
pub use assetc_colors::modifier::{
    Modifier, ModifierKind, ParamKind, ParamSpec, Params, Value, apply_modifiers,
};
use assetc_colors::modifier::{choice, float, int};

/// The smallest image size, px.
pub const MIN_SIZE: u32 = 1;
/// The largest image size, px.
pub const MAX_SIZE: u32 = 4096;

const SEED: ParamSpec = int("seed", "Seed", 0, u32::MAX as i64, 0);
const BASIS: ParamSpec = choice("basis", "Basis", &["value", "perlin", "simplex"], 1);
const INTERPOLATION: ParamSpec = choice(
    "interpolation",
    "Interpolation",
    &["linear", "cubic", "quintic"],
    2,
);
const fn frequency_x(default: i64) -> ParamSpec {
    int("frequency_x", "Frequency X", 1, 256, default)
}
const fn frequency_y(default: i64) -> ParamSpec {
    int("frequency_y", "Frequency Y", 1, 256, default)
}
const TRANSLATE_X: ParamSpec = float("translate_x", "Translate X", 0.0, 1.0, 0.0);
const TRANSLATE_Y: ParamSpec = float("translate_y", "Translate Y", 0.0, 1.0, 0.0);
const fn octaves(default: i64) -> ParamSpec {
    int("octaves", "Octaves", 1, 8, default)
}
const LACUNARITY: ParamSpec = int("lacunarity", "Lacunarity", 2, 4, 2);
const GAIN: ParamSpec = float("gain", "Gain", 0.0, 1.0, 0.5);
const DIMENSION: ParamSpec = float("dimension", "Dimension", 0.0, 2.0, 1.0);
const WARP: ParamSpec = float("warp", "Warp", 0.0, 4.0, 0.0);

const GENERATOR_CHECKERBOARD: &[ParamSpec] = &[
    int("cells_x", "Cells X", 1, 256, 8),
    int("cells_y", "Cells Y", 1, 256, 8),
    TRANSLATE_X,
    TRANSLATE_Y,
    ParamSpec {
        key: "color_a",
        label: "Color A",
        kind: ParamKind::Gray { default: 0.0 },
    },
    ParamSpec {
        key: "color_b",
        label: "Color B",
        kind: ParamKind::Gray { default: 1.0 },
    },
];
const GENERATOR_UNIFORM: &[ParamSpec] = &[SEED];
const GENERATOR_VALUE: &[ParamSpec] = &[
    SEED,
    INTERPOLATION,
    frequency_x(8),
    frequency_y(8),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(1),
    LACUNARITY,
    GAIN,
    WARP,
];
const GENERATOR_LATTICE: &[ParamSpec] = &[
    SEED,
    frequency_x(8),
    frequency_y(8),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(1),
    LACUNARITY,
    GAIN,
    WARP,
];
const GENERATOR_FBM: &[ParamSpec] = &[
    SEED,
    BASIS,
    frequency_x(4),
    frequency_y(4),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(5),
    LACUNARITY,
    GAIN,
    WARP,
];
const GENERATOR_VORONOI: &[ParamSpec] = &[
    SEED,
    frequency_x(8),
    frequency_y(8),
    TRANSLATE_X,
    TRANSLATE_Y,
    float("jitter", "Jitter", 0.0, 1.0, 1.0),
    choice(
        "metric",
        "Metric",
        &["euclidean", "manhattan", "chebyshev"],
        0,
    ),
    choice("output", "Output", &["f1", "f2", "edge", "cell"], 0),
    WARP,
];
const GENERATOR_PERLIN_WORLEY: &[ParamSpec] = &[
    SEED,
    frequency_x(4),
    frequency_y(4),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(4),
    GAIN,
    int("worley_octaves", "Worley octaves", 1, 4, 3),
    WARP,
];
const GENERATOR_MUSGRAVE: &[ParamSpec] = &[
    SEED,
    BASIS,
    frequency_x(4),
    frequency_y(4),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(6),
    LACUNARITY,
    DIMENSION,
    WARP,
];
const GENERATOR_RIDGED: &[ParamSpec] = &[
    SEED,
    BASIS,
    frequency_x(4),
    frequency_y(4),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(6),
    LACUNARITY,
    DIMENSION,
    float("offset", "Offset", 0.0, 2.0, 1.0),
    float("gain", "Gain", 0.0, 4.0, 2.0),
    WARP,
];
const GENERATOR_CURL: &[ParamSpec] = &[
    SEED,
    BASIS,
    frequency_x(4),
    frequency_y(4),
    TRANSLATE_X,
    TRANSLATE_Y,
    octaves(2),
    LACUNARITY,
    GAIN,
    WARP,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GeneratorKind {
    Checkerboard,
    Uniform,
    Value,
    #[default]
    Perlin,
    Simplex,
    Fbm,
    Voronoi,
    PerlinWorley,
    Musgrave,
    Ridged,
    Curl,
}

impl GeneratorKind {
    pub const ALL: [Self; 11] = [
        Self::Checkerboard,
        Self::Uniform,
        Self::Value,
        Self::Perlin,
        Self::Simplex,
        Self::Fbm,
        Self::Voronoi,
        Self::PerlinWorley,
        Self::Musgrave,
        Self::Ridged,
        Self::Curl,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Checkerboard => "Checkerboard",
            Self::Uniform => "Uniform",
            Self::Value => "Value",
            Self::Perlin => "Perlin",
            Self::Simplex => "Simplex",
            Self::Fbm => "fBm",
            Self::Voronoi => "Voronoi",
            Self::PerlinWorley => "Perlin-Worley",
            Self::Musgrave => "Musgrave",
            Self::Ridged => "Ridged",
            Self::Curl => "Curl",
        }
    }

    /// The `type` value in the file.
    pub fn key(self) -> &'static str {
        match self {
            Self::Checkerboard => "pattern/checkerboard",
            Self::Uniform => "noise/uniform",
            Self::Value => "noise/value",
            Self::Perlin => "noise/perlin",
            Self::Simplex => "noise/simplex",
            Self::Fbm => "noise/fbm",
            Self::Voronoi => "noise/voronoi",
            Self::PerlinWorley => "noise/perlin-worley",
            Self::Musgrave => "noise/musgrave",
            Self::Ridged => "noise/ridged",
            Self::Curl => "noise/curl",
        }
    }

    /// File name for a new file: the last part of the type.
    pub fn file_stem(self) -> &'static str {
        self.key().rsplit('/').next().unwrap_or("noise")
    }

    pub fn specs(self) -> &'static [ParamSpec] {
        match self {
            Self::Checkerboard => GENERATOR_CHECKERBOARD,
            Self::Uniform => GENERATOR_UNIFORM,
            Self::Value => GENERATOR_VALUE,
            Self::Perlin | Self::Simplex => GENERATOR_LATTICE,
            Self::Fbm => GENERATOR_FBM,
            Self::Voronoi => GENERATOR_VORONOI,
            Self::PerlinWorley => GENERATOR_PERLIN_WORLEY,
            Self::Musgrave => GENERATOR_MUSGRAVE,
            Self::Ridged => GENERATOR_RIDGED,
            Self::Curl => GENERATOR_CURL,
        }
    }

    /// The table holding its parameters, e.g. `noise.perlin`; see
    /// docs/spec/anoise.md#generator-sections.
    pub fn section(self) -> String {
        self.key().replace('/', ".")
    }

    /// The generator whose [`key`](Self::key) is `key`.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// Its place in [`ALL`](Self::ALL).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// An `.anoise` file's content: a generator, its parameters and the modifiers after it.
#[derive(Debug, Clone, PartialEq)]
pub struct Noise {
    pub kind: GeneratorKind,
    pub size: u32,
    /// Parameters of `kind`.
    pub params: Params,
    /// Kept parameters of other generators, in [`GeneratorKind::ALL`] order.
    pub kept: Vec<(GeneratorKind, Params)>,
    pub modifiers: Vec<Modifier>,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new(GeneratorKind::default())
    }
}

impl Noise {
    /// `kind` at size 512 with its default parameters and no modifiers.
    pub fn new(kind: GeneratorKind) -> Self {
        Self {
            kind,
            size: 512,
            params: Params::defaults(kind.specs()),
            kept: Vec::new(),
            modifiers: Vec::new(),
        }
    }

    /// Switches the generator, keeping the current parameters and restoring `kind`'s.
    pub fn set_kind(&mut self, kind: GeneratorKind) {
        if kind == self.kind {
            return;
        }
        let params = match self.kept.iter().position(|(k, _)| *k == kind) {
            Some(i) => self.kept.remove(i).1,
            None => Params::defaults(kind.specs()),
        };
        let old = std::mem::replace(&mut self.params, params);
        // All-default parameters are not written, so there is nothing to keep.
        if old != Params::defaults(self.kind.specs()) {
            self.kept.push((self.kind, old));
            self.kept.sort_by_key(|(k, _)| k.index());
        }
        self.kind = kind;
    }

    /// Whether the generated image has color (otherwise R = G = B).
    pub fn has_color(&self) -> bool {
        self.kind == GeneratorKind::Curl
            || self
                .modifiers
                .iter()
                .any(|m| m.enabled && m.kind == ModifierKind::Colorize)
    }

    /// The image, `size` × `size` RGBA8 and opaque; see docs/spec/anoise.md#generated-image.
    /// A size or parameter out of its range is clamped into it; a parameter of the wrong
    /// kind, or missing, is its default.
    pub fn generate(&self) -> Image {
        let side = self.size.clamp(MIN_SIZE, MAX_SIZE);
        let size = side as usize;
        let params = in_range(self.kind.specs(), &self.params);
        let mut pixels = generate_field(self.kind, &params, size);
        apply_modifiers(&self.modifiers, &mut pixels, size, size);
        Image {
            width: side,
            height: side,
            pixels: pixels
                .iter()
                .flat_map(|c| {
                    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    [r, g, b, 255]
                })
                .collect(),
        }
    }
}

/// `params` for `specs`, each value of the right kind and in its range.
fn in_range(specs: &[ParamSpec], params: &Params) -> Params {
    let mut clean = Params::defaults(specs);
    for (i, (spec, value)) in specs.iter().zip(&mut clean.0).enumerate() {
        *value = match (spec.kind, params.0.get(i)) {
            (ParamKind::Int { min, max, .. }, Some(Value::Int(v))) => {
                Value::Int((*v).clamp(min, max))
            }
            (ParamKind::Float { min, max, .. }, Some(Value::Float(v))) if !v.is_nan() => {
                Value::Float(v.clamp(min, max))
            }
            (ParamKind::Gray { .. }, Some(Value::Float(v))) if !v.is_nan() => {
                Value::Float(v.clamp(0.0, 1.0))
            }
            (ParamKind::Choice { options, .. }, Some(Value::Choice(c))) => {
                Value::Choice((*c).min(options.len() - 1))
            }
            _ => continue,
        };
    }
    clean
}

// ---- Hashing --------------------------------------------------------------------------

const SECOND: u32 = 0x5bd1e995;

pub fn splitmix64(z: u64) -> u64 {
    let z = z.wrapping_add(0x9E3779B97F4A7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn hash(x: u32, y: u32, s: u32) -> u64 {
    splitmix64(splitmix64((s as u64) << 32 | x as u64) ^ y as u64)
}

fn hash4(i: i64, j: i64, k: i64, l: i64, s: u32) -> u64 {
    let h = splitmix64(s as u64 ^ i as u64);
    let h = splitmix64(h ^ j as u64);
    let h = splitmix64(h ^ k as u64);
    splitmix64(h ^ l as u64)
}

fn unit(h: u64) -> f64 {
    (h >> 40) as f64 / (1u64 << 24) as f64
}

// ---- Basic noises ---------------------------------------------------------------------

/// Lattice periods: cells across and down the image.
type Periods = (u64, u64);

#[derive(Clone, Copy)]
enum Curve {
    Linear,
    Cubic,
    Quintic,
}

impl Curve {
    const ALL: [Self; 3] = [Self::Linear, Self::Cubic, Self::Quintic];

    fn apply(self, t: f64) -> f64 {
        match self {
            Self::Linear => t,
            Self::Cubic => t * t * (3.0 - 2.0 * t),
            Self::Quintic => t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
        }
    }
}

#[derive(Clone, Copy)]
enum Basis {
    Value(Curve),
    Perlin,
    Simplex,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn wrap(i: i64, period: u64) -> u32 {
    i.rem_euclid(period as i64) as u32
}

fn interpolate(x: f64, y: f64, curve: Curve, corner: impl Fn(i64, i64, f64, f64) -> f64) -> f64 {
    let (i, j) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - i as f64, y - j as f64);
    let at = |di: i64, dj: i64| corner(i + di, j + dj, fx - di as f64, fy - dj as f64);
    let (tx, ty) = (curve.apply(fx), curve.apply(fy));
    lerp(
        lerp(at(0, 0), at(1, 0), tx),
        lerp(at(0, 1), at(1, 1), tx),
        ty,
    )
}

/// Basic noise of `basis` with periods `(p, q)` at image position `(u, v)`, in 0..1.
fn basic(basis: Basis, u: f64, v: f64, (p, q): Periods, s: u32) -> f64 {
    let (x, y) = (u * p as f64, v * q as f64);
    match basis {
        Basis::Value(curve) => interpolate(x, y, curve, |i, j, _, _| {
            unit(hash(wrap(i, p), wrap(j, q), s))
        }),
        Basis::Perlin => {
            let n = interpolate(x, y, Curve::Quintic, |i, j, dx, dy| {
                let angle = TAU * unit(hash(wrap(i, p), wrap(j, q), s));
                angle.cos() * dx + angle.sin() * dy
            });
            (0.5 + n * FRAC_1_SQRT_2).clamp(0.0, 1.0)
        }
        Basis::Simplex => {
            let (r, t) = (p as f64 / TAU, q as f64 / TAU);
            let (a, b) = (TAU * u, TAU * v);
            let n = simplex4([r * a.cos(), r * a.sin(), t * b.cos(), t * b.sin()], s);
            (0.5 + 0.5 * n).clamp(0.0, 1.0)
        }
    }
}

fn worley(u: f64, v: f64, (p, q): Periods, s: u32) -> f64 {
    let (x, y) = (u * p as f64, v * q as f64);
    let (i, j) = (x.floor() as i64, y.floor() as i64);
    let mut nearest = f64::MAX;
    for dj in -1..=1 {
        for di in -1..=1 {
            let (ci, cj) = (i + di, j + dj);
            let (wi, wj) = (wrap(ci, p), wrap(cj, q));
            let fx = ci as f64 + unit(hash(wi, wj, s));
            let fy = cj as f64 + unit(hash(wi, wj, s ^ SECOND));
            nearest = nearest.min((fx - x).hypot(fy - y));
        }
    }
    nearest.clamp(0.0, 1.0)
}

/// Gradient `index` (0..32): permutations of (0, ±1, ±1, ±1), zero position first.
fn grad4(index: u64) -> [f64; 4] {
    let zero = (index / 8) as usize;
    let signs = index % 8;
    let mut g = [0.0; 4];
    let mut bit = 2;
    for (axis, c) in g.iter_mut().enumerate() {
        if axis == zero {
            continue;
        }
        *c = if signs >> bit & 1 == 1 { -1.0 } else { 1.0 };
        bit -= 1;
    }
    g
}

/// 4D simplex noise (Gustavson), about −1..1.
fn simplex4(c: [f64; 4], s: u32) -> f64 {
    let f4 = (5f64.sqrt() - 1.0) / 4.0;
    let g4 = (5.0 - 5f64.sqrt()) / 20.0;
    let skew = c.iter().sum::<f64>() * f4;
    let cell = c.map(|v| (v + skew).floor() as i64);
    let unskew = cell.iter().sum::<i64>() as f64 * g4;
    let d0: [f64; 4] = std::array::from_fn(|a| c[a] - (cell[a] as f64 - unskew));
    let mut rank = [0; 4];
    for a in 0..4 {
        for b in a + 1..4 {
            if d0[a] > d0[b] {
                rank[a] += 1;
            } else {
                rank[b] += 1;
            }
        }
    }
    let mut n = 0.0;
    for corner in 0..5i64 {
        let offset: [i64; 4] = match corner {
            0 => [0; 4],
            4 => [1; 4],
            k => rank.map(|r| (r >= 4 - k) as i64),
        };
        let d: [f64; 4] = std::array::from_fn(|a| d0[a] - offset[a] as f64 + corner as f64 * g4);
        let t = 0.6 - d.iter().map(|v| v * v).sum::<f64>();
        if t > 0.0 {
            let p: [i64; 4] = std::array::from_fn(|a| cell[a] + offset[a]);
            let g = grad4(hash4(p[0], p[1], p[2], p[3], s) % 32);
            let dot: f64 = (0..4).map(|a| g[a] * d[a]).sum();
            n += t * t * t * t * dot;
        }
    }
    27.0 * n
}

/// Weighted octave average, octave `o` at periods `periods × lacunarity^o`, seed `seed + o`;
/// see docs/spec/anoise.md#fractal-sum.
fn fbm(
    noise: impl Fn(Periods, u32) -> f64,
    (p, q): Periods,
    octaves: u32,
    lacunarity: u64,
    gain: f64,
    seed: u32,
) -> f64 {
    let (mut sum, mut total, mut weight) = (0.0, 0.0, 1.0);
    for o in 0..octaves {
        let m = lacunarity.pow(o);
        sum += weight * noise((p * m, q * m), seed.wrapping_add(o));
        total += weight;
        weight *= gain;
    }
    sum / total
}

// ---- Generators -----------------------------------------------------------------------

/// Seeds of the two warp noises, see docs/spec/anoise.md#domain.
const WARP_U: u32 = 0x68E31DA4;
const WARP_V: u32 = 0xB5297A4D;

fn generate_field(kind: GeneratorKind, params: &Params, size: usize) -> Vec<[f64; 3]> {
    let specs = kind.specs();
    let int = |key| match params.get(specs, key) {
        Value::Int(v) => *v,
        _ => unreachable!(),
    };
    let float = |key| match params.get(specs, key) {
        Value::Float(v) => *v,
        _ => unreachable!(),
    };
    let choice = |key| match params.get(specs, key) {
        Value::Choice(c) => *c,
        _ => unreachable!(),
    };
    // Shared parameters a generator may not have.
    let optional = |key| match params.find(specs, key) {
        Some(Value::Float(v)) => *v,
        _ => 0.0,
    };
    let seed = || int("seed") as u32;
    let periods = || (int("frequency_x") as u64, int("frequency_y") as u64);
    let octaves = || int("octaves") as u32;
    let lacunarity = || int("lacunarity") as u64;
    let basis = match kind {
        GeneratorKind::Value => Basis::Value(Curve::ALL[choice("interpolation")]),
        GeneratorKind::Simplex => Basis::Simplex,
        _ => match params.find(specs, "basis") {
            Some(Value::Choice(c)) => {
                [Basis::Value(Curve::Quintic), Basis::Perlin, Basis::Simplex][*c]
            }
            _ => Basis::Perlin,
        },
    };

    let (translate_x, translate_y, warp) = (
        optional("translate_x"),
        optional("translate_y"),
        optional("warp"),
    );
    let domain = |u: f64, v: f64| {
        let (u, v) = (u + translate_x, v + translate_y);
        if warp == 0.0 {
            return (u, v);
        }
        let (p, q) = periods();
        let a = basic(basis, u, v, (p, q), seed() ^ WARP_U);
        let b = basic(basis, u, v, (p, q), seed() ^ WARP_V);
        (
            u + warp * (2.0 * a - 1.0) / p as f64,
            v + warp * (2.0 * b - 1.0) / q as f64,
        )
    };
    let gray = |f: &dyn Fn(f64, f64, usize, usize) -> f64| -> Vec<[f64; 3]> {
        (0..size * size)
            .map(|i| {
                let (px, py) = (i % size, i / size);
                let u = (px as f64 + 0.5) / size as f64;
                let v = (py as f64 + 0.5) / size as f64;
                let (u, v) = domain(u, v);
                [f(u, v, px, py); 3]
            })
            .collect()
    };
    let fractal = |u, v, gain| {
        fbm(
            |pq, s| basic(basis, u, v, pq, s),
            periods(),
            octaves(),
            lacunarity(),
            gain,
            seed(),
        )
    };
    // Signed basis noise of octave `o`, for the multifractals.
    let octave = |u, v, o: u32| {
        let ((p, q), m) = (periods(), lacunarity().pow(o));
        2.0 * basic(basis, u, v, (p * m, q * m), seed().wrapping_add(o)) - 1.0
    };
    let multifractal_weight = |o: u32| (lacunarity() as f64).powf(-float("dimension") * o as f64);

    match kind {
        GeneratorKind::Checkerboard => {
            let (cells_x, cells_y) = (int("cells_x") as f64, int("cells_y") as f64);
            let (a, b) = (float("color_a"), float("color_b"));
            gray(&|u, v, _, _| {
                let parity =
                    ((u * cells_x).floor() as i64 + (v * cells_y).floor() as i64).rem_euclid(2);
                if parity == 0 { a } else { b }
            })
        }
        GeneratorKind::Uniform => gray(&|_, _, px, py| unit(hash(px as u32, py as u32, seed()))),
        GeneratorKind::Value
        | GeneratorKind::Perlin
        | GeneratorKind::Simplex
        | GeneratorKind::Fbm => gray(&|u, v, _, _| fractal(u, v, float("gain"))),
        GeneratorKind::Voronoi => {
            let (jitter, metric, output) = (float("jitter"), choice("metric"), choice("output"));
            gray(&|u, v, _, _| voronoi(u, v, periods(), seed(), jitter, metric, output))
        }
        GeneratorKind::PerlinWorley => gray(&|u, v, _, _| {
            let perlin = |pq, s| basic(Basis::Perlin, u, v, pq, s);
            let p = fbm(perlin, periods(), octaves(), 2, float("gain"), seed());
            let w = 1.0
                - fbm(
                    |pq, s| worley(u, v, pq, s),
                    periods(),
                    int("worley_octaves") as u32,
                    2,
                    0.5,
                    seed().wrapping_add(100),
                );
            ((p - w + 1.0) / (2.0 - w)).clamp(0.0, 1.0)
        }),
        GeneratorKind::Musgrave => gray(&|u, v, _, _| {
            let (mut value, mut max) = (1.0, 1.0);
            for o in 0..octaves() {
                let w = multifractal_weight(o);
                value *= 1.0 + octave(u, v, o) * w;
                max *= 1.0 + w;
            }
            value / max
        }),
        GeneratorKind::Ridged => {
            let (offset, gain) = (float("offset"), float("gain"));
            gray(&|u, v, _, _| {
                let signal = |o: u32| (offset - octave(u, v, o).abs()).powi(2);
                let mut previous = signal(0);
                let (mut sum, mut max) = (previous, offset * offset);
                for o in 1..octaves() {
                    let weight = (previous * gain).clamp(0.0, 1.0);
                    previous = signal(o) * weight;
                    let w = multifractal_weight(o);
                    sum += previous * w;
                    max += offset * offset * w;
                }
                if max > 0.0 {
                    (sum / max).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
        }
        GeneratorKind::Curl => {
            let psi: Vec<f64> = gray(&|u, v, _, _| 2.0 * fractal(u, v, float("gain")) - 1.0)
                .iter()
                .map(|c| c[0])
                .collect();
            let at = |x: usize, y: usize| psi[(y % size) * size + x % size];
            let vectors: Vec<[f64; 2]> = (0..size * size)
                .map(|i| {
                    let (x, y) = (i % size, i / size);
                    let dx = (at(x + 1, y) - at(x + size - 1, y)) / 2.0;
                    let dy = (at(x, y + 1) - at(x, y + size - 1)) / 2.0;
                    [dy, -dx]
                })
                .collect();
            let longest = vectors.iter().map(|[x, y]| x.hypot(*y)).fold(0.0, f64::max);
            let scale = if longest > 0.0 { 1.0 / longest } else { 1.0 };
            vectors
                .iter()
                .map(|[x, y]| [0.5 + 0.5 * x * scale, 0.5 + 0.5 * y * scale, 0.0])
                .collect()
        }
    }
}

fn voronoi(
    u: f64,
    v: f64,
    (p, q): Periods,
    s: u32,
    jitter: f64,
    metric: usize,
    output: usize,
) -> f64 {
    let (x, y) = (u * p as f64, v * q as f64);
    let (i, j) = (x.floor() as i64, y.floor() as i64);
    let (mut f1, mut f2, mut nearest) = (f64::MAX, f64::MAX, (0, 0));
    for dj in -2..=2 {
        for di in -2..=2 {
            let (ci, cj) = (i + di, j + dj);
            let (wi, wj) = (wrap(ci, p), wrap(cj, q));
            let fx = ci as f64 + 0.5 + jitter * (unit(hash(wi, wj, s)) - 0.5);
            let fy = cj as f64 + 0.5 + jitter * (unit(hash(wi, wj, s ^ SECOND)) - 0.5);
            let (dx, dy) = ((fx - x).abs(), (fy - y).abs());
            let d = match metric {
                0 => dx.hypot(dy),
                1 => dx + dy,
                _ => dx.max(dy),
            };
            if d < f1 {
                (f2, f1, nearest) = (f1, d, (wi, wj));
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    let value = match output {
        0 => f1,
        1 => f2,
        2 => f2 - f1,
        _ => unit(hash(nearest.0, nearest.1, s ^ 0x2545F491)),
    };
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small(kind: GeneratorKind) -> Noise {
        Noise {
            size: 32,
            ..Noise::new(kind)
        }
    }

    fn gray(image: &Image, x: u32, y: u32) -> u8 {
        image.pixels[((y * image.width + x) * 4) as usize]
    }

    #[test]
    fn splitmix_reference() {
        // The first output of SplitMix64 seeded with 0.
        assert_eq!(splitmix64(0), 0xE220A8397B1DCDAF);
        assert_ne!(hash(1, 0, 0), hash(0, 1, 0));
        assert_ne!(hash4(1, 0, 0, 0, 0), hash4(0, 1, 0, 0, 0));
    }

    #[test]
    fn grad4_table() {
        assert_eq!(grad4(0), [0.0, 1.0, 1.0, 1.0]);
        assert_eq!(grad4(1), [0.0, 1.0, 1.0, -1.0]);
        assert_eq!(grad4(4), [0.0, -1.0, 1.0, 1.0]);
        assert_eq!(grad4(8), [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(grad4(31), [-1.0, -1.0, -1.0, 0.0]);
    }

    #[test]
    fn every_generator_is_deterministic_and_varied() {
        for kind in GeneratorKind::ALL {
            let a = small(kind).generate();
            assert_eq!(a, small(kind).generate(), "{kind:?}");
            assert_eq!(a.pixels.len(), 32 * 32 * 4);
            let distinct: std::collections::HashSet<_> =
                a.pixels.as_chunks::<4>().0.iter().collect();
            assert!(distinct.len() >= 2, "{kind:?} is flat");
            let is_gray = a
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2]);
            assert_eq!(is_gray, kind != GeneratorKind::Curl, "{kind:?}");
            assert_eq!(small(kind).has_color(), kind == GeneratorKind::Curl);
        }
    }

    #[test]
    fn noises_tile_seamlessly() {
        for kind in GeneratorKind::ALL.into_iter().filter(|k| {
            !matches!(
                k,
                GeneratorKind::Checkerboard | GeneratorKind::Uniform | GeneratorKind::Curl
            )
        }) {
            let image = Noise {
                size: 128,
                ..Noise::new(kind)
            }
            .generate();
            for y in (0..128).step_by(9) {
                let (left, right) = (gray(&image, 0, y), gray(&image, 127, y));
                assert!(
                    left.abs_diff(right) < 40,
                    "{kind:?} y={y}: {left} vs {right}"
                );
            }
        }
    }

    #[test]
    fn simplex_stays_in_range() {
        let image = Noise {
            size: 64,
            ..Noise::new(GeneratorKind::Simplex)
        }
        .generate();
        let values: Vec<u8> = image.pixels.iter().step_by(4).copied().collect();
        let (lo, hi) = (values.iter().min().unwrap(), values.iter().max().unwrap());
        assert!(*lo < 90 && *hi > 165, "{lo}..{hi}");
    }

    fn with(kind: GeneratorKind, size: u32, set: &[(&str, Value)]) -> Image {
        let mut noise = Noise {
            size,
            ..Noise::new(kind)
        };
        for (key, value) in set {
            let i = kind.specs().iter().position(|s| s.key == *key).unwrap();
            noise.params.0[i] = value.clone();
        }
        noise.generate()
    }

    #[test]
    fn every_setting_keeps_tiling() {
        let settings: &[(GeneratorKind, &[(&str, Value)])] = &[
            (
                GeneratorKind::Perlin,
                &[
                    ("frequency_x", Value::Int(3)),
                    ("frequency_y", Value::Int(7)),
                ],
            ),
            (
                GeneratorKind::Simplex,
                &[
                    ("frequency_x", Value::Int(2)),
                    ("frequency_y", Value::Int(9)),
                ],
            ),
            (
                GeneratorKind::Value,
                &[
                    ("octaves", Value::Int(4)),
                    ("translate_x", Value::Float(0.37)),
                ],
            ),
            (GeneratorKind::Fbm, &[("warp", Value::Float(2.0))]),
            (
                GeneratorKind::Voronoi,
                &[("frequency_y", Value::Int(3)), ("warp", Value::Float(1.0))],
            ),
            (
                GeneratorKind::Ridged,
                &[
                    ("basis", Value::Choice(2)),
                    ("octaves", Value::Int(3)),
                    ("translate_y", Value::Float(0.5)),
                ],
            ),
        ];
        for (kind, set) in settings {
            let image = with(*kind, 128, set);
            for i in (0..128).step_by(9) {
                let (left, right) = (gray(&image, 0, i), gray(&image, 127, i));
                assert!(left.abs_diff(right) < 40, "{kind:?} {set:?} y={i}");
                let (top, bottom) = (gray(&image, i, 0), gray(&image, i, 127));
                assert!(top.abs_diff(bottom) < 40, "{kind:?} {set:?} x={i}");
            }
        }
    }

    #[test]
    fn settings_change_the_image() {
        let plain = with(GeneratorKind::Perlin, 32, &[]);
        for set in [
            ("frequency_x", Value::Int(3)),
            ("translate_x", Value::Float(0.25)),
            ("octaves", Value::Int(3)),
            ("warp", Value::Float(1.0)),
        ] {
            assert_ne!(
                with(GeneratorKind::Perlin, 32, std::slice::from_ref(&set)),
                plain,
                "{set:?}"
            );
        }
        // Gain only matters with more than one octave.
        let detail = [("octaves", Value::Int(3))];
        assert_ne!(
            with(
                GeneratorKind::Perlin,
                32,
                &[detail[0].clone(), ("gain", Value::Float(0.9))]
            ),
            with(GeneratorKind::Perlin, 32, &detail),
        );
        let value = with(GeneratorKind::Value, 32, &[]);
        assert_ne!(
            with(
                GeneratorKind::Value,
                32,
                &[("interpolation", Value::Choice(0))]
            ),
            value
        );
        // Translating by a whole image is no translation.
        let shifted = with(
            GeneratorKind::Value,
            32,
            &[("translate_x", Value::Float(1.0))],
        );
        for (a, b) in shifted.pixels.iter().zip(&value.pixels) {
            assert!(a.abs_diff(*b) <= 1);
        }
        // Voronoi F2 is never below F1.
        let f1 = with(GeneratorKind::Voronoi, 32, &[]);
        let f2 = with(GeneratorKind::Voronoi, 32, &[("output", Value::Choice(1))]);
        assert!(f1.pixels.iter().zip(&f2.pixels).all(|(a, b)| a <= b));
        assert_ne!(f1, f2);
    }

    #[test]
    fn disabled_modifiers_are_skipped() {
        let mut noise = small(GeneratorKind::Value);
        let plain = noise.generate();
        let mut invert = Modifier::new(ModifierKind::Invert);
        invert.enabled = false;
        noise.modifiers.push(invert);
        assert_eq!(noise.generate(), plain);
        noise.modifiers[0].enabled = true;
        assert_ne!(noise.generate(), plain);
    }

    #[test]
    fn switching_generators_keeps_their_parameters() {
        let mut noise = Noise::new(GeneratorKind::Fbm);
        noise.params.0[0] = Value::Int(7);
        noise.set_kind(GeneratorKind::Checkerboard);
        noise.set_kind(GeneratorKind::Voronoi);
        // Checkerboard's untouched defaults are not kept.
        assert_eq!(
            noise.kept,
            vec![(GeneratorKind::Fbm, noise.kept[0].1.clone())]
        );
        noise.set_kind(GeneratorKind::Fbm);
        assert_eq!(noise.params.0[0], Value::Int(7));
        assert!(noise.kept.is_empty());
        for kind in GeneratorKind::ALL {
            assert_eq!(GeneratorKind::from_key(kind.key()), Some(kind));
            assert_eq!(GeneratorKind::ALL[kind.index()], kind);
        }
        assert_eq!(GeneratorKind::Fbm.section(), "noise.fbm");
        assert_eq!(GeneratorKind::PerlinWorley.file_stem(), "perlin-worley");
    }

    #[test]
    fn values_out_of_range_are_clamped() {
        let image = with(GeneratorKind::Perlin, 16, &[("frequency_x", Value::Int(0))]);
        assert_eq!(
            image,
            with(GeneratorKind::Perlin, 16, &[("frequency_x", Value::Int(1))])
        );
        let wrong = with(
            GeneratorKind::Voronoi,
            16,
            &[
                ("output", Value::Choice(9)),
                ("jitter", Value::Float(f64::NAN)),
            ],
        );
        assert_eq!(
            wrong,
            with(GeneratorKind::Voronoi, 16, &[("output", Value::Choice(3))])
        );
        let mut noise = small(GeneratorKind::Fbm);
        noise.params = Params(Vec::new());
        assert_eq!(noise.generate(), small(GeneratorKind::Fbm).generate());
        noise.size = 0;
        assert_eq!(noise.generate().pixels.len(), 4);
    }
}
