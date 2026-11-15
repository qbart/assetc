//! `.agradient` files: gradients as stops, one vertical cell each in a grid of one image; see
//! docs/spec/agradient.md. Blending, dithering and the grid are assetc-colors'.

use std::path::Path;

pub use assetc_colors::gradient::{Dither, Stop, color_at};
pub use assetc_colors::grid::{CellGrid, Layout};
use assetc_colors::grid::{MAX_SIZE, check as in_range};
pub use assetc_colors::space::{format_hex as format_color, parse_hex as parse_color};

use crate::texture::Image;

/// A [`CellGrid`] the editor changes, written to a file.
pub trait GridFile: CellGrid {
    fn grid_mut(&mut self) -> ([&mut u32; 4], &mut Layout);

    /// The grid's keys in the written form.
    fn grid_toml(&self) -> String {
        let [width, height, cell_width, cell_height] = self.grid();
        format!(
            "width = {width}\nheight = {height}\ncell_width = {cell_width}\ncell_height = {cell_height}\nlayout = \"{}\"\n",
            self.layout().key()
        )
    }
}

/// Whether `file` is a gradient file (`*.agradient`, any case).
pub fn is_gradient(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("agradient"))
}

/// Reads and removes the grid keys from `table`; missing ones get `defaults`.
pub fn take_grid(
    table: &mut toml::Table,
    defaults: [u32; 4],
) -> Result<([u32; 4], Layout), String> {
    let mut grid = defaults;
    for (key, value) in ["width", "height", "cell_width", "cell_height"]
        .into_iter()
        .zip(&mut grid)
    {
        if let Some(v) = table.remove(key) {
            let v = v.as_integer().ok_or(format!("{key} must be an integer"))?;
            in_range(key, v, 1, i64::from(MAX_SIZE))?;
            *value = v as u32;
        }
    }
    let layout = match table.remove("layout") {
        None => Layout::Rows,
        Some(v) => v
            .as_str()
            .and_then(|key| Layout::ALL.into_iter().find(|l| l.key() == key))
            .ok_or_else(|| format!("unknown layout {v} (use rows or columns)"))?,
    };
    Ok((grid, layout))
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Gradient {
    pub name: String,
    pub dither: Dither,
    /// At least two, ordered by `at`.
    pub stops: Vec<Stop>,
    /// Run on this gradient's cell, its color only; see docs/spec/agradient.md#modifiers.
    pub modifiers: Vec<crate::noise::Modifier>,
    /// Its values of the file's gradient structs; see docs/features/structs.md.
    pub values: crate::structs::Values,
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
            modifiers: Vec::new(),
            values: crate::structs::Values::new(),
        }
    }

    /// Orders the stops by position, keeping ties in their current order.
    pub fn sort(&mut self) {
        self.stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    }

    /// The color at `t` without dithering, e.g. for a new stop placed there.
    pub fn color_at(&self, t: f64) -> [u8; 4] {
        color_at(&self.stops, t)
    }

    /// `width × height` RGBA8 of this gradient as a cell of the image: first stop at the bottom.
    pub fn render(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = assetc_colors::gradient::render(&self.stops, self.dither, width, height);
        // Modifiers change the color, never the alpha.
        if self.modifiers.iter().any(|m| m.enabled) {
            let mut rgb: Vec<[f64; 3]> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| [0, 1, 2].map(|c| f64::from(p[c]) / 255.0))
                .collect();
            crate::noise::apply_modifiers(
                &self.modifiers,
                &mut rgb,
                width as usize,
                height as usize,
            );
            for (p, c) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(rgb) {
                for k in 0..3 {
                    p[k] = (c[k].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
        pixels
    }

    /// `width × height` RGBA8 of this gradient on its side, first stop on the left, as the
    /// editor's bar shows it.
    pub fn render_sideways(&self, width: u32, height: u32) -> Vec<u8> {
        assetc_colors::gradient::render_sideways(&self.stops, self.dither, width, height)
    }

    fn is_opaque(&self) -> bool {
        self.stops.iter().all(|s| s.color[3] == 255)
    }
}

/// An `.agradient` file; see docs/spec/agradient.md.
#[derive(Clone, PartialEq, Debug)]
pub struct Gradients {
    pub width: u32,
    pub height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub layout: Layout,
    /// Structs attached to the file and to every gradient, and the file's values; see
    /// docs/features/structs.md.
    pub structs: Vec<crate::structs::Attachment>,
    pub gradient_structs: Vec<crate::structs::Attachment>,
    pub values: crate::structs::Values,
    pub gradients: Vec<Gradient>,
}

impl Gradients {
    /// The file New Color ▸ Gradient creates.
    pub fn new() -> Self {
        Self {
            width: 256,
            height: 256,
            cell_width: 16,
            cell_height: 64,
            layout: Layout::Rows,
            structs: Vec::new(),
            gradient_structs: Vec::new(),
            values: crate::structs::Values::new(),
            gradients: vec![Gradient::new("Gradient 1".into())],
        }
    }

    /// Whether some stop is not opaque.
    pub fn has_alpha(&self) -> bool {
        !self.gradients.iter().all(Gradient::is_opaque)
    }

    /// The scopes of its attached structs: the file and each gradient; see
    /// docs/features/structs.md#scopes.
    pub fn scopes(&self) -> Vec<crate::structs::Scope<'_>> {
        use crate::structs::{ASSET, ITEM, Owner, Scope};
        vec![
            Scope {
                scope: ASSET,
                attachments: &self.structs,
                owners: vec![Owner {
                    item: 0,
                    label: "file".into(),
                    values: &self.values,
                }],
            },
            Scope {
                scope: ITEM,
                attachments: &self.gradient_structs,
                owners: self
                    .gradients
                    .iter()
                    .enumerate()
                    .map(|(i, g)| Owner {
                        item: i as u32,
                        label: format!("gradient {}", i + 1),
                        values: &g.values,
                    })
                    .collect(),
            },
        ]
    }

    /// The image: one cell per gradient, in layout order, the rest transparent.
    pub fn render(&self) -> Result<Image, String> {
        self.render_cells(|i, width, height| self.gradients[i].render(width, height))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let ([width, height, cell_width, cell_height], layout) =
            take_grid(&mut table, [256, 256, 16, 64])?;
        let structs = crate::structs::take_toml_attachments(&mut table, "structs")?;
        let gradient_structs =
            crate::structs::take_toml_attachments(&mut table, "gradient_structs")?;
        let values = crate::structs::take_toml_values(&mut table)?;
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
            layout,
            structs,
            gradient_structs,
            values,
            gradients,
        };
        gradients.validate()?;
        Ok(gradients)
    }

    /// The written form, see docs/spec/agradient.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = self.grid_toml();
        crate::structs::write_toml_attachments(&mut out, "structs", &self.structs);
        crate::structs::write_toml_attachments(
            &mut out,
            "gradient_structs",
            &self.gradient_structs,
        );
        crate::structs::write_toml_values(&mut out, "values", &self.values);
        for gradient in &self.gradients {
            out += "\n[[gradients]]\n";
            out += &format!("name = {}\n", toml::Value::String(gradient.name.clone()));
            out += &format!("dither = \"{}\"\n", gradient.dither.key());
            write_stops(&gradient.stops, &mut out);
            crate::structs::write_toml_values(&mut out, "gradients.values", &gradient.values);
            crate::noise::write_modifier_tables(
                &gradient.modifiers,
                "gradients.modifiers",
                &mut out,
            );
        }
        out
    }
}

