//! `.aschema.bin` compiled schemas: types, fields and enum values; see
//! docs/spec/aschema.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;

const FILE: &str = "aschema.bin";

/// [`Field::flags`] bits.
pub const OPTIONAL: u32 = 1;
pub const READONLY: u32 = 2;
pub const HAS_MIN: u32 = 4;
pub const HAS_MAX: u32 = 8;
pub const HAS_STEP: u32 = 16;
pub const HAS_DEFAULT: u32 = 32;
pub const HAS_ORDER: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    /// FNV-1a-64 of `name`.
    pub name_hash: u64,
    pub name: String,
    pub kind: TypeKind,
    /// A struct's first field in [`Schema::fields`], an enum's first value in
    /// [`Schema::values`].
    pub first: u32,
    /// Its fields or values.
    pub count: u32,
}

/// A struct's field with every setting; text settings are empty when unset.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// FNV-1a-64 of `name`.
    pub name_hash: u64,
    pub name: String,
    /// The type expression (`list<ref<Weapon>>`); the spec's `type`.
    pub ty: String,
    pub label: String,
    pub group: String,
    pub doc: String,
    /// Compact JSON in its canonical form (`10`, `"#c8c8c8"`); `None` without one.
    pub default: Option<String>,
    pub show_if: String,
    pub hide_if: String,
    pub widget: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    /// Bits: [`OPTIONAL`], [`READONLY`], [`HAS_MIN`], [`HAS_MAX`], [`HAS_STEP`],
    /// [`HAS_DEFAULT`], [`HAS_ORDER`].
    pub flags: u32,
    pub order: Option<f32>,
}

impl Field {
    pub fn optional(&self) -> bool {
        self.flags & OPTIONAL != 0
    }

    pub fn readonly(&self) -> bool {
        self.flags & READONLY != 0
    }
}

/// An enum's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValue {
    /// FNV-1a-64 of `name`.
    pub name_hash: u64,
    pub name: String,
}

/// A schema's types; structs' fields and enums' values are ranges of `fields` and `values`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Schema {
    /// In the schema's order.
    pub types: Vec<Type>,
    /// Every struct's fields, struct after struct, each in field order.
    pub fields: Vec<Field>,
    /// Every enum's values, enum after enum.
    pub values: Vec<EnumValue>,
}

