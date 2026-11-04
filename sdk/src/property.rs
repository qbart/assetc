//! Key / value properties of meshes, shapes, paths, animation events, graphs and trees; see
//! docs/spec/amesh.bin.md#property-32-bytes.

use crate::Result;
use crate::hash::fnv1a64;
use crate::read::Bytes;

/// A property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

/// A key and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// FNV-1a-64 of `key`.
    pub key_hash: u64,
    pub key: String,
    pub value: PropertyValue,
}

/// The value of `key` among `properties`.
pub fn property<'a>(properties: &'a [Property], key: &str) -> Option<&'a PropertyValue> {
    let hash = fnv1a64(key.as_bytes());
    properties
        .iter()
        .find(|p| p.key_hash == hash && p.key == key)
        .map(|p| &p.value)
}

/// Every 32-byte property record of `chunk`, text in `strings`.
pub(crate) fn properties_of(chunk: Bytes<'_>, strings: Bytes<'_>) -> Result<Vec<Property>> {
    chunk
        .records(32)?
        .map(|p| {
            let value = match p.u32(16)? {
                0 => PropertyValue::Bool(p.u64(24)? != 0),
                1 => PropertyValue::Int(p.i64(24)?),
                2 => PropertyValue::Float(p.f64(24)?),
                3 => PropertyValue::Text(p.text_ref(24, strings)?),
                t => return Err(p.err(format!("unknown property type {t}"))),
            };
            Ok(Property {
                key_hash: p.u64(0)?,
                key: p.text_ref(8, strings)?,
                value,
            })
        })
        .collect()
}

/// `count` properties from `first` in a `PROPS` payload (absent: `None`), text in `strings`.
pub(crate) fn read_properties(
    props: Option<Bytes<'_>>,
    strings: Bytes<'_>,
    first: u32,
    count: u32,
) -> Result<Vec<Property>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let props = props.ok_or_else(|| strings.err("properties without a PROPS chunk"))?;
    let at = (first as usize)
        .checked_mul(32)
        .ok_or_else(|| props.err("property index overflows"))?;
    let len = (count as usize)
        .checked_mul(32)
        .ok_or_else(|| props.err("property count overflows"))?;
    properties_of(Bytes::new(props.slice(at, len)?, props.file), strings)
}