impl GridFile for Gradients {
    fn grid_mut(&mut self) -> ([&mut u32; 4], &mut Layout) {
        (
            [
                &mut self.width,
                &mut self.height,
                &mut self.cell_width,
                &mut self.cell_height,
            ],
            &mut self.layout,
        )
    }
}

impl CellGrid for Gradients {
    fn grid(&self) -> [u32; 4] {
        [self.width, self.height, self.cell_width, self.cell_height]
    }

    fn layout(&self) -> Layout {
        self.layout
    }

    fn used(&self) -> usize {
        self.gradients.len()
    }

    fn noun(&self) -> &'static str {
        "gradients"
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
    let stops = parse_stops(table.remove("stops").ok_or("missing stops")?)?;
    let modifiers = table
        .remove("modifiers")
        .map(crate::noise::parse_modifiers)
        .transpose()?
        .unwrap_or_default();
    let values = crate::structs::take_toml_values(&mut table)?;
    if let Some(key) = table.keys().next() {
        return Err(format!("unknown key {key}"));
    }
    let mut gradient = Gradient {
        name,
        dither,
        stops,
        modifiers,
        values,
    };
    gradient.sort();
    Ok(gradient)
}

/// `stops = [...]`: at least two stops, ordered by position; see docs/spec/agradient.md.
pub fn parse_stops(value: toml::Value) -> Result<Vec<Stop>, String> {
    let toml::Value::Array(items) = value else {
        return Err("stops must be an array of tables".into());
    };
    let mut stops: Vec<Stop> = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| parse_stop(item).map_err(|e| format!("stop {}: {e}", i + 1)))
        .collect::<Result<_, _>>()?;
    if stops.len() < 2 {
        return Err("needs at least two stops".into());
    }
    stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    Ok(stops)
}

