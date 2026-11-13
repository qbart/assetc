//! Texture paints of glTF models: their settings, `.apaint` files, painting texels with a
//! brush and the textures they compile to; see docs/features/texture-paint.md and
//! docs/spec/apaint.md.

use std::path::{Path, PathBuf};

use bevy::math::Vec3;

use crate::texture::{ColorSpace, Image};

/// The sizes a paint can have, in texels per side.
pub const SIZES: [u32; 5] = [256, 512, 1024, 2048, 4096];
pub const DEFAULT_SIZE: u32 = 1024;
const EXTENSION: &str = "apaint";

/// What a paint's texels hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// An sRGB color with straight alpha.
    #[default]
    Color,
    /// Independent linear masks, one per channel.
    Channels,
}

impl Mode {
    pub const ALL: [Self; 2] = [Self::Color, Self::Channels];

    pub fn key(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Channels => "channels",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Color => "Color",
            Self::Channels => "Channels",
        }
    }
}

/// The channels a Channels paint paints and exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channels {
    R,
    Rg,
    Rgb,
    #[default]
    Rgba,
}

impl Channels {
    pub const ALL: [Self; 4] = [Self::R, Self::Rg, Self::Rgb, Self::Rgba];

    pub fn key(self) -> &'static str {
        match self {
            Self::R => "r",
            Self::Rg => "rg",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::R => "R",
            Self::Rg => "RG",
            Self::Rgb => "RGB",
            Self::Rgba => "RGBA",
        }
    }

    /// How many channels, 1 to 4.
    pub fn count(self) -> usize {
        match self {
            Self::R => 1,
            Self::Rg => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

/// A texture painted onto a model; see docs/features/texture-paint.md#paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    pub name: String,
    pub mode: Mode,
    /// Texels per side, one of [`SIZES`].
    pub size: u32,
    /// Channels mode only.
    pub channels: Channels,
}

impl Paint {
    pub fn new(name: String) -> Self {
        Self {
            name,
            mode: Mode::Color,
            size: DEFAULT_SIZE,
            channels: Channels::Rgba,
        }
    }

    /// A short label of the mode and size (`Color 1024`, `RG 512`).
    pub fn label(&self) -> String {
        match self.mode {
            Mode::Color => format!("Color {}", self.size),
            Mode::Channels => format!("{} {}", self.channels.name(), self.size),
        }
    }
}

/// Whether `name` can name a paint: non-empty, ASCII letters, digits, `_` and `-`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The `[[paints]]` tables of a sidecar; see docs/spec/amod.md#paints.
pub fn parse(value: toml::Value) -> Result<Vec<Paint>, String> {
    let toml::Value::Array(items) = value else {
        return Err("paints is not an array of tables".into());
    };
    let mut out: Vec<Paint> = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let toml::Value::Table(mut table) = item else {
            return Err(format!("paint {} is not a table", i + 1));
        };
        let name = match table.remove("name") {
            Some(toml::Value::String(name)) => name,
            Some(_) => return Err(format!("paint {}: name is not a string", i + 1)),
            None => return Err(format!("paint {}: no name", i + 1)),
        };
        let fail = |e: String| format!("paint {name:?}: {e}");
        if !valid_name(&name) {
            return Err(fail(
                "name must be ASCII letters, digits, _ and - only".into(),
            ));
        }
        if out.iter().any(|p| p.name == name) {
            return Err(fail("name used twice".into()));
        }
        let key = |table: &mut toml::Table, key: &str| match table.remove(key) {
            None => Ok(None),
            Some(toml::Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(fail(format!("{key} is not a string"))),
        };
        let mode = match key(&mut table, "mode")? {
            None => Mode::Color,
            Some(k) => Mode::ALL
                .into_iter()
                .find(|m| m.key() == k)
                .ok_or_else(|| fail(format!("unknown mode {k:?} (use color or channels)")))?,
        };
        let channels = match key(&mut table, "channels")? {
            None => Channels::Rgba,
            Some(k) => Channels::ALL
                .into_iter()
                .find(|c| c.key() == k)
                .ok_or_else(|| {
                    fail(format!("unknown channels {k:?} (use r, rg, rgb or rgba)"))
                })?,
        };
        let size = match table.remove("size") {
            None => DEFAULT_SIZE,
            Some(toml::Value::Integer(n)) => u32::try_from(n)
                .ok()
                .filter(|n| SIZES.contains(n))
                .ok_or_else(|| fail(format!("size {n} is not 256, 512, 1024, 2048 or 4096")))?,
            Some(_) => return Err(fail("size is not an integer".into())),
        };
        if let Some(key) = table.keys().next() {
            return Err(fail(format!("unknown key {key:?}")));
        }
        out.push(Paint {
            name,
            mode,
            size,
            channels,
        });
    }
    Ok(out)
}

/// Appends the `[[paints]]` tables; see docs/spec/amod.md#written-form.
pub fn write(paints: &[Paint], out: &mut String) {
    for paint in paints {
        *out += "\n[[paints]]\n";
        *out += &format!("name = {}\n", toml::Value::from(paint.name.as_str()));
        *out += &format!("mode = \"{}\"\n", paint.mode.key());
        *out += &format!("size = {}\n", paint.size);
        if paint.mode == Mode::Channels {
            *out += &format!("channels = \"{}\"\n", paint.channels.key());
        }
    }
}

/// The `.apaint` file of paint `name` of `model`: `<model file name>.<name>.apaint`.
pub fn file_for(model: &Path, name: &str) -> PathBuf {
    let mut file = model.file_name().unwrap_or_default().to_os_string();
    file.push(format!(".{name}.{EXTENSION}"));
    model.with_file_name(file)
}

/// Whether `path` is an `.apaint` file.
pub fn is_apaint(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXTENSION)
}

