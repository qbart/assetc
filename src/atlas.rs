//! Atlases: a `*.atlas` folder's images packed into one texture, with metadata of where
//! each frame is; see docs/features/atlases.md, docs/spec/atlas.toml.md and
//! docs/spec/aatlas.bin.md. The packing is the assetc-atlas crate
//! (docs/features/atlas-crate.md); this is the folder, `atlas.toml` and `.aatlas.bin`.

use std::path::{Path, PathBuf};

pub use assetc_atlas::{Atlas, Packing, Settings};
use assetc_atlas::{MAX_CELL, MAX_COLUMNS, MAX_PADDING, MAX_SIZE};

use crate::amesh::Strings;
use crate::chunks::{Bytes, Writer};
use crate::hash::fnv1a64;

pub const FORMAT: &str = "AATLAS";
pub const VERSION: u32 = 1;
/// The settings file inside an atlas folder.
pub const CONFIG: &str = "atlas.toml";

/// Whether `dir` is an atlas folder (`*.atlas`, any case).
pub fn is_atlas(dir: &Path) -> bool {
    dir.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("atlas"))
}

/// An atlas' settings; see docs/spec/atlas.toml.md.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AtlasConfig {
    /// How it is packed.
    pub settings: Settings,
    /// Attached structs and their values; see docs/features/structs.md.
    pub structs: Vec<crate::structs::Attachment>,
    pub values: crate::structs::Values,
}

impl AtlasConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let mut config = Self {
            structs: crate::structs::take_toml_attachments(&mut table, "structs")?,
            values: crate::structs::take_toml_values(&mut table)?,
            ..Self::default()
        };
        let s = &mut config.settings;
        if let Some(v) = table.remove("packing") {
            let key = v.as_str().ok_or("packing must be a string")?;
            s.packing = Packing::from_key(key)
                .ok_or_else(|| format!("unknown packing {key:?} (use grid or pack)"))?;
        }
        let mut int = |key: &str, max: u32, value: &mut u32| -> Result<(), String> {
            if let Some(v) = table.remove(key) {
                let n = v.as_integer().ok_or(format!("{key} must be an integer"))?;
                if !(0..=i64::from(max)).contains(&n) {
                    return Err(format!("{key} must be 0–{max}"));
                }
                *value = n as u32;
            }
            Ok(())
        };
        int("cell_width", MAX_CELL, &mut s.cell_width)?;
        int("cell_height", MAX_CELL, &mut s.cell_height)?;
        int("columns", MAX_COLUMNS, &mut s.columns)?;
        int("padding", MAX_PADDING, &mut s.padding)?;
        int("extrude", MAX_PADDING, &mut s.extrude)?;
        int("max_size", MAX_SIZE, &mut s.max_size)?;
        for (key, value) in [("trim", &mut s.trim), ("power_of_two", &mut s.power_of_two)] {
            if let Some(v) = table.remove(key) {
                *value = v.as_bool().ok_or(format!("{key} must be true or false"))?;
            }
        }
        if let Some(key) = table.keys().next() {
            return Err(format!("unknown key {key}"));
        }
        s.validate()?;
        Ok(config)
    }

    /// The written form; see docs/spec/atlas.toml.md.
    pub fn to_toml(&self) -> String {
        let s = &self.settings;
        let mut out = format!(
            "packing = \"{}\"\ncell_width = {}\ncell_height = {}\ncolumns = {}\npadding = {}\n\
             extrude = {}\ntrim = {}\npower_of_two = {}\nmax_size = {}\n",
            s.packing.key(),
            s.cell_width,
            s.cell_height,
            s.columns,
            s.padding,
            s.extrude,
            s.trim,
            s.power_of_two,
            s.max_size
        );
        crate::structs::write_toml_attachments(&mut out, "structs", &self.structs);
        crate::structs::write_toml_values(&mut out, "values", &self.values);
        out
    }

    /// The folder's `atlas.toml`, or the defaults without one.
    pub fn read(dir: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(dir.join(CONFIG)) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{CONFIG}: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{CONFIG}: {e}")),
        }
    }
}

