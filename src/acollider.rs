//! A model's collider: its `.acollider` sidecar (docs/spec/acollider.md) and its
//! `.acollider.bin` (docs/spec/acollider.bin.md); see docs/features/colliders.md.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bevy::math::Vec3;

use crate::chunks::{Bytes, Writer};
use crate::decompose::{Collider, Settings};

pub const EXTENSION: &str = "acollider";
pub const FORMAT: &str = "ACOLL";
pub const VERSION: u32 = 1;

/// The sidecar of `model`: its full file name plus `.acollider`.
pub fn path_for(model: &Path) -> PathBuf {
    let mut name = model.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(EXTENSION);
    model.with_file_name(name)
}

pub fn is_acollider(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXTENSION)
}

/// One setting: its key, its label in the Collider tab, its range and its hint.
pub struct Key {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub hint: &'static str,
}

/// The settings, in the sidecar's and the tab's order; see docs/features/colliders.md#settings.
pub const KEYS: [Key; 4] = [
    Key {
        key: "max_hulls",
        label: "Max hulls",
        min: 1.0,
        max: 256.0,
        hint: "Most hulls in the collider. Fewer is faster to simulate; more follows the model closer.",
    },
    Key {
        key: "max_vertices",
        label: "Max vertices",
        min: 4.0,
        max: 255.0,
        hint: "Most corner points of one hull (most engines want 255 or fewer; 32–64 is common).",
    },
    Key {
        key: "error",
        label: "Error",
        min: 0.001,
        max: 0.5,
        hint: "How much empty space the hulls may hold in all, as a share of the model's volume (0.01 = 1%). Lower splits the model into more hulls.",
    },
    Key {
        key: "resolution",
        label: "Resolution",
        min: 16.0,
        max: 256.0,
        hint: "Voxels along the model's longest side. Higher sees thinner gaps and smaller details, and takes longer.",
    },
];

/// The value of setting `i` (of [`KEYS`]) in `settings`.
pub fn value(settings: &Settings, i: usize) -> f64 {
    match i {
        0 => settings.max_hulls as f64,
        1 => settings.max_vertices as f64,
        2 => settings.error as f64,
        _ => settings.resolution as f64,
    }
}

/// The settings of a sidecar's text.
pub fn parse(text: &str) -> Result<Settings, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    let mut settings = Settings::default();
    for (key, item) in &table {
        let Some(k) = KEYS.iter().position(|k| k.key == key) else {
            return Err(format!("unknown key `{key}`"));
        };
        let spec = &KEYS[k];
        let v = match (k, item) {
            (2, toml::Value::Float(f)) => *f,
            (_, toml::Value::Integer(i)) => *i as f64,
            (2, _) => return Err(format!("`{key}` must be a number")),
            _ => return Err(format!("`{key}` must be an integer")),
        };
        if !(spec.min..=spec.max).contains(&v) {
            return Err(format!(
                "`{key}` must be {} to {}, not {v}",
                spec.min, spec.max
            ));
        }
        match k {
            0 => settings.max_hulls = v as u32,
            1 => settings.max_vertices = v as u32,
            2 => settings.error = v as f32,
            _ => settings.resolution = v as u32,
        }
    }
    Ok(settings)
}

/// The sidecar of `model`, `None` when it has none; errors name the sidecar.
pub fn read(model: &Path) -> Result<Option<Settings>, String> {
    let path = path_for(model);
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match fs::read_to_string(&path) {
        Ok(text) => parse(&text).map(Some).map_err(|e| format!("{name}: {e}")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// The keys a sidecar's text sets.
pub fn keys_set(text: &str) -> Vec<&'static str> {
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return Vec::new();
    };
    KEYS.iter()
        .map(|k| k.key)
        .filter(|k| doc.contains_key(k))
        .collect()
}

/// `text` with setting `i` set to `value`, the rest kept; see docs/spec/acollider.md#written-by-assetc.
pub fn with_value(text: &str, i: usize, value: f64) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.message().to_owned())?;
    let key = KEYS[i].key;
    let item = if i == 2 {
        toml_edit::value(value)
    } else {
        toml_edit::value(value.round() as i64)
    };
    match doc.get_mut(key) {
        Some(existing) if existing.is_value() => {
            // Keeps the comment after the value.
            let decor = existing.as_value().map(|v| v.decor().clone());
            *existing = item;
            if let (Some(decor), Some(v)) = (decor, existing.as_value_mut()) {
                *v.decor_mut() = decor;
            }
        }
        _ => doc[key] = item,
    }
    Ok(ends_with_newline(doc.to_string()))
}

