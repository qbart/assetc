//! `assetc.aman.bin`, asset ID to output file; see docs/spec/aman.bin.md.

use crate::read::Bytes;
use crate::{Kind, Result};

/// The manifest's file name, at the root of the output directory.
pub const FILE_NAME: &str = "assetc.aman.bin";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub id: u64,
    pub kind: Kind,
    /// Relative to the output directory, `/`-separated.
    pub path: String,
}

/// Every asset of a build, sorted by ID.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub entries: Vec<ManifestEntry>,
}

impl Manifest {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let b = Bytes::new(bytes, "aman.bin");
        if b.slice(0, 4)? != b"AMAN" {
            return Err(b.err("bad magic"));
        }
        let version = b.u32(4)?;
        if version != 1 {
            return Err(b.err(format!("unsupported version {version}")));
        }
        let count = b.u32(8)? as usize;
        let strings_size = b.u32(12)? as usize;
        let strings_at = 16 + count * 16;
        let strings = Bytes::new(b.slice(strings_at, strings_size)?, b.file);
        let entries = Bytes::new(b.slice(16, count * 16)?, b.file)
            .records(16)?
            .map(|r| {
                Ok(ManifestEntry {
                    id: r.u64(0)?,
                    path: strings.text(r.u32(8)? as usize, r.u16(12)? as usize)?,
                    kind: Kind::from_u8(r.u8(14)?),
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { entries })
    }

    /// The entry of `id`, by binary search.
    pub fn get(&self, id: u64) -> Option<&ManifestEntry> {
        self.entries
            .binary_search_by_key(&id, |e| e.id)
            .ok()
            .map(|i| &self.entries[i])
    }
}
