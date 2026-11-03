//! Atlases: a `*.atlas` folder's images packed into one texture, with metadata of where
//! each frame is; see docs/features/atlases.md, docs/spec/atlas.toml.md and
//! docs/spec/aatlas.md.

use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::amesh::Strings;
use crate::chunks::{Bytes, Writer};
use crate::hash::fnv1a64;

pub const FORMAT: &str = "AATLAS";
pub const VERSION: u32 = 1;
/// The settings file inside an atlas folder.
pub const CONFIG: &str = "atlas.toml";

/// Whether `dir` is an atlas folder (`*.atlas`, any case).
pub fn is_atlas(dir: &Path) -> bool {
    dir.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("atlas"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Packing {
    #[default]
    Grid,
    Pack,
}

impl Packing {
    pub const ALL: [Self; 2] = [Self::Grid, Self::Pack];

    pub fn key(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Pack => "pack",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Grid => "Grid",
            Self::Pack => "Pack",
        }
    }
}

/// An atlas' settings; see docs/spec/atlas.toml.md.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasConfig {
    pub packing: Packing,
    pub cell_width: u32,
    pub cell_height: u32,
    pub columns: u32,
    pub padding: u32,
    pub extrude: u32,
    pub trim: bool,
    pub power_of_two: bool,
    pub max_size: u32,
}

impl Default for AtlasConfig {
    fn default() -> Self {
        Self {
            packing: Packing::Grid,
            cell_width: 0,
            cell_height: 0,
            columns: 0,
            padding: 2,
            extrude: 0,
            trim: false,
            power_of_two: false,
            max_size: 4096,
        }
    }
}

impl AtlasConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let mut config = Self::default();
        if let Some(v) = table.remove("packing") {
            let key = v.as_str().ok_or("packing must be a string")?;
            config.packing = Packing::ALL
                .into_iter()
                .find(|p| p.key() == key)
                .ok_or_else(|| format!("unknown packing {key:?} (use grid or pack)"))?;
        }
        let mut int = |key: &str, max: u32, value: &mut u32| -> Result<(), String> {
            if let Some(v) = table.remove(key) {
                let n = v.as_integer().ok_or(format!("{key} must be an integer"))?;
                if !(0..=i64::from(max)).contains(&n) {
                    return Err(format!("{key} must be 0–{max}"));
                }
                *value = n as u32;
            }
            Ok(())
        };
        int("cell_width", 4096, &mut config.cell_width)?;
        int("cell_height", 4096, &mut config.cell_height)?;
        int("columns", 1024, &mut config.columns)?;
        int("padding", 64, &mut config.padding)?;
        int("extrude", 64, &mut config.extrude)?;
        int("max_size", 16384, &mut config.max_size)?;
        for (key, value) in [
            ("trim", &mut config.trim),
            ("power_of_two", &mut config.power_of_two),
        ] {
            if let Some(v) = table.remove(key) {
                *value = v.as_bool().ok_or(format!("{key} must be true or false"))?;
            }
        }
        if let Some(key) = table.keys().next() {
            return Err(format!("unknown key {key}"));
        }
        if config.extrude > config.padding {
            return Err("extrude must not exceed padding".into());
        }
        if config.max_size == 0 {
            return Err("max_size must be 1–16384".into());
        }
        Ok(config)
    }

    /// The written form; see docs/spec/atlas.toml.md.
    pub fn to_toml(&self) -> String {
        format!(
            "packing = \"{}\"\ncell_width = {}\ncell_height = {}\ncolumns = {}\npadding = {}\n\
             extrude = {}\ntrim = {}\npower_of_two = {}\nmax_size = {}\n",
            self.packing.key(),
            self.cell_width,
            self.cell_height,
            self.columns,
            self.padding,
            self.extrude,
            self.trim,
            self.power_of_two,
            self.max_size
        )
    }

    /// The folder's `atlas.toml`, or the defaults without one.
    pub fn read(dir: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(dir.join(CONFIG)) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{CONFIG}: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{CONFIG}: {e}")),
        }
    }
}

/// The folder's frames: its direct children that are texture sources, sorted by file name.
pub fn frame_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && crate::build::is_texture_source(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}