/// `text` without setting `i`.
pub fn without_value(text: &str, i: usize) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.message().to_owned())?;
    let key = KEYS[i].key;
    let table = doc.as_table_mut();
    // Comments above the key stay, above the next key or at the end.
    let prefix = table
        .key(key)
        .and_then(|k| k.leaf_decor().prefix())
        .and_then(|p| p.as_str())
        .filter(|p| p.contains('#'))
        .map(str::to_owned);
    let next = table
        .iter()
        .map(|(k, _)| k.to_owned())
        .skip_while(|k| k != key)
        .nth(1);
    table.remove(key);
    if let Some(prefix) = prefix {
        match next.and_then(|n| table.key_mut(&n)) {
            Some(mut next) => {
                let old = next
                    .leaf_decor()
                    .prefix()
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_owned();
                next.leaf_decor_mut().set_prefix(format!("{prefix}{old}"));
            }
            None => {
                let trailing = doc.trailing().as_str().unwrap_or("").to_owned();
                doc.set_trailing(format!("{prefix}{trailing}"));
            }
        }
    }
    Ok(ends_with_newline(doc.to_string()))
}

fn ends_with_newline(mut text: String) -> String {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Makes the collider of the model in `file`; see docs/features/colliders.md#how-it-is-made.
pub fn make(file: &Path, settings: &Settings) -> Result<Collider, String> {
    let (positions, triangles) = crate::model::triangles(file)?;
    crate::decompose::decompose(&positions, &triangles, settings)
}

/// The `.acollider.bin` of `collider`, for the mesh with asset ID `mesh`.
pub fn encode(collider: &Collider, mesh: u64) -> Vec<u8> {
    let [min, max] = collider.bounds();
    let mut desc = Bytes(Vec::new());
    desc.u32(collider.hulls.len() as u32)
        .u32(collider.points() as u32)
        .u32(collider.faces() as u32)
        .u32(0)
        .f32s(&min.to_array())
        .f32s(&max.to_array())
        .f32s(&[collider.volume(), collider.model_volume])
        .u64(mesh)
        .u64(0);
    let (mut hulls, mut points, mut faces) = (Bytes(Vec::new()), Vec::new(), Vec::new());
    let (mut first_point, mut first_face) = (0u32, 0u32);
    for h in &collider.hulls {
        hulls
            .u32(first_point)
            .u32(h.hull.points.len() as u32)
            .u32(first_face)
            .u32(h.hull.faces.len() as u32)
            .f32s(&h.center.to_array())
            .f32s(&[h.volume])
            .f32s(&h.min.to_array())
            .f32s(&h.max.to_array())
            .u64(0);
        for p in &h.hull.points {
            points.extend(p.to_array().iter().flat_map(|f| f.to_le_bytes()));
        }
        for f in &h.hull.faces {
            faces.extend(f.iter().flat_map(|&i| (i as u32).to_le_bytes()));
        }
        first_point += h.hull.points.len() as u32;
        first_face += h.hull.faces.len() as u32;
    }
    Writer::new(FORMAT, VERSION)
        .chunk("DESC", desc.take())
        .chunk("HULLS", hulls.take())
        .chunk("POINTS", points)
        .chunk("FACES", faces)
        .finish()
}

/// What the Inspector shows of an `.acollider.bin`.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub chunks: usize,
    pub hulls: u32,
    pub points: u32,
    pub faces: u32,
    pub bounds: [Vec3; 2],
    pub volume: f32,
    pub model_volume: f32,
    pub mesh: u64,
}

pub fn summary(bytes: &[u8]) -> Result<Summary, String> {
    let reader = crate::chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported ACOLL version {}", reader.version));
    }
    let desc = reader.require("DESC")?;
    if desc.len() < 64 {
        return Err("DESC is too short".into());
    }
    let u32_at = |at: usize| u32::from_le_bytes(desc[at..at + 4].try_into().unwrap());
    let f32_at = |at: usize| f32::from_le_bytes(desc[at..at + 4].try_into().unwrap());
    let vec_at = |at: usize| Vec3::new(f32_at(at), f32_at(at + 4), f32_at(at + 8));
    let summary = Summary {
        chunks: reader.entries().len(),
        hulls: u32_at(0),
        points: u32_at(4),
        faces: u32_at(8),
        bounds: [vec_at(16), vec_at(28)],
        volume: f32_at(40),
        model_volume: f32_at(44),
        mesh: u64::from_le_bytes(desc[48..56].try_into().unwrap()),
    };
    let sizes = [
        ("HULLS", summary.hulls as usize * 64),
        ("POINTS", summary.points as usize * 12),
        ("FACES", summary.faces as usize * 12),
    ];
    for (name, size) in sizes {
        if reader.require(name)?.len() != size {
            return Err(format!("{name} does not match DESC"));
        }
    }
    Ok(summary)
}

