//! `.apalette` files: named colors, one cell each in a grid of one image; see
//! docs/spec/apalette.md.

use std::path::Path;

use crate::gradient::{CellGrid, Layout, format_color, parse_color, take_grid};
use crate::texture::Image;

/// Whether `file` is a palette file (`*.apalette`, any case).
pub fn is_palette(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("apalette"))
}

/// A named color; `color` is straight (not premultiplied) RGBA.
#[derive(Clone, PartialEq, Debug)]
pub struct Swatch {
    pub name: String,
    pub color: [u8; 4],
}

/// An `.apalette` file; see docs/spec/apalette.md.
#[derive(Clone, PartialEq, Debug)]
pub struct Palette {
    pub width: u32,
    pub height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub layout: Layout,
    pub colors: Vec<Swatch>,
}

impl Palette {
    /// The file New Color ▸ Palette creates.
    pub fn new() -> Self {
        Self {
            width: 128,
            height: 128,
            cell_width: 16,
            cell_height: 16,
            layout: Layout::Rows,
            colors: vec![
                Swatch {
                    name: "Black".into(),
                    color: [0, 0, 0, 255],
                },
                Swatch {
                    name: "White".into(),
                    color: [255; 4],
                },
            ],
        }
    }

    /// Whether some color is not opaque.
    pub fn has_alpha(&self) -> bool {
        self.colors.iter().any(|c| c.color[3] != 255)
    }

    /// The image: each color fills its cell, the rest transparent.
    pub fn render(&self) -> Result<Image, String> {
        self.render_cells(|i, width, height| self.colors[i].color.repeat((width * height) as usize))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let ([width, height, cell_width, cell_height], layout) =
            take_grid(&mut table, [128, 128, 16, 16])?;
        let colors = match table.remove("colors") {
            None => Vec::new(),
            Some(toml::Value::Array(items)) => items
                .into_iter()
                .enumerate()
                .map(|(i, item)| parse_swatch(item).map_err(|e| format!("color {}: {e}", i + 1)))
                .collect::<Result<_, _>>()?,
            Some(_) => return Err("colors must be an array of tables".into()),
        };
        if let Some(key) = table.keys().next() {
            return Err(format!("unknown key {key}"));
        }
        let palette = Self {
            width,
            height,
            cell_width,
            cell_height,
            layout,
            colors,
        };
        palette.validate()?;
        Ok(palette)
    }

    /// The written form, see docs/spec/apalette.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = self.grid_toml();
        for swatch in &self.colors {
            out += "\n[[colors]]\n";
            out += &format!("name = {}\n", toml::Value::String(swatch.name.clone()));
            out += &format!("color = \"{}\"\n", format_color(swatch.color));
        }
        out
    }
}

impl CellGrid for Palette {
    fn grid(&self) -> [u32; 4] {
        [self.width, self.height, self.cell_width, self.cell_height]
    }

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

    fn layout(&self) -> Layout {
        self.layout
    }

    fn used(&self) -> usize {
        self.colors.len()
    }

    fn noun(&self) -> &'static str {
        "colors"
    }
}

fn parse_swatch(item: toml::Value) -> Result<Swatch, String> {
    let toml::Value::Table(mut table) = item else {
        return Err("must be a table".into());
    };
    let name = match table.remove("name") {
        None => String::new(),
        Some(v) => v.as_str().ok_or("name must be a string")?.to_owned(),
    };
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
    Ok(Swatch { name, color })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"width = 4
height = 2
cell_width = 2
cell_height = 1
layout = "columns"

[[colors]]
name = "Black"
color = "#000000"

[[colors]]
name = "Glass"
color = "#9fd8ff80"

[[colors]]
name = ""
color = "#ff0000"
"##;

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn written_form_round_trips() {
        let palette = Palette::parse(SAMPLE).unwrap();
        assert_eq!(palette.colors.len(), 3);
        assert_eq!(palette.colors[1].color, [0x9f, 0xd8, 0xff, 0x80]);
        assert!(palette.has_alpha());
        assert_eq!(palette.to_toml(), SAMPLE);
        assert_eq!(
            Palette::parse(&Palette::new().to_toml()).unwrap(),
            Palette::new()
        );
        assert!(!Palette::new().has_alpha());
    }

    #[test]
    fn colors_fill_their_cells_in_layout_order() {
        let image = Palette::parse(SAMPLE).unwrap().render().unwrap();
        assert_eq!((image.width, image.height), (4, 2));
        // Columns: top to bottom, then right.
        assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&image, 1, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&image, 0, 1), [0x9f, 0xd8, 0xff, 0x80]);
        assert_eq!(pixel(&image, 3, 0), [255, 0, 0, 255]);
        // The fourth cell is unused.
        assert_eq!(pixel(&image, 3, 1), [0; 4]);
    }

    #[test]
    fn errors() {
        let err = |text: &str| Palette::parse(text).unwrap_err();
        assert_eq!(err("[[colors]]\nname = \"a\""), "color 1: missing color");
        assert_eq!(
            err("[[colors]]\ncolor = \"red\""),
            "color 1: invalid color \"red\" (use #rrggbb or #rrggbbaa)"
        );
        assert_eq!(
            err("[[colors]]\ncolor = \"#000000\"\nshade = 1"),
            "color 1: unknown key shade"
        );
        assert_eq!(err("tint = 1"), "unknown key tint");
        let three = "[[colors]]\ncolor = \"#000000\"\n".repeat(3);
        assert_eq!(
            err(&format!(
                "width = 2\nheight = 1\ncell_width = 1\ncell_height = 1\n{three}"
            )),
            "3 colors need 3 cells but the grid has 2"
        );
    }
}
