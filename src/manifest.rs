//! `assets.manifest` reader/writer, see docs/spec/manifest.md.

use std::fmt;

pub const FILE_NAME: &str = "assets.manifest";
const MAGIC: [u8; 4] = *b"ACMF";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 16;
const ENTRY_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Texture = 0,
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
#[cfg_attr(not(test), expect(dead_code, reason = "no runtime reader yet"))]
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
        expected.extend_from_slice(b"ACMF");
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
