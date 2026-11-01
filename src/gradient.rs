//! `.gradient` files: gradients as stops, one vertical cell each in a grid of one image; see
//! docs/spec/gradient.md. Rendering follows `img/gradient.go` of SoftKiwiGames/cli.

use std::path::Path;

use crate::texture::Image;

const MAX_SIZE: i64 = 4096;

const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

const BAYER8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Whether `file` is a gradient file (`*.gradient`, any case).
pub fn is_gradient(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gradient"))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dither {
    #[default]
    None,
    Crosshatch,
    Pixel,
}

impl Dither {
    pub const ALL: [Self; 3] = [Self::None, Self::Crosshatch, Self::Pixel];

    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Crosshatch => "crosshatch",
            Self::Pixel => "pixel",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Crosshatch => "Crosshatch",
            Self::Pixel => "Pixel",
        }
    }
}

/// A color at a position; `color` is straight (not premultiplied) RGBA.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stop {
    pub at: f64,
    pub color: [u8; 4],
}

#[derive(Clone, PartialEq, Debug)]
pub struct Gradient {
    pub name: String,
    pub dither: Dither,
    /// At least two, ordered by `at`.
    pub stops: Vec<Stop>,
}

impl Gradient {
    /// Black to white, no dithering.
    pub fn new(name: String) -> Self {
        Self {
            name,
            dither: Dither::None,
            stops: vec![
                Stop {
                    at: 0.0,
                    color: [0, 0, 0, 255],
                },
                Stop {
                    at: 1.0,
                    color: [255, 255, 255, 255],
                },
            ],
        }
    }

    /// Orders the stops by position, keeping ties in their current order.
    pub fn sort(&mut self) {
        self.stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    }

    /// The color at `t` without dithering, e.g. for a new stop placed there.
    pub fn color_at(&self, t: f64) -> [u8; 4] {
        shade(&self.stops, Dither::None, t, 0, 0)
    }

    /// `width × height` RGBA8 of this gradient as a cell of the image: first stop at the bottom.
    pub fn render(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let t = position(height - 1 - y, height);
            for x in 0..width {
                pixels.extend(shade(&self.stops, self.dither, t, x, y));
            }
        }
        pixels
    }

    /// `width × height` RGBA8 of this gradient on its side, first stop on the left, as the
    /// editor's bar shows it.
    pub fn render_sideways(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend(shade(&self.stops, self.dither, position(x, width), x, y));
            }
        }
        pixels
    }

    fn is_opaque(&self) -> bool {
        self.stops.iter().all(|s| s.color[3] == 255)
    }
}

/// Position of pixel `i` along a gradient `length` pixels long: the ends are exact.
fn position(i: u32, length: u32) -> f64 {
    if length > 1 {
        i as f64 / (length - 1) as f64
    } else {
        0.0
    }
}