/// The folder's frames: its direct children that are texture sources, sorted by file name.
pub fn frame_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && crate::build::is_texture_source(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}

/// Reads and packs an atlas folder.
pub fn build(dir: &Path) -> Result<(AtlasConfig, Atlas), String> {
    let config = AtlasConfig::read(dir)?;
    let images = frame_files(dir)
        .into_iter()
        .map(|path| {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let file = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            crate::imagefile::read(&path)
                .map(|(image, _)| (name, image))
                .map_err(|e| format!("{file}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if images.is_empty() {
        return Err("no frames (png, jpg, jpeg or tga images in the folder)".into());
    }
    let atlas = assetc_atlas::pack(&config.settings, images)?;
    Ok((config, atlas))
}

/// The `.aatlas.bin` metadata; see docs/spec/aatlas.bin.md.
pub fn encode(atlas: &Atlas, padding: u32, texture: u64) -> Vec<u8> {
    let (width, height) = atlas.image.dimensions();
    let mut desc = Bytes::default();
    desc.u32(width)
        .u32(height)
        .u32(atlas.frames.len() as u32)
        .u32(u32::from(atlas.packing == Packing::Pack))
        .u32(atlas.columns)
        .u32(atlas.cell.0)
        .u32(atlas.cell.1)
        .u32(padding)
        .u64(texture)
        .u64(0);
    let mut frames = Bytes::default();
    let mut strings = Strings::default();
    for f in &atlas.frames {
        let (at, len) = strings.add(&f.name);
        frames
            .u64(fnv1a64(f.name.as_bytes()))
            .u32(at)
            .u32(len)
            .u32(f.x)
            .u32(f.y)
            .u32(f.width)
            .u32(f.height)
            .u32(f.source_width)
            .u32(f.source_height)
            .u32(f.offset_x)
            .u32(f.offset_y)
            .f32s(&f.uv(width, height));
    }
    let mut writer = Writer::new(FORMAT, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("FRAMES", frames.take())
        .chunk("STRINGS", strings.0);
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imagefile::Image;

    #[test]
    fn folders_pack_in_name_order_and_encode() {
        let dir = tempfile::tempdir().unwrap();
        let atlas_dir = dir.path().join("fx.atlas");
        for (name, shade) in [("b", 20), ("a", 10), ("c", 30)] {
            crate::imagefile::tests::png(
                &atlas_dir.join(format!("{name}.png")),
                &Image::filled(8, 8, [shade, 0, 0, 255]),
            );
        }
        std::fs::write(atlas_dir.join(CONFIG), "packing = \"pack\"\nextrude = 1\n").unwrap();
        let (config, atlas) = build(&atlas_dir).unwrap();
        assert_eq!(config.settings.packing, Packing::Pack);
        let names: Vec<_> = atlas.frames.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        let bytes = encode(&atlas, config.settings.padding, 7);
        let reader = crate::chunks::Reader::parse(&bytes, FORMAT).unwrap();
        assert_eq!(reader.require("FRAMES").unwrap().len(), 3 * 64);

        let empty = dir.path().join("empty.atlas");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(build(&empty).unwrap_err().starts_with("no frames (png"));
    }

    #[test]
    fn settings_round_trip() {
        let config = AtlasConfig {
            settings: Settings {
                packing: Packing::Pack,
                padding: 4,
                extrude: 2,
                ..Settings::default()
            },
            ..AtlasConfig::default()
        };
        assert_eq!(AtlasConfig::parse(&config.to_toml()).unwrap(), config);
        assert_eq!(
            AtlasConfig::parse("extrude = 5\npadding = 1").unwrap_err(),
            "extrude must not exceed padding"
        );
        assert_eq!(
            AtlasConfig::parse("padding = 65").unwrap_err(),
            "padding must be 0–64"
        );
        assert_eq!(
            AtlasConfig::parse("max_size = 0").unwrap_err(),
            "max_size must be 1–16384"
        );
        assert!(AtlasConfig::parse("size = 1").is_err());
    }
}
