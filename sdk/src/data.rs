//! `.adata.bin` compiled data: instances as trees of typed values, every field resolved; see
//! docs/spec/adata.bin.md.

use std::fmt;

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;
use crate::schema::Schema;

const FILE: &str = "adata.bin";
const VALUE: usize = 24;
/// Deeper values than this are rejected rather than recursed into.
const MAX_DEPTH: usize = 256;

/// A typed value; see docs/spec/adata.bin.md#value-24-bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An optional field without a value.
    Null,
    Bool(bool),
    /// Any integer type.
    Int(i64),
    /// Any float type.
    Float(f64),
    /// `string`.
    Text(String),
    /// `rgb`, `rgba`: `R G B A` (alpha `255` for `rgb`).
    Color([u8; 4]),
    /// A struct's fields in field order, by name hash (FNV-1a-64; the file stores no field
    /// names: look them up with [`Value::get`] or name them with [`Value::named`]).
    Struct(Vec<(u64, Value)>),
    /// `list<T>`; also a `curve` (points, each a list of `x y in out`) and a `gradient` (stops,
    /// each a list of `t` and a color).
    List(Vec<Value>),
    /// `ref<S>`: the target's id and its index in [`Data::refs`]; `None` when the ref is `""`
    /// or names no instance of the project.
    Ref {
        id: String,
        target: Option<u32>,
    },
    /// An enum value: its name and its index in the enum.
    Enum {
        name: String,
        index: u32,
    },
    /// `asset<K>`: the asset ID of the file's main output (`0` for `""`).
    Asset(u64),
}

impl Value {
    /// The struct field `name`.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.field(fnv1a64(name.as_bytes()))
    }

    /// The struct field whose name hashes to `name_hash`.
    pub fn field(&self, name_hash: u64) -> Option<&Value> {
        match self {
            Self::Struct(fields) => fields.iter().find(|(h, _)| *h == name_hash).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// A float, or an integer as one.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// A text, an enum value's name or a ref's id.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(s) | Self::Enum { name: s, .. } | Self::Ref { id: s, .. } => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// Displays the value with struct fields named from `schema` (hashes it lacks stay
    /// `#hash`).
    pub fn named<'a>(&'a self, schema: &'a Schema) -> Show<'a> {
        Show {
            value: self,
            schema: Some(schema),
        }
    }
}

/// A [`Value`] displayed on one line, JSON-like; from [`Value::named`] or `Display`.
pub struct Show<'a> {
    value: &'a Value,
    schema: Option<&'a Schema>,
}

impl fmt::Display for Show<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let show = |value| Show {
            value,
            schema: self.schema,
        };
        match self.value {
            Value::Null => f.write_str("null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x:?}"),
            Value::Text(s) => write!(f, "{s:?}"),
            Value::Color([r, g, b, a]) => write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}"),
            Value::Struct(fields) => {
                f.write_str("{")?;
                for (i, (hash, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    match self.schema.and_then(|s| s.field_name(*hash)) {
                        Some(name) => f.write_str(name)?,
                        None => write!(f, "#{hash:016x}")?,
                    }
                    write!(f, ": {}", show(value))?;
                }
                f.write_str("}")
            }
            Value::List(items) => {
                f.write_str("[")?;
                for (i, value) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", show(value))?;
                }
                f.write_str("]")
            }
            Value::Ref { id, .. } => write!(f, "ref {id:?}"),
            Value::Enum { name, .. } => f.write_str(name),
            Value::Asset(id) => write!(f, "asset {id:#018x}"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Show {
            value: self,
            schema: None,
        }
        .fmt(f)
    }
}

/// A struct the instances are of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    /// FNV-1a-64 of `name`.
    pub name_hash: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// Index into [`Data::types`]; the spec's `type`.
    pub type_index: u32,
    /// The name of its type.
    pub type_name: String,
    /// FNV-1a-64 of `id`.
    pub id_hash: u64,
    pub id: String,
    /// Its struct value, every field resolved.
    pub value: Value,
}

impl Instance {
    /// Its field `name`.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.value.get(name)
    }
}

/// Where an instance a ref names lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// Asset ID of the `.adata.bin` holding the target (this file's own ID for a target in it).
    pub file: u64,
    /// FNV-1a-64 of the target's `$id`.
    pub id_hash: u64,
    /// The target's index in that file's [`Data::instances`].
    pub instance: u32,
}

/// A data file's instances, in the file's order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Data {
    /// The structs the instances are of, in order of first use.
    pub types: Vec<TypeRef>,
    pub instances: Vec<Instance>,
    /// The instances refs point at, each once, in order of first use.
    pub refs: Vec<Target>,
}

