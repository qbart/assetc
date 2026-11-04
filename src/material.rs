//! PBR materials: `.amat` sources and `.amat.bin` files, see docs/features/materials.md,
//! docs/spec/amat.md and docs/spec/amat.bin.md.

use serde::Deserialize;

use crate::texture::ColorSpace;

/// Suffix of material source files: `<name>.amat`.
pub const SOURCE_SUFFIX: &str = ".amat";
/// Suffix materials used to have, rejected with a hint; see
/// docs/features/materials.md#old-mattoml-files.
pub const OLD_SUFFIX: &str = ".mat.toml";
pub const SIZE: usize = 112;
const MAGIC: [u8; 4] = *b"ACMT";
/// 2: struct data may follow the record; see docs/spec/amat.bin.md#struct-data.
const VERSION: u32 = 2;
/// The format of the struct data after the record.
pub const ASTRUCT: &str = "ASTRUCT";

/// Whether `path` is a material source, `<name>.amat`.
pub fn is_source(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(SOURCE_SUFFIX))
        .is_some_and(|name| !name.is_empty())
}

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

    /// Name shown in the Material tab.
    pub fn label(self) -> &'static str {
        match self {
            Self::BaseColor => "Base color",
            Self::Normal => "Normal",
            Self::Roughness => "Roughness",
            Self::Metallic => "Metallic",
            Self::Occlusion => "Occlusion",
            Self::Emissive => "Emissive",
            Self::Height => "Height",
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

impl NormalY {
    pub fn key(self) -> &'static str {
        match self {
            Self::Gl => "gl",
            Self::Dx => "dx",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeightMode {
    #[default]
    Parallax,
    Displacement,
}

impl HeightMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Parallax => "parallax",
            Self::Displacement => "displacement",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

impl AlphaMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Mask => "mask",
            Self::Blend => "blend",
        }
    }
}

/// A parsed `<name>.amat`; see docs/spec/amat.md.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Source {
    pub base_color: Option<[f32; 4]>,
    pub metallic: Option<f32>,
    pub roughness: Option<f32>,
    pub occlusion_strength: Option<f32>,
    pub normal_scale: Option<f32>,
    pub normal_y: Option<NormalY>,
    pub emissive: Option<[f32; 3]>,
    pub emissive_strength: Option<f32>,
    pub height_mode: Option<HeightMode>,
    pub height_scale: Option<f32>,
    pub height_midlevel: Option<f32>,
    pub alpha_mode: Option<AlphaMode>,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: Option<bool>,
    pub textures: SourceTextures,
    /// Attached structs and their values; see docs/features/structs.md.
    #[serde(skip)]
    pub structs: Vec<crate::structs::Attachment>,
    #[serde(skip)]
    pub values: crate::structs::Values,
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
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

    pub fn set(&mut self, slot: Slot, path: Option<String>) {
        *match slot {
            Slot::BaseColor => &mut self.base_color,
            Slot::Normal => &mut self.normal,
            Slot::Roughness => &mut self.roughness,
            Slot::Metallic => &mut self.metallic,
            Slot::Occlusion => &mut self.occlusion,
            Slot::Emissive => &mut self.emissive,
            Slot::Height => &mut self.height,
        } = path;
    }
}

impl Source {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let structs = crate::structs::take_toml_attachments(&mut table, "structs")?;
        let values = crate::structs::take_toml_values(&mut table)?;
        let source: Self = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        source.validate()?;
        Ok(Self {
            structs,
            values,
            ..source
        })
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
            alpha_mode: self.alpha_mode.unwrap_or_default(),
            double_sided: self.double_sided.unwrap_or_default(),
            height_mode: self.height_mode.unwrap_or_default(),
            textures,
        }
    }

    /// The file's text: the keys that are set, in spec order; see
    /// docs/spec/amat.md#written-by-assetc.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        let mut line = |key: &str, value: String| out += &format!("{key} = {value}\n");
        let list = |values: &[f32]| {
            let values: Vec<String> = values.iter().map(|v| float(*v)).collect();
            format!("[{}]", values.join(", "))
        };
        let quoted = |s: &str| format!("{s:?}");
        if let Some(v) = self.base_color {
            line("base_color", list(&v));
        }
        for (key, v) in [("metallic", self.metallic), ("roughness", self.roughness)] {
            if let Some(v) = v {
                line(key, float(v));
            }
        }
        for (key, v) in [
            ("occlusion_strength", self.occlusion_strength),
            ("normal_scale", self.normal_scale),
        ] {
            if let Some(v) = v {
                line(key, float(v));
            }
        }
        if let Some(v) = self.normal_y {
            line("normal_y", quoted(v.key()));
        }
        if let Some(v) = self.emissive {
            line("emissive", list(&v));
        }
        if let Some(v) = self.emissive_strength {
            line("emissive_strength", float(v));
        }
        if let Some(v) = self.height_mode {
            line("height_mode", quoted(v.key()));
        }
        for (key, v) in [
            ("height_scale", self.height_scale),
            ("height_midlevel", self.height_midlevel),
        ] {
            if let Some(v) = v {
                line(key, float(v));
            }
        }
        if let Some(v) = self.alpha_mode {
            line("alpha_mode", quoted(v.key()));
        }
        if let Some(v) = self.alpha_cutoff {
            line("alpha_cutoff", float(v));
        }
        if let Some(v) = self.double_sided {
            line("double_sided", v.to_string());
        }
        let textures: Vec<_> = Slot::ALL
            .into_iter()
            .filter_map(|slot| Some((slot, self.textures.get(slot)?)))
            .collect();
        if !textures.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out += "[textures]\n";
            for (slot, path) in textures {
                out += &format!("{} = {}\n", slot.name(), quoted(path));
            }
        }
        crate::structs::write_toml_attachments(&mut out, "structs", &self.structs);
        crate::structs::write_toml_values(&mut out, "values", &self.values);
        out.trim_start_matches('\n').to_owned()
    }
}

