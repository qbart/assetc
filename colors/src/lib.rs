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
