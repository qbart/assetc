//! Menu bar logo, see docs/features/gui.md#logo.

use bevy_egui::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::{RgbaImage, imageops};

const PNG: &[u8] = include_bytes!("../assets/icons/logo.png");
pub const HEIGHT: f32 = 16.0;
/// Pixels at or below this alpha are treated as empty when trimming.
const ALPHA_THRESHOLD: u8 = 8;

/// Loads the logo as a texture sized for `HEIGHT` points at the current scale factor.
pub fn load(ctx: &egui::Context) -> TextureHandle {
    let image = image::load_from_memory_with_format(PNG, image::ImageFormat::Png)
        .expect("embedded logo is a valid PNG")
        .to_rgba8();
    let image = trim(&image);
    let height = (HEIGHT * ctx.pixels_per_point()).round().max(1.0) as u32;
    let width = (image.width() as f32 * height as f32 / image.height() as f32).round() as u32;
    let image = imageops::resize(&image, width, height, imageops::FilterType::Lanczos3);
    ctx.load_texture(
        "logo",
        ColorImage::from_rgba_unmultiplied([width as usize, height as usize], image.as_raw()),
        TextureOptions::LINEAR,
    )
}

/// Crops to the bounding box of visible pixels.
fn trim(image: &RgbaImage) -> RgbaImage {
    let opaque = image
        .enumerate_pixels()
        .filter(|(_, _, p)| p[3] > ALPHA_THRESHOLD);
    let bounds = opaque.fold(None, |acc: Option<(u32, u32, u32, u32)>, (x, y, _)| {
        Some(match acc {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        })
    });
    match bounds {
        Some((x0, y0, x1, y1)) => {
            imageops::crop_imm(image, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
        }
        None => image.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn trim_crops_transparent_border() {
        let mut image = RgbaImage::new(10, 8);
        image.put_pixel(2, 3, Rgba([255, 0, 0, 255]));
        image.put_pixel(6, 4, Rgba([0, 255, 0, 9]));
        image.put_pixel(9, 7, Rgba([0, 0, 255, ALPHA_THRESHOLD]));
        let trimmed = trim(&image);
        assert_eq!(trimmed.dimensions(), (5, 2));
        assert_eq!(trimmed.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn embedded_logo_is_wider_than_tall_after_trim() {
        let image = image::load_from_memory(PNG).unwrap().to_rgba8();
        let trimmed = trim(&image);
        assert_eq!(trimmed.dimensions(), (1223, 587));
    }
}
