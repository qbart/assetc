//! The container shared by assetc's binary formats; see docs/spec/chunks.md.

use crate::read::Bytes;
use crate::{Error, Result};

const HEADER: usize = 32;
const ENTRY: usize = 32;

/// A parsed chunked file: its format name, version and chunks by name.
#[derive(Debug, Clone)]
pub struct ChunkFile<'a> {
    pub format: String,
    pub version: u32,
    pub chunks: Vec<(String, &'a [u8])>,
}

impl<'a> ChunkFile<'a> {
    /// Parses any chunked file, checking the table and every payload range.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let b = Bytes::new(bytes, "chunked file");
        let name = |raw: &[u8]| {
            let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
            String::from_utf8_lossy(&raw[..end]).into_owned()
        };
        let format = name(b.slice(0, 8)?);
        let version = b.u32(8)?;
        let count = b.u32(12)? as usize;
        let mut chunks = Vec::with_capacity(count.min(1024));
        for i in 0..count {
            let at = HEADER + i * ENTRY;
            let offset = usize::try_from(b.u64(at + 16)?).map_err(|_| b.err("offset too big"))?;
            let size = usize::try_from(b.u64(at + 24)?).map_err(|_| b.err("size too big"))?;
            let chunk_name = name(b.slice(at, 8)?);
            let payload = b
                .slice(offset, size)
                .map_err(|_| b.err(format!("chunk {chunk_name} runs past the end")))?;
            chunks.push((chunk_name, payload));
        }
        Ok(Self {
            format,
            version,
            chunks,
        })
    }

    /// Parses a file of `format` (e.g. `AMESH`) whose version is one of `versions`; `file`
    /// names it in errors.
    pub fn open(bytes: &'a [u8], format: &str, versions: &[u32], file: &str) -> Result<Self> {
        let parsed = Self::parse(bytes).map_err(|e| e.in_file(file))?;
        if parsed.format != format {
            return Err(Error::format(
                file,
                format!("format is {:?}, not {format}", parsed.format),
            ));
        }
        if !versions.contains(&parsed.version) {
            return Err(Error::format(
                file,
                format!("unsupported version {}", parsed.version),
            ));
        }
        Ok(parsed)
    }

    /// The payload of chunk `name`, if present.
    pub fn chunk(&self, name: &str) -> Option<&'a [u8]> {
        self.chunks
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, bytes)| *bytes)
    }

    /// The payload of required chunk `name`; `file` names the format in the error.
    pub fn require(&self, name: &str, file: &str) -> Result<&'a [u8]> {
        self.chunk(name)
            .ok_or_else(|| Error::format(file, format!("missing chunk {name}")))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    /// A chunked file of `format` with `chunks`, laid out as the spec says.
    pub fn file(format: &str, version: u32, chunks: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = vec![0u8; 32 + 32 * chunks.len()];
        out[..format.len()].copy_from_slice(format.as_bytes());
        out[8..12].copy_from_slice(&version.to_le_bytes());
        out[12..16].copy_from_slice(&(chunks.len() as u32).to_le_bytes());
        for (i, (name, payload)) in chunks.iter().enumerate() {
            while out.len() % 16 != 0 {
                out.push(0);
            }
            let at = 32 + 32 * i;
            out[at..at + name.len()].copy_from_slice(name.as_bytes());
            let offset = out.len() as u64;
            out[at + 16..at + 24].copy_from_slice(&offset.to_le_bytes());
            out[at + 24..at + 32].copy_from_slice(&(payload.len() as u64).to_le_bytes());
            out.extend_from_slice(payload);
        }
        out
    }

    #[test]
    fn reads_chunks_and_rejects_bad_ranges() {
        let bytes = file("ATEST", 2, &[("DESC", &[1, 2, 3]), ("STRINGS", b"hi")]);
        let parsed = super::ChunkFile::open(&bytes, "ATEST", &[2], "test").unwrap();
        assert_eq!(parsed.chunk("DESC"), Some(&[1u8, 2, 3][..]));
        assert_eq!(parsed.chunk("STRINGS"), Some(&b"hi"[..]));
        assert!(parsed.chunk("NOPE").is_none());
        assert!(super::ChunkFile::open(&bytes, "ATEST", &[1], "test").is_err());
        assert!(super::ChunkFile::open(&bytes, "OTHER", &[2], "test").is_err());
        assert!(super::ChunkFile::parse(&bytes[..bytes.len() - 1]).is_err());
    }
}