/// Where a frame is in the atlas; see docs/spec/aatlas.md#frame-64-bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub offset_x: u32,
    pub offset_y: u32,
}

/// A frame ready to pack: its name, pixels (trimmed or whole), where those start in the
/// source, and the source's size.
type Part = (String, RgbaImage, (u32, u32), (u32, u32));

/// A packed atlas: its image and frames (in name order), and the grid's shape.
pub struct Packed {
    pub image: RgbaImage,
    pub frames: Vec<Frame>,
    pub columns: u32,
    pub cell: (u32, u32),
}

/// The bounds of the pixels with alpha above 0, or the whole image when there are none.
fn opaque_bounds(image: &RgbaImage) -> (u32, u32, u32, u32) {
    let (w, h) = image.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in image.enumerate_pixels() {
        if p[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
    }
    if x1 == 0 {
        (0, 0, w, h)
    } else {
        (x0, y0, x1 - x0, y1 - y0)
    }
}

/// Places `image` at `(x, y)` in `atlas`, its edge repeated `extrude` px outward.
fn place(atlas: &mut RgbaImage, image: &RgbaImage, x: u32, y: u32, extrude: u32) {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let (aw, ah) = atlas.dimensions();
    let e = extrude as i64;
    for ty in (y as i64 - e)..(y as i64 + h as i64 + e) {
        for tx in (x as i64 - e)..(x as i64 + w as i64 + e) {
            if tx < 0 || ty < 0 || tx >= aw as i64 || ty >= ah as i64 {
                continue;
            }
            let sx = (tx - x as i64).clamp(0, w as i64 - 1) as u32;
            let sy = (ty - y as i64).clamp(0, h as i64 - 1) as u32;
            atlas.put_pixel(tx as u32, ty as u32, *image.get_pixel(sx, sy));
        }
    }
}

/// Packs named images (in name order) by `config`; see docs/features/atlases.md#settings.
pub fn pack(config: &AtlasConfig, images: Vec<(String, RgbaImage)>) -> Result<Packed, String> {
    if images.is_empty() {
        return Err("no frames (png, jpg, jpeg or tga images in the folder)".into());
    }
    let pad = config.padding;
    let mut frames = Vec::with_capacity(images.len());
    let (atlas, columns, cell) = match config.packing {
        Packing::Grid => {
            let cw = match config.cell_width {
                0 => images.iter().map(|(_, i)| i.width()).max().unwrap_or(1),
                w => w,
            }
            .max(1);
            let ch = match config.cell_height {
                0 => images.iter().map(|(_, i)| i.height()).max().unwrap_or(1),
                h => h,
            }
            .max(1);
            let n = images.len() as u32;
            let columns = match config.columns {
                0 => (n as f64).sqrt().ceil() as u32,
                c => c.min(n),
            }
            .max(1);
            let rows = n.div_ceil(columns);
            let (width, height) = (pad + columns * (cw + pad), pad + rows * (ch + pad));
            let (width, height) = rounded(config, width, height)?;
            let mut atlas = RgbaImage::new(width, height);
            for (i, (name, image)) in images.into_iter().enumerate() {
                let (sw, sh) = image.dimensions();
                let scale = (cw as f64 / sw as f64).min(ch as f64 / sh as f64).min(1.0);
                let image = if scale < 1.0 {
                    let (w, h) = (
                        ((sw as f64 * scale).round() as u32).max(1),
                        ((sh as f64 * scale).round() as u32).max(1),
                    );
                    image::imageops::resize(&image, w, h, image::imageops::FilterType::Lanczos3)
                } else {
                    image
                };
                let (w, h) = image.dimensions();
                let (col, row) = (i as u32 % columns, i as u32 / columns);
                let x = pad + col * (cw + pad) + (cw - w) / 2;
                let y = pad + row * (ch + pad) + (ch - h) / 2;
                place(&mut atlas, &image, x, y, config.extrude);
                frames.push(Frame {
                    name,
                    x,
                    y,
                    width: w,
                    height: h,
                    source_width: sw,
                    source_height: sh,
                    offset_x: 0,
                    offset_y: 0,
                });
            }
            (atlas, columns, (cw, ch))
        }
        Packing::Pack => {
            // Trimmed when asked: (name, crop, its offset, source size).
            let parts: Vec<Part> = images
                .into_iter()
                .map(|(name, image)| {
                    let size = image.dimensions();
                    if config.trim {
                        let (x, y, w, h) = opaque_bounds(&image);
                        let crop = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                        (name, crop, (x, y), size)
                    } else {
                        (name, image, (0, 0), size)
                    }
                })
                .collect();
            let mut order: Vec<usize> = (0..parts.len()).collect();
            order.sort_by(|&a, &b| {
                let (pa, pb) = (&parts[a].1, &parts[b].1);
                pb.height()
                    .cmp(&pa.height())
                    .then(pb.width().cmp(&pa.width()))
                    .then(parts[a].0.cmp(&parts[b].0))
            });
            let widest = parts.iter().map(|p| p.1.width()).max().unwrap_or(1);
            let shelves = |width: u32| {
                let (mut x, mut y, mut shelf) = (pad, pad, 0);
                let mut spots = vec![(0, 0); parts.len()];
                for &i in &order {
                    let (w, h) = parts[i].1.dimensions();
                    if x > pad && x + w + pad > width {
                        y += shelf + pad;
                        x = pad;
                        shelf = 0;
                    }
                    spots[i] = (x, y);
                    x += w + pad;
                    shelf = shelf.max(h);
                }
                (spots, y + shelf + pad)
            };
            let mut width = (widest + 2 * pad).next_power_of_two();
            let (mut spots, mut height) = shelves(width);
            while height > width && width * 2 <= config.max_size {
                width *= 2;
                (spots, height) = shelves(width);
            }
            let (aw, ah) = rounded(config, width.max(widest + 2 * pad), height)?;
            let mut atlas = RgbaImage::new(aw, ah);
            for (i, (name, image, (ox, oy), (sw, sh))) in parts.into_iter().enumerate() {
                let (x, y) = spots[i];
                place(&mut atlas, &image, x, y, config.extrude);
                frames.push(Frame {
                    name,
                    x,
                    y,
                    width: image.width(),
                    height: image.height(),
                    source_width: sw,
                    source_height: sh,
                    offset_x: ox,
                    offset_y: oy,
                });
            }
            (atlas, 0, (0, 0))
        }
    };
    Ok(Packed {
        image: atlas,
        frames,
        columns,
        cell,
    })
}

/// The atlas size, rounded to powers of two when asked, within the max size.
fn rounded(config: &AtlasConfig, width: u32, height: u32) -> Result<(u32, u32), String> {
    let (width, height) = if config.power_of_two {
        (width.next_power_of_two(), height.next_power_of_two())
    } else {
        (width, height)
    };
    if width.max(height) > config.max_size {
        return Err(format!(
            "the atlas would be {width} × {height} px, larger than max_size {}",
            config.max_size
        ));
    }
    Ok((width.max(1), height.max(1)))
}

/// Reads and packs an atlas folder.
pub fn build(dir: &Path) -> Result<(AtlasConfig, Packed), String> {
    let config = AtlasConfig::read(dir)?;
    let images = frame_files(dir)
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let file = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            image::open(&path)
                .map(|i| (name, i.to_rgba8()))
                .map_err(|e| format!("{file}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let packed = pack(&config, images)?;
    Ok((config, packed))
}

/// `grid 4 × 3` or `pack`.
pub fn packing_summary(config: &AtlasConfig, packed: &Packed) -> String {
    match config.packing {
        Packing::Grid => {
            let rows = (packed.frames.len() as u32).div_ceil(packed.columns.max(1));
            format!("grid {} × {rows}", packed.columns)
        }
        Packing::Pack => "pack".into(),
    }
}

/// The `.aatlas` metadata; see docs/spec/aatlas.md.
pub fn encode(config: &AtlasConfig, packed: &Packed, texture: u64) -> Vec<u8> {
    let (width, height) = packed.image.dimensions();
    let mut desc = Bytes::default();
    desc.u32(width)
        .u32(height)
        .u32(packed.frames.len() as u32)
        .u32(u32::from(config.packing == Packing::Pack))
        .u32(packed.columns)
        .u32(packed.cell.0)
        .u32(packed.cell.1)
        .u32(config.padding)
        .u64(texture)
        .u64(0);
    let mut frames = Bytes::default();
    let mut strings = Strings::default();
    for f in &packed.frames {
        let (at, len) = strings.add(&f.name);
        frames
            .u64(fnv1a64(f.name.as_bytes()))
            .u32(at)
            .u32(len)
            .u32(f.x)
            .u32(f.y)
            .u32(f.width)
            .u32(f.height)
            .u32(f.source_width)
            .u32(f.source_height)
            .u32(f.offset_x)
            .u32(f.offset_y)
            .f32s(&[
                f.x as f32 / width as f32,
                f.y as f32 / height as f32,
                (f.x + f.width) as f32 / width as f32,
                (f.y + f.height) as f32 / height as f32,
            ]);
    }
    let mut writer = Writer::new(FORMAT, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("FRAMES", frames.take())
        .chunk("STRINGS", strings.0);
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(size, size, image::Rgba(color))
    }

    #[test]
    fn grids_are_flipbooks_in_name_order() {
        let config = AtlasConfig::default();
        let images = (0..5)
            .map(|i| (format!("f{i}"), square(8, [i * 40, 0, 0, 255])))
            .collect();
        let packed = pack(&config, images).unwrap();
        // 5 frames: 3 columns, 2 rows of 8 px cells, 2 px padding.
        assert_eq!(packed.image.dimensions(), (2 + 3 * 10, 2 + 2 * 10));
        assert_eq!(packed.frames[4].name, "f4");
        assert_eq!((packed.frames[4].x, packed.frames[4].y), (12, 12));
        assert_eq!(packed.image.get_pixel(12, 12).0, [160, 0, 0, 255]);
        // Padding stays transparent.
        assert_eq!(packed.image.get_pixel(0, 0).0, [0; 4]);
        assert_eq!(packing_summary(&config, &packed), "grid 3 × 2");
    }

    #[test]
    fn packing_trims_extrudes_and_fits() {
        let config = AtlasConfig {
            packing: Packing::Pack,
            trim: true,
            extrude: 1,
            power_of_two: true,
            ..AtlasConfig::default()
        };
        // A 16 px image with an opaque 4 × 4 square at (6, 2).
        let mut sprite = RgbaImage::new(16, 16);
        for y in 2..6 {
            for x in 6..10 {
                sprite.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        let images = vec![
            ("a".into(), sprite),
            ("b".into(), square(8, [0, 0, 255, 255])),
        ];
        let packed = pack(&config, images).unwrap();
        let a = &packed.frames[0];
        assert_eq!((a.width, a.height, a.offset_x, a.offset_y), (4, 4, 6, 2));
        assert_eq!((a.source_width, a.source_height), (16, 16));
        let (w, h) = packed.image.dimensions();
        assert!(w.is_power_of_two() && h.is_power_of_two());
        // Extruded: the pixel left of `b` repeats its edge.
        let b = &packed.frames[1];
        assert_eq!(packed.image.get_pixel(b.x - 1, b.y).0, [0, 0, 255, 255]);
        let bytes = encode(&config, &packed, 7);
        let reader = crate::chunks::Reader::parse(&bytes, FORMAT).unwrap();
        assert_eq!(reader.require("FRAMES").unwrap().len(), 2 * 64);
        let too_big = AtlasConfig {
            max_size: 8,
            ..config
        };
        assert!(pack(&too_big, vec![("a".into(), square(16, [255; 4]))]).is_err());
    }

    #[test]
    fn settings_round_trip() {
        let config = AtlasConfig {
            packing: Packing::Pack,
            padding: 4,
            extrude: 2,
            ..AtlasConfig::default()
        };
        assert_eq!(AtlasConfig::parse(&config.to_toml()).unwrap(), config);
        assert!(AtlasConfig::parse("extrude = 5\npadding = 1").is_err());
        assert!(AtlasConfig::parse("size = 1").is_err());
    }
}