/// `stops = [...]` with one stop per line, ordered by position.
pub fn write_stops(stops: &[Stop], out: &mut String) {
    *out += "stops = [\n";
    let mut stops = stops.to_vec();
    stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    for stop in stops {
        *out += &format!(
            "    {{ at = {}, color = \"{}\" }},\n",
            float(stop.at),
            format_color(stop.color)
        );
    }
    *out += "]\n";
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

/// Shortest form that reads back the same, always with a decimal point.
fn float(v: f64) -> String {
    let s = v.to_string();
    if s.contains(['.', 'e', 'E']) {
        s
    } else {
        format!("{s}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structs_round_trip() {
        let text = r##"width = 32
height = 64
cell_width = 16
cell_height = 64
layout = "rows"

[[structs]]
schema = "game.aschema"
type = "Theme"

[[gradient_structs]]
schema = "game.aschema"
type = "Biome"

[values.Theme]
name = "Night"

[[gradients]]
name = "Sky"
dither = "none"
stops = [
    { at = 0.0, color = "#000000" },
    { at = 1.0, color = "#ffffff" },
]

[gradients.values.Biome]
heat = { max = 2.5, min = 1 }
wet = true
"##;
        let file = Gradients::parse(text).unwrap();
        assert_eq!(file.structs[0].ty, "Theme");
        assert_eq!(file.gradient_structs[0].ty, "Biome");
        assert_eq!(file.values["Theme"]["name"], "Night");
        assert_eq!(file.gradients[0].values["Biome"]["heat"]["max"], 2.5);
        assert_eq!(file.to_toml(), text);
        assert!(
            Gradients::parse(&text.replace("type = \"Theme\"", "type = \"Theme\"\nx = 1")).is_err()
        );
    }

    const SAMPLE: &str = r##"width = 32
height = 64
cell_width = 16
cell_height = 32
layout = "rows"

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
    fn modifiers_change_their_cell_but_not_alpha() {
        let text =
            format!("{SAMPLE}\n[[gradients.modifiers]]\ntype = \"invert\"\nenabled = true\n");
        let gradients = Gradients::parse(&text).unwrap();
        assert_eq!(gradients.gradients[1].modifiers.len(), 1);
        assert_eq!(gradients.to_toml(), text);
        let plain = Gradients::parse(SAMPLE).unwrap().render().unwrap();
        let image = gradients.render().unwrap();
        // The first cell is untouched; the second inverted, its alpha kept.
        assert_eq!(pixel(&image, 0, 31), pixel(&plain, 0, 31));
        let (before, after) = (pixel(&plain, 16, 31), pixel(&image, 16, 31));
        assert_eq!(
            after,
            [255 - before[0], 255 - before[1], 255 - before[2], before[3]]
        );
        let (before, after) = (pixel(&plain, 16, 0), pixel(&image, 16, 0));
        assert_eq!(after[3], before[3]);
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

        // Columns fill top to bottom first: the second gradient goes below the first.
        let mut columns = Gradients::parse(SAMPLE).unwrap();
        columns.layout = Layout::Columns;
        let image = columns.render().unwrap();
        assert_eq!(pixel(&image, 0, 63), [255, 255, 255, 255]);
        assert_eq!(pixel(&image, 16, 31), [0; 4]);
        assert_eq!(columns.cell(1), (0, 1));
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
        assert!(err("layout = \"spiral\"").starts_with("unknown layout"));
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
}
