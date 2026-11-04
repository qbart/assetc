//! Structs attached to assets: what is attached to each scope, every owner's values, their
//! problems, their TOML form and their chunks; see docs/features/structs.md and
//! docs/spec/astruct.md.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::adata::{self, Target};
use crate::aschema::Schema;

/// A struct attached to a scope: its schema, relative to the folder of the file holding the
/// attachment, and the struct's name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attachment {
    pub schema: String,
    pub ty: String,
    /// Other keys of a JSON attachment, kept in their order.
    pub extra: Map<String, Value>,
}

impl Attachment {
    pub fn new(schema: String, ty: String) -> Self {
        Self {
            schema,
            ty,
            extra: Map::new(),
        }
    }
}

/// An owner's values: per attached struct's name, the fields it sets.
pub type Values = Map<String, Value>;

/// The scope numbers of docs/spec/astruct.md#attached-40-bytes.
pub const ASSET: u32 = 0;
pub const ITEM: u32 = 1;
pub const EDGE: u32 = 2;

/// One scope of an asset: what is attached to it and every owner's values.
pub struct Scope<'a> {
    pub scope: u32,
    pub attachments: &'a [Attachment],
    pub owners: Vec<Owner<'a>>,
}

/// An owner of a scope: its index in its chunk, a label for messages (`node dash`) and its
/// values.
pub struct Owner<'a> {
    pub item: u32,
    pub label: String,
    pub values: &'a Values,
}

// JSON files.

/// Attachments from a JSON array; `key` names it in errors.
pub fn parse_attachments(value: Value, key: &str) -> Result<Vec<Attachment>, String> {
    let Value::Array(items) = value else {
        return Err(format!("{key} is not an array"));
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let Value::Object(mut o) = item else {
                return Err(format!("{key} {}: not an object", i + 1));
            };
            let mut text = |name: &str| match o.shift_remove(name) {
                Some(Value::String(s)) => Ok(s),
                _ => Err(format!("{key} {}: {name} is not a string", i + 1)),
            };
            Ok(Attachment {
                schema: text("schema")?,
                ty: text("type")?,
                extra: o,
            })
        })
        .collect()
}

pub fn attachments_value(list: &[Attachment]) -> Value {
    Value::Array(
        list.iter()
            .map(|a| {
                let mut o = Map::new();
                o.insert("schema".into(), a.schema.clone().into());
                o.insert("type".into(), a.ty.clone().into());
                o.extend(a.extra.clone());
                Value::Object(o)
            })
            .collect(),
    )
}

/// An owner's `values` from JSON: an object of objects.
pub fn parse_values(value: Value) -> Result<Values, String> {
    let Value::Object(o) = value else {
        return Err("values is not an object".into());
    };
    for (ty, v) in &o {
        if !v.is_object() {
            return Err(format!("values {ty:?} is not an object"));
        }
    }
    Ok(o)
}

/// Takes `key`'s attachments out of a JSON object; none without the key.
pub fn take_attachments(o: &mut Map<String, Value>, key: &str) -> Result<Vec<Attachment>, String> {
    o.shift_remove(key)
        .map(|v| parse_attachments(v, key))
        .transpose()
        .map(|a| a.unwrap_or_default())
}

/// Takes `values` out of a JSON object; empty without the key.
pub fn take_values(o: &mut Map<String, Value>) -> Result<Values, String> {
    o.shift_remove("values")
        .map(parse_values)
        .transpose()
        .map(|v| v.unwrap_or_default())
}

/// Puts `key`'s attachments into a JSON object, when there are any.
pub fn put_attachments(o: &mut Map<String, Value>, key: &str, list: &[Attachment]) {
    if !list.is_empty() {
        o.insert(key.into(), attachments_value(list));
    }
}

/// Puts `values` into a JSON object, when there are any.
pub fn put_values(o: &mut Map<String, Value>, values: &Values) {
    if !values.is_empty() {
        o.insert("values".into(), Value::Object(values.clone()));
    }
}

// TOML files; see docs/features/structs.md#values-in-toml.

/// A TOML value as a/1 data: `{ "$null" = true }` is `null`.
pub fn from_toml(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::from(i),
        toml::Value::Float(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Array(items.into_iter().map(from_toml).collect()),
        toml::Value::Table(t) if t.len() == 1 && t.get("$null") == Some(&true.into()) => {
            Value::Null
        }
        toml::Value::Table(t) => {
            Value::Object(t.into_iter().map(|(k, v)| (k, from_toml(v))).collect())
        }
    }
}

