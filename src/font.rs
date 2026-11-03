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

// ---- Compiling: SDF atlas and `.afont.bin`, see docs/features/fonts.md#build -------------

/// Atlas pixels per em, distance spread in pixels, and the value on the outline.
const EM_SIZE: f32 = 48.0;
const SPREAD: f32 = 6.0;
const ON_EDGE: f32 = 128.0;
const ATLAS_WIDTH: usize = 512;
/// Straight segments per curve when measuring distances.
const CURVE_SEGMENTS: usize = 16;

pub const FORMAT: &str = "AFONT";
pub const VERSION: u32 = 1;

/// A compiled font: the `.afont.bin` bytes and its atlas.
pub struct Compiled {
    pub afont: Vec<u8>,
    pub atlas: Image,
    pub glyphs: usize,
    pub kerning: usize,
}

/// One glyph's distance field: size, offset from the pen (pixels, y down) and texels.
struct Sdf {
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    texels: Vec<u8>,
}

/// The outline's straight segments in pixels, y up.
/// Straight segments, start and end point.
type Segment = [[f32; 2]; 2];

fn segments(font: &FontRef, id: GlyphId, scale: f32) -> Option<(Vec<Segment>, [f32; 4])> {
    let outline = font.outline(id)?;
    let p = |q: ab_glyph::Point| [q.x * scale, q.y * scale];
    let mut lines = Vec::new();
    let flatten = |lines: &mut Vec<[[f32; 2]; 2]>, at: &dyn Fn(f32) -> [f32; 2]| {
        let mut previous = at(0.0);
        for k in 1..=CURVE_SEGMENTS {
            let next = at(k as f32 / CURVE_SEGMENTS as f32);
            lines.push([previous, next]);
            previous = next;
        }
    };
    for curve in &outline.curves {
        match *curve {
            ab_glyph::OutlineCurve::Line(a, b) => lines.push([p(a), p(b)]),
            ab_glyph::OutlineCurve::Quad(a, b, c) => {
                let (a, b, c) = (p(a), p(b), p(c));
                flatten(&mut lines, &|t| {
                    let u = 1.0 - t;
                    [0, 1].map(|i| u * u * a[i] + 2.0 * u * t * b[i] + t * t * c[i])
                });
            }
            ab_glyph::OutlineCurve::Cubic(a, b, c, d) => {
                let (a, b, c, d) = (p(a), p(b), p(c), p(d));
                flatten(&mut lines, &|t| {
                    let u = 1.0 - t;
                    [0, 1].map(|i| {
                        u * u * u * a[i]
                            + 3.0 * u * u * t * b[i]
                            + 3.0 * u * t * t * c[i]
                            + t * t * t * d[i]
                    })
                });
            }
        }
    }
    // Bounds are stored top-left first: min holds the top (largest y).
    let b = outline.bounds;
    let bounds = [
        b.min.x * scale,
        b.max.y * scale,
        b.max.x * scale,
        b.min.y * scale,
    ];
    (!lines.is_empty()).then_some((lines, bounds))
}

