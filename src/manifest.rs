//! `assetc.aman.bin` reader/writer, see docs/spec/aman.bin.md.

use std::fmt;
use std::path::Path;

pub const FILE_NAME: &str = "assetc.aman.bin";
/// The manifest's names before, removed by builds; see docs/features/build.md.
pub const LEGACY_FILE_NAMES: [&str; 3] = ["assets.manifest", "assets.aman", "assets.aman.bin"];
/// Extensions of outputs before they ended in `.bin`, removed by builds; see
/// docs/features/build.md#removing-stale-outputs.
pub const LEGACY_EXTENSIONS: [&str; 9] = [
    "amesh", "amat", "aanim", "afont", "atype", "arec", "agph", "atre", "aatlas",
];
pub const MAGIC: [u8; 4] = *b"AMAN";
pub const VERSION: u32 = 1;
const HEADER_SIZE: usize = 16;
const ENTRY_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Texture = 0,
    Material = 1,
    Mesh = 2,
    Animation = 3,
    Font = 4,
    Schema = 5,
    Data = 6,
    Graph = 7,
    Tree = 8,
    Atlas = 9,
}

impl Kind {
    pub const ALL: [Self; 10] = [
        Self::Texture,
        Self::Material,
        Self::Mesh,
        Self::Animation,
        Self::Font,
        Self::Schema,
        Self::Data,
        Self::Graph,
        Self::Tree,
        Self::Atlas,
    ];