impl Schema {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ASCHEMA", &[1], FILE)?;
        let desc = Bytes::new(file.require("DESC", FILE)?, FILE);
        let (type_count, field_count, value_count) = (
            desc.u32(0)? as usize,
            desc.u32(4)? as usize,
            desc.u32(8)? as usize,
        );
        let strings = Bytes::new(file.require("STRINGS", FILE)?, FILE);
        let types = Bytes::new(file.require("TYPES", FILE)?, FILE)
            .records_n(32, type_count)?
            .map(|r| {
                let kind = match r.u32(16)? {
                    0 => TypeKind::Struct,
                    1 => TypeKind::Enum,
                    other => return Err(r.err(format!("unknown type kind {other}"))),
                };
                let (first, count) = (r.u32(20)?, r.u32(24)?);
                let total = match kind {
                    TypeKind::Struct => field_count,
                    TypeKind::Enum => value_count,
                };
                if first as u64 + count as u64 > total as u64 {
                    return Err(r.err("a type's fields or values run past the end"));
                }
                Ok(Type {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    kind,
                    first,
                    count,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let fields = Bytes::new(file.require("FIELDS", FILE)?, FILE)
            .records_n(112, field_count)?
            .map(|r| {
                let flags = r.u32(104)?;
                let when = |bit: u32, v: f64| (flags & bit != 0).then_some(v);
                Ok(Field {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                    ty: r.text_ref(16, strings)?,
                    label: r.text_ref(24, strings)?,
                    group: r.text_ref(32, strings)?,
                    doc: r.text_ref(40, strings)?,
                    default: (flags & HAS_DEFAULT != 0).then_some(r.text_ref(48, strings)?),
                    show_if: r.text_ref(56, strings)?,
                    hide_if: r.text_ref(64, strings)?,
                    widget: r.text_ref(72, strings)?,
                    min: when(HAS_MIN, r.f64(80)?),
                    max: when(HAS_MAX, r.f64(88)?),
                    step: when(HAS_STEP, r.f64(96)?),
                    flags,
                    order: (flags & HAS_ORDER != 0).then_some(r.f32(108)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let values = Bytes::new(file.require("VALUES", FILE)?, FILE)
            .records_n(16, value_count)?
            .map(|r| {
                Ok(EnumValue {
                    name_hash: r.u64(0)?,
                    name: r.text_ref(8, strings)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            types,
            fields,
            values,
        })
    }

    /// The type named `name`.
    pub fn type_named(&self, name: &str) -> Option<&Type> {
        let hash = fnv1a64(name.as_bytes());
        self.types
            .iter()
            .find(|t| t.name_hash == hash && t.name == name)
    }

    /// A struct's fields (empty for an enum).
    pub fn fields_of(&self, ty: &Type) -> &[Field] {
        match ty.kind {
            TypeKind::Struct => self.range(&self.fields, ty),
            TypeKind::Enum => &[],
        }
    }

    /// An enum's values (empty for a struct).
    pub fn values_of(&self, ty: &Type) -> &[EnumValue] {
        match ty.kind {
            TypeKind::Enum => self.range(&self.values, ty),
            TypeKind::Struct => &[],
        }
    }

    /// The name of the field whose name hashes to `name_hash`, in any struct.
    pub fn field_name(&self, name_hash: u64) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.name_hash == name_hash)
            .map(|f| f.name.as_str())
    }

    fn range<'a, T>(&self, items: &'a [T], ty: &Type) -> &'a [T] {
        let first = ty.first as usize;
        items.get(first..first + ty.count as usize).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::graph::tests::text;

    fn sample(field_count: u32) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut types = Vec::new();
        for (name, kind, count) in [("Weapon", 0u32, 1u32), ("Kind", 1, 2)] {
            types.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            types.extend(text(&mut strings, name));
            for v in [kind, 0, count, 0] {
                types.extend(v.to_le_bytes());
            }
        }
        let mut fields = fnv1a64(b"damage").to_le_bytes().to_vec();
        for t in [
            "damage",
            "i32",
            "Damage",
            "Combat",
            "Hit points",
            "10",
            "",
            "",
            "slider",
        ] {
            fields.extend(text(&mut strings, t));
        }
        for v in [0.0f64, 100.0, 0.0] {
            fields.extend(v.to_le_bytes());
        }
        fields.extend((HAS_MAX | HAS_DEFAULT | READONLY | HAS_ORDER).to_le_bytes());
        fields.extend(2.0f32.to_le_bytes());
        let mut values = Vec::new();
        for name in ["melee", "ranged"] {
            values.extend(fnv1a64(name.as_bytes()).to_le_bytes());
            values.extend(text(&mut strings, name));
        }
        let mut desc = Vec::new();
        for v in [2u32, field_count, 2, 0] {
            desc.extend(v.to_le_bytes());
        }
        file(
            "ASCHEMA",
            1,
            &[
                ("DESC", &desc),
                ("TYPES", &types),
                ("FIELDS", &fields),
                ("VALUES", &values),
                ("STRINGS", &strings),
            ],
        )
    }

    #[test]
    fn decodes_types_fields_and_values() {
        let schema = Schema::decode(&sample(1)).unwrap();
        let weapon = schema.type_named("Weapon").unwrap();
        assert_eq!(weapon.kind, TypeKind::Struct);
        let field = &schema.fields_of(weapon)[0];
        assert_eq!((field.name.as_str(), field.ty.as_str()), ("damage", "i32"));
        assert_eq!(
            (field.label.as_str(), field.group.as_str()),
            ("Damage", "Combat")
        );
        assert_eq!(field.doc, "Hit points");
        assert_eq!(field.default.as_deref(), Some("10"));
        assert_eq!(
            (field.min, field.max, field.step),
            (None, Some(100.0), None)
        );
        assert_eq!((field.widget.as_str(), field.order), ("slider", Some(2.0)));
        assert!(field.readonly() && !field.optional());
        let kind = schema.type_named("Kind").unwrap();
        let names: Vec<&str> = schema
            .values_of(kind)
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(names, ["melee", "ranged"]);
        assert_eq!(schema.field_name(fnv1a64(b"damage")), Some("damage"));
    }

    #[test]
    fn rejects_fields_past_the_end() {
        assert!(Schema::decode(&sample(2)).is_err());
    }
}