/// The `.apaint` files next to `model` that belong to it, by name.
pub fn files_of(model: &Path) -> Vec<PathBuf> {
    let Some(dir) = model.parent() else {
        return Vec::new();
    };
    let Some(prefix) = model.file_name().map(|n| format!("{}.", n.to_string_lossy())) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    }) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_apaint(p))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .and_then(|n| {
                    n.strip_prefix(&prefix)?
                        .strip_suffix(&format!(".{EXTENSION}"))
                        .map(valid_name)
                })
                .unwrap_or(false)
        })
        .map(|p| dir.join(p.file_name().unwrap()))
        .collect();
    out.sort();
    out
}

/// The texels of an `.apaint` file as RGBA8, `None` when there is no file.
pub fn read(path: &Path) -> Result<Option<Image>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    Ok(Some(Image {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    }))
}

/// Writes square RGBA8 `pixels` of `size` as an `.apaint` file; see docs/spec/apaint.md.
pub fn write_file(path: &Path, size: u32, pixels: &[u8]) -> Result<(), String> {
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive)
        .write_image(pixels, size, size, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// Square texels of `size` for the editor: the file's, resampled when its size differs;
/// unpainted when there is none or it cannot be read.
pub fn load(path: &Path, size: u32) -> Vec<u8> {
    match read(path) {
        Ok(Some(image)) if image.width == size && image.height == size => image.pixels,
        Ok(Some(image)) => resample(&image, size),
        _ => blank(size),
    }
}

/// Unpainted texels of `size`.
pub fn blank(size: u32) -> Vec<u8> {
    vec![0; size as usize * size as usize * 4]
}

/// `image` resampled to `size` × `size`, bilinear per channel.
pub fn resample(image: &Image, size: u32) -> Vec<u8> {
    let Some(buffer) = image::RgbaImage::from_raw(image.width, image.height, image.pixels.clone())
    else {
        return blank(size);
    };
    image::imageops::resize(&buffer, size, size, image::imageops::FilterType::Triangle).into_raw()
}

/// How a paint's texture is written; see docs/features/texture-paint.md#build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Export {
    /// Four channels, in this color space.
    Rgba(ColorSpace),
    /// The first 1 (R) or 2 (RG) channels, linear.
    Unorm(usize),
}

