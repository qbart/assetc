#![doc = include_str!("../README.md")]

pub use assetc_colors::Image;

use imgio::mip::Filter;
use imgio::resize::ResizeOptions;
use imgio::{Format, Surface};

/// The largest cell side, px.
pub const MAX_CELL: u32 = 4096;
/// The most cells per grid row.
pub const MAX_COLUMNS: u32 = 1024;
/// The most padding (and so extrusion) around a frame, px.
pub const MAX_PADDING: u32 = 64;
/// The largest `max_size`, px.
pub const MAX_SIZE: u32 = 16384;

/// How frames are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Packing {
    /// Every frame in a cell of one size, row by row: a flipbook.
    #[default]
    Grid,
    /// Frames at their own sizes on shelves, packed tightly.
    Pack,
}

impl Packing {
    pub const ALL: [Self; 2] = [Self::Grid, Self::Pack];

    /// As `atlas.toml` writes it: `grid`, `pack`.
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

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// How an atlas is packed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub packing: Packing,
    /// Grid: the cell width, px; `0` is the widest frame's.
    pub cell_width: u32,
    /// Grid: the cell height, px; `0` is the tallest frame's.
    pub cell_height: u32,
    /// Grid: cells per row; `0` is `ceil(√frames)`.
    pub columns: u32,
    /// Pixels around every frame, between frames and to the atlas' edge.
    pub padding: u32,
    /// Pixels of each frame's edge repeated outward into its padding.
    pub extrude: u32,
    /// Pack: crop each frame to its pixels with alpha above 0.
    pub trim: bool,
    /// Round the atlas' sides up to powers of two.
    pub power_of_two: bool,
    /// The largest side allowed, px.
    pub max_size: u32,
}

impl Default for Settings {
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

impl Settings {
    /// The first value out of range.
    pub fn validate(&self) -> Result<(), String> {
        for (key, value, max) in [
            ("cell_width", self.cell_width, MAX_CELL),
            ("cell_height", self.cell_height, MAX_CELL),
            ("columns", self.columns, MAX_COLUMNS),
            ("padding", self.padding, MAX_PADDING),
            ("extrude", self.extrude, MAX_PADDING),
        ] {
            if value > max {
                return Err(format!("{key} must be 0–{max}"));
            }
        }
        if self.extrude > self.padding {
            return Err("extrude must not exceed padding".into());
        }
        if !(1..=MAX_SIZE).contains(&self.max_size) {
            return Err(format!("max_size must be 1–{MAX_SIZE}"));
        }
        Ok(())
    }
}

/// Where a frame is in the atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub name: String,
    /// The frame's pixels in the atlas, px: left.
    pub x: u32,
    /// Top.
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The size of the image it came from.
    pub source_width: u32,
    pub source_height: u32,
    /// Where the frame's pixels start in that image (trimmed, or `0`).
    pub offset_x: u32,
    pub offset_y: u32,
}

impl Frame {
    /// `[u0, v0, u1, v1]`: the rectangle as 0–1 coordinates of a `width × height` atlas,
    /// top-left origin.
    pub fn uv(&self, width: u32, height: u32) -> [f32; 4] {
        [
            self.x as f32 / width as f32,
            self.y as f32 / height as f32,
            (self.x + self.width) as f32 / width as f32,
            (self.y + self.height) as f32 / height as f32,
        ]
    }
}

/// A packed atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    pub image: Image,
    /// In the order the images were given.
    pub frames: Vec<Frame>,
    pub packing: Packing,
    /// Grid: cells per row; `0` for pack.
    pub columns: u32,
    /// Grid: the cell size, px; `(0, 0)` for pack.
    pub cell: (u32, u32),
}

impl Atlas {
    /// The first frame named `name`.
    pub fn frame(&self, name: &str) -> Option<&Frame> {
        self.frames.iter().find(|f| f.name == name)
    }

    /// The `index`th frame's [`Frame::uv`] in this atlas.
    pub fn uv(&self, index: usize) -> Option<[f32; 4]> {
        let (w, h) = self.image.dimensions();
        self.frames.get(index).map(|f| f.uv(w, h))
    }

    /// `grid 4 × 3` or `pack`.
    pub fn summary(&self) -> String {
        match self.packing {
            Packing::Grid => {
                let rows = (self.frames.len() as u32).div_ceil(self.columns.max(1));
                format!("grid {} × {rows}", self.columns)
            }
            Packing::Pack => "pack".into(),
        }
    }
}

/// The rectangle `(x, y, width, height)` of the pixels with alpha above 0, or the whole
/// image when there are none.
pub fn trim_bounds(image: &Image) -> (u32, u32, u32, u32) {
    let (w, h) = image.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (i, p) in image.texels().iter().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
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

/// Draws `image` into `atlas` at `(x, y)`, its edge pixels repeated `extrude` px outward;
/// what falls outside the atlas is dropped.
pub fn blit(atlas: &mut Image, image: &Image, x: u32, y: u32, extrude: u32) {
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
            atlas.set_pixel(tx as u32, ty as u32, image.pixel(sx, sy));
        }
    }
}

