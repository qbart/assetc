//! Menu bar logo, see docs/features/gui.md#logo.

use crate::imagefile::{Image, Resample};
use bevy_egui::egui::{self, ColorImage, TextureHandle, TextureOptions};

const PNG: &[u8] = include_bytes!("../assets/icons/pipe_logo.png");
pub const HEIGHT: f32 = 16.0;
/// Pixels at or below this alpha are treated as empty when trimming.
const ALPHA_THRESHOLD: u8 = 8;

/// Loads the logo as a texture sized for `HEIGHT` points at the current scale factor.
pub fn load(ctx: &egui::Context) -> TextureHandle {
    let (image, _) = crate::imagefile::decode(PNG, "png").expect("embedded logo is a valid PNG");
    let image = trim(&image);
    let height = (HEIGHT * ctx.pixels_per_point()).round().max(1.0) as u32;
    let width = (image.width as f32 * height as f32 / image.height as f32).round() as u32;
    let image = crate::imagefile::resize(&image, width, height, Resample::Lanczos3);
    ctx.load_texture(
        "logo",
        ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &image.pixels),
        TextureOptions::LINEAR,
    )
}

/// Crops to the bounding box of visible pixels.
fn trim(image: &Image) -> Image {
    let w = image.width;
    let opaque = image
        .texels()
        .iter()
        .enumerate()
        .filter(|(_, p)| p[3] > ALPHA_THRESHOLD)
        .map(|(i, _)| (i as u32 % w, i as u32 / w));
    let bounds = opaque.fold(None, |acc: Option<(u32, u32, u32, u32)>, (x, y)| {
        Some(match acc {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        })
    });
    match bounds {
        Some((x0, y0, x1, y1)) => image.crop(x0, y0, x1 - x0 + 1, y1 - y0 + 1),
        None => image.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_crops_transparent_border() {
        let mut image = Image::new(10, 8);
        image.set_pixel(2, 3, [255, 0, 0, 255]);
        image.set_pixel(6, 4, [0, 255, 0, 9]);
        image.set_pixel(9, 7, [0, 0, 255, ALPHA_THRESHOLD]);
        let trimmed = trim(&image);
        assert_eq!(trimmed.dimensions(), (5, 2));
        assert_eq!(trimmed.pixel(0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn embedded_logo_is_wider_than_tall_after_trim() {
        let (image, _) = crate::imagefile::decode(PNG, "png").unwrap();
        let trimmed = trim(&image);
        assert_eq!(trimmed.dimensions(), (59, 45));
    }
}