/// The mip chain of paint `paint` of `model` and how to write it; see
/// docs/features/texture-paint.md#build.
pub fn compile(model: &Path, paint: &Paint) -> Result<(Vec<Image>, Export), String> {
    let path = file_for(model, &paint.name);
    let file = path.file_name().unwrap_or_default().to_string_lossy();
    let mut pixels = match read(&path).map_err(|e| format!("{file}: {e}"))? {
        None => blank(paint.size),
        Some(image) if image.width == paint.size && image.height == paint.size => image.pixels,
        Some(image) => {
            return Err(format!(
                "{file} is {} × {}, not {size} × {size}",
                image.width,
                image.height,
                size = paint.size
            ));
        }
    };
    let (color, export) = match (paint.mode, paint.channels) {
        (Mode::Color, _) => (ColorSpace::Srgb, Export::Rgba(ColorSpace::Srgb)),
        (Mode::Channels, Channels::R) => (ColorSpace::Linear, Export::Unorm(1)),
        (Mode::Channels, Channels::Rg) => (ColorSpace::Linear, Export::Unorm(2)),
        (Mode::Channels, Channels::Rgb) => {
            for texel in pixels.chunks_exact_mut(4) {
                texel[3] = 255;
            }
            (ColorSpace::Linear, Export::Rgba(ColorSpace::Linear))
        }
        (Mode::Channels, Channels::Rgba) => (ColorSpace::Linear, Export::Rgba(ColorSpace::Linear)),
    };
    let base = Image {
        width: paint.size,
        height: paint.size,
        pixels,
    };
    Ok((crate::texture::mip_chain(base, color), export))
}

/// What a stroke moves texels toward; see docs/features/texture-paint.md#painting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// All four channels to this sRGB color with alpha.
    Color([u8; 4]),
    /// One channel (0 R … 3 A) to this value.
    Channel(usize, u8),
}

/// A dab of the brush: a sphere in the posed model's space.
#[derive(Debug, Clone, Copy)]
pub struct Brush {
    pub center: Vec3,
    pub radius: f32,
    /// 0–1: the part of the radius at full strength.
    pub hardness: f32,
    /// 0–1.
    pub strength: f32,
    pub target: Target,
}

/// How a canvas is shown on the model; see docs/features/texture-paint.md#3d-view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// The color laid over gray by its alpha.
    Color,
    /// One channel alone, in its color (A in white), black where 0.
    Channel(usize),
}

/// The texels of a paint being painted.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub size: u32,
    /// RGBA8, rows top to bottom.
    pub pixels: Vec<u8>,
    /// Bumped whenever `pixels` change.
    pub generation: u64,
    /// Texels changed since [`Canvas::take_dirty`]: min x, min y, max x, max y (inclusive).
    dirty: Option<[u32; 4]>,
    stroke: Option<Stroke>,
}

/// A stroke in progress: the texels as it began and the largest weight each got.
#[derive(Debug, Clone)]
struct Stroke {
    base: Vec<u8>,
    weights: Vec<f32>,
    /// The last dab's center, to fill the stroke in between.
    last: Option<Vec3>,
}

impl Canvas {
    pub fn new(size: u32, pixels: Vec<u8>) -> Self {
        debug_assert_eq!(pixels.len(), size as usize * size as usize * 4);
        Self {
            size,
            pixels,
            generation: 0,
            dirty: Some([0, 0, size - 1, size - 1]),
            stroke: None,
        }
    }

    /// Ends the stroke in progress; whether there was one that changed texels.
    pub fn end_stroke(&mut self) -> bool {
        self.stroke
            .take()
            .is_some_and(|s| s.weights.iter().any(|&w| w > 0.0))
    }

    pub fn in_stroke(&self) -> bool {
        self.stroke.is_some()
    }

