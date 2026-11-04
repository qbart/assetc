//! `.pak` packs: a build's output in one file; see docs/features/pack.md and
//! docs/spec/pak.md.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use globset::GlobSet;

use crate::config;
use crate::hash::fnv1a64;
use crate::manifest::{self, Kind};

pub const MAGIC: [u8; 4] = *b"APAK";
pub const VERSION: u32 = 1;
pub const HEADER_SIZE: u64 = 32;
/// Bytes of a contents record before its path.
const RECORD_SIZE: u64 = 28;
pub const KIND_MANIFEST: u8 = 10;
pub const KIND_OTHER: u8 = 255;
/// Entry kinds in the order stats list them.
pub const KINDS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, KIND_MANIFEST, KIND_OTHER];

/// One packed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub offset: u64,
    pub size: u64,
    pub id: u64,
    pub kind: u8,
    /// Relative to the output directory, `/`-separated.
    pub path: String,
}

/// A pack's header and contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contents {
    pub alignment: u32,
    pub toc_bytes: u64,
    pub entries: Vec<Entry>,
    /// The whole pack's size.
    pub bytes: u64,
}

impl Contents {
    /// Bytes of the files themselves.
    pub fn data(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// Bytes of alignment padding.
    pub fn padding(&self) -> u64 {
        self.bytes
            .saturating_sub(HEADER_SIZE + self.toc_bytes + self.data())
    }

    /// Entries and bytes of each kind that has any, in [`KINDS`] order.
    pub fn by_kind(&self) -> Vec<(u8, usize, u64)> {
        KINDS
            .into_iter()
            .filter_map(|kind| {
                let of: Vec<&Entry> = self.entries.iter().filter(|e| e.kind == kind).collect();
                (!of.is_empty()).then(|| (kind, of.len(), of.iter().map(|e| e.size).sum()))
            })
            .collect()
    }
}

/// The kind of an output-relative path; see docs/spec/pak.md#contents.
pub fn kind_of(path: &str) -> u8 {
    if path == manifest::FILE_NAME {
        return KIND_MANIFEST;
    }
    Kind::of(Path::new(path)).map_or(KIND_OTHER, |kind| kind as u8)
}

/// Singular and plural names of an entry kind.
pub fn kind_names(kind: u8) -> (&'static str, &'static str) {
    match kind {
        0 => ("texture", "textures"),
        1 => ("material", "materials"),
        2 => ("mesh", "meshes"),
        3 => ("animation", "animations"),
        4 => ("font", "fonts"),
        5 => ("schema", "schemas"),
        6 => ("data file", "data files"),
        7 => ("graph", "graphs"),
        8 => ("tree", "trees"),
        9 => ("atlas", "atlases"),
        KIND_MANIFEST => ("manifest", "manifest"),
        _ => ("other", "other"),
    }
}

/// Singular and plural names of an entry kind, capitalized, as the Inspector shows them.
pub fn kind_title(kind: u8) -> (&'static str, &'static str) {
    match kind {
        0 => ("Texture", "Textures"),
        1 => ("Material", "Materials"),
        2 => ("Mesh", "Meshes"),
        3 => ("Animation", "Animations"),
        4 => ("Font", "Fonts"),
        5 => ("Schema", "Schemas"),
        6 => ("Data", "Data files"),
        7 => ("Graph", "Graphs"),
        8 => ("Tree", "Trees"),
        9 => ("Atlas", "Atlases"),
        KIND_MANIFEST => ("Manifest", "Manifest"),
        _ => ("Other", "Other"),
    }
}

/// Whether `file` is a pack (`*.pak`, any case).
pub fn is_pack(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pak"))
}

/// `[pack]` checked; see docs/spec/assetc.toml.md#pack.
pub struct Settings {
    pub file: String,
    pub alignment: u32,
    include: GlobSet,
    exclude: GlobSet,
}

impl Settings {
    pub fn new(pack: &config::Pack) -> Result<Self, String> {
        let fail = |e: String| format!("{} [pack]: {e}", config::FILE_NAME);
        let file = pack.file.replace('\\', "/");
        if !file.to_ascii_lowercase().ends_with(".pak")
            || file.starts_with('/')
            || file.split('/').any(|part| part.is_empty() || part == "..")
        {
            return Err(fail(format!(
                "file {:?} must end in .pak and stay inside output",
                pack.file
            )));
        }
        if !pack.alignment.is_power_of_two() || !(16..=65536).contains(&pack.alignment) {
            return Err(fail(format!(
                "alignment {} is not a power of two from 16 to 65536",
                pack.alignment
            )));
        }
        let globs = |patterns: &[String]| {
            crate::texture::glob_set(patterns).map_err(|e| fail(e.to_string()))
        };
        Ok(Self {
            file,
            alignment: pack.alignment,
            include: globs(&pack.include)?,
            exclude: globs(&pack.exclude)?,
        })
    }

