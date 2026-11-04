//! `.amat.bin` PBR materials; see docs/spec/amat.bin.md.

use crate::Result;
use crate::read::Bytes;

const SIZE: usize = 112;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlphaMode {
    #[default]
    Opaque,
    /// Pixels with alpha below [`Material::alpha_cutoff`] are not drawn.
    Mask,
    Blend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeightMode {
    #[default]
    Parallax,
    Displacement,
}

/// Texture asset IDs; `0` means no texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextureIds {
    /// sRGB.
    pub base_color: u64,
    /// Linear, OpenGL style (+Y up).
    pub normal: u64,
    /// Linear: occlusion R, roughness G, metallic B.
    pub orm: u64,
    /// sRGB.
    pub emissive: u64,
    /// Linear, height in R (white is high).
    pub height: u64,
}

/// A glTF metallic-roughness material; factors multiply their textures.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Linear RGBA.
    pub base_color: [f32; 4],
    /// Linear RGB.
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub metallic: f32,
    pub roughness: f32,
    pub occlusion_strength: f32,
    pub normal_scale: f32,
    pub height_scale: f32,
    pub height_midlevel: f32,
    pub alpha_cutoff: f32,
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
    pub height_mode: HeightMode,
    pub textures: TextureIds,
}

impl Material {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let b = Bytes::new(bytes, "amat.bin");
        if bytes.len() != SIZE {
            return Err(b.err(format!("size {} is not {SIZE}", bytes.len())));
        }
        if b.slice(0, 4)? != b"ACMT" {
            return Err(b.err("bad magic"));
        }
        let version = b.u32(4)?;
        if version != 1 {
            return Err(b.err(format!("unsupported version {version}")));
        }
        let flags = b.u32(68)?;
        let alpha_mode = match flags & 3 {
            0 => AlphaMode::Opaque,
            1 => AlphaMode::Mask,
            2 => AlphaMode::Blend,
            _ => return Err(b.err("bad alpha mode")),
        };
        Ok(Self {
            base_color: b.f32s(8)?,
            emissive: b.f32s(24)?,
            emissive_strength: b.f32(36)?,
            metallic: b.f32(40)?,
            roughness: b.f32(44)?,
            occlusion_strength: b.f32(48)?,
            normal_scale: b.f32(52)?,
            height_scale: b.f32(56)?,
            height_midlevel: b.f32(60)?,
            alpha_cutoff: b.f32(64)?,
            alpha_mode,
            double_sided: flags & 4 != 0,
            height_mode: if flags & 8 != 0 {
                HeightMode::Displacement
            } else {
                HeightMode::Parallax
            },
            textures: TextureIds {
                base_color: b.u64(72)?,
                normal: b.u64(80)?,
                orm: b.u64(88)?,
                emissive: b.u64(96)?,
                height: b.u64(104)?,
            },
        })
    }
}