/// a/1 data as a TOML value: `null` as `{ "$null" = true }`.
pub fn to_toml(value: &Value) -> toml::Value {
    match value {
        Value::Null => {
            let mut t = toml::Table::new();
            t.insert("$null".into(), true.into());
            toml::Value::Table(t)
        }
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) if !n.is_f64() => i.into(),
            _ => n.as_f64().unwrap_or(0.0).into(),
        },
        Value::String(s) => s.clone().into(),
        Value::Array(items) => toml::Value::Array(items.iter().map(to_toml).collect()),
        Value::Object(o) => {
            toml::Value::Table(o.iter().map(|(k, v)| (k.clone(), to_toml(v))).collect())
        }
    }
}

/// Takes `[[key]]`'s attachments out of a TOML table; none without the key.
pub fn take_toml_attachments(
    table: &mut toml::Table,
    key: &str,
) -> Result<Vec<Attachment>, String> {
    let Some(value) = table.remove(key) else {
        return Ok(Vec::new());
    };
    let toml::Value::Array(items) = value else {
        return Err(format!("{key} is not an array of tables"));
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let toml::Value::Table(mut t) = item else {
                return Err(format!("{key} {}: not a table", i + 1));
            };
            let mut text = |name: &str| match t.remove(name) {
                Some(toml::Value::String(s)) => Ok(s),
                _ => Err(format!("{key} {}: {name} is not a string", i + 1)),
            };
            let attachment = Attachment::new(text("schema")?, text("type")?);
            match t.keys().next() {
                Some(other) => Err(format!("{key} {}: unknown key {other}", i + 1)),
                None => Ok(attachment),
            }
        })
        .collect()
}

/// Takes `values` out of a TOML table: a table of tables; empty without the key.
pub fn take_toml_values(table: &mut toml::Table) -> Result<Values, String> {
    let Some(value) = table.remove("values") else {
        return Ok(Values::new());
    };
    let toml::Value::Table(t) = value else {
        return Err("values is not a table".into());
    };
    t.into_iter()
        .map(|(ty, v)| match v {
            toml::Value::Table(_) => Ok((ty, from_toml(v))),
            _ => Err(format!("values.{ty} is not a table")),
        })
        .collect()
}

/// Appends `[[key]]` tables, each after a blank line.
pub fn write_toml_attachments(out: &mut String, key: &str, list: &[Attachment]) {
    for a in list {
        out.push_str(&format!("\n[[{key}]]\n"));
        out.push_str(&format!(
            "schema = {}\n",
            toml::Value::from(a.schema.as_str())
        ));
        out.push_str(&format!("type = {}\n", toml::Value::from(a.ty.as_str())));
    }
}

/// Appends a `[<prefix>.<Type>]` table per struct with values, each after a blank line, its
/// fields as inline values.
pub fn write_toml_values(out: &mut String, prefix: &str, values: &Values) {
    for (ty, v) in values {
        let Value::Object(fields) = v else {
            continue;
        };
        if fields.is_empty() {
            continue;
        }
        out.push_str(&format!("\n[{prefix}.{}]\n", toml_key(ty)));
        for (name, v) in fields {
            out.push_str(&format!("{} = {}\n", toml_key(name), to_toml(v)));
        }
    }
}

/// A bare key when it can be one, else quoted.
fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_owned()
    } else {
        toml::Value::from(key).to_string()
    }
}

// Schemas, problems and building.

/// An attachment's schema as read, with the project's instances its refs can name.
pub struct Loaded {
    pub schema: Result<Schema, String>,
    pub targets: Vec<Target>,
    /// Input-relative, for the schema's asset ID.
    pub rel: String,
}

/// The schemas of attachments, by the path they are written with.
pub type Schemas = HashMap<String, Loaded>;

/// Reads the schema of every attachment in `scopes`, relative to `dir` (the folder of the file
/// holding them); refs reach the projects under `input`.
pub fn load(dir: &Path, input: Option<&Path>, scopes: &[Scope]) -> Schemas {
    let mut out = Schemas::new();
    for a in scopes.iter().flat_map(|s| s.attachments) {
        if out.contains_key(&a.schema) {
            continue;
        }
        let path = dir.join(&a.schema);
        let schema = if a.schema.is_empty() {
            Err("no schema".into())
        } else {
            std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| Schema::parse(&text))
        };
        let targets = match input {
            Some(input) if schema.is_ok() => adata::project(input, &path),
            _ => Vec::new(),
        };
        let rel = input
            .and_then(|input| {
                let path = path.canonicalize().ok()?;
                let input = input.canonicalize().ok()?;
                Some(
                    path.strip_prefix(input)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/"),
                )
            })
            .unwrap_or_else(|| a.schema.clone());
        out.insert(
            a.schema.clone(),
            Loaded {
                schema,
                targets,
                rel,
            },
        );
    }
    out
}