    /// Extension of the files assets of this kind are written to, without the leading `.`;
    /// every kind but textures ends in `.bin`, see docs/features/build.md#outputs.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Texture => "ktx2",
            Self::Material => "amat.bin",
            Self::Mesh => "amesh.bin",
            Self::Animation => "aanim.bin",
            Self::Font => "afont.bin",
            Self::Schema => "aschema.bin",
            Self::Data => "adata.bin",
            Self::Graph => "agraph.bin",
            Self::Tree => "atree.bin",
            Self::Atlas => "aatlas.bin",
        }
    }

    /// The kind whose extension `file`'s name ends in, any case.
    pub fn of(file: &Path) -> Option<Self> {
        let name = file.file_name()?.to_str()?.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|kind| name.ends_with(&format!(".{}", kind.extension())))
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Texture => "Texture",
            Self::Material => "Material",
            Self::Mesh => "Mesh",
            Self::Animation => "Animation",
            Self::Font => "Font",
            Self::Schema => "Schema",
            Self::Data => "Data",
            Self::Graph => "Graph",
            Self::Tree => "Tree",
            Self::Atlas => "Atlas",
        }
    }

    /// Output path (relative to `output`) of an asset; asset paths of every kind but textures
    /// already end in their extension (`.amat.bin`, `.amesh.bin`, `.aanim.bin`, …).
    pub fn output_path(self, asset_path: &str) -> String {
        match self {
            Self::Texture => format!("{asset_path}.{}", self.extension()),
            _ => asset_path.to_owned(),
        }
    }

    /// Inverse of [`Self::output_path`].
    pub fn asset_path(self, output: &str) -> &str {
        match self {
            Self::Texture => output.strip_suffix(".ktx2").unwrap_or(output),
            _ => output,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    /// Relative to the output directory, `/`-separated.
    pub path: String,
}

/// Encodes entries sorted by ID. IDs must be unique.
pub fn encode(entries: &[Entry]) -> Vec<u8> {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.id);
    debug_assert!(sorted.windows(2).all(|w| w[0].id != w[1].id));

    let strings_size: usize = sorted.iter().map(|e| e.path.len()).sum();
    let mut out = Vec::with_capacity(HEADER_SIZE + ENTRY_SIZE * sorted.len() + strings_size);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
    out.extend_from_slice(&(strings_size as u32).to_le_bytes());

    let mut offset = 0u32;
    for e in &sorted {
        out.extend_from_slice(&e.id.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(e.path.len() as u16).to_le_bytes());
        out.push(e.kind as u8);
        out.push(0);
        offset += e.path.len() as u32;
    }
    for e in &sorted {
        out.extend_from_slice(e.path.as_bytes());
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError(&'static str);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid manifest: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

/// Decodes a manifest, skipping entries of unknown kinds.
pub fn decode(bytes: &[u8]) -> Result<Vec<Entry>, DecodeError> {
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    if bytes.len() < HEADER_SIZE || bytes[..4] != MAGIC {
        return Err(DecodeError("bad magic"));
    }
    if u32_at(4) != VERSION {
        return Err(DecodeError("unsupported version"));
    }
    let count = u32_at(8) as usize;
    let strings_size = u32_at(12) as usize;
    let strings_start = HEADER_SIZE + ENTRY_SIZE * count;
    if bytes.len() != strings_start + strings_size {
        return Err(DecodeError("size mismatch"));
    }
    let strings = &bytes[strings_start..];

    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let e = &bytes[HEADER_SIZE + i * ENTRY_SIZE..][..ENTRY_SIZE];
        let id = u64::from_le_bytes(e[0..8].try_into().unwrap());
        let offset = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
        let len = u16::from_le_bytes(e[12..14].try_into().unwrap()) as usize;
        let path = strings
            .get(offset..offset + len)
            .ok_or(DecodeError("path out of bounds"))?;
        let path = std::str::from_utf8(path).map_err(|_| DecodeError("path is not UTF-8"))?;
        let kind = match e[14] {
            0 => Kind::Texture,
            1 => Kind::Material,
            2 => Kind::Mesh,
            3 => Kind::Animation,
            4 => Kind::Font,
            5 => Kind::Schema,
            6 => Kind::Data,
            7 => Kind::Graph,
            8 => Kind::Tree,
            9 => Kind::Atlas,
            _ => continue,
        };
        entries.push(Entry {
            id,
            kind,
            path: path.to_owned(),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, path: &str) -> Entry {
        Entry {
            id,
            kind: Kind::Texture,
            path: path.into(),
        }
    }

    #[test]
    fn round_trip_sorted_by_id() {
        let entries = [entry(9, "b.ktx2"), entry(3, "dir/a.ktx2")];
        let decoded = decode(&encode(&entries)).unwrap();
        assert_eq!(decoded, [entry(3, "dir/a.ktx2"), entry(9, "b.ktx2")]);
    }

    #[test]
    fn matches_spec_example_layout() {
        let bytes = encode(&[entry(0x0102_0304_0506_0708, "textures/dirt.ktx2")]);
        let mut expected = Vec::new();
        expected.extend_from_slice(b"AMAN");
        expected.extend_from_slice(&[1, 0, 0, 0, 1, 0, 0, 0, 18, 0, 0, 0]);
        expected.extend_from_slice(&[8, 7, 6, 5, 4, 3, 2, 1]);
        expected.extend_from_slice(&[0, 0, 0, 0, 18, 0, 0, 0]);
        expected.extend_from_slice(b"textures/dirt.ktx2");
        assert_eq!(bytes, expected);
    }

    #[test]
    fn empty_manifest() {
        let bytes = encode(&[]);
        assert_eq!(bytes.len(), HEADER_SIZE);
        assert_eq!(decode(&bytes).unwrap(), []);
    }

    #[test]
    fn rejects_corruption() {
        let bytes = encode(&[entry(1, "a.ktx2")]);
        assert_eq!(
            decode(&bytes[..bytes.len() - 1]),
            Err(DecodeError("size mismatch"))
        );
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert_eq!(decode(&bad), Err(DecodeError("bad magic")));
        let mut bad = bytes.clone();
        bad[HEADER_SIZE + 12] = 99; // path_len past strings
        assert_eq!(decode(&bad), Err(DecodeError("path out of bounds")));
    }

    #[test]
    fn skips_unknown_kinds() {
        let mut bytes = encode(&[entry(1, "a.ktx2"), entry(2, "b.bin")]);
        bytes[HEADER_SIZE + ENTRY_SIZE + 14] = 200;
        assert_eq!(decode(&bytes).unwrap(), [entry(1, "a.ktx2")]);
    }
}