/// `v` rounded to 4 decimals, always with a decimal point (`1.0`, `0.25`).
fn float(v: f32) -> String {
    let rounded = (f64::from(v) * 1e4).round() / 1e4;
    let text = format!("{rounded}");
    if text.contains(['.', 'e', 'N', 'i']) {
        text
    } else {
        format!("{text}.0")
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

/// The contents of a `.amat.bin` file.
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

/// The `ASTRUCT` file after a material's record, if there is one; see
/// docs/spec/amat.bin.md#struct-data.
pub fn struct_data(bytes: &[u8]) -> Option<&[u8]> {
    bytes.get(SIZE..).filter(|rest| !rest.is_empty())
}

/// The record of an `.amat.bin`, its struct data checked to be an `ASTRUCT` file.
pub fn decode(bytes: &[u8]) -> Result<Material, String> {
    if bytes.len() < SIZE {
        return Err(format!(
            "invalid material: size {} is below {SIZE}",
            bytes.len()
        ));
    }
    if bytes[..4] != MAGIC {
        return Err("invalid material: bad magic".into());
    }
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let f32_at = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    match (u32_at(4), struct_data(bytes)) {
        (1, None) | (VERSION, _) => {}
        (1, Some(_)) => {
            return Err("invalid material: version 1 with bytes after the record".into());
        }
        _ => return Err("invalid material: unsupported version".into()),
    }
    if let Some(rest) = struct_data(bytes) {
        crate::chunks::Reader::parse(rest, ASTRUCT)
            .map_err(|e| format!("invalid material: struct data: {e}"))?;
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
    fn structs_round_trip() {
        let text = "roughness = 0.5\n\n[[structs]]\nschema = \"game.aschema\"\ntype = \"Surface\"\n\n[values.Surface]\nsound = \"stone\"\n";
        let source = Source::parse(text).unwrap();
        assert_eq!(source.structs[0].ty, "Surface");
        assert_eq!(source.values["Surface"]["sound"], "stone");
        assert_eq!(source.to_toml(), text);
        let only = "[[structs]]\nschema = \"game.aschema\"\ntype = \"Surface\"\n";
        assert_eq!(Source::parse(only).unwrap().to_toml(), only);
    }

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
        assert_eq!(s.normal_y, Some(NormalY::Dx));
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
    fn writes_set_keys_in_spec_order() {
        assert_eq!(Source::default().to_toml(), "");
        let text = "base_color = [0.8, 0.8, 0.8, 1.0]\nroughness = 0.5\nnormal_y = \"dx\"\n\
                    alpha_mode = \"mask\"\ndouble_sided = true\n\n[textures]\n\
                    normal = \"../n.png\"\nheight = \"h.png\"\n";
        let mut s = Source::parse(text).unwrap();
        assert_eq!(s.to_toml(), text);
        s.metallic = Some(0.123_456);
        s.textures.set(Slot::Normal, None);
        let written = s.to_toml();
        assert!(
            written.contains("metallic = 0.1235\nroughness"),
            "{written}"
        );
        assert_eq!(
            Source::parse(&written).unwrap().textures.get(Slot::Normal),
            None
        );
        assert_eq!(float(1.0), "1.0");
        assert_eq!(float(-2.5), "-2.5");
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
        assert_eq!(&bytes[..8], b"ACMT\x02\0\0\0");
        assert_eq!(bytes[68], 2 | 4 | 8);
        assert_eq!(bytes[88], 3);
        assert_eq!(decode(&bytes).unwrap(), m);

        let mut bad = bytes.clone();
        bad[68] = 3;
        assert!(decode(&bad).is_err());
        assert!(decode(&bytes[..100]).is_err());

        // Struct data after the record must be an ASTRUCT file; version 1 has none.
        let mut with = bytes.clone();
        with.extend(crate::chunks::Writer::new(ASTRUCT, 1).finish());
        assert_eq!(decode(&with).unwrap(), m);
        assert!(struct_data(&with).is_some());
        let mut junk = bytes.clone();
        junk.extend([1, 2, 3]);
        assert!(decode(&junk).is_err());
        let mut old = with.clone();
        old[4] = 1;
        assert!(decode(&old).is_err());
        old.truncate(SIZE);
        assert_eq!(decode(&old).unwrap(), m);
    }
}