/// The signed distance field of one glyph, `None` without an outline.
fn glyph_sdf(font: &FontRef, id: GlyphId, scale: f32) -> Option<Sdf> {
    let (lines, [x_min, y_min, x_max, y_max]) = segments(font, id, scale)?;
    let pad = SPREAD as i32;
    // Pixels, y down from the baseline.
    let (x0, x1) = (x_min.floor() as i32 - pad, x_max.ceil() as i32 + pad);
    let (y0, y1) = ((-y_max).floor() as i32 - pad, (-y_min).ceil() as i32 + pad);
    let (width, height) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut texels = Vec::with_capacity(width * height);
    for row in 0..height {
        for column in 0..width {
            // Texel center, back to y up.
            let px = x0 as f32 + column as f32 + 0.5;
            let py = -(y0 as f32 + row as f32 + 0.5);
            let mut nearest = f32::MAX;
            let mut winding = 0;
            for [a, b] in &lines {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let length = dx * dx + dy * dy;
                let t = if length > 0.0 {
                    (((px - a[0]) * dx + (py - a[1]) * dy) / length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (ex, ey) = (a[0] + t * dx - px, a[1] + t * dy - py);
                nearest = nearest.min(ex * ex + ey * ey);
                // Non-zero winding of a ray toward +x.
                let cross = dx * (py - a[1]) - (px - a[0]) * dy;
                if a[1] <= py && b[1] > py && cross > 0.0 {
                    winding += 1;
                } else if b[1] <= py && a[1] > py && cross < 0.0 {
                    winding -= 1;
                }
            }
            let distance = nearest.sqrt() * if winding != 0 { 1.0 } else { -1.0 };
            texels.push(
                (ON_EDGE + distance * ON_EDGE / SPREAD)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            );
        }
    }
    Some(Sdf {
        width,
        height,
        x: x0,
        y: y0,
        texels,
    })
}

/// Compiles the font in `data`; `atlas_id` is its atlas texture's asset ID.
pub fn compile(data: &[u8], atlas_id: u64) -> Result<Compiled, String> {
    let font = font(data)?;
    let units = font.units_per_em().unwrap_or(1000.0);
    let (em_per_unit, scale) = (1.0 / units, EM_SIZE / units);

    // Codepoint 0 is the `.notdef` fallback, then the characters the font has.
    let mut glyphs: Vec<(u32, GlyphId)> = vec![(0, GlyphId(0))];
    for c in (0x20..=0x7Eu32).chain(0xA0..=0xFF) {
        let id = font.glyph_id(char::from_u32(c).expect("Latin-1"));
        if id.0 != 0 {
            glyphs.push((c, id));
        }
    }
    let fields: Vec<Option<Sdf>> = glyphs
        .iter()
        .map(|&(_, id)| glyph_sdf(&font, id, scale))
        .collect();
    if fields.iter().all(Option::is_none) {
        return Err("no glyphs".into());
    }

    // Shelf packing in codepoint order.
    let width = fields
        .iter()
        .flatten()
        .map(|f| f.width)
        .max()
        .unwrap_or(0)
        .max(ATLAS_WIDTH)
        .next_multiple_of(4);
    let (mut x, mut y, mut row) = (0, 0, 0);
    let mut places = Vec::with_capacity(fields.len());
    for field in &fields {
        let Some(f) = field else {
            places.push((0, 0));
            continue;
        };
        if x + f.width > width {
            (x, y, row) = (0, y + row, 0);
        }
        places.push((x, y));
        x += f.width;
        row = row.max(f.height);
    }
    let height = (y + row).max(1).next_multiple_of(4);
    let mut atlas = vec![0u8; width * height];
    for (field, &(ax, ay)) in fields.iter().zip(&places) {
        if let Some(f) = field {
            for r in 0..f.height {
                let start = (ay + r) * width + ax;
                atlas[start..start + f.width]
                    .copy_from_slice(&f.texels[r * f.width..(r + 1) * f.width]);
            }
        }
    }

    // GLYPHS, by codepoint (already ascending).
    let mut glyph_bytes = Vec::with_capacity(glyphs.len() * 40);
    let (w, h) = (width as f32, height as f32);
    for ((&(codepoint, id), field), &(ax, ay)) in glyphs.iter().zip(&fields).zip(&places) {
        let mut record = [0.0f32; 9];
        record[0] = font.h_advance_unscaled(id) * em_per_unit;
        if let Some(f) = field {
            let em = 1.0 / EM_SIZE;
            record[1..5].copy_from_slice(&[
                f.x as f32 * em,
                -(f.y + f.height as i32) as f32 * em,
                (f.x + f.width as i32) as f32 * em,
                -f.y as f32 * em,
            ]);
            record[5..9].copy_from_slice(&[
                ax as f32 / w,
                ay as f32 / h,
                (ax + f.width) as f32 / w,
                (ay + f.height) as f32 / h,
            ]);
        }
        glyph_bytes.extend(codepoint.to_le_bytes());
        for v in record {
            glyph_bytes.extend(v.to_le_bytes());
        }
    }

    // KERNING, by (left, right).
    let mut kerning = Vec::new();
    for &(left, a) in glyphs.iter().skip(1) {
        for &(right, b) in glyphs.iter().skip(1) {
            let k = font.kern_unscaled(a, b);
            if k != 0.0 {
                kerning.extend(left.to_le_bytes());
                kerning.extend(right.to_le_bytes());
                kerning.extend((k * em_per_unit).to_le_bytes());
            }
        }
    }
    let kerning_count = kerning.len() / 12;

    let mut desc = Vec::with_capacity(56);
    desc.extend((glyphs.len() as u32).to_le_bytes());
    desc.extend((kerning_count as u32).to_le_bytes());
    desc.extend(atlas_id.to_le_bytes());
    for v in [
        font.ascent_unscaled() * em_per_unit,
        font.descent_unscaled() * em_per_unit,
        font.line_gap_unscaled() * em_per_unit,
        255.0 * SPREAD / ON_EDGE,
        ON_EDGE / 255.0,
        EM_SIZE,
    ] {
        desc.extend(v.to_le_bytes());
    }
    for v in [width as u32, height as u32, 0, 0] {
        desc.extend(v.to_le_bytes());
    }
    let afont = crate::chunks::Writer::new(FORMAT, VERSION)
        .chunk("DESC", desc)
        .chunk("GLYPHS", glyph_bytes)
        .chunk("KERNING", kerning)
        .finish();
    Ok(Compiled {
        afont,
        atlas: Image {
            width: width as u32,
            height: height as u32,
            pixels: atlas.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        },
        glyphs: glyphs.len(),
        kerning: kerning_count,
    })
}

pub type Rows = Vec<(&'static str, String)>;

/// Info rows of an `.afont.bin` and its atlas's asset ID; see docs/features/fonts.md#compiled-fonts.
pub fn afont_rows(bytes: &[u8]) -> Result<(Rows, u64), String> {
    let reader = crate::chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported {FORMAT} version {}", reader.version));
    }
    let desc = reader.require("DESC")?;
    if desc.len() < 56 {
        return Err("DESC is too short".into());
    }
    let u = |at| crate::chunks::u32_at(desc, 0, 56, at).unwrap_or(0);
    let f = |at| crate::chunks::f32_at(desc, 0, 56, at).unwrap_or(0.0);
    let atlas = u64::from_le_bytes(desc[8..16].try_into().unwrap());
    Ok((
        vec![
            (
                "Format",
                format!(
                    "{FORMAT} v{}, {} chunks",
                    reader.version,
                    reader.entries().len()
                ),
            ),
            ("Glyphs", u(0).to_string()),
            ("Kerning pairs", u(4).to_string()),
            ("Em size", format!("{} px", f(36))),
            ("Distance range", format!("{:.2} texels", f(28))),
            ("Ascent", format!("{:.3}", f(16))),
            ("Descent", format!("{:.3}", f(20))),
            ("Line gap", format!("{:.3}", f(24))),
        ],
        atlas,
    ))
}

/// What a chunk holds, for the Chunks view.
pub fn describe(name: &str) -> &'static str {
    match name {
        "DESC" => "Font descriptor",
        "GLYPHS" => "Glyphs",
        "KERNING" => "Kerning pairs",
        _ => "Unknown chunk",
    }
}

const DESC_FIELDS: [&str; 13] = [
    "glyph_count",
    "kerning_count",
    "atlas",
    "ascent",
    "descent",
    "line_gap",
    "distance_range",
    "edge_value",
    "em_size",
    "atlas_width",
    "atlas_height",
    "flags",
    "reserved",
];

pub fn table(bytes: &[u8], name: &str) -> Option<crate::amesh::Table> {
    let reader = crate::chunks::Reader::parse(bytes, FORMAT).ok()?;
    let payload = reader.chunk(name)?;
    let table = |columns: Vec<&str>, rows: usize, numbered: bool| crate::amesh::Table {
        columns: columns.into_iter().map(str::to_owned).collect(),
        rows,
        numbered,
    };
    Some(match name {
        "DESC" => table(vec!["Field", "Value"], DESC_FIELDS.len(), false),
        "GLYPHS" => table(
            vec!["Codepoint", "Advance", "Plane", "UV"],
            payload.len() / 40,
            true,
        ),
        "KERNING" => table(vec!["Left", "Right", "Advance"], payload.len() / 12, true),
        _ => table(vec!["Offset", "Bytes"], payload.len().div_ceil(16), false),
    })
}

fn codepoint(c: u32) -> String {
    match char::from_u32(c).filter(|c| !c.is_control()) {
        Some(ch) if c != 0 => format!("U+{c:04X} {ch}"),
        _ => format!("U+{c:04X}"),
    }
}

pub fn cell(bytes: &[u8], name: &str, row: usize, column: usize) -> String {
    let Ok(reader) = crate::chunks::Reader::parse(bytes, FORMAT) else {
        return String::new();
    };
    let Some(p) = reader.chunk(name) else {
        return String::new();
    };
    let Some(table) = table(bytes, name) else {
        return String::new();
    };
    if table.numbered && column == 0 {
        return row.to_string();
    }
    let column = if table.numbered { column - 1 } else { column };
    let u = |index, stride, at| crate::chunks::u32_at(p, index, stride, at).unwrap_or(0);
    let f = |index, stride, at| crate::chunks::f32_at(p, index, stride, at).unwrap_or(0.0);
    let four = |index, at: usize| {
        (0..4usize)
            .map(|k| format!("{:.3}", f(index, 40, at + k * 4)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match name {
        "DESC" if column == 0 => DESC_FIELDS[row].to_owned(),
        "DESC" => match row {
            0 | 1 => u(0, 56, row * 4).to_string(),
            2 => p
                .get(8..16)
                .map(|b| format!("{:#018x}", u64::from_le_bytes(b.try_into().unwrap())))
                .unwrap_or_default(),
            3..=8 => format!("{:.3}", f(0, 56, 16 + (row - 3) * 4)),
            _ => u(0, 56, 40 + (row - 9) * 4).to_string(),
        },
        "GLYPHS" => match column {
            0 => codepoint(u(row, 40, 0)),
            1 => format!("{:.3}", f(row, 40, 4)),
            2 => four(row, 8),
            _ => four(row, 24),
        },
        "KERNING" => match column {
            0 | 1 => codepoint(u(row, 12, column * 4)),
            _ => format!("{:.3}", f(row, 12, 8)),
        },
        _ => {
            let chunk = p.get(row * 16..(row * 16 + 16).min(p.len())).unwrap_or(&[]);
            if column == 0 {
                format!("{:#06x}", row * 16)
            } else {
                chunk
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
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
    fn compiles_an_sdf_atlas_and_afont() {
        let compiled = compile(MONOGRAM, 42).unwrap();
        assert!(compiled.glyphs > 95);
        let reader = crate::chunks::Reader::parse(&compiled.afont, FORMAT).unwrap();
        let desc = reader.require("DESC").unwrap();
        assert_eq!(desc.len(), 56);
        assert_eq!(
            crate::chunks::u32_at(desc, 0, 56, 0),
            Some(compiled.glyphs as u32)
        );
        assert_eq!(desc[8..16], 42u64.to_le_bytes());
        assert_eq!(crate::chunks::f32_at(desc, 0, 56, 36), Some(48.0));
        let width = crate::chunks::u32_at(desc, 0, 56, 40).unwrap();
        assert_eq!(width, compiled.atlas.width);
        assert_eq!(width % 4, 0);
        let glyphs = reader.require("GLYPHS").unwrap();
        assert_eq!(glyphs.len(), compiled.glyphs * 40);
        // Glyph 0 is the fallback; codepoints ascend.
        let codepoints: Vec<u32> = (0..compiled.glyphs)
            .map(|i| crate::chunks::u32_at(glyphs, i, 40, 0).unwrap())
            .collect();
        assert_eq!(codepoints[0], 0);
        assert!(codepoints.windows(2).all(|w| w[0] < w[1]));
        // The space has no outline but advances.
        let space = codepoints.iter().position(|&c| c == 0x20).unwrap();
        assert!(crate::chunks::f32_at(glyphs, space, 40, 4).unwrap() > 0.0);
        assert_eq!(crate::chunks::f32_at(glyphs, space, 40, 24), Some(0.0));
        assert_eq!(crate::chunks::f32_at(glyphs, space, 40, 32), Some(0.0));

        // The 'I' field: inside its stem the value is above the edge, far outside 0.
        let i = codepoints
            .iter()
            .position(|&c| c == u32::from('I'))
            .unwrap();
        let uv = |at| crate::chunks::f32_at(glyphs, i, 40, at).unwrap();
        let (left, top, right, bottom) = (uv(24), uv(28), uv(32), uv(36));
        assert!(right > left && bottom > top);
        let texel = |u: f32, v: f32| {
            let x = (u * compiled.atlas.width as f32) as usize;
            let y = (v * compiled.atlas.height as f32) as usize;
            compiled.atlas.pixels[(y * compiled.atlas.width as usize + x) * 4]
        };
        assert!(texel((left + right) / 2.0, (top + bottom) / 2.0) > 128);
        assert_eq!(texel(left, top), 0);
    }

    #[test]
    fn afont_rows_and_tables() {
        let compiled = compile(MONOGRAM, 7).unwrap();
        let (rows, atlas) = afont_rows(&compiled.afont).unwrap();
        assert_eq!(atlas, 7);
        assert_eq!(rows[0], ("Format", "AFONT v1, 3 chunks".into()));
        assert_eq!(row(&rows, "Em size"), "48 px");
        assert_eq!(row(&rows, "Distance range"), "11.95 texels");
        let glyphs = table(&compiled.afont, "GLYPHS").unwrap();
        assert_eq!(glyphs.rows, compiled.glyphs);
        assert_eq!(cell(&compiled.afont, "GLYPHS", 0, 1), "U+0000");
        assert_eq!(cell(&compiled.afont, "GLYPHS", 1, 1), "U+0020  ");
        assert_eq!(cell(&compiled.afont, "DESC", 8, 1), "48.000");
        assert_eq!(cell(&compiled.afont, "DESC", 9, 0), "atlas_width");
        assert_eq!(describe("KERNING"), "Kerning pairs");
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
