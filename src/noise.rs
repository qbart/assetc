//! Procedural `.noise` textures: generators, modifiers and the file format.
//! See docs/spec/noise.md; parameters are declared once per type in tables, and parsing,
//! writing and the editor are driven by them.

use std::f64::consts::{FRAC_1_SQRT_2, TAU};

use crate::texture::Image;

/// The type of a parameter, with its range and default.
#[derive(Debug, Clone, Copy)]
pub enum ParamKind {
    Int {
        min: i64,
        max: i64,
        default: i64,
    },
    Float {
        min: f64,
        max: f64,
        default: f64,
    },
    Choice {
        options: &'static [&'static str],
        default: usize,
    },
    /// A float in 0..=1.
    Gray {
        default: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    /// Floats and grays.
    Float(f64),
    Choice(usize),
}

/// Parameter values, in the order of their [`ParamSpec`]s.
#[derive(Debug, Clone, PartialEq)]
pub struct Params(pub Vec<Value>);

const fn int(
    key: &'static str,
    label: &'static str,
    min: i64,
    max: i64,
    default: i64,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Int { min, max, default },
    }
}

const fn float(
    key: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    default: f64,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Float { min, max, default },
    }
}

const fn choice(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
    default: usize,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Choice { options, default },
    }
}

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
    /// docs/spec/noise.md#generator-sections.
    pub fn section(self) -> String {
        self.key().replace('/', ".")
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

const MODIFIER_NONE: &[ParamSpec] = &[];
const MODIFIER_LEVELS: &[ParamSpec] = &[
    float("black", "Black", 0.0, 1.0, 0.0),
    float("white", "White", 0.0, 1.0, 1.0),
    float("gamma", "Gamma", 0.1, 10.0, 1.0),
];
const MODIFIER_BRIGHTNESS_CONTRAST: &[ParamSpec] = &[
    float("brightness", "Brightness", -1.0, 1.0, 0.0),
    float("contrast", "Contrast", -1.0, 1.0, 0.0),
];
const MODIFIER_THRESHOLD: &[ParamSpec] = &[
    float("level", "Level", 0.0, 1.0, 0.5),
    float("softness", "Softness", 0.0, 0.5, 0.0),
];
const MODIFIER_POSTERIZE: &[ParamSpec] = &[int("steps", "Steps", 2, 32, 4)];
const MODIFIER_BLUR: &[ParamSpec] = &[
    int("radius", "Radius", 1, 64, 2),
    choice("edges", "Edges", &["wrap", "clamp"], 0),
];
const MODIFIER_COLORIZE: &[ParamSpec] = &[
    float("hue", "Hue", 0.0, 360.0, 180.0),
    float("saturation", "Saturation", 0.0, 1.0, 0.5),
    float("lightness", "Lightness", -1.0, 1.0, 0.0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKind {
    Invert,
    Levels,
    BrightnessContrast,
    Threshold,
    Posterize,
    Blur,
    Normalize,
    Colorize,
}

impl ModifierKind {
    pub const ALL: [Self; 8] = [
        Self::Invert,
        Self::Levels,
        Self::BrightnessContrast,
        Self::Threshold,
        Self::Posterize,
        Self::Blur,
        Self::Normalize,
        Self::Colorize,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Invert => "Invert",
            Self::Levels => "Levels",
            Self::BrightnessContrast => "Brightness / Contrast",
            Self::Threshold => "Threshold",
            Self::Posterize => "Posterize",
            Self::Blur => "Blur",
            Self::Normalize => "Normalize",
            Self::Colorize => "Colorize",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Invert => "invert",
            Self::Levels => "levels",
            Self::BrightnessContrast => "brightness-contrast",
            Self::Threshold => "threshold",
            Self::Posterize => "posterize",
            Self::Blur => "blur",
            Self::Normalize => "normalize",
            Self::Colorize => "colorize",
        }
    }

    pub fn specs(self) -> &'static [ParamSpec] {
        match self {
            Self::Invert | Self::Normalize => MODIFIER_NONE,
            Self::Levels => MODIFIER_LEVELS,
            Self::BrightnessContrast => MODIFIER_BRIGHTNESS_CONTRAST,
            Self::Threshold => MODIFIER_THRESHOLD,
            Self::Posterize => MODIFIER_POSTERIZE,
            Self::Blur => MODIFIER_BLUR,
            Self::Colorize => MODIFIER_COLORIZE,
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Modifier {
    pub kind: ModifierKind,
    pub enabled: bool,
    pub params: Params,
}

impl Modifier {
    pub fn new(kind: ModifierKind) -> Self {
        Self {
            kind,
            enabled: true,
            params: Params::defaults(kind.specs()),
        }
    }
}

/// A `.noise` file.
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

impl Params {
    pub fn defaults(specs: &[ParamSpec]) -> Self {
        Self(
            specs
                .iter()
                .map(|spec| match spec.kind {
                    ParamKind::Int { default, .. } => Value::Int(default),
                    ParamKind::Float { default, .. } | ParamKind::Gray { default } => {
                        Value::Float(default)
                    }
                    ParamKind::Choice { default, .. } => Value::Choice(default),
                })
                .collect(),
        )
    }

    /// Reads `specs` from `table` (removing them); missing keys get defaults.
    fn take(specs: &[ParamSpec], table: &mut toml::Table) -> Result<Self, String> {
        let mut params = Self::defaults(specs);
        for (spec, value) in specs.iter().zip(&mut params.0) {
            let Some(raw) = table.remove(spec.key) else {
                continue;
            };
            let bad = || format!("{} has an invalid value {raw}", spec.key);
            *value = match spec.kind {
                ParamKind::Int { min, max, .. } => {
                    let v = raw.as_integer().ok_or_else(bad)?;
                    in_range(spec.key, v as f64, min as f64, max as f64)?;
                    Value::Int(v)
                }
                ParamKind::Float { min, max, .. } => {
                    let v = number(&raw).ok_or_else(bad)?;
                    in_range(spec.key, v, min, max)?;
                    Value::Float(v)
                }
                ParamKind::Gray { .. } => {
                    let v = number(&raw).ok_or_else(bad)?;
                    in_range(spec.key, v, 0.0, 1.0)?;
                    Value::Float(v)
                }
                ParamKind::Choice { options, .. } => {
                    let v = raw.as_str().ok_or_else(bad)?;
                    let i = options.iter().position(|o| *o == v).ok_or_else(|| {
                        format!("{} must be one of {}", spec.key, options.join(", "))
                    })?;
                    Value::Choice(i)
                }
            };
        }
        Ok(params)
    }

    fn write(&self, specs: &[ParamSpec], out: &mut String) {
        for (spec, value) in specs.iter().zip(&self.0) {
            let value = match (value, spec.kind) {
                (Value::Int(v), _) => toml::Value::Integer(*v),
                (Value::Float(v), _) => toml::Value::Float(*v),
                (Value::Choice(i), ParamKind::Choice { options, .. }) => {
                    toml::Value::String(options[*i].into())
                }
                (Value::Choice(_), _) => unreachable!("choice without options"),
            };
            out.push_str(&format!("{} = {value}\n", spec.key));
        }
    }

    /// The value of `key` as text, as shown in the Inspector.
    pub fn display(&self, specs: &[ParamSpec], i: usize) -> String {
        match (&self.0[i], specs[i].kind) {
            (Value::Int(v), _) => v.to_string(),
            (Value::Float(v), _) => format!("{v:.2}"),
            (Value::Choice(c), ParamKind::Choice { options, .. }) => options[*c].into(),
            (Value::Choice(c), _) => c.to_string(),
        }
    }

    fn get<'a>(&'a self, specs: &[ParamSpec], key: &str) -> &'a Value {
        self.find(specs, key)
            .unwrap_or_else(|| panic!("no parameter {key}"))
    }

    fn find<'a>(&'a self, specs: &[ParamSpec], key: &str) -> Option<&'a Value> {
        specs.iter().position(|s| s.key == key).map(|i| &self.0[i])
    }
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
        .filter(|f| f.is_finite())
}

