//! Attached structs: the `STRUCTS`, `SVALUES`, `SREFS` and `SSTRINGS` chunks any format may
//! carry, and the standalone `ASTRUCT` file after a material's record; see
//! docs/spec/astruct.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::data::{TARGET, Target, VALUE, Value, Values};
use crate::hash::fnv1a64;
use crate::read::Bytes;

const FILE: &str = "astruct";
const ATTACHED: usize = 40;

/// What owns an attached struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// The asset itself (`0`); its item is `0`.
    Asset,
    /// Each node (graphs, trees), gradient or color (`1`).
    Item,
    /// Each edge of a graph (`2`).
    Edge,
    /// A custom material's params (`3`); its item is `0`.
    Params,
    /// A scope this SDK does not know; kept so its records are not lost.
    Other(u32),
}

impl Scope {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => Self::Asset,
            1 => Self::Item,
            2 => Self::Edge,
            3 => Self::Params,
            other => Self::Other(other),
        }
    }

    /// The value the file stores.
    pub fn to_u32(self) -> u32 {
        match self {
            Self::Asset => 0,
            Self::Item => 1,
            Self::Edge => 2,
            Self::Params => 3,
            Self::Other(value) => value,
        }
    }
}

/// One owner's value of one attached struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Attached {
    pub scope: Scope,
    /// The owner's index in the format's chunk of its scope (a tree's nodes breadth first);
    /// `0` for [`Scope::Asset`].
    pub item: u32,
    /// The struct's name; the spec's `type`.
    pub type_name: String,
    /// FNV-1a-64 of `type_name`.
    pub type_hash: u64,
    /// Asset ID of the struct's schema (`.aschema.bin`), which names its fields.
    pub schema: u64,
    /// A [`Value::Struct`], every field resolved; ref targets index [`Structs::refs`].
    pub value: Value,
}

impl Attached {
    /// Its field `name`.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.value.get(name)
    }
}

/// An asset's attached structs; empty when it has none.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Structs {
    /// Sorted by scope, then item, then the scope's attachment order.
    pub attached: Vec<Attached>,
    /// The instances refs point at, each once, in order of first use.
    pub refs: Vec<Target>,
}

