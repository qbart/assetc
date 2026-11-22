#![doc = include_str!("../README.md")]

pub mod blend;
pub mod gradient;
pub mod grid;
pub mod heat;
pub mod lut;
pub mod modifier;
pub mod space;
pub mod vision;

pub use space::ColorSpace;

/// An RGBA8 image, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `width × height × 4` bytes: R, G, B, A per pixel.
    pub pixels: Vec<u8>,
}

impl Image {
    /// Transparent black.
    pub fn new(width: u32, height: u32) -> Self {
        Self::filled(width, height, [0; 4])
    }

    /// Every pixel `rgba`.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        Self {
            width,
            height,
            pixels: rgba.repeat(width as usize * height as usize),
        }
    }

    /// Pixel `(x, y)` is `f(x, y)`.
    pub fn from_fn(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&f(x, y));
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Width and height.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn at(&self, x: u32, y: u32) -> usize {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x}, {y}) outside the image"
        );
        (y as usize * self.width as usize + x as usize) * 4
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = self.at(x, y);
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let i = self.at(x, y);
        self.pixels[i..i + 4].copy_from_slice(&rgba);
    }

    /// The pixels, rows top to bottom.
    pub fn texels(&self) -> &[[u8; 4]] {
        self.pixels.as_chunks::<4>().0
    }

    pub fn texels_mut(&mut self) -> &mut [[u8; 4]] {
        self.pixels.as_chunks_mut::<4>().0
    }

    /// The `width × height` pixels from `(x, y)`, which must lie inside the image.
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        assert!(x + width <= self.width && y + height <= self.height);
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for row in y..y + height {
            let start = self.at(x, row);
            pixels.extend_from_slice(&self.pixels[start..start + width as usize * 4]);
        }
        Self {
            width,
            height,
            pixels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Image;

    #[test]
    fn pixels_and_crops() {
        let mut image = Image::from_fn(3, 2, |x, y| [x as u8, y as u8, 0, 255]);
        assert_eq!(image.pixel(2, 1), [2, 1, 0, 255]);
        image.set_pixel(0, 1, [9; 4]);
        assert_eq!(image.texels()[3], [9; 4]);
        let crop = image.crop(1, 1, 2, 1);
        assert_eq!(crop.pixels, [1, 1, 0, 255, 2, 1, 0, 255]);
        image.texels_mut()[0] = [7; 4];
        assert_eq!(image.pixel(0, 0), [7; 4]);
        assert_eq!(
            Image::filled(1, 2, [1, 2, 3, 4]).pixels,
            [1, 2, 3, 4, 1, 2, 3, 4]
        );
        assert_eq!(Image::new(2, 2).pixels, [0; 16]);
    }
}