fn in_range(key: &str, v: f64, min: f64, max: f64) -> Result<(), String> {
    if (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(format!("{key} must be {min}..={max}, not {v}"))
    }
}

impl Noise {
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

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let kind = match table.remove("type") {
            None => GeneratorKind::default(),
            Some(v) => v
                .as_str()
                .and_then(GeneratorKind::from_key)
                .ok_or_else(|| format!("unknown type {v}"))?,
        };
        let size = match table.remove("size") {
            None => 512,
            Some(v) => {
                let v = v.as_integer().ok_or("size must be an integer")?;
                in_range("size", v as f64, 1.0, 4096.0)?;
                v as u32
            }
        };
        let modifiers = match table.remove("modifiers") {
            None => Vec::new(),
            Some(value) => parse_modifiers(value)?,
        };
        let mut sections = Vec::new();
        for (group, value) in table {
            let toml::Value::Table(group_table) = value else {
                return Err(format!(
                    "unknown key {group} (parameters go in [{}])",
                    kind.section()
                ));
            };
            for (name, value) in group_table {
                let section = format!("{group}.{name}");
                let generator = GeneratorKind::from_key(&format!("{group}/{name}"))
                    .ok_or_else(|| format!("unknown section [{section}]"))?;
                let toml::Value::Table(mut values) = value else {
                    return Err(format!("[{section}] must be a table"));
                };
                let params = Params::take(generator.specs(), &mut values)
                    .map_err(|e| format!("[{section}] {e}"))?;
                if let Some(key) = values.keys().next() {
                    return Err(format!("[{section}] has no parameter {key}"));
                }
                sections.push((generator, params));
            }
        }
        let params = match sections.iter().position(|(k, _)| *k == kind) {
            Some(i) => sections.remove(i).1,
            None => Params::defaults(kind.specs()),
        };
        sections.retain(|(k, p)| *p != Params::defaults(k.specs()));
        sections.sort_by_key(|(k, _)| k.index());
        Ok(Self {
            kind,
            size,
            params,
            kept: sections,
            modifiers,
        })
    }

    /// The written form, see docs/spec/noise.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = format!(
            "type = {}\nsize = {}\n\n[{}]\n",
            toml::Value::String(self.kind.key().into()),
            self.size,
            self.kind.section()
        );
        self.params.write(self.kind.specs(), &mut out);
        for (kind, params) in &self.kept {
            if *params != Params::defaults(kind.specs()) {
                out.push_str(&format!("\n[{}]\n", kind.section()));
                params.write(kind.specs(), &mut out);
            }
        }
        write_modifiers(&self.modifiers, &mut out);
        out
    }

    /// Whether the generated image has color (otherwise R = G = B).
    pub fn has_color(&self) -> bool {
        self.kind == GeneratorKind::Curl
            || self
                .modifiers
                .iter()
                .any(|m| m.enabled && m.kind == ModifierKind::Colorize)
    }

    /// The image, see docs/spec/noise.md#generated-image.
    pub fn generate(&self) -> Image {
        let size = self.size as usize;
        let mut pixels = generate_field(self.kind, &self.params, size);
        for m in self.modifiers.iter().filter(|m| m.enabled) {
            apply_modifier(m, &mut pixels, size, size);
        }
        Image {
            width: self.size,
            height: self.size,
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

/// A `modifiers` array of tables, as in `.noise` files and `.amod` sidecars.
pub fn parse_modifiers(value: toml::Value) -> Result<Vec<Modifier>, String> {
    let toml::Value::Array(items) = value else {
        return Err("modifiers must be an array of tables".into());
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| parse_modifier(item).map_err(|e| format!("modifier {}: {e}", i + 1)))
        .collect()
}

/// Appends `[[modifiers]]` tables, each after a blank line.
pub fn write_modifiers(modifiers: &[Modifier], out: &mut String) {
    write_modifier_tables(modifiers, "modifiers", out);
}

/// Appends `[[<table>]]` tables (`modifiers`, `gradients.modifiers`), each after a blank line.
pub fn write_modifier_tables(modifiers: &[Modifier], table: &str, out: &mut String) {
    for m in modifiers {
        out.push_str(&format!(
            "\n[[{table}]]\ntype = {}\nenabled = {}\n",
            toml::Value::String(m.kind.key().into()),
            m.enabled
        ));
        m.params.write(m.kind.specs(), out);
    }
}

/// `Levels, Blur (off)`, or `none`.
pub fn modifier_summary(modifiers: &[Modifier]) -> String {
    if modifiers.is_empty() {
        return "none".into();
    }
    modifiers
        .iter()
        .map(|m| {
            let off = if m.enabled { "" } else { " (off)" };
            format!("{}{off}", m.kind.name())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs the enabled modifiers on a `width × height` RGB image (0–1 per channel).
pub fn apply_modifiers(
    modifiers: &[Modifier],
    pixels: &mut [[f64; 3]],
    width: usize,
    height: usize,
) {
    for m in modifiers.iter().filter(|m| m.enabled) {
        apply_modifier(m, pixels, width, height);
    }
}

fn parse_modifier(item: toml::Value) -> Result<Modifier, String> {
    let toml::Value::Table(mut table) = item else {
        return Err("not a table".into());
    };
    let kind = table
        .remove("type")
        .and_then(|v| v.as_str().and_then(ModifierKind::from_key))
        .ok_or("missing or unknown type")?;
    let enabled = match table.remove("enabled") {
        None => true,
        Some(v) => v.as_bool().ok_or("enabled must be true or false")?,
    };
    let params = Params::take(kind.specs(), &mut table)?;
    if let Some(key) = table.keys().next() {
        return Err(format!("{} has no parameter {key}", kind.key()));
    }
    Ok(Modifier {
        kind,
        enabled,
        params,
    })
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
/// see docs/spec/noise.md#fractal-sum.
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

/// Seeds of the two warp noises, see docs/spec/noise.md#domain.
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

// ---- Modifiers ------------------------------------------------------------------------

fn apply_modifier(m: &Modifier, pixels: &mut [[f64; 3]], width: usize, height: usize) {
    let specs = m.kind.specs();
    let float = |key| match m.params.get(specs, key) {
        Value::Float(v) => *v,
        _ => unreachable!(),
    };
    let int = |key| match m.params.get(specs, key) {
        Value::Int(v) => *v,
        _ => unreachable!(),
    };
    let each = |pixels: &mut [[f64; 3]], f: &dyn Fn(f64) -> f64| {
        for c in pixels.iter_mut().flatten() {
            *c = f(*c).clamp(0.0, 1.0);
        }
    };
    match m.kind {
        ModifierKind::Invert => each(pixels, &|c| 1.0 - c),
        ModifierKind::Levels => {
            let (black, white, gamma) = (float("black"), float("white"), float("gamma"));
            each(pixels, &|c| {
                if white <= black {
                    if c >= black { 1.0 } else { 0.0 }
                } else {
                    ((c - black) / (white - black))
                        .clamp(0.0, 1.0)
                        .powf(1.0 / gamma)
                }
            });
        }
        ModifierKind::BrightnessContrast => {
            let (brightness, contrast) = (float("brightness"), float("contrast"));
            each(pixels, &|c| (c - 0.5) * (1.0 + contrast) + 0.5 + brightness);
        }
        ModifierKind::Threshold => {
            let (level, softness) = (float("level"), float("softness"));
            each(pixels, &|c| {
                if softness == 0.0 {
                    if c >= level { 1.0 } else { 0.0 }
                } else {
                    let t = ((c - (level - softness)) / (2.0 * softness)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                }
            });
        }
        ModifierKind::Posterize => {
            let steps = (int("steps") - 1) as f64;
            each(pixels, &|c| (c * steps).round() / steps);
        }
        ModifierKind::Blur => {
            let radius = int("radius") as usize;
            let wrap = matches!(m.params.get(specs, "edges"), Value::Choice(0));
            for _ in 0..3 {
                box_blur(pixels, width, height, radius, true, wrap);
                box_blur(pixels, width, height, radius, false, wrap);
            }
        }
        ModifierKind::Normalize => {
            let (min, max) = pixels
                .iter()
                .flatten()
                .fold((f64::MAX, f64::MIN), |(lo, hi), c| (lo.min(*c), hi.max(*c)));
            if max > min {
                each(pixels, &|c| (c - min) / (max - min));
            }
        }
        ModifierKind::Colorize => {
            let (hue, saturation, lightness) =
                (float("hue"), float("saturation"), float("lightness"));
            for c in pixels.iter_mut() {
                let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                let l = if lightness > 0.0 {
                    l * (1.0 - lightness) + lightness
                } else {
                    l * (1.0 + lightness)
                };
                *c = hsl_to_rgb(hue, saturation, l).map(|v| v.clamp(0.0, 1.0));
            }
        }
    }
}

/// HSL (hue in degrees) to RGB; see docs/spec/noise.md#colorize.
fn hsl_to_rgb(hue: f64, saturation: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * saturation;
    let h = (hue / 60.0).clamp(0.0, 6.0);
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// One `(2 radius + 1)`-wide box blur pass along rows or columns of a `width × height` image;
/// past the edges it wraps around or repeats the edge pixel.
fn box_blur(
    pixels: &mut [[f64; 3]],
    width: usize,
    height: usize,
    radius: usize,
    rows: bool,
    wrap: bool,
) {
    let (lines, length) = if rows {
        (height, width)
    } else {
        (width, height)
    };
    let span = (2 * radius + 1) as f64;
    let index = |line: usize, k: usize| {
        if rows {
            line * width + k
        } else {
            k * width + line
        }
    };
    let mut out = vec![[0.0; 3]; length];
    for line in 0..lines {
        let at = |k: isize| {
            let k = if wrap {
                k.rem_euclid(length as isize)
            } else {
                k.clamp(0, length as isize - 1)
            };
            pixels[index(line, k as usize)]
        };
        let mut sum = [0.0; 3];
        for k in -(radius as isize)..=radius as isize {
            let p = at(k);
            (0..3).for_each(|c| sum[c] += p[c]);
        }
        for (k, slot) in out.iter_mut().enumerate() {
            *slot = sum.map(|s| s / span);
            let (enter, leave) = (
                at(k as isize + radius as isize + 1),
                at(k as isize - radius as isize),
            );
            (0..3).for_each(|c| sum[c] += enter[c] - leave[c]);
        }
        for (k, value) in out.iter().enumerate() {
            pixels[index(line, k)] = *value;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn blur_works_on_any_size_with_either_edge() {
        // A 4 × 2 image, white in the leftmost column only.
        let image = || {
            (0..8)
                .map(|i| if i % 4 == 0 { [1.0; 3] } else { [0.0; 3] })
                .collect::<Vec<_>>()
        };
        let mut blur = Modifier::new(ModifierKind::Blur);
        blur.params.0[0] = Value::Int(1);
        let mut wrapped = image();
        apply_modifiers(std::slice::from_ref(&blur), &mut wrapped, 4, 2);
        blur.params.0[1] = Value::Choice(1);
        let mut clamped = image();
        apply_modifiers(std::slice::from_ref(&blur), &mut clamped, 4, 2);
        // Wrapping carries the left column into the right one; clamping does not.
        assert!(wrapped[3][0] > clamped[3][0]);
        // Rows stay alike: the blur is per line of the right length.
        assert_eq!(wrapped[0], wrapped[4]);
        // Disabled modifiers are skipped.
        blur.enabled = false;
        let mut untouched = image();
        apply_modifiers(&[blur], &mut untouched, 4, 2);
        assert_eq!(untouched, image());
    }

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
    fn written_form_round_trips() {
        let mut noise = Noise::new(GeneratorKind::Fbm);
        noise.modifiers.push(Modifier::new(ModifierKind::Levels));
        let mut off = Modifier::new(ModifierKind::Colorize);
        off.enabled = false;
        noise.modifiers.push(off);
        let text = noise.to_toml();
        assert!(text.starts_with(
            "type = \"noise/fbm\"\nsize = 512\n\n[noise.fbm]\nseed = 0\nbasis = \"perlin\"\nfrequency_x = 4\nfrequency_y = 4\ntranslate_x = 0.0\ntranslate_y = 0.0\noctaves = 5\nlacunarity = 2\ngain = 0.5\nwarp = 0.0\n\n[[modifiers]]\ntype = \"levels\"\nenabled = true\nblack = 0.0\n"
        ), "{text}");
        assert!(
            text.contains("hue = 180.0\nsaturation = 0.5\nlightness = 0.0\n"),
            "{text}"
        );
        assert_eq!(Noise::parse(&text).unwrap(), noise);
        assert_eq!(Noise::parse("").unwrap(), Noise::default());
    }

    #[test]
    fn switching_type_keeps_parameters() {
        let mut noise = Noise::new(GeneratorKind::Fbm);
        noise.params.0[0] = Value::Int(7);
        noise.set_kind(GeneratorKind::Checkerboard);
        noise.set_kind(GeneratorKind::Voronoi);
        // Checkerboard's untouched defaults are not kept.
        assert_eq!(noise.kept.len(), 1);
        let text = noise.to_toml();
        assert!(text.starts_with("type = \"noise/voronoi\"\nsize = 512\n\n[noise.voronoi]\n"));
        assert!(text.contains("\n[noise.fbm]\nseed = 7\n"), "{text}");
        assert!(!text.contains("[pattern.checkerboard]"), "{text}");
        let mut read = Noise::parse(&text).unwrap();
        assert_eq!(read, noise);
        read.set_kind(GeneratorKind::Fbm);
        assert_eq!(read.params.0[0], Value::Int(7));
        assert!(read.kept.is_empty());
        // Only the current section affects the image.
        let mut plain = Noise::new(GeneratorKind::Voronoi);
        plain.size = 16;
        noise.size = 16;
        assert_eq!(noise.generate(), plain.generate());
    }

    #[test]
    fn parameters_belong_to_their_type() {
        assert!(
            Noise::parse("type = \"pattern/checkerboard\"\n[pattern.checkerboard]\nseed = 1")
                .is_err()
        );
        assert!(Noise::parse("[noise.perlin]\nbasis = \"value\"").is_err());
        assert!(Noise::parse("[noise.uniform]\nwarp = 1").is_err());
        // Parameters must be in a section.
        let error = Noise::parse("type = \"noise/perlin\"\nseed = 3").unwrap_err();
        assert!(error.contains("[noise.perlin]"), "{error}");
        let ok = Noise::parse("type = \"noise/fbm\"\n[noise.fbm]\noctaves = 3\ngain = 1").unwrap();
        assert_eq!(ok.params.display(GeneratorKind::Fbm.specs(), 6), "3");
        // Other sections are validated too.
        assert!(Noise::parse("[noise.voronoi]\njitter = 3").is_err());
        for bad in [
            "size = 0",
            "size = 5000",
            "type = \"noise/blue\"",
            "type = \"checkerboard\"",
            "[noise.blue]",
            "[pattern]\nperlin = 1",
            "noise = 1",
            "[noise.perlin]\nfrequency_x = 0",
            "[noise.perlin]\ntranslate_y = 1.5",
            "[noise.perlin]\nwarp = 5",
            "[noise.value]\ninterpolation = \"cosine\"",
            "[noise.fbm]\nbasis = \"cubic\"",
            "[noise.fbm]\nlacunarity = 2.5",
            "[[modifiers]]\ntype = \"levels\"\ngamma = 0",
            "[[modifiers]]\ntype = \"blur\"\nsteps = 2",
            "[[modifiers]]\ntype = \"sharpen\"",
        ] {
            assert!(Noise::parse(bad).is_err(), "{bad}");
        }
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

    #[test]
    fn modifiers() {
        let apply = |kind: ModifierKind, set: &[(usize, Value)], input: [f64; 3]| {
            let mut m = Modifier::new(kind);
            for (i, v) in set {
                m.params.0[*i] = v.clone();
            }
            let mut pixels = vec![input; 4];
            apply_modifier(&m, &mut pixels, 2, 2);
            pixels[0]
        };
        assert_eq!(apply(ModifierKind::Invert, &[], [0.25; 3]), [0.75; 3]);
        assert_eq!(
            apply(ModifierKind::Levels, &[(0, Value::Float(0.5))], [0.75; 3]),
            [0.5; 3]
        );
        assert_eq!(apply(ModifierKind::Threshold, &[], [0.6; 3]), [1.0; 3]);
        assert_eq!(
            apply(ModifierKind::Posterize, &[(0, Value::Int(2))], [0.4; 3]),
            [0.0; 3]
        );
        // Colorize: mid gray at full saturation is the pure hue; no saturation keeps gray.
        let colorize = |hue, saturation, lightness, c| {
            let params = [
                (0, Value::Float(hue)),
                (1, Value::Float(saturation)),
                (2, Value::Float(lightness)),
            ];
            apply(ModifierKind::Colorize, &params, c).map(|v| (v * 1e6).round() / 1e6)
        };
        assert_eq!(colorize(0.0, 1.0, 0.0, [0.5; 3]), [1.0, 0.0, 0.0]);
        assert_eq!(colorize(120.0, 1.0, 0.0, [0.5; 3]), [0.0, 1.0, 0.0]);
        assert_eq!(colorize(240.0, 1.0, 0.0, [0.5; 3]), [0.0, 0.0, 1.0]);
        assert_eq!(colorize(360.0, 1.0, 0.0, [0.5; 3]), [1.0, 0.0, 0.0]);
        assert_eq!(colorize(60.0, 0.0, 0.0, [0.25; 3]), [0.25; 3]);
        assert_eq!(colorize(30.0, 1.0, 1.0, [0.0; 3]), [1.0; 3]);
        assert_eq!(colorize(30.0, 1.0, -1.0, [1.0; 3]), [0.0; 3]);
        assert_eq!(colorize(30.0, 1.0, -0.5, [1.0; 3]), [1.0, 0.5, 0.0]);
        // Blur of a uniform image keeps it; blur spreads a single bright pixel.
        assert_eq!(
            apply(ModifierKind::Blur, &[], [0.3; 3]).map(|c| (c * 1e6).round()),
            [300000.0; 3]
        );
        let mut pixels = vec![[0.0; 3]; 16];
        pixels[5] = [1.0; 3];
        apply_modifier(&Modifier::new(ModifierKind::Blur), &mut pixels, 4, 4);
        let total: f64 = pixels.iter().map(|c| c[0]).sum();
        assert!((total - 1.0).abs() < 1e-9 && pixels[5][0] < 1.0);
        // Normalize stretches to 0..1.
        let mut pixels = vec![[0.2; 3], [0.6; 3]];
        apply_modifier(&Modifier::new(ModifierKind::Normalize), &mut pixels, 2, 1);
        assert_eq!(pixels, [[0.0; 3], [1.0; 3]]);
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
}
