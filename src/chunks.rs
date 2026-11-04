//! The chunked container of assetc's structured formats, see docs/spec/chunks.md.

const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 32;
const ALIGN: usize = 16;

/// An 8-byte, zero-padded ASCII name.
fn name8(name: &str) -> [u8; 8] {
    assert!(
        (1..=8).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        "bad chunk or format name {name:?}"
    );
    let mut out = [0; 8];
    out[..name.len()].copy_from_slice(name.as_bytes());
    out
}

/// Collects chunks in order and writes the file.
pub struct Writer {
    format: [u8; 8],
    version: u32,
    chunks: Vec<([u8; 8], Vec<u8>)>,
}

impl Writer {
    pub fn new(format: &str, version: u32) -> Self {
        Self {
            format: name8(format),
            version,
            chunks: Vec::new(),
        }
    }

    pub fn chunk(&mut self, name: &str, payload: Vec<u8>) -> &mut Self {
        let name = name8(name);
        assert!(
            self.chunks.iter().all(|(n, _)| *n != name),
            "duplicate chunk"
        );
        self.chunks.push((name, payload));
        self
    }

    pub fn finish(&self) -> Vec<u8> {
        let table_end = HEADER_SIZE + ENTRY_SIZE * self.chunks.len();
        let mut offsets = Vec::with_capacity(self.chunks.len());
        let mut end = table_end;
        for (_, payload) in &self.chunks {
            let offset = end.next_multiple_of(ALIGN);
            offsets.push(offset);
            end = offset + payload.len();
        }
        let mut out = Vec::with_capacity(end);
        out.extend_from_slice(&self.format);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&(self.chunks.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 16]);
        for ((name, payload), offset) in self.chunks.iter().zip(&offsets) {
            out.extend_from_slice(name);
            out.extend_from_slice(&[0; 8]);
            out.extend_from_slice(&(*offset as u64).to_le_bytes());
            out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        }
        for ((_, payload), offset) in self.chunks.iter().zip(&offsets) {
            out.resize(*offset, 0);
            out.extend_from_slice(payload);
        }
        out
    }
}

/// A parsed file: its version and chunks by name.
pub struct Reader<'a> {
    pub version: u32,
    chunks: Vec<([u8; 8], &'a [u8])>,
    offsets: Vec<u64>,
}

/// A chunk-table entry, for display.
pub struct Entry {
    pub name: String,
    pub offset: u64,
    pub size: u64,
}

impl<'a> Reader<'a> {
    /// Checks the header, table and payload ranges of a `format` file.
    pub fn parse(bytes: &'a [u8], format: &str) -> Result<Self, String> {
        if bytes.len() < HEADER_SIZE || bytes[..8] != name8(format) {
            return Err(format!("not an {format} file"));
        }
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let version = u32_at(8);
        let count = u32_at(12) as usize;
        let table_end = count
            .checked_mul(ENTRY_SIZE)
            .and_then(|t| t.checked_add(HEADER_SIZE))
            .filter(|&end| end <= bytes.len())
            .ok_or("chunk table runs past the end of the file")?;
        let mut chunks = Vec::with_capacity(count);
        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            let entry = HEADER_SIZE + i * ENTRY_SIZE;
            let name: [u8; 8] = bytes[entry..entry + 8].try_into().unwrap();
            let (offset, size) = (u64_at(entry + 16), u64_at(entry + 24));
            let end = offset
                .checked_add(size)
                .filter(|&end| offset >= table_end as u64 && end <= bytes.len() as u64)
                .ok_or_else(|| format!("chunk {} runs past the end of the file", show(&name)))?;
            chunks.push((name, &bytes[offset as usize..end as usize]));
            offsets.push(offset);
        }
        Ok(Self {
            version,
            chunks,
            offsets,
        })
    }

    /// Every chunk in table order.
    pub fn entries(&self) -> Vec<Entry> {
        self.chunks
            .iter()
            .zip(&self.offsets)
            .map(|((name, payload), offset)| Entry {
                name: show(name),
                offset: *offset,
                size: payload.len() as u64,
            })
            .collect()
    }

    pub fn chunk(&self, name: &str) -> Option<&'a [u8]> {
        let name = name8(name);
        self.chunks
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, p)| *p)
    }

    /// A chunk the format requires.
    pub fn require(&self, name: &str) -> Result<&'a [u8], String> {
        self.chunk(name)
            .ok_or_else(|| format!("missing {name} chunk"))
    }
}