/// Info rows of an `.acollider.bin`; see docs/features/colliders.md#inspector-info.
pub fn rows(bytes: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let s = summary(bytes)?;
    let share = if s.model_volume > 0.0 {
        format!("{:.0}%", s.volume / s.model_volume * 100.0)
    } else {
        "—".into()
    };
    let [min, max] = s.bounds;
    Ok(vec![
        (
            "Format",
            format!("{FORMAT} v{VERSION}, {} chunks", s.chunks),
        ),
        ("Hulls", s.hulls.to_string()),
        ("Points", s.points.to_string()),
        ("Faces", s.faces.to_string()),
        (
            "Volume",
            format!("{:.3} ({share} of the model's)", s.volume),
        ),
        (
            "Bounds",
            format!(
                "{:.2}, {:.2}, {:.2} – {:.2}, {:.2}, {:.2}",
                min.x, min.y, min.z, max.x, max.y, max.z
            ),
        ),
        ("Mesh", format!("{:#018x}", s.mesh)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompose::{decompose, tests::cuboid};

    #[test]
    fn sidecar_settings() {
        assert_eq!(parse("").unwrap(), Settings::default());
        let s =
            parse("max_hulls = 8\nerror = 0.25\nresolution = 128\nmax_vertices = 64\n").unwrap();
        assert_eq!(
            s,
            Settings {
                max_hulls: 8,
                max_vertices: 64,
                error: 0.25,
                resolution: 128
            }
        );
    }

    #[test]
    fn sidecar_errors() {
        assert_eq!(parse("hulls = 3").unwrap_err(), "unknown key `hulls`");
        assert_eq!(
            parse("max_hulls = 0").unwrap_err(),
            "`max_hulls` must be 1 to 256, not 0"
        );
        assert_eq!(
            parse("error = 0.9").unwrap_err(),
            "`error` must be 0.001 to 0.5, not 0.9"
        );
        assert_eq!(
            parse("resolution = 64.5").unwrap_err(),
            "`resolution` must be an integer"
        );
        assert_eq!(
            parse("error = \"1%\"").unwrap_err(),
            "`error` must be a number"
        );
        assert!(parse("[mesh]").unwrap_err().contains("unknown key `mesh`"));
    }

    #[test]
    fn editing_keeps_the_rest() {
        let text = "# tuned for the arch\nmax_hulls = 8 # enough\n";
        let edited = with_value(text, 0, 12.0).unwrap();
        assert_eq!(edited, "# tuned for the arch\nmax_hulls = 12 # enough\n");
        let edited = with_value(&edited, 2, 0.025).unwrap();
        assert_eq!(
            edited,
            "# tuned for the arch\nmax_hulls = 12 # enough\nerror = 0.025\n"
        );
        assert_eq!(keys_set(&edited), ["max_hulls", "error"]);
        assert_eq!(parse(&edited).unwrap().max_hulls, 12);
        let cleared = (0..4).fold(edited.clone(), |t, i| without_value(&t, i).unwrap());
        assert_eq!(cleared, "# tuned for the arch\n");
        assert!(keys_set(&cleared).is_empty());
        assert_eq!(parse(&cleared).unwrap(), Settings::default());
        assert_eq!(with_value("", 3, 96.0).unwrap(), "resolution = 96\n");
    }

    #[test]
    fn encodes_hulls_back_to_back() {
        let (mut p, mut t) = (Vec::new(), Vec::new());
        cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 1.0), &mut p, &mut t);
        cuboid(
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(5.0, 1.0, 1.0),
            &mut p,
            &mut t,
        );
        let collider = decompose(&p, &t, &Settings::default()).unwrap();
        let bytes = encode(&collider, 0x1234);
        let s = summary(&bytes).unwrap();
        assert_eq!((s.chunks, s.hulls, s.points, s.faces), (4, 2, 16, 24));
        assert_eq!(s.bounds, [Vec3::ZERO, Vec3::new(5.0, 1.0, 1.0)]);
        assert!((s.volume - 3.0).abs() < 1e-4);
        assert_eq!(s.mesh, 0x1234);
        let reader = crate::chunks::Reader::parse(&bytes, FORMAT).unwrap();
        let hulls = reader.chunk("HULLS").unwrap();
        // The larger hull first; the second starts after it.
        let at = |i: usize, o: usize| {
            u32::from_le_bytes(hulls[i * 64 + o..i * 64 + o + 4].try_into().unwrap())
        };
        assert_eq!((at(0, 0), at(0, 4), at(0, 8), at(0, 12)), (0, 8, 0, 12));
        assert_eq!((at(1, 0), at(1, 4), at(1, 8), at(1, 12)), (8, 8, 12, 12));
        let volume = f32::from_le_bytes(hulls[28..32].try_into().unwrap());
        assert!((volume - 2.0).abs() < 1e-4);
        assert!(summary(&bytes[..100]).is_err());
        let rows = rows(&bytes).unwrap();
        assert_eq!(rows[0], ("Format", "ACOLL v1, 4 chunks".into()));
        assert_eq!(rows[1], ("Hulls", "2".into()));
        assert_eq!(rows[6], ("Mesh", "0x0000000000001234".into()));
    }
}