/// The color of pixel `(x, y)` (cell-local) at position `t`; see docs/spec/gradient.md#image.
fn shade(stops: &[Stop], dither: Dither, t: f64, x: u32, y: u32) -> [u8; 4] {
    let mut k = 0;
    while k + 2 < stops.len() && t > stops[k + 1].at {
        k += 1;
    }
    let (a, b) = (stops[k], stops[k + 1]);
    let span = b.at - a.at;
    let s = if span > 0.0 {
        ((t - a.at) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (x, y) = (x as usize, y as usize);
    let d = match dither {
        Dither::Pixel => {
            let threshold = f64::from(BAYER4[y % 4][x % 4]) / 16.0;
            return if s > threshold { b.color } else { a.color };
        }
        Dither::None => 0.5,
        Dither::Crosshatch => f64::from(BAYER8[y % 8][x % 8]) / 64.0,
    };
    let (aa, ba) = (f64::from(a.color[3]), f64::from(b.color[3]));
    let alpha = aa + (ba - aa) * s;
    let stored = quantize(alpha, 0.5);
    if stored == 0 {
        return [0; 4];
    }
    let channel = |c: usize| {
        let pa = f64::from(a.color[c]) * aa / 255.0;
        let pb = f64::from(b.color[c]) * ba / 255.0;
        quantize((pa + (pb - pa) * s) * 255.0 / alpha, d)
    };
    [channel(0), channel(1), channel(2), stored]
}

fn quantize(v: f64, d: f64) -> u8 {
    (v + d).floor().clamp(0.0, 255.0) as u8
}

/// A `.gradient` file; see docs/spec/gradient.md.
#[derive(Clone, PartialEq, Debug)]
pub struct Gradients {
    pub width: u32,
    pub height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub gradients: Vec<Gradient>,
}

impl Gradients {
    /// The file New Gradient creates.
    pub fn new() -> Self {
        Self {
            width: 256,
            height: 256,
            cell_width: 16,
            cell_height: 64,
            gradients: vec![Gradient::new("Gradient 1".into())],
        }
    }

    pub fn columns(&self) -> u32 {
        self.width / self.cell_width.max(1)
    }

    pub fn rows(&self) -> u32 {
        self.height / self.cell_height.max(1)
    }

    /// How many cells the grid has.
    pub fn capacity(&self) -> usize {
        (self.columns() * self.rows()) as usize
    }

    /// Checks what the editor can get wrong: sizes and whether the gradients fit.
    pub fn validate(&self) -> Result<(), String> {
        for (key, value) in [("width", self.width), ("height", self.height)] {
            in_range(key, i64::from(value), 1, MAX_SIZE)?;
        }
        in_range(
            "cell_width",
            i64::from(self.cell_width),
            1,
            i64::from(self.width),
        )?;
        in_range(
            "cell_height",
            i64::from(self.cell_height),
            1,
            i64::from(self.height),
        )?;
        let n = self.gradients.len();
        if n > self.capacity() {
            return Err(format!(
                "{n} gradients need {n} cells but the grid has {}",
                self.capacity()
            ));
        }
        Ok(())
    }

    /// Whether some stop is not opaque.
    pub fn has_alpha(&self) -> bool {
        !self.gradients.iter().all(Gradient::is_opaque)
    }

    /// The image: one cell per gradient, left to right then top to bottom, the rest transparent.
    pub fn render(&self) -> Result<Image, String> {
        self.validate()?;
        let (width, height) = (self.width, self.height);
        let (cell_width, cell_height) = (self.cell_width, self.cell_height);
        let mut pixels = vec![0; (width * height * 4) as usize];
        for (i, gradient) in self.gradients.iter().enumerate() {
            let i = i as u32;
            let (left, top) = (
                i % self.columns() * cell_width,
                i / self.columns() * cell_height,
            );
            let cell = gradient.render(cell_width, cell_height);
            let row_bytes = (cell_width * 4) as usize;
            for (y, row) in cell.chunks_exact(row_bytes).enumerate() {
                let start = (((top + y as u32) * width + left) * 4) as usize;
                pixels[start..start + row_bytes].copy_from_slice(row);
            }
        }
        Ok(Image {
            width,
            height,
            pixels,
        })
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let mut size = |key: &str, default: u32| -> Result<u32, String> {
            match table.remove(key) {
                None => Ok(default),
                Some(v) => {
                    let v = v.as_integer().ok_or(format!("{key} must be an integer"))?;
                    in_range(key, v, 1, MAX_SIZE)?;
                    Ok(v as u32)
                }
            }
        };
        let width = size("width", 256)?;
        let height = size("height", 256)?;
        let cell_width = size("cell_width", 16)?;
        let cell_height = size("cell_height", 64)?;
        let gradients = match table.remove("gradients") {
            None => Vec::new(),
            Some(toml::Value::Array(items)) => items
                .into_iter()
                .enumerate()
                .map(|(i, item)| {
                    parse_gradient(item).map_err(|e| format!("gradient {}: {e}", i + 1))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("gradients must be an array of tables".into()),
        };
        if let Some(key) = table.keys().next() {
            return Err(format!("unknown key {key}"));
        }
        let gradients = Self {
            width,
            height,
            cell_width,
            cell_height,
            gradients,
        };
        gradients.validate()?;
        Ok(gradients)
    }

    /// The written form, see docs/spec/gradient.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = format!(
            "width = {}\nheight = {}\ncell_width = {}\ncell_height = {}\n",
            self.width, self.height, self.cell_width, self.cell_height
        );
        for gradient in &self.gradients {
            out += "\n[[gradients]]\n";
            out += &format!("name = {}\n", toml::Value::String(gradient.name.clone()));
            out += &format!("dither = \"{}\"\n", gradient.dither.key());
            out += "stops = [\n";
            let mut stops = gradient.stops.clone();
            stops.sort_by(|a, b| a.at.total_cmp(&b.at));
            for stop in stops {
                out += &format!(
                    "    {{ at = {}, color = \"{}\" }},\n",
                    float(stop.at),
                    format_color(stop.color)
                );
            }
            out += "]\n";
        }
        out
    }
}

fn parse_gradient(item: toml::Value) -> Result<Gradient, String> {
    let toml::Value::Table(mut table) = item else {
        return Err("must be a table".into());
    };
    let name = match table.remove("name") {
        None => String::new(),
        Some(v) => v.as_str().ok_or("name must be a string")?.to_owned(),
    };
    let dither = match table.remove("dither") {
        None => Dither::None,
        Some(v) => v
            .as_str()
            .and_then(|key| Dither::ALL.into_iter().find(|d| d.key() == key))
            .ok_or_else(|| format!("unknown dither {v} (use none, crosshatch or pixel)"))?,
    };
    let stops: Vec<Stop> = match table.remove("stops") {
        Some(toml::Value::Array(items)) => items
            .into_iter()
            .enumerate()
            .map(|(i, item)| parse_stop(item).map_err(|e| format!("stop {}: {e}", i + 1)))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("stops must be an array of tables".into()),
        None => return Err("missing stops".into()),
    };
    if stops.len() < 2 {
        return Err("needs at least two stops".into());
    }
    if let Some(key) = table.keys().next() {
        return Err(format!("unknown key {key}"));
    }
    let mut gradient = Gradient {
        name,
        dither,
        stops,
    };
    gradient.sort();
    Ok(gradient)
}

fn parse_stop(item: toml::Value) -> Result<Stop, String> {
    let toml::Value::Table(mut table) = item else {
        return Err("must be a table".into());
    };
    let at = match table.remove("at") {
        Some(toml::Value::Float(v)) => v,
        Some(toml::Value::Integer(v)) => v as f64,
        Some(_) => return Err("at must be a number".into()),
        None => return Err("missing at".into()),
    };
    if !(0.0..=1.0).contains(&at) {
        return Err(format!("at {at} is outside 0–1"));
    }
    let color = match table.remove("color") {
        Some(v) => {
            let text = v.as_str().ok_or("color must be a string")?;
            parse_color(text)
                .filter(|_| text.starts_with('#'))
                .ok_or_else(|| format!("invalid color {text:?} (use #rrggbb or #rrggbbaa)"))?
        }
        None => return Err("missing color".into()),
    };
    if let Some(key) = table.keys().next() {
        return Err(format!("unknown key {key}"));
    }
    Ok(Stop { at, color })
}

/// `#rrggbb` or `#rrggbbaa` (the `#` optional, any case) as straight RGBA.
pub fn parse_color(text: &str) -> Option<[u8; 4]> {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !matches!(hex.len(), 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok();
    let alpha = if hex.len() == 8 { byte(3)? } else { 255 };
    Some([byte(0)?, byte(1)?, byte(2)?, alpha])
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque.
pub fn format_color([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// Shortest form that reads back the same, always with a decimal point.
fn float(v: f64) -> String {
    let s = v.to_string();
    if s.contains(['.', 'e', 'E']) {
        s
    } else {
        format!("{s}.0")
    }
}

fn in_range(key: &str, v: i64, min: i64, max: i64) -> Result<(), String> {
    if (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(format!("{key} {v} is outside {min}–{max}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"width = 32
height = 64
cell_width = 16
cell_height = 32

[[gradients]]
name = "Sunset"
dither = "none"
stops = [
    { at = 0.0, color = "#1b1036" },
    { at = 0.5, color = "#ff6b35" },
    { at = 1.0, color = "#ffe66d" },
]

[[gradients]]
name = "Fade"
dither = "crosshatch"
stops = [
    { at = 0.0, color = "#ffffff" },
    { at = 1.0, color = "#ffffff00" },
]
"##;

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn written_form_round_trips() {
        let gradients = Gradients::parse(SAMPLE).unwrap();
        assert_eq!(gradients.gradients.len(), 2);
        assert_eq!(gradients.gradients[1].dither, Dither::Crosshatch);
        assert_eq!(gradients.to_toml(), SAMPLE);
        assert_eq!(
            Gradients::parse(&Gradients::new().to_toml()).unwrap(),
            Gradients::new()
        );
    }

    #[test]
    fn stops_are_sorted_stably_when_read() {
        let text = "[[gradients]]\nstops = [{ at = 1, color = \"#ffffff\" }, { at = 0.5, color = \"#ff0000\" }, { at = 0.5, color = \"#00ff00\" }]";
        let g = &Gradients::parse(text).unwrap().gradients[0];
        let colors: Vec<_> = g.stops.iter().map(|s| format_color(s.color)).collect();
        assert_eq!(colors, ["#ff0000", "#00ff00", "#ffffff"]);
    }

    #[test]
    fn cells_fill_the_grid_bottom_up() {
        let image = Gradients::parse(SAMPLE).unwrap().render().unwrap();
        assert_eq!((image.width, image.height), (32, 64));
        // First cell: first stop on the bottom row, last on the top, every column alike.
        assert_eq!(pixel(&image, 0, 31), [0x1b, 0x10, 0x36, 255]);
        assert_eq!(pixel(&image, 15, 31), [0x1b, 0x10, 0x36, 255]);
        assert_eq!(pixel(&image, 0, 0), [0xff, 0xe6, 0x6d, 255]);
        // Second cell to its right.
        assert_eq!(pixel(&image, 16, 31), [255, 255, 255, 255]);
        assert_eq!(pixel(&image, 31, 0)[3], 0);
        // Unused cells (the second row): transparent.
        assert_eq!(pixel(&image, 5, 40), [0; 4]);
    }

    #[test]
    fn blending_is_premultiplied() {
        // Fading to transparent keeps the color instead of darkening toward black.
        let mut g = Gradient::new(String::new());
        g.stops[0].color = [255, 0, 0, 255];
        g.stops[1].color = [0, 0, 0, 0];
        let mid = g.color_at(0.5);
        assert_eq!(mid, [255, 0, 0, 128]);
        // Opaque black to white rounds to the middle.
        assert_eq!(
            Gradient::new(String::new()).color_at(0.5),
            [128, 128, 128, 255]
        );
    }

    #[test]
    fn pixel_dither_keeps_the_palette() {
        let mut g = Gradient::new(String::new());
        g.dither = Dither::Pixel;
        let pixels = g.render(4, 64);
        for p in pixels.as_chunks::<4>().0 {
            assert!(p == &[0, 0, 0, 255] || p == &[255, 255, 255, 255], "{p:?}");
        }
        // Crosshatch varies between neighbours where plain rounding does not.
        g.dither = Dither::Crosshatch;
        let column = g.render(1, 64);
        g.dither = Dither::None;
        assert_ne!(column, g.render(1, 64));
        // The bar shows the same blend on its side.
        assert_eq!(g.render_sideways(64, 1)[..4], g.render(1, 64)[63 * 4..]);
    }

    #[test]
    fn gradients_must_fit() {
        let mut g = Gradients::new();
        assert_eq!((g.columns(), g.rows(), g.capacity()), (16, 4, 64));
        g.width = 40;
        g.height = 64;
        g.gradients.push(Gradient::new("2".into()));
        assert!(g.validate().is_ok());
        g.gradients.push(Gradient::new("3".into()));
        assert_eq!(
            g.validate().unwrap_err(),
            "3 gradients need 3 cells but the grid has 2"
        );
    }

    #[test]
    fn errors_name_the_problem() {
        let err = |text: &str| Gradients::parse(text).unwrap_err();
        assert_eq!(err("depth = 1"), "unknown key depth");
        assert_eq!(err("cell_height = 300"), "cell_height 300 is outside 1–256");
        assert!(
            err("[[gradients]]\nstops = [{ at = 0, color = \"#000000\" }]")
                .contains("at least two")
        );
        assert!(
            err(
                "[[gradients]]\nstops = [{ at = 0, color = \"red\" }, { at = 1, color = \"#fff\" }]"
            )
            .starts_with("gradient 1: stop 1: invalid color")
        );
        assert!(err("[[gradients]]\nstops = [{ at = 2, color = \"#000000\" }, { at = 1, color = \"#ffffff\" }]").contains("outside 0–1"));
        assert!(err("[[gradients]]\ndither = \"fancy\"\nstops = []").contains("unknown dither"));
    }

    #[test]
    fn colors_parse_and_format() {
        assert_eq!(parse_color("#FF8000"), Some([255, 128, 0, 255]));
        assert_eq!(parse_color("ff800080"), Some([255, 128, 0, 128]));
        assert_eq!(parse_color("#fff"), None);
        assert_eq!(format_color([255, 128, 0, 255]), "#ff8000");
        assert_eq!(format_color([255, 128, 0, 128]), "#ff800080");
    }
}
