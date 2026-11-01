//! PBR materials: `.mat.toml` sources and `.mat` files, see docs/features/materials.md
//! and docs/spec/material.md.

use serde::Deserialize;

use crate::texture::ColorSpace;

/// Suffix of material source files: `<name>.mat.toml`.
pub const SOURCE_SUFFIX: &str = ".mat.toml";
pub const SIZE: usize = 112;
const MAGIC: [u8; 4] = *b"ACMT";
const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Slot {
    BaseColor,
    Normal,
    Roughness,
    Metallic,
    Occlusion,
    Emissive,
    Height,
}

impl Slot {
    pub const ALL: [Self; 7] = [
        Self::BaseColor,
        Self::Normal,
        Self::Roughness,
        Self::Metallic,
        Self::Occlusion,
        Self::Emissive,
        Self::Height,
    ];

    /// Key in `[textures]` and the name shown to users.
    pub fn name(self) -> &'static str {
        match self {
            Self::BaseColor => "base_color",
            Self::Normal => "normal",
            Self::Roughness => "roughness",
            Self::Metallic => "metallic",
            Self::Occlusion => "occlusion",
            Self::Emissive => "emissive",
            Self::Height => "height",
        }
    }

    /// File name suffix, e.g. `n` in `brick.n.jpg`; the base color has none.
    pub fn suffix(self) -> Option<&'static str> {
        match self {
            Self::BaseColor => None,
            Self::Normal => Some("n"),
            Self::Roughness => Some("r"),
            Self::Metallic => Some("m"),
            Self::Occlusion => Some("ao"),
            Self::Emissive => Some("e"),
            Self::Height => Some("h"),
        }
    }

    pub fn from_suffix(suffix: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.suffix() == Some(suffix))
    }

    pub fn color_space(self) -> ColorSpace {
        match self {
            Self::BaseColor | Self::Emissive => ColorSpace::Srgb,
            _ => ColorSpace::Linear,
        }
    }

    /// Channel in the packed ORM texture, for slots that are packed.
    pub fn orm_channel(self) -> Option<usize> {
        match self {
            Self::Occlusion => Some(0),
            Self::Roughness => Some(1),
            Self::Metallic => Some(2),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NormalY {
    #[default]
    Gl,
    Dx,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeightMode {
    #[default]
    Parallax,
    Displacement,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

/// A parsed `<name>.mat.toml`; see docs/spec/mat.toml.md.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Source {
    pub base_color: Option<[f32; 4]>,
    pub metallic: Option<f32>,
    pub roughness: Option<f32>,
    pub occlusion_strength: Option<f32>,
    pub normal_scale: Option<f32>,
    pub normal_y: NormalY,
    pub emissive: Option<[f32; 3]>,
    pub emissive_strength: Option<f32>,
    pub height_mode: HeightMode,
    pub height_scale: Option<f32>,
    pub height_midlevel: Option<f32>,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: bool,
    pub textures: SourceTextures,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SourceTextures {
    base_color: Option<String>,
    normal: Option<String>,
    roughness: Option<String>,
    metallic: Option<String>,
    occlusion: Option<String>,
    emissive: Option<String>,
    height: Option<String>,
}

impl SourceTextures {
    pub fn get(&self, slot: Slot) -> Option<&str> {
        match slot {
            Slot::BaseColor => &self.base_color,
            Slot::Normal => &self.normal,
            Slot::Roughness => &self.roughness,
            Slot::Metallic => &self.metallic,
            Slot::Occlusion => &self.occlusion,
            Slot::Emissive => &self.emissive,
            Slot::Height => &self.height,
        }
        .as_deref()
    }
}

impl Source {
    pub fn parse(text: &str) -> Result<Self, String> {
        let source: Self = toml::from_str(text).map_err(|e| e.message().to_owned())?;
        source.validate()?;
        Ok(source)
    }

    fn validate(&self) -> Result<(), String> {
        let unit = |key: &str, v: f32| check(key, v, (0.0..=1.0).contains(&v), "0..=1");
        let non_negative = |key: &str, v: f32| check(key, v, v >= 0.0, ">= 0");
        for v in self.base_color.into_iter().flatten() {
            unit("base_color", v)?;
        }
        for v in self.emissive.into_iter().flatten() {
            unit("emissive", v)?;
        }
        for (key, v) in [
            ("metallic", self.metallic),
            ("roughness", self.roughness),
            ("occlusion_strength", self.occlusion_strength),
            ("height_midlevel", self.height_midlevel),
            ("alpha_cutoff", self.alpha_cutoff),
        ] {
            v.map_or(Ok(()), |v| unit(key, v))?;
        }
        for (key, v) in [
            ("emissive_strength", self.emissive_strength),
            ("height_scale", self.height_scale),
        ] {
            v.map_or(Ok(()), |v| non_negative(key, v))?;
        }
        self.normal_scale
            .map_or(Ok(()), |v| check("normal_scale", v, true, "finite"))
    }

    /// The compiled material; `maps` are the slots that have a texture.
    pub fn material(&self, maps: &[Slot], textures: TextureIds) -> Material {
        let has = |slot| maps.contains(&slot);
        Material {
            base_color: self.base_color.unwrap_or([1.0; 4]),
            emissive: self.emissive.unwrap_or(if has(Slot::Emissive) {
                [1.0; 3]
            } else {
                [0.0; 3]
            }),
            emissive_strength: self.emissive_strength.unwrap_or(1.0),
            metallic: self
                .metallic
                .unwrap_or(if has(Slot::Metallic) { 1.0 } else { 0.0 }),
            roughness: self.roughness.unwrap_or(1.0),
            occlusion_strength: self.occlusion_strength.unwrap_or(1.0),
            normal_scale: self.normal_scale.unwrap_or(1.0),
            height_scale: self.height_scale.unwrap_or(0.05),
            height_midlevel: self.height_midlevel.unwrap_or(0.5),
            alpha_cutoff: self.alpha_cutoff.unwrap_or(0.5),
            alpha_mode: self.alpha_mode,
            double_sided: self.double_sided,
            height_mode: self.height_mode,
            textures,
        }
    }
}

fn check(key: &str, v: f32, in_range: bool, range: &str) -> Result<(), String> {
    if v.is_finite() && in_range {
        Ok(())
    } else {
        Err(format!("{key} = {v} is out of range ({range})"))
    }
}

/// Texture asset IDs; `0` means no texture.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextureIds {
    pub base_color: u64,
    pub normal: u64,
    pub orm: u64,
    pub emissive: u64,
    pub height: u64,
}

/// The contents of a `.mat` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub base_color: [f32; 4],
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

pub fn encode(m: &Material) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIZE);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let scalars = [
        m.emissive_strength,
        m.metallic,
        m.roughness,
        m.occlusion_strength,
        m.normal_scale,
        m.height_scale,
        m.height_midlevel,
        m.alpha_cutoff,
    ];
    for v in m.base_color.iter().chain(&m.emissive).chain(&scalars) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let alpha = match m.alpha_mode {
        AlphaMode::Opaque => 0,
        AlphaMode::Mask => 1,
        AlphaMode::Blend => 2,
    };
    let flags = alpha
        | u32::from(m.double_sided) << 2
        | u32::from(m.height_mode == HeightMode::Displacement) << 3;
    out.extend_from_slice(&flags.to_le_bytes());
    let t = &m.textures;
    for id in [t.base_color, t.normal, t.orm, t.emissive, t.height] {
        out.extend_from_slice(&id.to_le_bytes());
    }
    debug_assert_eq!(out.len(), SIZE);
    out
}

pub fn decode(bytes: &[u8]) -> Result<Material, String> {
    if bytes.len() != SIZE {
        return Err(format!(
            "invalid material: size {} is not {SIZE}",
            bytes.len()
        ));
    }
    if bytes[..4] != MAGIC {
        return Err("invalid material: bad magic".into());
    }
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let f32_at = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    if u32_at(4) != VERSION {
        return Err("invalid material: unsupported version".into());
    }
    let flags = u32_at(68);
    let alpha_mode = match flags & 3 {
        0 => AlphaMode::Opaque,
        1 => AlphaMode::Mask,
        2 => AlphaMode::Blend,
        _ => return Err("invalid material: bad alpha mode".into()),
    };
    Ok(Material {
        base_color: std::array::from_fn(|i| f32_at(8 + i * 4)),
        emissive: std::array::from_fn(|i| f32_at(24 + i * 4)),
        emissive_strength: f32_at(36),
        metallic: f32_at(40),
        roughness: f32_at(44),
        occlusion_strength: f32_at(48),
        normal_scale: f32_at(52),
        height_scale: f32_at(56),
        height_midlevel: f32_at(60),
        alpha_cutoff: f32_at(64),
        alpha_mode,
        double_sided: flags & 4 != 0,
        height_mode: if flags & 8 != 0 {
            HeightMode::Displacement
        } else {
            HeightMode::Parallax
        },
        textures: TextureIds {
            base_color: u64_at(72),
            normal: u64_at(80),
            orm: u64_at(88),
            emissive: u64_at(96),
            height: u64_at(104),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_round_trip() {
        for slot in Slot::ALL {
            if let Some(suffix) = slot.suffix() {
                assert_eq!(Slot::from_suffix(suffix), Some(slot));
            }
        }
        assert_eq!(Slot::from_suffix("x"), None);
        assert_eq!(Slot::from_suffix("N"), None);
    }

    #[test]
    fn empty_source_uses_defaults() {
        let m = Source::parse("")
            .unwrap()
            .material(&[], TextureIds::default());
        assert_eq!(m.base_color, [1.0; 4]);
        assert_eq!((m.metallic, m.roughness), (0.0, 1.0));
        assert_eq!(m.emissive, [0.0; 3]);
        assert_eq!((m.height_scale, m.height_midlevel), (0.05, 0.5));
        assert_eq!(m.alpha_mode, AlphaMode::Opaque);
        assert_eq!(m.height_mode, HeightMode::Parallax);
    }

    #[test]
    fn maps_change_metallic_and_emissive_defaults() {
        let maps = [Slot::Metallic, Slot::Emissive];
        let m = Source::default().material(&maps, TextureIds::default());
        assert_eq!(m.metallic, 1.0);
        assert_eq!(m.emissive, [1.0; 3]);
    }

    #[test]
    fn parses_scalars_and_textures() {
        let s = Source::parse(
            r#"
            base_color = [0.5, 0.25, 1.0, 1.0]
            roughness = 0.9
            normal_y = "dx"
            height_mode = "displacement"
            alpha_mode = "mask"
            double_sided = true
            [textures]
            occlusion = "../shared/ao.png"
            "#,
        )
        .unwrap();
        assert_eq!(s.normal_y, NormalY::Dx);
        assert_eq!(s.textures.get(Slot::Occlusion), Some("../shared/ao.png"));
        assert_eq!(s.textures.get(Slot::Normal), None);
        let m = s.material(&[], TextureIds::default());
        assert_eq!(m.base_color, [0.5, 0.25, 1.0, 1.0]);
        assert_eq!(m.roughness, 0.9);
        assert!(m.double_sided);
        assert_eq!(m.alpha_mode, AlphaMode::Mask);
        assert_eq!(m.height_mode, HeightMode::Displacement);
    }

    #[test]
    fn rejects_bad_sources() {
        for bad in [
            "roughnes = 1.0",
            "roughness = 1.5",
            "metallic = -0.1",
            "base_color = [1.0, 1.0, 1.0]",
            "emissive_strength = -1.0",
            "normal_scale = nan",
            "alpha_mode = \"cutout\"",
            "[textures]\nalbedo = \"a.png\"",
        ] {
            assert!(Source::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn encode_layout_and_round_trip() {
        let mut m = Source::default().material(
            &[],
            TextureIds {
                base_color: 1,
                normal: 2,
                orm: 3,
                emissive: 4,
                height: 5,
            },
        );
        m.alpha_mode = AlphaMode::Blend;
        m.double_sided = true;
        m.height_mode = HeightMode::Displacement;
        let bytes = encode(&m);
        assert_eq!(bytes.len(), SIZE);
        assert_eq!(&bytes[..8], b"ACMT\x01\0\0\0");
        assert_eq!(bytes[68], 2 | 4 | 8);
        assert_eq!(bytes[88], 3);
        assert_eq!(decode(&bytes).unwrap(), m);

        let mut bad = bytes.clone();
        bad[68] = 3;
        assert!(decode(&bad).is_err());
        assert!(decode(&bytes[..100]).is_err());
    }
}
