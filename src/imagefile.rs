//! Image files through imgio: decoding to RGBA8, writing PNGs, resizing; see
//! docs/features/build.md#textures.

use std::path::Path;

use imgio::file::{self, FileFormat};
use imgio::mip::Filter;
use imgio::resize::ResizeOptions;
use imgio::{Format, Surface};

pub use assetc_colors::Image;

/// What a decoded file held, before it became RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channels {
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
}

impl Channels {
    pub fn has_color(self) -> bool {
        matches!(self, Self::Rgb | Self::Rgba)
    }

    pub fn has_alpha(self) -> bool {
        matches!(self, Self::GrayAlpha | Self::Rgba)
    }
}

/// Reads and decodes an image file.
pub fn read(path: &Path) -> Result<(Image, Channels), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    decode(&bytes, extension)
}

/// Decodes a file to RGBA8: gray as R = G = B, a missing alpha 255, 16 bits rounded to 8. Its
/// format comes from its first bytes; a TGA (no signature) from `extension`.
pub fn decode(bytes: &[u8], extension: &str) -> Result<(Image, Channels), String> {
    let format = match FileFormat::detect(bytes) {
        Some(f) if f != FileFormat::Tga => Some(f),
        detected => FileFormat::from_extension(extension).or(detected),
    }
    .ok_or("not an image file assetc reads")?;
    let image = file::decode_as(format, bytes).map_err(|e| e.to_string())?;
    to_rgba8(&image)
}

/// A 16-bit value as 8 bits.
fn narrow(v: u16) -> u8 {
    ((u32::from(v) + 128) / 257) as u8
}

fn to_rgba8(image: &file::Image) -> Result<(Image, Channels), String> {
    let (w, h) = (image.width, image.height);
    let data: &[u8] = &image.data;
    let u16s = || {
        data.as_chunks::<2>()
            .0
            .iter()
            .map(|b| narrow(u16::from_le_bytes(*b)))
    };
    let (pixels, channels): (Vec<u8>, _) = match image.format {
        Format::R8Unorm => (
            data.iter().flat_map(|&v| [v, v, v, 255]).collect(),
            Channels::Gray,
        ),
        Format::Rg8Unorm => (
            data.as_chunks::<2>()
                .0
                .iter()
                .flat_map(|&[v, a]| [v, v, v, a])
                .collect(),
            Channels::GrayAlpha,
        ),
        Format::Rgb8Srgb | Format::Rgb8Unorm => (
            data.as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[r, g, b]| [r, g, b, 255])
                .collect(),
            Channels::Rgb,
        ),
        Format::Rgba8Srgb | Format::Rgba8Unorm => (data.to_vec(), Channels::Rgba),
        Format::R16Unorm => (
            u16s().flat_map(|v| [v, v, v, 255]).collect(),
            Channels::Gray,
        ),
        Format::Rg16Unorm => {
            let v: Vec<u8> = u16s().collect();
            (
                v.as_chunks::<2>()
                    .0
                    .iter()
                    .flat_map(|&[v, a]| [v, v, v, a])
                    .collect(),
                Channels::GrayAlpha,
            )
        }
        Format::Rgba16Unorm => {
            // A 16-bit RGB PNG comes as RGBA with alpha 65535.
            let opaque = data.as_chunks::<8>().0.iter().all(|t| t[6..] == [255, 255]);
            let channels = if opaque {
                Channels::Rgb
            } else {
                Channels::Rgba
            };
            (u16s().collect(), channels)
        }
        _ => {
            let rgba = imgio::convert::convert_to_buffer(
                &image.surface().map_err(|e| e.to_string())?,
                Format::Rgba8Unorm,
            )
            .map_err(|e| e.to_string())?;
            (rgba.to_vec(), Channels::Rgba)
        }
    };
    Ok((
        Image {
            width: w,
            height: h,
            pixels,
        },
        channels,
    ))
}

/// The image as an 8-bit RGBA PNG.
pub fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let surface = Surface::new(Format::Rgba8Unorm, image.width, image.height, &image.pixels)
        .map_err(|e| e.to_string())?;
    file::encode(FileFormat::Png, &surface)
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