    /// Whether the output file at `rel` is packed.
    fn packs(&self, rel: &str) -> bool {
        self.include.is_match(rel) && !self.exclude.is_match(rel) && !is_pack(Path::new(rel))
    }
}

/// The output files a pack holds, `output`-relative and sorted by path.
pub fn files(output: &Path, settings: &Settings) -> Vec<String> {
    let mut files: Vec<String> = crate::build::files_under(output)
        .iter()
        .filter_map(|file| crate::build::relative(output, file).ok())
        .filter(|rel| settings.packs(rel))
        .collect();
    files.sort();
    files
}

fn align(at: u64, alignment: u32) -> u64 {
    at.next_multiple_of(alignment as u64)
}

/// The asset ID of each file the manifest in `output` lists, by output path.
fn manifest_ids(output: &Path) -> HashMap<String, u64> {
    fs::read(output.join(manifest::FILE_NAME))
        .ok()
        .and_then(|bytes| manifest::decode(&bytes).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.path, e.id))
        .collect()
}

/// Writes the pack of `output`; see docs/features/pack.md#output.
pub fn write(output: &Path, settings: &Settings) -> Result<(PathBuf, Contents), String> {
    let files = files(output, settings);
    let ids = manifest_ids(output);
    let mut entries = Vec::with_capacity(files.len());
    let mut toc_bytes = 0;
    for path in files {
        let size = fs::metadata(output.join(&path))
            .map_err(|e| format!("{path}: {e}"))?
            .len();
        if path.len() > u16::MAX as usize {
            return Err(format!("{path}: path too long"));
        }
        toc_bytes += RECORD_SIZE + path.len() as u64;
        entries.push(Entry {
            offset: 0,
            size,
            id: ids
                .get(&path)
                .copied()
                .unwrap_or_else(|| fnv1a64(path.as_bytes())),
            kind: kind_of(&path),
            path,
        });
    }
    let mut at = HEADER_SIZE + toc_bytes;
    for entry in &mut entries {
        entry.offset = align(at, settings.alignment);
        at = entry.offset + entry.size;
    }
    let contents = Contents {
        alignment: settings.alignment,
        toc_bytes,
        bytes: at,
        entries,
    };

    let target = output.join(&settings.file);
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let partial = target.with_extension("pak.partial");
    let result = write_to(output, &partial, &contents);
    if let Err(e) = result {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    fs::rename(&partial, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok((target, contents))
}

fn write_to(output: &Path, file: &Path, contents: &Contents) -> Result<(), String> {
    let io = |e: std::io::Error| format!("{}: {e}", file.display());
    let mut out = std::io::BufWriter::new(fs::File::create(file).map_err(io)?);
    out.write_all(&encode_head(contents)).map_err(io)?;
    let mut at = HEADER_SIZE + contents.toc_bytes;
    for entry in &contents.entries {
        out.write_all(&vec![0; (entry.offset - at) as usize])
            .map_err(io)?;
        let bytes =
            fs::read(output.join(&entry.path)).map_err(|e| format!("{}: {e}", entry.path))?;
        if bytes.len() as u64 != entry.size {
            return Err(format!("{}: changed while packing", entry.path));
        }
        out.write_all(&bytes).map_err(io)?;
        at = entry.offset + entry.size;
    }
    out.flush().map_err(io)
}

/// The header and contents table.
pub fn encode_head(contents: &Contents) -> Vec<u8> {
    let mut out = Vec::with_capacity((HEADER_SIZE + contents.toc_bytes) as usize);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(contents.entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&contents.alignment.to_le_bytes());
    out.extend_from_slice(&contents.toc_bytes.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    for entry in &contents.entries {
        out.extend_from_slice(&entry.offset.to_le_bytes());
        out.extend_from_slice(&entry.size.to_le_bytes());
        out.extend_from_slice(&entry.id.to_le_bytes());
        out.push(entry.kind);
        out.push(0);
        out.extend_from_slice(&(entry.path.len() as u16).to_le_bytes());
        out.extend_from_slice(entry.path.as_bytes());
    }
    out
}

/// Reads a pack's header and contents, not its data; see docs/spec/pak.md#reading.
pub fn read(file: &Path) -> Result<Contents, String> {
    let mut f = fs::File::open(file).map_err(|e| e.to_string())?;
    let bytes = f.metadata().map_err(|e| e.to_string())?.len();
    let mut header = [0; HEADER_SIZE as usize];
    f.read_exact(&mut header)
        .map_err(|_| "too short for a pack header".to_owned())?;
    if header[..4] != MAGIC {
        return Err("not a pack (no APAK magic)".into());
    }
    let u32_at = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let u64_at = |b: &[u8], at: usize| u64::from_le_bytes(b[at..at + 8].try_into().unwrap());
    let version = u32_at(&header, 4);
    if version != VERSION {
        return Err(format!("unknown pack version {version}"));
    }
    let count = u32_at(&header, 8) as usize;
    let alignment = u32_at(&header, 12);
    let toc_bytes = u64_at(&header, 16);
    if HEADER_SIZE + toc_bytes > bytes {
        return Err("contents run past the end of the file".into());
    }
    let mut toc = vec![0; toc_bytes as usize];
    f.read_exact(&mut toc).map_err(|e| e.to_string())?;
    let mut entries = Vec::with_capacity(count.min(toc.len() / RECORD_SIZE as usize));
    let mut at = 0;
    for i in 0..count {
        let record = toc
            .get(at..at + RECORD_SIZE as usize)
            .ok_or_else(|| format!("entry {i}: contents too short"))?;
        let len = u16::from_le_bytes([record[26], record[27]]) as usize;
        let start = at + RECORD_SIZE as usize;
        let path = toc
            .get(start..start + len)
            .ok_or_else(|| format!("entry {i}: contents too short"))?;
        let entry = Entry {
            offset: u64_at(record, 0),
            size: u64_at(record, 8),
            id: u64_at(record, 16),
            kind: record[24],
            path: String::from_utf8_lossy(path).into_owned(),
        };
        if entry
            .offset
            .checked_add(entry.size)
            .is_none_or(|end| end > bytes)
        {
            return Err(format!("{}: runs past the end of the file", entry.path));
        }
        entries.push(entry);
        at = start + len;
    }
    Ok(Contents {
        alignment,
        toc_bytes,
        entries,
        bytes,
    })
}

/// `Packed <n> files (<size>) into <path>`.
pub fn summary(contents: &Contents, file: &str) -> String {
    let n = contents.entries.len();
    format!(
        "Packed {n} {} ({}) into {file}",
        if n == 1 { "file" } else { "files" },
        crate::inspect::format_size(contents.bytes)
    )
}

/// The layout line and a line per kind, as `assetc pack` prints them.
pub fn stats(contents: &Contents) -> Vec<String> {
    let size = crate::inspect::format_size;
    let mut lines = vec![format!(
        "  layout     {} header + {} contents + {} data ({} alignment)",
        size(HEADER_SIZE),
        size(contents.toc_bytes),
        size(contents.data()),
        size(contents.padding())
    )];
    for (kind, count, bytes) in contents.by_kind() {
        let name = kind_names(kind).1;
        lines.push(format!("  {name:<12} {count:>4}   {}", size(bytes)));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(pack: config::Pack) -> Settings {
        Settings::new(&pack).unwrap()
    }

    #[test]
    fn packs_round_trip_aligned_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path();
        fs::create_dir_all(out.join("models")).unwrap();
        fs::write(out.join("b.ktx2"), b"texture").unwrap();
        fs::write(out.join("models/Fox.amesh.bin"), b"mesh bytes").unwrap();
        fs::write(out.join("notes.txt"), b"").unwrap();
        fs::write(out.join("old.pak"), b"skip me").unwrap();
        fs::write(out.join(".hidden"), b"skip me").unwrap();
        let entries = vec![manifest::Entry {
            id: 42,
            kind: Kind::Texture,
            path: "b.ktx2".into(),
        }];
        fs::write(out.join(manifest::FILE_NAME), manifest::encode(&entries)).unwrap();

        let (file, contents) = write(out, &settings(config::Pack::default())).unwrap();
        assert_eq!(file, out.join("assetc.pak"));
        let paths: Vec<&str> = contents.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "assetc.aman.bin",
                "b.ktx2",
                "models/Fox.amesh.bin",
                "notes.txt"
            ]
        );
        let kinds: Vec<u8> = contents.entries.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [KIND_MANIFEST, 0, 2, KIND_OTHER]);
        assert_eq!(contents.entries[1].id, 42);
        assert_eq!(contents.entries[2].id, fnv1a64(b"models/Fox.amesh.bin"));
        assert!(contents.entries.iter().all(|e| e.offset % 16 == 0));

        let bytes = fs::read(&file).unwrap();
        assert_eq!(bytes.len() as u64, contents.bytes);
        let mesh = &contents.entries[2];
        assert_eq!(
            &bytes[mesh.offset as usize..(mesh.offset + mesh.size) as usize],
            b"mesh bytes"
        );
        assert_eq!(read(&file).unwrap(), contents);
        // Deterministic, and the pack does not pack itself.
        let (_, again) = write(out, &settings(config::Pack::default())).unwrap();
        assert_eq!(again, contents);
        assert_eq!(fs::read(&file).unwrap(), bytes);
    }

    #[test]
    fn include_exclude_and_alignment() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path();
        fs::write(out.join("a.ktx2"), b"a").unwrap();
        fs::write(out.join("a.orm.ktx2"), b"orm").unwrap();
        fs::write(out.join("b.amat.bin"), b"m").unwrap();
        let pack = config::Pack {
            file: "packs/game.pak".into(),
            alignment: 4096,
            include: vec!["**/*.ktx2".into()],
            exclude: vec!["**/*.orm.ktx2".into()],
        };
        let (file, contents) = write(out, &settings(pack)).unwrap();
        assert_eq!(file, out.join("packs/game.pak"));
        assert_eq!(contents.entries.len(), 1);
        assert_eq!(contents.entries[0].offset, 4096);
        assert_eq!(contents.bytes, 4097);
        assert_eq!(contents.padding(), 4096 - 32 - contents.toc_bytes);
    }

    #[test]
    fn bad_settings_are_errors() {
        let bad = |pack: config::Pack| Settings::new(&pack).err().unwrap();
        assert!(
            bad(config::Pack {
                alignment: 24,
                ..Default::default()
            })
            .contains("alignment 24")
        );
        assert!(
            bad(config::Pack {
                file: "../x.pak".into(),
                ..Default::default()
            })
            .contains("stay inside output")
        );
        assert!(
            bad(config::Pack {
                file: "x.zip".into(),
                ..Default::default()
            })
            .contains("end in .pak")
        );
    }

    #[test]
    fn truncated_packs_do_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.pak");
        fs::write(&file, b"APAK").unwrap();
        assert!(read(&file).is_err());
        let contents = Contents {
            alignment: 16,
            toc_bytes: RECORD_SIZE + 1,
            entries: vec![Entry {
                offset: 64,
                size: 100,
                id: 1,
                kind: 0,
                path: "a".into(),
            }],
            bytes: 0,
        };
        fs::write(&file, encode_head(&contents)).unwrap();
        assert!(read(&file).unwrap_err().contains("past the end"));
    }
}