/// The struct an attachment names: its schema and index, or why not.
pub fn resolve_attachment<'a>(
    a: &Attachment,
    schemas: &'a Schemas,
) -> Result<(&'a Schema, usize), String> {
    let loaded = schemas
        .get(&a.schema)
        .ok_or_else(|| format!("{}: schema {} is not loaded", a.ty, a.schema))?;
    let schema = loaded
        .schema
        .as_ref()
        .map_err(|e| format!("{}: schema {}: {e}", a.ty, a.schema))?;
    let si = adata::struct_index(schema, &a.ty)
        .ok_or_else(|| format!("{} is not a struct of {}", a.ty, a.schema))?;
    Ok((schema, si))
}

/// A problem of attached structs; see docs/features/structs.md#problems.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub scope: u32,
    /// The attachment's place in its scope, for its own problems; `None` for an owner's.
    pub attachment: Option<usize>,
    /// The owner's label, for problems of its values.
    pub owner: Option<String>,
    pub ty: String,
    /// The struct's top-level field the problem is under.
    pub field: Option<String>,
    pub message: String,
    pub warning: bool,
}

impl Problem {
    /// The message with its owner: `node dash: Step/cost: …`.
    pub fn text(&self) -> String {
        match &self.owner {
            Some(owner) => format!("{owner}: {}", self.message),
            None => self.message.clone(),
        }
    }
}

/// Every problem of `scopes`; asset paths are checked against `assets` when given.
pub fn check(scopes: &[Scope], schemas: &Schemas, assets: Option<&[String]>) -> Vec<Problem> {
    let mut out = Vec::new();
    for scope in scopes {
        for (k, a) in scope.attachments.iter().enumerate() {
            let own = |message: String| Problem {
                scope: scope.scope,
                attachment: Some(k),
                owner: None,
                ty: a.ty.clone(),
                field: None,
                message,
                warning: false,
            };
            if let Err(e) = resolve_attachment(a, schemas) {
                out.push(own(e));
            }
            if scope.attachments[..k].iter().any(|o| o.ty == a.ty) {
                out.push(own(format!("{} is attached twice", a.ty)));
            }
        }
        for owner in &scope.owners {
            for (ty, v) in owner.values {
                if !scope.attachments.iter().any(|a| a.ty == *ty) {
                    out.push(Problem {
                        scope: scope.scope,
                        attachment: None,
                        owner: Some(owner.label.clone()),
                        ty: ty.clone(),
                        field: None,
                        message: format!("values for {ty}, which is not attached"),
                        warning: true,
                    });
                    continue;
                }
                let a = scope.attachments.iter().find(|a| a.ty == *ty);
                let (Some(a), Some(fields)) = (a, v.as_object()) else {
                    continue;
                };
                let Ok((schema, si)) = resolve_attachment(a, schemas) else {
                    continue;
                };
                let targets = &schemas[&a.schema].targets;
                let ids: HashSet<(&str, &str)> = targets
                    .iter()
                    .map(|t| (t.ty.as_str(), t.id.as_str()))
                    .collect();
                for (field, message, warning) in
                    adata::check_values(schema, si, fields, ty, ids, assets)
                {
                    out.push(Problem {
                        scope: scope.scope,
                        attachment: None,
                        owner: Some(owner.label.clone()),
                        ty: ty.clone(),
                        field,
                        message,
                        warning,
                    });
                }
            }
        }
    }
    out
}

/// The struct chunks of `scopes` (none without attachments), or the first error; see
/// docs/features/structs.md#build. `dir` is the folder of the file holding the attachments.
pub fn compile(
    dir: &Path,
    input: &Path,
    scopes: &[Scope],
) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    if scopes.iter().all(|s| s.attachments.is_empty()) {
        return Ok(Vec::new());
    }
    let schemas = load(dir, Some(input), scopes);
    let assets = adata::asset_files(input);
    let errors: Vec<Problem> = check(scopes, &schemas, Some(&assets))
        .into_iter()
        .filter(|p| !p.warning)
        .collect();
    if let Some(first) = errors.first() {
        let mut message = first.text();
        if errors.len() > 1 {
            message += &format!(" (and {} more)", errors.len() - 1);
        }
        return Err(message);
    }
    Ok(crate::compiled::encode_structs(scopes, &schemas))
}