    /// Every texel back to unpainted.
    pub fn clear(&mut self) {
        self.stroke = None;
        self.pixels.fill(0);
        self.touch_all();
    }

    pub fn is_blank(&self) -> bool {
        self.pixels.iter().all(|&b| b == 0)
    }

    fn touch_all(&mut self) {
        self.generation += 1;
        self.dirty = Some([0, 0, self.size - 1, self.size - 1]);
    }

    /// The texels changed since the last call.
    pub fn take_dirty(&mut self) -> Option<[u32; 4]> {
        self.dirty.take()
    }

    /// The centers of the dabs from the stroke's last dab to `center`, a quarter of the
    /// radius apart, ending at `center`; starts a stroke when none is in progress.
    pub fn dab_centers(&mut self, center: Vec3, radius: f32) -> Vec<Vec3> {
        let size = self.size as usize;
        let stroke = self.stroke.get_or_insert_with(|| Stroke {
            base: self.pixels.clone(),
            weights: vec![0.0; size * size],
            last: None,
        });
        let step = (radius / 4.0).max(1e-6);
        let out = match stroke.last {
            Some(last) => {
                let n = ((center.distance(last) / step).ceil() as usize).clamp(1, 256);
                (1..=n)
                    .map(|i| last.lerp(center, i as f32 / n as f32))
                    .collect()
            }
            None => vec![center],
        };
        stroke.last = Some(center);
        out
    }