/// The SDK reads everything the build and pack write; see docs/features/sdk.md#conformance.
#[cfg(test)]
mod sdk_conformance {
    use assetc_sdk::{self as sdk, Kind};

    fn decode(assets: &sdk::Assets, entry: &sdk::Entry) -> Result<(), sdk::Error> {
        match entry.kind {
            Kind::Texture => assets.load::<sdk::Texture>(entry.id).map(drop),
            Kind::Material => assets.load::<sdk::Material>(entry.id).map(drop),
            Kind::Mesh => assets.load::<sdk::Mesh>(entry.id).map(drop),
            Kind::Animation => assets.load::<sdk::Animation>(entry.id).map(drop),
            Kind::Font => assets.load::<sdk::Font>(entry.id).map(drop),
            Kind::Schema => assets.load::<sdk::Schema>(entry.id).map(drop),
            Kind::Data => assets.load::<sdk::Data>(entry.id).map(drop),
            Kind::Graph => assets.load::<sdk::Graph>(entry.id).map(drop),
            Kind::Tree => assets.load::<sdk::Tree>(entry.id).map(drop),
            Kind::Atlas => assets.load::<sdk::Atlas>(entry.id).map(drop),
            Kind::Manifest => assets.load::<sdk::Manifest>(entry.id).map(drop),
            Kind::Other => Ok(()),
        }
    }

    #[test]
    fn sdk_reads_every_sample() {
        let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("build");
        let config = crate::config::Config::default();
        let outcome = crate::build::run(&samples, &output, &config).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let settings = super::Settings::new(&config.pack).unwrap();
        let (pak, contents) = super::write(&output, &settings).unwrap();

        let packed = sdk::Assets::open(&pak).unwrap();
        assert_eq!(packed.entries().len(), contents.entries.len());
        let loose = sdk::Assets::open(&output).unwrap();
        for assets in [&packed, &loose] {
            for entry in assets.entries() {
                decode(assets, entry).unwrap_or_else(|e| panic!("{}: {e}", entry.path));
            }
        }
        // Every kind the samples have is read, not skipped.
        for kind in [
            Kind::Texture,
            Kind::Material,
            Kind::Mesh,
            Kind::Animation,
            Kind::Font,
            Kind::Schema,
            Kind::Data,
            Kind::Graph,
            Kind::Tree,
            Kind::Atlas,
        ] {
            assert!(packed.of_kind(kind).next().is_some(), "{kind:?}");
        }
    }
}