/// The schema's `.aschema.bin` asset ID, from its input-relative path.
pub fn schema_id(rel: &str) -> u64 {
    let stem = rel.rsplit_once('.').map_or(rel, |(s, _)| s);
    crate::hash::fnv1a64(format!("{stem}.aschema.bin").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_values_round_trip() {
        let json: Value = serde_json::json!({
            "mass": 2.5,
            "lives": 3,
            "tags": ["a", "b"],
            "stats": { "hp": 40 },
            "nothing": null,
            "curve": [[0.0, 0.0, 1.0, 1.0]],
        });
        let toml = to_toml(&json);
        assert_eq!(from_toml(toml.clone()), json);
        let mut values = Values::new();
        values.insert("Pickup".into(), json);
        let mut out = String::new();
        write_toml_values(&mut out, "values", &values);
        assert!(
            out.starts_with("\n[values.Pickup]\nmass = 2.5\nlives = 3\n"),
            "{out}"
        );
        assert!(out.contains("nothing = { \"$null\" = true }\n"), "{out}");
        let mut table: toml::Table = out.parse().unwrap();
        let back = take_toml_values(&mut table).unwrap();
        assert_eq!(back, values);
    }

    #[test]
    fn toml_attachments_round_trip() {
        let list = vec![
            Attachment::new("game.aschema".into(), "Pickup".into()),
            Attachment::new("../defs/x.aschema".into(), "Loot".into()),
        ];
        let mut out = String::new();
        write_toml_attachments(&mut out, "structs", &list);
        let mut table: toml::Table = out.parse().unwrap();
        assert_eq!(take_toml_attachments(&mut table, "structs").unwrap(), list);
        let mut bad: toml::Table = "[[structs]]\nschema = \"a\"\ntype = \"B\"\nx = 1\n"
            .parse()
            .unwrap();
        assert!(take_toml_attachments(&mut bad, "structs").is_err());
    }

    #[test]
    fn problems_and_chunks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("game.aschema"),
            r#"{ "$dialect": "a/1", "types": {
                "Step": { "kind": "struct", "fields": [
                    { "name": "cost", "type": "i32", "default": 1, "max": 10 },
                    { "name": "name", "type": "string" }
                ] },
                "Mode": { "kind": "enum", "values": ["a"] }
            } }"#,
        )
        .unwrap();
        let attachments = vec![
            Attachment::new("game.aschema".into(), "Step".into()),
            Attachment::new("game.aschema".into(), "Mode".into()),
            Attachment::new("nope.aschema".into(), "X".into()),
        ];
        let mut values = Values::new();
        values.insert("Step".into(), serde_json::json!({ "cost": 12 }));
        values.insert("Other".into(), serde_json::json!({}));
        let scopes = [Scope {
            scope: ITEM,
            attachments: &attachments,
            owners: vec![Owner {
                item: 0,
                label: "node a".into(),
                values: &values,
            }],
        }];
        let schemas = load(dir.path(), Some(dir.path()), &scopes);
        let problems: Vec<String> = check(&scopes, &schemas, None)
            .iter()
            .map(|p| format!("{}{}", if p.warning { "w " } else { "" }, p.text()))
            .collect();
        assert_eq!(
            problems,
            [
                "Mode is not a struct of game.aschema",
                "X: schema nope.aschema: No such file or directory (os error 2)",
                "w node a: Step/cost: 12 is above the maximum 10",
                "w node a: values for Other, which is not attached",
            ]
        );
        let err = compile(dir.path(), dir.path(), &scopes).unwrap_err();
        assert_eq!(err, "Mode is not a struct of game.aschema (and 1 more)");

        // Only Step: one record per owner, resolved.
        let attachments = &attachments[..1];
        let none = Values::new();
        let scopes = [Scope {
            scope: ITEM,
            attachments,
            owners: vec![
                Owner {
                    item: 0,
                    label: "node a".into(),
                    values: &values,
                },
                Owner {
                    item: 1,
                    label: "node b".into(),
                    values: &none,
                },
            ],
        }];
        let chunks = compile(dir.path(), dir.path(), &scopes).unwrap();
        let names: Vec<_> = chunks.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["STRUCTS", "SVALUES", "SREFS", "SSTRINGS"]);
        let records = &chunks[0].1;
        assert_eq!(records.len(), 80);
        // The second owner: item 1, its value after the first's struct and fields.
        assert_eq!(u32::from_le_bytes(records[44..48].try_into().unwrap()), 1);
        assert_eq!(
            u64::from_le_bytes(records[64..72].try_into().unwrap()),
            schema_id("game.aschema")
        );
    }
}
