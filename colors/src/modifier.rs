//! Color modifiers: levels, brightness and contrast, threshold, posterize, blur, colorize and
//! more, each with its parameters declared once; see docs/spec/anoise.md#modifiers in the
//! assetc repository.

use crate::space::{hsl_to_rgb, luminance};

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

/// A parameter: its key in files, its label and its type.
#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

/// A parameter's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    /// Floats and grays.
    Float(f64),
    /// The index of the option.
    Choice(usize),
}

/// Parameter values, in the order of their [`ParamSpec`]s.
#[derive(Debug, Clone, PartialEq)]
pub struct Params(pub Vec<Value>);

/// An integer parameter.
pub const fn int(
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

/// A float parameter.
pub const fn float(
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

/// A parameter that is one of `options`.
pub const fn choice(
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

impl Params {
    /// Every parameter of `specs` at its default.
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

    /// Parameter `i` as text, as assetc's Inspector shows it.
    pub fn display(&self, specs: &[ParamSpec], i: usize) -> String {
        match (&self.0[i], specs[i].kind) {
            (Value::Int(v), _) => v.to_string(),
            (Value::Float(v), _) => format!("{v:.2}"),
            (Value::Choice(c), ParamKind::Choice { options, .. }) => options[*c].into(),
            (Value::Choice(c), _) => c.to_string(),
        }
    }

    /// The value of `key`; panics when `specs` has no such parameter.
    pub fn get<'a>(&'a self, specs: &[ParamSpec], key: &str) -> &'a Value {
        self.find(specs, key)
            .unwrap_or_else(|| panic!("no parameter {key}"))
    }

    /// The value of `key`, when `specs` has it.
    pub fn find<'a>(&'a self, specs: &[ParamSpec], key: &str) -> Option<&'a Value> {
        specs.iter().position(|s| s.key == key).map(|i| &self.0[i])
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

/// What a modifier does.
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

    /// Its key in files.
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

    /// The kind whose key is `key`.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// Its parameters.
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
}

/// A modifier and its parameter values.
#[derive(Debug, Clone, PartialEq)]
pub struct Modifier {
    pub kind: ModifierKind,
    pub enabled: bool,
    pub params: Params,
}

impl Modifier {
    /// On, with its parameters at their defaults.
    pub fn new(kind: ModifierKind) -> Self {
        Self {
            kind,
            enabled: true,
            params: Params::defaults(kind.specs()),
        }
    }
}

/// Runs the enabled modifiers in order on a `width × height` RGB image (0–1 per channel, rows
/// top to bottom).
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
                let l = luminance(*c);
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
    use super::*;

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

    #[test]
    fn params_default_find_and_show() {
        let specs = ModifierKind::Blur.specs();
        let params = Params::defaults(specs);
        assert_eq!(params.get(specs, "radius"), &Value::Int(2));
        assert_eq!(params.display(specs, 1), "wrap");
        assert!(params.find(specs, "hue").is_none());
        assert_eq!(
            ModifierKind::from_key("brightness-contrast"),
            Some(ModifierKind::BrightnessContrast)
        );
    }
}
