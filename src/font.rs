//! Font inspection and specimen sheets, see docs/features/fonts.md.

use std::collections::BTreeMap;
use std::path::Path;

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, point};
use ttf_parser::{Face, name_id};

use crate::texture::Image;
use crate::theme::{TEXT, TEXT_WEAK};

const PANGRAM: &str = "The quick brown fox jumps over the lazy dog";
/// Waterfall em sizes, px.
const SIZES: [f32; 6] = [12.0, 16.0, 24.0, 32.0, 48.0, 72.0];
const MARGIN: f32 = 32.0;
const GAP: f32 = 32.0;
const COLUMNS: usize = 16;
const CELL: usize = 64;
const GRID_SIZE: f32 = 40.0;
const MAX_CHARACTERS: usize = 1024;

/// Whether `file` is a font assetc reads (`*.ttf`, `*.otf`, any case).
pub fn is_font(file: &Path) -> bool {
    file.extension().is_some_and(|ext| {
        ["ttf", "otf"]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

/// Inspector rows for the font in `data`; see docs/features/fonts.md#inspector.
pub fn rows(data: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let face = Face::parse(data, 0).map_err(|e| format!("not a valid font: {e}"))?;
    let name = |ids: &[u16]| {
        ids.iter().find_map(|&id| {
            face.names()
                .into_iter()
                .filter(|n| n.name_id == id && n.is_unicode())
                .find_map(|n| n.to_string())
        })
    };
    let mut rows = Vec::new();
    let names = [
        (
            "Family",
            [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY].as_slice(),
        ),
        (
            "Style",
            [name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY].as_slice(),
        ),
        ("Version", [name_id::VERSION].as_slice()),
    ];
    for (label, ids) in names {
        if let Some(value) = name(ids) {
            rows.push((label, value));
        }
    }
    let tables = face.tables();
    let outlines = if tables.glyf.is_some() {
        "TrueType"
    } else if tables.cff.is_some() {
        "CFF"
    } else if tables.cff2.is_some() {
        "CFF2"
    } else {
        "none"
    };
    rows.push(("Outlines", outlines.into()));
    rows.push(("Glyphs", face.number_of_glyphs().to_string()));
    rows.push(("Characters", characters(&font(data)?).len().to_string()));
    rows.push(("Units per em", face.units_per_em().to_string()));
    rows.push(("Ascent", face.ascender().to_string()));
    rows.push(("Descent", face.descender().to_string()));
    rows.push(("Line gap", face.line_gap().to_string()));
    Ok(rows)
}

/// The specimen sheet of the font in `data`; see docs/features/fonts.md#specimen.
pub fn specimen(data: &[u8]) -> Result<Image, String> {
    let font = font(data)?;
    let shown: Vec<_> = characters(&font)
        .into_iter()
        .filter(|(c, _)| !c.is_control() && !c.is_whitespace())
        .take(MAX_CHARACTERS)
        .collect();

    // Measure first: the sheet is as wide as its widest part.
    let widest = SIZES
        .iter()
        .map(|&size| line_width(&font, em(&font, size)))
        .fold(0.0, f32::max);
    let grid_width = if shown.is_empty() { 0 } else { COLUMNS * CELL };
    let waterfall: f32 = SIZES
        .iter()
        .map(|&size| {
            let scaled = font.as_scaled(em(&font, size));
            scaled.ascent() - scaled.descent() + scaled.line_gap()
        })
        .sum();
    let grid_top = (MARGIN + waterfall + GAP).ceil() as usize;
    let rows = shown.len().div_ceil(COLUMNS);
    let width = (widest.max(grid_width as f32) + 2.0 * MARGIN).ceil() as usize + 1;
    let height = if shown.is_empty() {
        (MARGIN + waterfall + MARGIN).ceil() as usize
    } else {
        grid_top + rows * CELL + MARGIN as usize + 1
    };
    let mut sheet = Sheet {
        width,
        height,
        pixels: vec![0; width * height * 4],
    };

    let mut top = MARGIN;
    for size in SIZES {
        let scale = em(&font, size);
        let scaled = font.as_scaled(scale);
        let baseline = top + scaled.ascent();
        let mut x = MARGIN;
        let mut previous = None;
        for c in PANGRAM.chars() {
            let id = font.glyph_id(c);
            if let Some(previous) = previous {
                x += scaled.kern(previous, id);
            }
            sheet.glyph(&font, id, scale, x, baseline);
            x += scaled.h_advance(id);
            previous = Some(id);
        }
        top += scaled.ascent() - scaled.descent() + scaled.line_gap();
    }

    let left = MARGIN as usize;
    let line = [TEXT_WEAK.r(), TEXT_WEAK.g(), TEXT_WEAK.b(), 77];
    let scale = em(&font, GRID_SIZE);
    let scaled = font.as_scaled(scale);
    for (i, (_, id)) in shown.iter().enumerate() {
        let (cx, cy) = (left + i % COLUMNS * CELL, grid_top + i / COLUMNS * CELL);
        // Shared edges are set, not blended, so neighbours draw them once.
        for d in 0..=CELL {
            sheet.set(cx + d, cy, line);
            sheet.set(cx + d, cy + CELL, line);
            sheet.set(cx, cy + d, line);
            sheet.set(cx + CELL, cy + d, line);
        }
        let x = cx as f32 + (CELL as f32 - scaled.h_advance(*id)) / 2.0;
        let baseline = cy as f32
            + (CELL as f32 - (scaled.ascent() - scaled.descent())) / 2.0
            + scaled.ascent();
        sheet.glyph(&font, *id, scale, x, baseline);
    }

    Ok(Image {
        width: width as u32,
        height: height as u32,
        pixels: sheet.pixels,
    })
}

fn font(data: &[u8]) -> Result<FontRef<'_>, String> {
    FontRef::try_from_slice(data).map_err(|e| format!("not a valid font: {e}"))
}

/// Mapped characters by code point.
fn characters(font: &FontRef) -> BTreeMap<char, GlyphId> {
    font.codepoint_ids().map(|(id, c)| (c, id)).collect()
}

/// The scale at which an em is `size` px (ab_glyph scales by ascent − descent).
fn em(font: &FontRef, size: f32) -> PxScale {
    let height = font.height_unscaled();
    let units = font.units_per_em().unwrap_or(height);
    PxScale::from(size * height / units)
}

fn line_width(font: &FontRef, scale: PxScale) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut previous = None;
    let mut width = 0.0;
    for c in PANGRAM.chars() {
        let id = font.glyph_id(c);
        if let Some(previous) = previous {
            width += scaled.kern(previous, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    width
}

/// Straight-alpha RGBA8 canvas.
struct Sheet {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Sheet {
    fn set(&mut self, x: usize, y: usize, rgba: [u8; 4]) {
        if x < self.width && y < self.height {
            let i = (y * self.width + x) * 4;
            self.pixels[i..i + 4].copy_from_slice(&rgba);
        }
    }

    /// Draws glyph `id` in `TEXT` with its coverage as alpha, over what is there.
    fn glyph(&mut self, font: &FontRef, id: GlyphId, scale: PxScale, x: f32, baseline: f32) {
        let glyph = id.with_scale_and_position(scale, point(x, baseline));
        let Some(outlined) = font.outline_glyph(glyph) else {
            return;
        };
        let bounds = outlined.px_bounds();
        let color = [TEXT.r(), TEXT.g(), TEXT.b()].map(f32::from);
        outlined.draw(|gx, gy, coverage| {
            let (x, y) = (
                bounds.min.x as i64 + gx as i64,
                bounds.min.y as i64 + gy as i64,
            );
            if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
                return;
            }
            let i = (y as usize * self.width + x as usize) * 4;
            let a = coverage.clamp(0.0, 1.0);
            let dst = &mut self.pixels[i..i + 4];
            let dst_a = f32::from(dst[3]) / 255.0;
            let out_a = a + dst_a * (1.0 - a);
            if out_a > 0.0 {
                for c in 0..3 {
                    let blended = (color[c] * a + f32::from(dst[c]) * dst_a * (1.0 - a)) / out_a;
                    dst[c] = blended.round() as u8;
                }
            }
            dst[3] = (out_a * 255.0).round() as u8;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONOGRAM: &[u8] = include_bytes!("../samples/monogram.ttf");

    fn row<'a>(rows: &'a [(&str, String)], label: &str) -> &'a str {
        &rows.iter().find(|(l, _)| *l == label).unwrap().1
    }

    #[test]
    fn fonts_by_extension() {
        assert!(is_font(Path::new("a/b.TTF")));
        assert!(is_font(Path::new("b.otf")));
        assert!(!is_font(Path::new("b.woff2")));
    }

    #[test]
    fn rows_describe_the_font() {
        let rows = rows(MONOGRAM).unwrap();
        assert!(!row(&rows, "Family").is_empty());
        assert_eq!(row(&rows, "Outlines"), "TrueType");
        assert!(row(&rows, "Glyphs").parse::<u32>().unwrap() > 0);
        assert!(row(&rows, "Characters").parse::<u32>().unwrap() >= 95);
        assert!(row(&rows, "Descent").parse::<i32>().unwrap() <= 0);
    }

    #[test]
    fn specimen_draws_text_on_transparent() {
        let image = specimen(MONOGRAM).unwrap();
        assert!(image.width >= (COLUMNS * CELL) as u32 + 64);
        assert_eq!(
            image.pixels.len(),
            (image.width * image.height * 4) as usize
        );
        let pixels = image.pixels.as_chunks::<4>().0;
        // Corners are margin: transparent.
        assert_eq!(pixels[0][3], 0);
        // Some text is drawn in the text color.
        assert!(
            pixels
                .iter()
                .any(|p| p[3] == 255 && p[..3] == [TEXT.r(), TEXT.g(), TEXT.b()])
        );
    }

    #[test]
    fn invalid_fonts_fail() {
        assert!(
            rows(b"not a font")
                .unwrap_err()
                .starts_with("not a valid font")
        );
        assert!(specimen(b"not a font").is_err());
    }
}
