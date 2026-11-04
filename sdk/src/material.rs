//! `.amat.bin` PBR materials; see docs/spec/amat.bin.md.

use crate::Result;
use crate::read::Bytes;
use crate::structs::Structs;

const FILE: &str = "amat.bin";
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
    /// Attached structs (version 2, from the `ASTRUCT` file after the record):
    /// [`Scope::Asset`](crate::structs::Scope::Asset) the material.
    pub structs: Structs,
}

impl Material {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let b = Bytes::new(bytes, FILE);
        if bytes.len() < SIZE {
            return Err(b.err(format!("size {} is below {SIZE}", bytes.len())));
        }
        if b.slice(0, 4)? != b"ACMT" {
            return Err(b.err("bad magic"));
        }
        let (record, rest) = bytes.split_at(SIZE);
        let b = Bytes::new(record, FILE);
        // Version 2 may carry an `ASTRUCT` file after the record; version 1 ends with it.
        let structs = match b.u32(4)? {
            1 if rest.is_empty() => Structs::default(),
            1 => return Err(b.err(format!("version 1 is {} bytes, not {SIZE}", bytes.len()))),
            2 if rest.is_empty() => Structs::default(),
            2 => Structs::decode(rest).map_err(|e| e.in_file(FILE))?,
            version => return Err(b.err(format!("unsupported version {version}"))),
        };
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
            structs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Value;
    use crate::structs::Scope;

    /// A material record of `version`: a mask, double-sided, base color texture `7`.
    fn record(version: u32) -> Vec<u8> {
        let mut out = b"ACMT".to_vec();
        out.extend(version.to_le_bytes());
        for v in [1.0f32, 0.5, 0.25, 1.0] {
            out.extend(v.to_le_bytes());
        }
        out.resize(68, 0);
        out.extend((1u32 | 4).to_le_bytes());
        out.extend(7u64.to_le_bytes());
        out.resize(SIZE, 0);
        out
    }

    #[test]
    fn reads_version_1_and_2() {
        for version in [1, 2] {
            let m = Material::decode(&record(version)).unwrap();
            assert_eq!(m.base_color, [1.0, 0.5, 0.25, 1.0]);
            assert_eq!((m.alpha_mode, m.double_sided), (AlphaMode::Mask, true));
            assert_eq!(m.textures.base_color, 7);
            assert!(m.structs.is_empty());
        }
        assert!(Material::decode(&record(3)).is_err());
        assert!(Material::decode(&record(1)[..SIZE - 1]).is_err());
    }

    #[test]
    fn reads_struct_data_after_the_record() {
        let mut bytes = record(2);
        bytes.extend(crate::structs::tests::astruct());
        let m = Material::decode(&bytes).unwrap();
        let meta = m.structs.get(Scope::Asset, 0, "Meta").unwrap();
        assert_eq!(meta.get("tier").and_then(Value::as_int), Some(3));
        // Version 1 ends with the record; bytes after a version 2 record must be ASTRUCT.
        let mut v1 = record(1);
        v1.extend(crate::structs::tests::astruct());
        assert!(Material::decode(&v1).is_err());
        let mut junk = record(2);
        junk.extend([0; 40]);
        assert!(Material::decode(&junk).is_err());
    }
}
