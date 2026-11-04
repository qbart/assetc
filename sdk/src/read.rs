//! Bounds-checked little-endian reads; every reader goes through these, so bad input is an
//! error, never a panic.

use crate::{Error, Result};

/// `bytes` of format `file`, read at absolute offsets.
#[derive(Clone, Copy)]
pub(crate) struct Bytes<'a> {
    pub bytes: &'a [u8],
    pub file: &'static str,
}

impl<'a> Bytes<'a> {
    pub fn new(bytes: &'a [u8], file: &'static str) -> Self {
        Self { bytes, file }
    }

    pub fn err(&self, message: impl Into<String>) -> Error {
        Error::format(self.file, message)
    }

    pub fn slice(&self, at: usize, len: usize) -> Result<&'a [u8]> {
        at.checked_add(len)
            .and_then(|end| self.bytes.get(at..end))
            .ok_or_else(|| {
                self.err(format!(
                    "{len} bytes at {at} run past the end ({} bytes)",
                    self.bytes.len()
                ))
            })
    }

    pub fn array<const N: usize>(&self, at: usize) -> Result<[u8; N]> {
        Ok(self.slice(at, N)?.try_into().expect("N bytes"))
    }

    pub fn u8(&self, at: usize) -> Result<u8> {
        Ok(self.array::<1>(at)?[0])
    }

    pub fn u16(&self, at: usize) -> Result<u16> {
        self.array(at).map(u16::from_le_bytes)
    }

    pub fn u32(&self, at: usize) -> Result<u32> {
        self.array(at).map(u32::from_le_bytes)
    }

    pub fn i32(&self, at: usize) -> Result<i32> {
        self.array(at).map(i32::from_le_bytes)
    }

    pub fn u64(&self, at: usize) -> Result<u64> {
        self.array(at).map(u64::from_le_bytes)
    }

    pub fn i64(&self, at: usize) -> Result<i64> {
        self.array(at).map(i64::from_le_bytes)
    }

    pub fn f32(&self, at: usize) -> Result<f32> {
        self.array(at).map(f32::from_le_bytes)
    }

    pub fn f64(&self, at: usize) -> Result<f64> {
        self.array(at).map(f64::from_le_bytes)
    }

    /// `N` consecutive f32s.
    pub fn f32s<const N: usize>(&self, at: usize) -> Result<[f32; N]> {
        let mut out = [0.0; N];
        for (i, v) in out.iter_mut().enumerate() {
            *v = self.f32(at + i * 4)?;
        }
        Ok(out)
    }

    /// The `stride`-byte records of the whole payload; its size must be a multiple of
    /// `stride`.
    pub fn records(&self, stride: usize) -> Result<impl Iterator<Item = Bytes<'a>> + 'a> {
        if !self.bytes.len().is_multiple_of(stride) {
            return Err(self.err(format!(
                "{} bytes are not a whole number of {stride}-byte records",
                self.bytes.len()
            )));
        }
        let file = self.file;
        Ok(self
            .bytes
            .chunks_exact(stride)
            .map(move |bytes| Bytes { bytes, file }))
    }

    /// Exactly `count` records of `stride` bytes.
    pub fn records_n(
        &self,
        stride: usize,
        count: usize,
    ) -> Result<impl Iterator<Item = Bytes<'a>> + 'a> {
        let needed = stride
            .checked_mul(count)
            .ok_or_else(|| self.err("record count overflows"))?;
        if self.bytes.len() < needed {
            return Err(self.err(format!(
                "{count} records of {stride} bytes need {needed} bytes, have {}",
                self.bytes.len()
            )));
        }
        let file = self.file;
        Ok(self.bytes[..needed]
            .chunks_exact(stride)
            .map(move |bytes| Bytes { bytes, file }))
    }

    /// UTF-8 text at `at` of `len` bytes.
    pub fn text(&self, at: usize, len: usize) -> Result<String> {
        let bytes = self.slice(at, len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| self.err(format!("text at {at} is not UTF-8")))
    }

    /// Text referenced as `offset` u32 and `len` u32 at `at` in this record, into `strings`.
    pub fn text_ref(&self, at: usize, strings: Bytes<'_>) -> Result<String> {
        strings.text(self.u32(at)? as usize, self.u32(at + 4)? as usize)
    }
}