pub fn write_png(path: &Path, image: &Image) -> Result<(), String> {
    std::fs::write(path, encode_png(image)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// How [`resize`] samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resample {
    /// Lanczos-3, color weighted by alpha so transparent pixels do not bleed: for images.
    Lanczos3,
    /// Bilinear, each channel on its own: for data, and quick previews.
    Bilinear,
}

/// Resizes to `width × height` with the values as stored (no sRGB decoding).
pub fn resize(image: &Image, width: u32, height: u32, resample: Resample) -> Image {
    if (image.width, image.height) == (width, height) {
        return image.clone();
    }
    let options = ResizeOptions {
        filter: match resample {
            Resample::Lanczos3 => Filter::Lanczos3,
            Resample::Bilinear => Filter::Tent,
        },
        srgb: Some(false),
        premultiply: resample == Resample::Lanczos3,
        ..Default::default()
    };
    let resized = Surface::new(Format::Rgba8Unorm, image.width, image.height, &image.pixels)
        .and_then(|s| imgio::resize::resize(&s, width.max(1), height.max(1), &options))
        .expect("resizing an RGBA8 image");
    Image {
        width: resized.width,
        height: resized.height,
        pixels: resized.data.to_vec(),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A 4 × 4 baseline JPEG of `#c86432` (made with ffmpeg), for tests that need one.
    pub const JPEG: &[u8] = include_bytes!("testdata/orange.jpg");

    /// A PNG of `image`, for tests that need files.
    pub fn png(path: &Path, image: &Image) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        write_png(path, image).unwrap();
    }

    #[test]
    fn png_round_trips() {
        let image = Image::from_fn(3, 2, |x, y| [x as u8 * 50, y as u8 * 90, 7, 200]);
        let (back, channels) = decode(&encode_png(&image).unwrap(), "png").unwrap();
        assert_eq!((back, channels), (image, Channels::Rgba));
    }

    #[test]
    fn gray_and_rgb_become_rgba() {
        let gray = Surface::new(Format::R8Unorm, 2, 1, &[10, 20]).unwrap();
        let bytes = file::encode(FileFormat::Png, &gray).unwrap();
        let (image, channels) = decode(&bytes, "png").unwrap();
        assert_eq!(channels, Channels::Gray);
        assert_eq!(image.pixels, [10, 10, 10, 255, 20, 20, 20, 255]);
        let rgb = Surface::new(Format::Rgb8Unorm, 1, 1, &[1, 2, 3]).unwrap();
        let bytes = file::encode(FileFormat::Tga, &rgb).unwrap();
        // A TGA by its extension.
        let (image, channels) = decode(&bytes, "TGA").unwrap();
        assert_eq!(
            (image.pixels, channels),
            (vec![1, 2, 3, 255], Channels::Rgb)
        );
        assert!(decode(b"not an image", "png").is_err());
    }

    #[test]
    fn jpegs_are_rgb() {
        let (image, channels) = decode(JPEG, "jpg").unwrap();
        assert_eq!((image.dimensions(), channels), ((4, 4), Channels::Rgb));
        let p = image.pixel(1, 1);
        assert!(
            p[0].abs_diff(200) < 8 && p[1].abs_diff(100) < 8 && p[3] == 255,
            "{p:?}"
        );
    }

    /// An 8-bit gray (and alpha) PNG.
    pub fn gray_png(path: &Path, w: u32, h: u32, value: u8, alpha: bool) {
        let (format, texel): (_, &[u8]) = if alpha {
            (Format::Rg8Unorm, &[value, 255])
        } else {
            (Format::R8Unorm, &[value])
        };
        let data = texel.repeat((w * h) as usize);
        let surface = Surface::new(format, w, h, &data).unwrap();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(path, &file::encode(FileFormat::Png, &surface).unwrap()[..]).unwrap();
    }

    #[test]
    fn sixteen_bits_round_to_eight() {
        assert_eq!(
            [narrow(0), narrow(257), narrow(65535), narrow(32896)],
            [0, 1, 255, 128]
        );
    }

    #[test]
    fn resizes_without_bleeding() {
        // A transparent red pixel next to an opaque green one: Lanczos keeps green, bilinear
        // averages the channels.
        let image = Image::from_fn(2, 1, |x, _| {
            if x == 0 {
                [255, 0, 0, 0]
            } else {
                [0, 255, 0, 255]
            }
        });
        let half = resize(&image, 1, 1, Resample::Lanczos3);
        assert_eq!(half.pixels[..3], [0, 255, 0]);
        let half = resize(&image, 1, 1, Resample::Bilinear);
        assert_eq!(half.pixels, [128, 128, 0, 128]);
        assert_eq!(resize(&image, 2, 1, Resample::Lanczos3), image);
    }
}