impl Structs {
    /// A standalone `ASTRUCT` file (as after a material's record).
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ASTRUCT", &[1], FILE)?;
        file.require("STRUCTS", FILE)?;
        Self::from_chunks(&file, FILE)
    }

    /// The struct chunks of `file`, a chunked file of format `name` (named in errors); empty
    /// without `STRUCTS`. With it, the other three are required.
    pub fn from_chunks(file: &ChunkFile<'_>, name: &'static str) -> Result<Self> {
        let Some(structs) = file.chunk("STRUCTS") else {
            return Ok(Self::default());
        };
        let require = |chunk| Ok::<_, crate::Error>(Bytes::new(file.require(chunk, name)?, name));
        let values = require("SVALUES")?;
        let strings = require("SSTRINGS")?;
        let refs = require("SREFS")?
            .records(TARGET)?
            .map(|r| Target::read(&r))
            .collect::<Result<Vec<_>>>()?;
        let value_count = values.records(VALUE)?.count();
        let mut reader = Values::new(values, value_count, strings, refs.len())?;
        let attached = Bytes::new(structs, name)
            .records(ATTACHED)?
            .map(|r| {
                let value = reader.read(r.u32(32)? as usize)?;
                if !matches!(value, Value::Struct(_)) {
                    return Err(r.err("an attached struct's value is not a struct"));
                }
                Ok(Attached {
                    scope: Scope::from_u32(r.u32(0)?),
                    item: r.u32(4)?,
                    type_hash: r.u64(8)?,
                    type_name: r.text_ref(16, strings)?,
                    schema: r.u64(24)?,
                    value,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { attached, refs })
    }

    pub fn is_empty(&self) -> bool {
        self.attached.is_empty()
    }

    /// Every attached struct, in file order.
    pub fn all(&self) -> &[Attached] {
        &self.attached
    }

    /// The structs of owner `item` of `scope`, in attachment order.
    pub fn of(&self, scope: Scope, item: u32) -> impl Iterator<Item = &Attached> {
        self.attached
            .iter()
            .filter(move |a| a.scope == scope && a.item == item)
    }

    /// The struct named `type_name` of owner `item` of `scope`.
    pub fn get(&self, scope: Scope, item: u32, type_name: &str) -> Option<&Attached> {
        let hash = fnv1a64(type_name.as_bytes());
        self.of(scope, item)
            .find(|a| a.type_hash == hash && a.type_name == type_name)
    }

    /// Where the ref `value` points; `None` for another value or a ref without a target.
    pub fn target(&self, value: &Value) -> Option<&Target> {
        match value {
            Value::Ref {
                target: Some(t), ..
            } => self.refs.get(*t as usize),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::data::tests::{raw, value};
    use crate::graph::tests::text;

    /// The four struct chunks: a `Step { speed: 2.5, next: ref "b" }` on item 1 of scope 1, and
    /// a `Meta { tier: 3 }` on the asset; `scope` overrides the first record's scope.
    pub fn chunks(scope: u32) -> [(&'static str, Vec<u8>); 4] {
        let mut strings = Vec::new();
        let mut structs = Vec::new();
        let at = |t: &[u8]| u32::from_le_bytes(t[..4].try_into().unwrap());
        // Records sorted by scope: the asset's Meta first.
        for (scope, item, name, value) in [(0u32, 0u32, "Meta", 3u32), (scope, 1, "Step", 0)] {
            structs.extend(scope.to_le_bytes());
            structs.extend(item.to_le_bytes());
            structs.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            structs.extend(text(&mut strings, name));
            structs.extend(0x5c4e3au64.to_le_bytes());
            structs.extend(value.to_le_bytes());
            structs.extend(0u32.to_le_bytes());
        }
        let b = text(&mut strings, "b");
        // 0: Step; 1–2 its fields; 3: Meta; 4 its field.
        let mut values = value("", 6, 2, [1, 0]);
        values.extend(raw("speed", 3, 2.5f64.to_le_bytes()));
        values.extend(value("next", 8, 1, [at(&b), 0]));
        values.extend(value("", 6, 1, [4, 0]));
        values.extend(raw("tier", 2, 3i64.to_le_bytes()));
        let mut refs = 0xf11eu64.to_le_bytes().to_vec();
        refs.extend(fnv1a64(b"b").to_le_bytes());
        refs.extend(1u32.to_le_bytes());
        refs.extend(0u32.to_le_bytes());
        [
            ("STRUCTS", structs),
            ("SVALUES", values),
            ("SREFS", refs),
            ("SSTRINGS", strings),
        ]
    }

    pub fn astruct() -> Vec<u8> {
        let chunks = chunks(1);
        let chunks: Vec<(&str, &[u8])> = chunks.iter().map(|(n, p)| (*n, &p[..])).collect();
        file("ASTRUCT", 1, &chunks)
    }

    #[test]
    fn decodes_owners_values_and_refs() {
        let structs = Structs::decode(&astruct()).unwrap();
        assert_eq!(structs.all().len(), 2);
        let meta = structs.get(Scope::Asset, 0, "Meta").unwrap();
        assert_eq!((meta.schema, meta.type_name.as_str()), (0x5c4e3a, "Meta"));
        assert_eq!(meta.get("tier").and_then(Value::as_int), Some(3));
        let step = structs.get(Scope::Item, 1, "Step").unwrap();
        assert_eq!(step.get("speed").and_then(Value::as_float), Some(2.5));
        let next = step.get("next").unwrap();
        assert_eq!(next.as_str(), Some("b"));
        assert_eq!(
            structs.target(next).map(|t| (t.file, t.instance)),
            Some((0xf11e, 1))
        );
        assert_eq!(structs.of(Scope::Item, 0).count(), 0);
        assert!(structs.get(Scope::Item, 1, "Meta").is_none());
    }

    #[test]
    fn keeps_unknown_scopes() {
        let chunks = chunks(7);
        let chunks: Vec<(&str, &[u8])> = chunks.iter().map(|(n, p)| (*n, &p[..])).collect();
        let structs = Structs::decode(&file("ASTRUCT", 1, &chunks)).unwrap();
        assert_eq!(structs.of(Scope::Other(7), 1).count(), 1);
        assert_eq!(Scope::Other(7).to_u32(), 7);
    }

    #[test]
    fn rejects_bad_chunks() {
        let all = chunks(1);
        let pick = |skip: &str| -> Vec<u8> {
            let kept: Vec<(&str, &[u8])> = all
                .iter()
                .filter(|(n, _)| *n != skip)
                .map(|(n, p)| (*n, &p[..]))
                .collect();
            file("ASTRUCT", 1, &kept)
        };
        assert!(Structs::decode(&pick("SVALUES")).is_err());
        assert!(Structs::decode(&pick("STRUCTS")).is_err());
        // A record pointing at a field rather than a struct value.
        let mut structs = all[0].1.clone();
        structs[32..36].copy_from_slice(&4u32.to_le_bytes());
        let kept: Vec<(&str, &[u8])> = [("STRUCTS", &structs[..])]
            .into_iter()
            .chain(all[1..].iter().map(|(n, p)| (*n, &p[..])))
            .collect();
        assert!(Structs::decode(&file("ASTRUCT", 1, &kept)).is_err());
        // Truncated values.
        let values = &all[1].1[..all[1].1.len() - 1];
        let kept: Vec<(&str, &[u8])> = vec![
            ("STRUCTS", &all[0].1),
            ("SVALUES", values),
            ("SREFS", &all[2].1),
            ("SSTRINGS", &all[3].1),
        ];
        assert!(Structs::decode(&file("ASTRUCT", 1, &kept)).is_err());
    }
}
