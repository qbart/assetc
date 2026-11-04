//! `.pak` packs; see docs/spec/pak.md.

use std::path::Path;

use crate::read::Bytes;
use crate::{Kind, Result};

const FILE: &str = "pak";

/// One file in a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackEntry {
    /// Relative to the output directory, `/`-separated.
    pub path: String,
    pub id: u64,
    pub kind: Kind,
    /// Byte offset of the file in the pack.
    pub offset: u64,
    pub size: u64,
}

/// A pack in memory: its contents table and bytes.
#[derive(Debug, Clone)]
pub struct Pack {
    pub version: u32,
    pub alignment: u32,
    /// Bytes of the contents table, right after the 32-byte header.
    pub toc_bytes: u64,
    /// Sorted by path, as in the file.
    pub entries: Vec<PackEntry>,
    bytes: Vec<u8>,
}

impl Pack {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::decode(std::fs::read(path)?)
    }

    /// Whether `bytes` start like a pack.
    pub fn is_pack(bytes: &[u8]) -> bool {
        bytes.starts_with(b"APAK")
    }

    pub fn decode(bytes: Vec<u8>) -> Result<Self> {
        let b = Bytes::new(&bytes, FILE);
        if b.slice(0, 4)? != b"APAK" {
            return Err(b.err("bad magic"));
        }
        let version = b.u32(4)?;
        if version != 1 {
            return Err(b.err(format!("unsupported version {version}")));
        }
        let count = b.u32(8)? as usize;
        let alignment = b.u32(12)?;
        let toc_bytes = b.u64(16)?;
        let mut entries = Vec::with_capacity(count.min(1 << 16));
        let mut at = 32;
        for _ in 0..count {
            let path_len = b.u16(at + 26)? as usize;
            let entry = PackEntry {
                offset: b.u64(at)?,
                size: b.u64(at + 8)?,
                id: b.u64(at + 16)?,
                kind: Kind::from_u8(b.u8(at + 24)?),
                path: b.text(at + 28, path_len)?,
            };
            let end = entry.offset.checked_add(entry.size);
            if end.is_none_or(|end| end > bytes.len() as u64) {
                return Err(b.err(format!("{} runs past the end of the pack", entry.path)));
            }
            entries.push(entry);
            at += 28 + path_len;
        }
        Ok(Self {
            version,
            alignment,
            toc_bytes,
            entries,
            bytes,
        })
    }

    /// The bytes of `entry` (one of [`Self::entries`]).
    pub fn bytes(&self, entry: &PackEntry) -> &[u8] {
        &self.bytes[entry.offset as usize..(entry.offset + entry.size) as usize]
    }

    /// The entry at `path`, by binary search.
    pub fn find(&self, path: &str) -> Option<&PackEntry> {
        self.entries
            .binary_search_by(|e| e.path.as_bytes().cmp(path.as_bytes()))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// The file's whole size.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pack of `files`, laid out as the spec says with 16-byte alignment.
    pub fn pack(files: &[(&str, u64, u8, &[u8])]) -> Vec<u8> {
        let toc: usize = files.iter().map(|(p, ..)| 28 + p.len()).sum();
        let align = |n: usize| n.div_ceil(16) * 16;
        let mut offsets = Vec::new();
        let mut end = 32 + toc;
        for (.., data) in files {
            let at = align(end);
            offsets.push(at);
            end = at + data.len();
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"APAK");
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&(files.len() as u32).to_le_bytes());
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&(toc as u64).to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        for ((path, id, kind, data), at) in files.iter().zip(&offsets) {
            out.extend_from_slice(&(*at as u64).to_le_bytes());
            out.extend_from_slice(&(data.len() as u64).to_le_bytes());
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&[*kind, 0]);
            out.extend_from_slice(&(path.len() as u16).to_le_bytes());
            out.extend_from_slice(path.as_bytes());
        }
        for ((.., data), at) in files.iter().zip(&offsets) {
            out.resize(*at, 0);
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn reads_entries() {
        let bytes = pack(&[("a.ktx2", 7, 0, b"abc"), ("b/c.amat.bin", 9, 1, b"xy")]);
        let p = Pack::decode(bytes).unwrap();
        assert_eq!(p.entries.len(), 2);
        let e = p.find("b/c.amat.bin").unwrap();
        assert_eq!((e.id, e.kind), (9, Kind::Material));
        assert_eq!(p.bytes(e), b"xy");
        assert!(p.find("nope").is_none());
        let mut bad = pack(&[("a", 1, 0, b"abc")]);
        bad.truncate(bad.len() - 1);
        assert!(Pack::decode(bad).is_err());
    }
}