    /// Paints the texels of the triangle with posed corners `corners` and `TEXCOORD_0`
    /// coordinates `uvs` that `brush` reaches; see docs/features/texture-paint.md#painting.
    pub fn paint_triangle(&mut self, corners: [Vec3; 3], uvs: [[f32; 2]; 3], brush: &Brush) {
        let size = self.size as usize;
        let s = self.size as f32;
        let r = brush.radius;
        if r <= 0.0 || brush.strength <= 0.0 {
            return;
        }
        // The triangle's nearest point to the center must be within reach.
        if closest_on_triangle(brush.center, corners).distance(brush.center) >= r {
            return;
        }
        let [p0, p1, p2] = corners;
        let [t0, t1, t2] = uvs.map(|[u, v]| bevy::math::Vec2::new(u * s, v * s));
        let (f1, f2) = (t1 - t0, t2 - t0);
        let area = f1.perp_dot(f2);
        if area.abs() < 1e-9 {
            return;
        }
        let (e1, e2) = (p1 - p0, p2 - p0);
        // Texel-space bounds of the brush's disk in the triangle's plane.
        let normal = e1.cross(e2);
        let n_len = normal.length();
        if n_len < 1e-12 {
            return;
        }
        let n = normal / n_len;
        let offset = (brush.center - p0).dot(n);
        let disk = (r * r - offset * offset).max(0.0).sqrt();
        let projected = brush.center - n * offset;
        let (g11, g12, g22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
        let det = g11 * g22 - g12 * g12;
        // A vector in the plane, in texels.
        let to_texels = |v: Vec3| {
            let (a, b) = (e1.dot(v), e2.dot(v));
            let l1 = (g22 * a - g12 * b) / det;
            let l2 = (g11 * b - g12 * a) / det;
            f1 * l1 + f2 * l2
        };
        let a = e1 / e1.length();
        let b = n.cross(a);
        let (ga, gb) = (to_texels(a), to_texels(b));
        let mid = t0 + to_texels(projected - p0);
        let half = bevy::math::Vec2::new(ga.x.hypot(gb.x), ga.y.hypot(gb.y)) * disk;
        let lo = t0.min(t1).min(t2).max(mid - half) - 1.0;
        let hi = t0.max(t1).max(t2).min(mid + half) + 1.0;
        if lo.x > hi.x || lo.y > hi.y {
            return;
        }
        // Each edge's length in texels, to measure how far outside it a texel is.
        let lengths = [(t2 - t1).length(), (t0 - t2).length(), (t1 - t0).length()];
        let (x0, x1) = (lo.x.floor() as i64, hi.x.ceil() as i64);
        let (y0, y1) = (lo.y.floor() as i64, hi.y.ceil() as i64);
        // At most four wraps of the texture each way.
        if (x1 - x0) > 4 * size as i64 || (y1 - y0) > 4 * size as i64 {
            return;
        }
        let Some(stroke) = &mut self.stroke else {
            return;
        };
        let mut touched: Option<[u32; 4]> = None;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = bevy::math::Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let l1 = (c - t0).perp_dot(f2) / area;
                let l2 = f1.perp_dot(c - t0) / area;
                let l0 = 1.0 - l1 - l2;
                let ls = [l0, l1, l2];
                // Within a texel outside every edge.
                let outside = (0..3).any(|i| ls[i] * area.abs() / lengths[i].max(1e-6) < -1.0);
                if outside {
                    continue;
                }
                let [k0, k1, k2] = ls.map(|l| l.clamp(0.0, 1.0));
                let sum = (k0 + k1 + k2).max(1e-6);
                let p = (p0 * k0 + p1 * k1 + p2 * k2) / sum;
                let d = p.distance(brush.center) / r;
                if d >= 1.0 {
                    continue;
                }
                let w = brush.strength * falloff(d, brush.hardness);
                let (xi, yi) = (x.rem_euclid(size as i64) as usize, y.rem_euclid(size as i64) as usize);
                let i = yi * size + xi;
                if w <= stroke.weights[i] {
                    continue;
                }
                stroke.weights[i] = w;
                let base = &stroke.base[i * 4..i * 4 + 4];
                let texel = &mut self.pixels[i * 4..i * 4 + 4];
                let lerp = |from: u8, to: u8| {
                    (f32::from(from) + (f32::from(to) - f32::from(from)) * w).round() as u8
                };
                match brush.target {
                    Target::Color(color) => {
                        for k in 0..4 {
                            texel[k] = lerp(base[k], color[k]);
                        }
                    }
                    Target::Channel(k, value) => texel[k] = lerp(base[k], value),
                }
                let (xi, yi) = (xi as u32, yi as u32);
                touched = Some(match touched {
                    None => [xi, yi, xi, yi],
                    Some([a, b, c, d]) => [a.min(xi), b.min(yi), c.max(xi), d.max(yi)],
                });
            }
        }
        if let Some([a, b, c, d]) = touched {
            self.generation += 1;
            self.dirty = Some(match self.dirty {
                None => [a, b, c, d],
                Some([e, f, g, h]) => [a.min(e), b.min(f), c.max(g), d.max(h)],
            });
        }
    }

    /// Writes how texels `rect` (min x, min y, max x, max y) are shown into `out`, an RGBA8
    /// image of the canvas's size.
    pub fn display_into(&self, display: Display, rect: [u32; 4], out: &mut [u8]) {
        let size = self.size as usize;
        let [x0, y0, x1, y1] = rect.map(|v| v as usize);
        for y in y0..=y1.min(size - 1) {
            for x in x0..=x1.min(size - 1) {
                let i = (y * size + x) * 4;
                let t = &self.pixels[i..i + 4];
                let shown = match display {
                    Display::Color => {
                        let a = u32::from(t[3]);
                        let over = |c: u8| ((u32::from(c) * a + 128 * (255 - a) + 127) / 255) as u8;
                        [over(t[0]), over(t[1]), over(t[2]), 255]
                    }
                    Display::Channel(k) => {
                        let v = t[k];
                        match k {
                            0 => [v, 0, 0, 255],
                            1 => [0, v, 0, 255],
                            2 => [0, 0, v, 255],
                            _ => [v, v, v, 255],
                        }
                    }
                };
                out[i..i + 4].copy_from_slice(&shown);
            }
        }
    }
}