/// The chunked file `bytes` (of `format`) with `extra` chunks added after its own; `bytes`
/// itself when there are none.
pub fn with_chunks(
    bytes: Vec<u8>,
    format: &str,
    extra: Vec<(&'static str, Vec<u8>)>,
) -> Result<Vec<u8>, String> {
    if extra.is_empty() {
        return Ok(bytes);
    }
    let reader = Reader::parse(&bytes, format)?;
    let mut writer = Writer::new(format, reader.version);
    for (name, payload) in &reader.chunks {
        writer.chunks.push((*name, payload.to_vec()));
    }
    for (name, payload) in extra {
        writer.chunk(name, payload);
    }
    Ok(writer.finish())
}

fn show(name: &[u8; 8]) -> String {
    String::from_utf8_lossy(name)
        .trim_end_matches('\0')
        .to_owned()
}

/// Little-endian byte building for chunk payloads.
#[derive(Default)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn f32s(&mut self, v: &[f32]) -> &mut Self {
        for f in v {
            self.0.extend_from_slice(&f.to_le_bytes());
        }
        self
    }

    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

/// Little-endian field `at` of record `index` (of `stride` bytes) in a payload.
pub fn u32_at(payload: &[u8], index: usize, stride: usize, at: usize) -> Option<u32> {
    let start = index.checked_mul(stride)?.checked_add(at)?;
    Some(u32::from_le_bytes(
        payload.get(start..start + 4)?.try_into().ok()?,
    ))
}

pub fn f32_at(payload: &[u8], index: usize, stride: usize, at: usize) -> Option<f32> {
    u32_at(payload, index, stride, at).map(f32::from_bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_spec() {
        let mut writer = Writer::new("AMESH", 1);
        writer
            .chunk("DESC", vec![1, 2, 3])
            .chunk("VERTICES", vec![9; 20]);
        let file = writer.finish();
        assert_eq!(&file[..8], b"AMESH\0\0\0");
        assert_eq!(u32::from_le_bytes(file[8..12].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(file[12..16].try_into().unwrap()), 2);
        // Table at 32, 32 bytes per entry; first payload at 96 (16-aligned).
        assert_eq!(&file[32..40], b"DESC\0\0\0\0");
        assert_eq!(u64::from_le_bytes(file[48..56].try_into().unwrap()), 96);
        assert_eq!(&file[64..72], b"VERTICES");
        assert_eq!(u64::from_le_bytes(file[80..88].try_into().unwrap()), 112);
        assert_eq!(file.len(), 132);
        // Padding is zero.
        assert!(file[99..112].iter().all(|&b| b == 0));

        let reader = Reader::parse(&file, "AMESH").unwrap();
        assert_eq!(reader.version, 1);
        assert_eq!(reader.chunk("DESC"), Some(&[1, 2, 3][..]));
        assert_eq!(reader.chunk("VERTICES").unwrap().len(), 20);
        assert!(reader.chunk("SKIN").is_none());
        assert!(reader.require("SKIN").unwrap_err().contains("SKIN"));
    }

    #[test]
    fn bad_files_are_rejected() {
        let file = Writer::new("AMESH", 1).chunk("DESC", vec![0; 8]).finish();
        assert!(Reader::parse(&file, "AANIM").is_err());
        assert!(Reader::parse(&file[..40], "AMESH").is_err());
        let mut broken = file.clone();
        broken[56..64].copy_from_slice(&1000u64.to_le_bytes());
        assert!(Reader::parse(&broken, "AMESH").is_err());
    }
}