impl Data {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ADATA", &[1, 2], FILE)?;
        let desc = Bytes::new(file.require("DESC", FILE)?, FILE);
        let instance_count = desc.u32(0)? as usize;
        let value_count = desc.u32(4)? as usize;
        let strings = Bytes::new(file.require("STRINGS", FILE)?, FILE);
        let types = Bytes::new(file.require("TYPES", FILE)?, FILE)
            .records_n(16, desc.u32(8)? as usize)?
            .map(|r| {
                Ok(TypeRef {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        // Version 1 has no REFS and no `ref_count`.
        let refs = if file.version >= 2 {
            Bytes::new(file.require("REFS", FILE)?, FILE)
                .records_n(24, desc.u32(12)? as usize)?
                .map(|r| {
                    Ok(Target {
                        file: r.u64(0)?,
                        id_hash: r.u64(8)?,
                        instance: r.u32(16)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let values = Bytes::new(file.require("VALUES", FILE)?, FILE);
        let needed = value_count
            .checked_mul(VALUE)
            .ok_or_else(|| values.err("value count overflows"))?;
        let mut reader = Reader {
            values: Bytes::new(values.slice(0, needed)?, FILE),
            strings,
            value_count,
            ref_count: refs.len(),
            budget: value_count,
        };
        let instances = Bytes::new(file.require("INSTANCE", FILE)?, FILE)
            .records_n(32, instance_count)?
            .map(|r| {
                let type_index = r.u32(0)?;
                let type_name = types
                    .get(type_index as usize)
                    .ok_or_else(|| r.err(format!("type {type_index} is not in TYPES")))?
                    .name
                    .clone();
                let (_, value) = reader.value(r.u32(4)? as usize, 0)?;
                Ok(Instance {
                    type_index,
                    type_name,
                    id_hash: r.u64(8)?,
                    id: r.text_ref(16, strings)?,
                    value,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            types,
            instances,
            refs,
        })
    }

    /// The instance with `id`.
    pub fn instance(&self, id: &str) -> Option<&Instance> {
        let hash = fnv1a64(id.as_bytes());
        self.instances
            .iter()
            .find(|i| i.id_hash == hash && i.id == id)
    }

    /// The instances of type `name`.
    pub fn of_type<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Instance> + 'a {
        self.instances.iter().filter(move |i| i.type_name == name)
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

/// Reads `VALUES` into trees; each value is read once (`budget`), children after their parent.
struct Reader<'a> {
    values: Bytes<'a>,
    strings: Bytes<'a>,
    value_count: usize,
    ref_count: usize,
    budget: usize,
}

impl Reader<'_> {
    /// The value in `slot` and its name hash.
    fn value(&mut self, slot: usize, depth: usize) -> Result<(u64, Value)> {
        if depth > MAX_DEPTH {
            return Err(self.values.err("values nest too deep"));
        }
        self.budget = self
            .budget
            .checked_sub(1)
            .ok_or_else(|| self.values.err("values are read more than once"))?;
        let at = slot
            .checked_mul(VALUE)
            .ok_or_else(|| self.values.err("value index overflows"))?;
        let r = Bytes::new(self.values.slice(at, VALUE)?, FILE);
        let (kind, count) = (r.u32(8)?, r.u32(12)?);
        let value = match kind {
            0 => Value::Null,
            1 => Value::Bool(r.u64(16)? != 0),
            2 => Value::Int(r.i64(16)?),
            3 => Value::Float(r.f64(16)?),
            4 => Value::Text(self.strings.text(r.u32(16)? as usize, count as usize)?),
            5 => Value::Color(r.array(16)?),
            6 | 7 => {
                let first = r.u32(16)? as usize;
                let end = first.saturating_add(count as usize);
                if count > 0 && (first <= slot || end > self.value_count) {
                    return Err(r.err(format!("children of value {slot} are out of place")));
                }
                let items = (first..end)
                    .map(|child| self.value(child, depth + 1))
                    .collect::<Result<Vec<_>>>()?;
                if kind == 6 {
                    Value::Struct(items)
                } else {
                    Value::List(items.into_iter().map(|(_, v)| v).collect())
                }
            }
            8 => {
                let target = match r.u32(20)? {
                    u32::MAX => None,
                    t if (t as usize) < self.ref_count => Some(t),
                    t => return Err(r.err(format!("ref target {t} is not in REFS"))),
                };
                Value::Ref {
                    id: self.strings.text(r.u32(16)? as usize, count as usize)?,
                    target,
                }
            }
            9 => Value::Enum {
                name: self.strings.text(r.u32(16)? as usize, count as usize)?,
                index: r.u32(20)?,
            },
            10 => Value::Asset(r.u64(16)?),
            other => return Err(r.err(format!("unknown value kind {other}"))),
        };
        Ok((r.u64(0)?, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::graph::tests::text;

    fn value(name: &str, kind: u32, count: u32, payload: [u32; 2]) -> Vec<u8> {
        let hash = if name.is_empty() {
            0
        } else {
            fnv1a64(name.as_bytes())
        };
        let mut out = hash.to_le_bytes().to_vec();
        for v in [kind, count, payload[0], payload[1]] {
            out.extend(v.to_le_bytes());
        }
        out
    }

    fn raw(name: &str, kind: u32, payload: [u8; 8]) -> Vec<u8> {
        let mut out = value(name, kind, 0, [0, 0]);
        out[16..].copy_from_slice(&payload);
        out
    }

    fn sample(drop_refs: bool) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut types = fnv1a64(b"Weapon").to_le_bytes().to_vec();
        types.extend(text(&mut strings, "Weapon"));
        let sword = text(&mut strings, "sword");
        let mut instance = vec![0u8; 8];
        instance.extend(fnv1a64(b"sword").to_le_bytes());
        instance.extend(&sword);
        instance.extend([0; 8]);
        let at = |t: &[u8]| u32::from_le_bytes(t[..4].try_into().unwrap());
        let name = text(&mut strings, "Sword");
        let kind = text(&mut strings, "melee");
        // 0: the struct; 1–7 its fields; 8–9 the list's items.
        let mut values = value("", 6, 7, [1, 0]);
        values.extend(value("name", 4, 5, [at(&name), 0]));
        values.extend(raw("damage", 2, 12i64.to_le_bytes()));
        values.extend(raw("speed", 3, 1.5f64.to_le_bytes()));
        values.extend(raw("tint", 5, [255, 128, 0, 255, 0, 0, 0, 0]));
        values.extend(value("kind", 9, 5, [at(&kind), 0]));
        values.extend(value("tags", 7, 2, [8, 0]));
        values.extend(value("next", 8, 5, [at(&sword), 0]));
        values.extend(raw("", 1, 1u64.to_le_bytes()));
        values.extend(value("", 0, 0, [0, 0]));
        let mut refs = 0xf11eu64.to_le_bytes().to_vec();
        refs.extend(fnv1a64(b"sword").to_le_bytes());
        refs.extend([0; 8]);
        let mut desc = Vec::new();
        for v in [1u32, 10, 1, 1] {
            desc.extend(v.to_le_bytes());
        }
        let mut chunks: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc),
            ("TYPES", &types),
            ("INSTANCE", &instance),
            ("VALUES", &values),
        ];
        if !drop_refs {
            chunks.push(("REFS", &refs));
        }
        chunks.push(("STRINGS", &strings));
        file("ADATA", 2, &chunks)
    }

    #[test]
    fn decodes_instances_as_value_trees() {
        let data = Data::decode(&sample(false)).unwrap();
        let sword = data.instance("sword").unwrap();
        assert_eq!(sword.type_name, "Weapon");
        assert_eq!(sword.get("name").and_then(Value::as_str), Some("Sword"));
        assert_eq!(sword.get("damage").and_then(Value::as_int), Some(12));
        assert_eq!(sword.get("speed").and_then(Value::as_float), Some(1.5));
        assert_eq!(sword.get("tint"), Some(&Value::Color([255, 128, 0, 255])));
        assert_eq!(
            sword.get("kind"),
            Some(&Value::Enum {
                name: "melee".into(),
                index: 0
            })
        );
        let tags = sword.get("tags").and_then(Value::as_list).unwrap();
        assert_eq!(tags, [Value::Bool(true), Value::Null]);
        let next = sword.get("next").unwrap();
        assert_eq!(
            data.target(next).map(|t| (t.file, t.instance)),
            Some((0xf11e, 0))
        );
        assert_eq!(data.of_type("Weapon").count(), 1);
        assert!(sword.value.to_string().starts_with("{#"));
    }

    #[test]
    fn displays_with_field_names() {
        let data = Data::decode(&sample(false)).unwrap();
        let mut schema = Schema::default();
        for name in ["name", "damage", "speed", "tint", "kind", "tags", "next"] {
            schema.fields.push(crate::schema::Field {
                name_hash: fnv1a64(name.as_bytes()),
                name: name.into(),
                ty: String::new(),
                label: String::new(),
                group: String::new(),
                doc: String::new(),
                default: None,
                show_if: String::new(),
                hide_if: String::new(),
                widget: String::new(),
                min: None,
                max: None,
                step: None,
                flags: 0,
                order: None,
            });
        }
        assert_eq!(
            data.instances[0].value.named(&schema).to_string(),
            "{name: \"Sword\", damage: 12, speed: 1.5, tint: #ff8000ff, kind: melee, \
             tags: [true, null], next: ref \"sword\"}"
        );
    }

    #[test]
    fn rejects_missing_refs() {
        assert!(Data::decode(&sample(true)).is_err());
    }
}