/// 1 up to `hardness`, then smoothly down to 0 at 1.
fn falloff(d: f32, hardness: f32) -> f32 {
    let h = hardness.clamp(0.0, 1.0);
    if d <= h {
        return 1.0;
    }
    if h >= 1.0 {
        return 0.0;
    }
    let t = ((d - h) / (1.0 - h)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The point of triangle `abc` nearest to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_canvas(size: u32) -> Canvas {
        Canvas::new(size, blank(size))
    }

    /// A unit square in the XY plane mapped onto the whole texture (v down as y goes up).
    fn paint_square(canvas: &mut Canvas, brush: &Brush) {
        let corners = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            canvas.paint_triangle(
                [corners[a], corners[b], corners[c]],
                [uvs[a], uvs[b], uvs[c]],
                brush,
            );
        }
    }

    fn texel(canvas: &Canvas, x: usize, y: usize) -> [u8; 4] {
        let i = (y * canvas.size as usize + x) * 4;
        canvas.pixels[i..i + 4].try_into().unwrap()
    }

    fn brush(center: Vec3, target: Target) -> Brush {
        Brush {
            center,
            radius: 0.25,
            hardness: 1.0,
            strength: 1.0,
            target,
        }
    }

    #[test]
    fn paints_texels_inside_the_sphere() {
        let mut canvas = quad_canvas(16);
        let center = Vec3::new(0.5, 0.5, 0.0);
        canvas.dab_centers(center, 0.25);
        paint_square(&mut canvas, &brush(center, Target::Color([255, 0, 0, 255])));
        assert_eq!(texel(&canvas, 8, 8), [255, 0, 0, 255]);
        assert_eq!(texel(&canvas, 0, 0), [0, 0, 0, 0]);
        assert_eq!(texel(&canvas, 15, 15), [0, 0, 0, 0]);
        assert!(canvas.end_stroke());
        assert!(!canvas.end_stroke());
    }

    #[test]
    fn channels_paint_alone_and_strokes_do_not_build_up() {
        let mut canvas = quad_canvas(8);
        let center = Vec3::new(0.5, 0.5, 0.0);
        let mut b = brush(center, Target::Channel(1, 255));
        b.strength = 0.5;
        canvas.dab_centers(center, 0.25);
        paint_square(&mut canvas, &b);
        paint_square(&mut canvas, &b);
        assert_eq!(texel(&canvas, 4, 4), [0, 128, 0, 0]);
        canvas.end_stroke();
        // A new stroke starts from there.
        canvas.dab_centers(center, 0.25);
        paint_square(&mut canvas, &b);
        assert_eq!(texel(&canvas, 4, 4), [0, 192, 0, 0]);
    }

    #[test]
    fn far_triangles_are_left_alone() {
        let mut canvas = quad_canvas(8);
        let center = Vec3::new(0.5, 0.5, 5.0);
        canvas.dab_centers(center, 0.25);
        paint_square(&mut canvas, &brush(center, Target::Color([255; 4])));
        assert!(canvas.is_blank());
        assert!(!canvas.end_stroke());
    }

    #[test]
    fn strokes_fill_in_between() {
        let mut canvas = quad_canvas(8);
        assert_eq!(canvas.dab_centers(Vec3::ZERO, 1.0).len(), 1);
        let between = canvas.dab_centers(Vec3::X, 1.0);
        assert_eq!(between.len(), 4);
        assert_eq!(*between.last().unwrap(), Vec3::X);
    }

    #[test]
    fn falloff_is_hard_then_smooth() {
        assert_eq!(falloff(0.3, 0.5), 1.0);
        assert_eq!(falloff(1.0, 0.5), 0.0);
        assert!((falloff(0.75, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(falloff(0.5, 1.0), 1.0);
    }

    #[test]
    fn displays_color_over_gray_and_channels_in_their_color() {
        let mut canvas = quad_canvas(2);
        canvas.pixels[..4].copy_from_slice(&[255, 0, 0, 255]);
        canvas.pixels[4..8].copy_from_slice(&[10, 20, 30, 0]);
        canvas.pixels[8..12].copy_from_slice(&[0, 0, 0, 200]);
        let mut out = vec![0; 16];
        canvas.display_into(Display::Color, [0, 0, 1, 1], &mut out);
        assert_eq!(out[..8], [255, 0, 0, 255, 128, 128, 128, 255]);
        canvas.display_into(Display::Channel(0), [0, 0, 1, 1], &mut out);
        assert_eq!(out[..8], [255, 0, 0, 255, 10, 0, 0, 255]);
        canvas.display_into(Display::Channel(3), [0, 0, 1, 1], &mut out);
        assert_eq!(out[8..12], [200, 200, 200, 255]);
        assert_eq!(out[12..], [0, 0, 0, 255]);
    }

    #[test]
    fn tables_round_trip() {
        let text = "[[paints]]\nname = \"wear\"\nmode = \"channels\"\nsize = 512\nchannels = \"rg\"\n\n[[paints]]\nname = \"dirt\"\n";
        let table: toml::Table = text.parse().unwrap();
        let paints = parse(table["paints"].clone()).unwrap();
        assert_eq!(paints[0].channels, Channels::Rg);
        assert_eq!(paints[0].size, 512);
        assert_eq!(paints[1], Paint::new("dirt".into()));
        let mut out = String::new();
        write(&paints, &mut out);
        assert_eq!(
            out,
            "\n[[paints]]\nname = \"wear\"\nmode = \"channels\"\nsize = 512\nchannels = \"rg\"\n\n[[paints]]\nname = \"dirt\"\nmode = \"color\"\nsize = 1024\n"
        );
        for bad in [
            "[[paints]]\nname = \"a b\"\n",
            "[[paints]]\nname = \"a\"\nsize = 300\n",
            "[[paints]]\nname = \"a\"\nmode = \"rgb\"\n",
            "[[paints]]\nname = \"a\"\nchannels = \"rgbx\"\n",
            "[[paints]]\nname = \"a\"\nfoo = 1\n",
            "[[paints]]\nname = \"a\"\n\n[[paints]]\nname = \"a\"\n",
        ] {
            let table: toml::Table = bad.parse().unwrap();
            assert!(parse(table["paints"].clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn files_round_trip_and_compile() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("Fox.glb");
        std::fs::write(&model, "").unwrap();
        let path = file_for(&model, "wear");
        assert_eq!(path.file_name().unwrap(), "Fox.glb.wear.apaint");
        let mut pixels = blank(256);
        pixels[..4].copy_from_slice(&[1, 2, 3, 4]);
        write_file(&path, 256, &pixels).unwrap();
        std::fs::write(dir.path().join("Fox.glb.amod"), "").unwrap();
        std::fs::write(dir.path().join("Foxy.glb.x.apaint"), "").unwrap();
        assert_eq!(files_of(&model), [path.clone()]);
        assert_eq!(load(&path, 256), pixels);
        assert_eq!(load(&path, 512).len(), 512 * 512 * 4);

        let mut paint = Paint::new("wear".into());
        paint.size = 256;
        paint.mode = Mode::Channels;
        paint.channels = Channels::Rgb;
        let (levels, export) = compile(&model, &paint).unwrap();
        assert_eq!(export, Export::Rgba(ColorSpace::Linear));
        assert_eq!(levels.len(), 9);
        assert_eq!(levels[0].pixels[..4], [1, 2, 3, 255]);
        paint.channels = Channels::R;
        assert_eq!(compile(&model, &paint).unwrap().1, Export::Unorm(1));
        paint.size = 512;
        let err = compile(&model, &paint).unwrap_err();
        assert_eq!(err, "Fox.glb.wear.apaint is 256 × 256, not 512 × 512");
        // No file: unpainted.
        paint.name = "none".into();
        let (levels, _) = compile(&model, &paint).unwrap();
        assert!(levels[0].pixels.iter().all(|&b| b == 0));
    }
}