/// Lanczos-3, color weighted by alpha so transparent pixels do not bleed.
fn resize(image: &Image, width: u32, height: u32) -> Image {
    let options = ResizeOptions {
        filter: Filter::Lanczos3,
        srgb: Some(false),
        premultiply: true,
        ..Default::default()
    };
    let resized = Surface::new(Format::Rgba8Unorm, image.width, image.height, &image.pixels)
        .and_then(|s| imgio::resize::resize(&s, width, height, &options))
        .expect("resizing an RGBA8 image");
    Image {
        width: resized.width,
        height: resized.height,
        pixels: resized.data.to_vec(),
    }
}

/// A frame ready to pack: its name, pixels (trimmed or whole), where those start in the
/// source, and the source's size.
type Part = (String, Image, (u32, u32), (u32, u32));

/// Packs named images by `settings`: grid cells or shelves, trimmed, padded, extruded and
/// rounded as asked. Frames keep the order given.
pub fn pack(settings: &Settings, images: Vec<(String, Image)>) -> Result<Atlas, String> {
    settings.validate()?;
    if images.is_empty() {
        return Err("no frames".into());
    }
    let pad = settings.padding;
    let mut frames = Vec::with_capacity(images.len());
    let (image, columns, cell) = match settings.packing {
        Packing::Grid => {
            let cw = match settings.cell_width {
                0 => images.iter().map(|(_, i)| i.width).max().unwrap_or(1),
                w => w,
            }
            .max(1);
            let ch = match settings.cell_height {
                0 => images.iter().map(|(_, i)| i.height).max().unwrap_or(1),
                h => h,
            }
            .max(1);
            let n = images.len() as u32;
            let columns = match settings.columns {
                0 => (n as f64).sqrt().ceil() as u32,
                c => c.min(n),
            }
            .max(1);
            let rows = n.div_ceil(columns);
            let (width, height) = (pad + columns * (cw + pad), pad + rows * (ch + pad));
            let (width, height) = rounded(settings, width, height)?;
            let mut atlas = Image::new(width, height);
            for (i, (name, image)) in images.into_iter().enumerate() {
                let (sw, sh) = image.dimensions();
                let scale = (cw as f64 / sw as f64).min(ch as f64 / sh as f64).min(1.0);
                let image = if scale < 1.0 {
                    let (w, h) = (
                        ((sw as f64 * scale).round() as u32).max(1),
                        ((sh as f64 * scale).round() as u32).max(1),
                    );
                    resize(&image, w, h)
                } else {
                    image
                };
                let (w, h) = image.dimensions();
                let (col, row) = (i as u32 % columns, i as u32 / columns);
                let x = pad + col * (cw + pad) + (cw - w) / 2;
                let y = pad + row * (ch + pad) + (ch - h) / 2;
                blit(&mut atlas, &image, x, y, settings.extrude);
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
            let parts: Vec<Part> = images
                .into_iter()
                .map(|(name, image)| {
                    let size = image.dimensions();
                    if settings.trim {
                        let (x, y, w, h) = trim_bounds(&image);
                        let crop = image.crop(x, y, w, h);
                        (name, crop, (x, y), size)
                    } else {
                        (name, image, (0, 0), size)
                    }
                })
                .collect();
            // Tallest first, then widest, then by name.
            let mut order: Vec<usize> = (0..parts.len()).collect();
            order.sort_by(|&a, &b| {
                let (pa, pb) = (&parts[a].1, &parts[b].1);
                pb.height
                    .cmp(&pa.height)
                    .then(pb.width.cmp(&pa.width))
                    .then(parts[a].0.cmp(&parts[b].0))
            });
            let widest = parts.iter().map(|p| p.1.width).max().unwrap_or(1);
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
            // The narrowest power of two keeping the atlas no taller than wide.
            let mut width = (widest + 2 * pad).next_power_of_two();
            let (mut spots, mut height) = shelves(width);
            while height > width && width * 2 <= settings.max_size {
                width *= 2;
                (spots, height) = shelves(width);
            }
            let (aw, ah) = rounded(settings, width.max(widest + 2 * pad), height)?;
            let mut atlas = Image::new(aw, ah);
            for (i, (name, image, (ox, oy), (sw, sh))) in parts.into_iter().enumerate() {
                let (x, y) = spots[i];
                blit(&mut atlas, &image, x, y, settings.extrude);
                frames.push(Frame {
                    name,
                    x,
                    y,
                    width: image.width,
                    height: image.height,
                    source_width: sw,
                    source_height: sh,
                    offset_x: ox,
                    offset_y: oy,
                });
            }
            (atlas, 0, (0, 0))
        }
    };
    Ok(Atlas {
        image,
        frames,
        packing: settings.packing,
        columns,
        cell,
    })
}

/// The atlas size, rounded to powers of two when asked, within the max size.
fn rounded(settings: &Settings, width: u32, height: u32) -> Result<(u32, u32), String> {
    let (width, height) = if settings.power_of_two {
        (width.next_power_of_two(), height.next_power_of_two())
    } else {
        (width, height)
    };
    if width.max(height) > settings.max_size {
        return Err(format!(
            "the atlas would be {width} × {height} px, larger than max_size {}",
            settings.max_size
        ));
    }
    Ok((width.max(1), height.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: u32, color: [u8; 4]) -> Image {
        Image::filled(size, size, color)
    }

    #[test]
    fn grids_are_flipbooks_in_order() {
        let settings = Settings::default();
        let images = (0..5)
            .map(|i| (format!("f{i}"), square(8, [i * 40, 0, 0, 255])))
            .collect();
        let atlas = pack(&settings, images).unwrap();
        // 5 frames: 3 columns, 2 rows of 8 px cells, 2 px padding.
        assert_eq!(atlas.image.dimensions(), (2 + 3 * 10, 2 + 2 * 10));
        assert_eq!(atlas.frames[4].name, "f4");
        assert_eq!((atlas.frames[4].x, atlas.frames[4].y), (12, 12));
        assert_eq!(atlas.image.pixel(12, 12), [160, 0, 0, 255]);
        // Padding stays transparent.
        assert_eq!(atlas.image.pixel(0, 0), [0; 4]);
        assert_eq!(atlas.summary(), "grid 3 × 2");
        assert_eq!(atlas.frame("f1").map(|f| f.x), Some(12));
        assert_eq!(
            atlas.uv(0),
            Some([2.0 / 32.0, 2.0 / 22.0, 10.0 / 32.0, 10.0 / 22.0])
        );
        assert_eq!(atlas.uv(5), None);
    }

    #[test]
    fn large_frames_shrink_into_their_cells() {
        let settings = Settings {
            cell_width: 8,
            cell_height: 8,
            ..Settings::default()
        };
        let atlas = pack(
            &settings,
            vec![("wide".into(), Image::filled(32, 16, [9; 4]))],
        )
        .unwrap();
        let f = &atlas.frames[0];
        // Aspect kept, centered: 8 × 4 in an 8 × 8 cell.
        assert_eq!((f.x, f.y, f.width, f.height), (2, 4, 8, 4));
        assert_eq!((f.source_width, f.source_height), (32, 16));
        assert_eq!(atlas.image.pixel(5, 5), [9; 4]);
    }

    #[test]
    fn packing_trims_extrudes_and_fits() {
        let settings = Settings {
            packing: Packing::Pack,
            trim: true,
            extrude: 1,
            power_of_two: true,
            ..Settings::default()
        };
        // A 16 px image with an opaque 4 × 4 square at (6, 2).
        let mut sprite = Image::new(16, 16);
        for y in 2..6 {
            for x in 6..10 {
                sprite.set_pixel(x, y, [0, 255, 0, 255]);
            }
        }
        assert_eq!(trim_bounds(&sprite), (6, 2, 4, 4));
        assert_eq!(trim_bounds(&Image::new(3, 2)), (0, 0, 3, 2));
        let images = vec![
            ("a".into(), sprite),
            ("b".into(), square(8, [0, 0, 255, 255])),
        ];
        let atlas = pack(&settings, images).unwrap();
        assert_eq!(atlas.summary(), "pack");
        let a = &atlas.frames[0];
        assert_eq!((a.width, a.height, a.offset_x, a.offset_y), (4, 4, 6, 2));
        assert_eq!((a.source_width, a.source_height), (16, 16));
        let (w, h) = atlas.image.dimensions();
        assert!(w.is_power_of_two() && h.is_power_of_two());
        // Extruded: the pixel left of `b` repeats its edge.
        let b = &atlas.frames[1];
        assert_eq!(atlas.image.pixel(b.x - 1, b.y), [0, 0, 255, 255]);
        let too_big = Settings {
            max_size: 8,
            ..settings
        };
        assert_eq!(
            pack(&too_big, vec![("a".into(), square(16, [255; 4]))]).unwrap_err(),
            "the atlas would be 32 × 32 px, larger than max_size 8"
        );
    }

    #[test]
    fn blits_clip_at_the_edges() {
        let mut atlas = Image::new(4, 4);
        blit(&mut atlas, &square(2, [7; 4]), 0, 0, 2);
        assert_eq!(atlas.pixel(3, 3), [7; 4]);
        blit(&mut atlas, &Image::new(0, 0), 1, 1, 1);
    }

    #[test]
    fn settings_are_checked() {
        assert_eq!(
            pack(&Settings::default(), Vec::new()).unwrap_err(),
            "no frames"
        );
        let bad = |s: Settings| s.validate().unwrap_err();
        assert_eq!(
            bad(Settings {
                extrude: 3,
                ..Settings::default()
            }),
            "extrude must not exceed padding"
        );
        assert_eq!(
            bad(Settings {
                padding: 65,
                ..Settings::default()
            }),
            "padding must be 0–64"
        );
        assert_eq!(
            bad(Settings {
                max_size: 0,
                ..Settings::default()
            }),
            "max_size must be 1–16384"
        );
        assert_eq!(Packing::from_key("pack"), Some(Packing::Pack));
        assert_eq!(Packing::from_key("shelf"), None);
    }
}
