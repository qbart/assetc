//! `.adata` files: instances of a schema's structs in the a/1 property format; see
//! docs/aschema.md#4-data-file-adata, docs/spec/adata.md and docs/features/instances.md.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::aschema::{self, Body, DIALECT, Field, Schema, Ty, parse_type};

/// Whether `file` is a data file (`*.adata`, any case).
pub fn is_data(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("adata"))
}

/// A data file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataFile {
    /// `$schema`: the schema's path relative to this file's folder.
    pub schema: String,
    pub instances: Vec<Instance>,
    /// Other top-level keys, kept in their order.
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub ty: String,
    pub id: String,
    /// Other `$`-keys, kept in their order.
    pub meta: Map<String, Value>,
    /// Field values and keys that are not fields, in their order.
    pub values: Map<String, Value>,
}

/// An instance of the project, the data files sharing a schema; see
/// docs/features/instances.md#references.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub ty: String,
    pub id: String,
    /// Its file, input-relative with forward slashes.
    pub file: String,
    /// Its index in the file's instances.
    pub index: usize,
}

/// A data problem of one instance; see docs/features/instances.md#problems.
#[derive(Debug, Clone, PartialEq)]
pub struct DataProblem {
    pub instance: usize,
    /// The instance's field it is under, `None` for the instance itself.
    pub field: Option<String>,
    pub message: String,
    pub warning: bool,
}

impl DataFile {
    pub fn new(schema: String) -> Self {
        Self {
            schema,
            ..Self::default()
        }
    }

    /// Reads a data file; the errors that make it unusable as one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
        let Value::Object(mut top) = value else {
            return Err("not a JSON object".into());
        };
        aschema::check_dialect(&mut top)?;
        let schema = match top.shift_remove("$schema") {
            Some(Value::String(s)) => s,
            Some(_) => return Err("$schema is not a string".into()),
            None => return Err("no $schema".into()),
        };
        let Some(Value::Array(items)) = top.shift_remove("instances") else {
            return Err("no instances array".into());
        };
        let instances = items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let Value::Object(mut o) = item else {
                    return Err(format!("instance {i} is not an object"));
                };
                let mut text = |key: &str| match o.shift_remove(key) {
                    Some(Value::String(s)) => s,
                    _ => String::new(),
                };
                let (ty, id) = (text("$type"), text("$id"));
                let (meta, values) = o.into_iter().partition(|(k, _)| k.starts_with('$'));
                Ok(Instance {
                    ty,
                    id,
                    meta,
                    values,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            schema,
            instances,
            extra: top,
        })
    }

    /// The written form, normalised by `schema`; see docs/spec/adata.md#written-form.
    pub fn to_json(&self, schema: Option<&Schema>) -> String {
        let mut top = Map::new();
        top.insert("$dialect".into(), DIALECT.into());
        top.insert("$schema".into(), self.schema.clone().into());
        let instances = self
            .instances
            .iter()
            .map(|inst| {
                let mut o = Map::new();
                o.insert("$type".into(), inst.ty.clone().into());
                o.insert("$id".into(), inst.id.clone().into());
                o.extend(inst.meta.clone());
                let si = schema.and_then(|s| struct_index(s, &inst.ty).map(|si| (s, si)));
                match si {
                    Some((schema, si)) => o.extend(normalise(schema, si, &inst.values, None)),
                    None => o.extend(inst.values.clone()),
                }
                Value::Object(o)
            })
            .collect();
        top.insert("instances".into(), Value::Array(instances));
        top.extend(self.extra.clone());
        let mut text = serde_json::to_string_pretty(&Value::Object(top)).unwrap_or_default();
        text.push('\n');
        text
    }

    /// An id for a new instance of struct `ty`: its name in snake case, numbered while taken.
    pub fn free_id(&self, ty: &str) -> String {
        let mut stem = String::new();
        for (i, c) in ty.chars().enumerate() {
            if c.is_ascii_uppercase() && i > 0 {
                stem.push('_');
            }
            stem.push(c.to_ascii_lowercase());
        }
        if !aschema::is_identifier(&stem) {
            stem = "item".into();
        }
        let taken = |id: &str| self.instances.iter().any(|i| i.ty == ty && i.id == id);
        (1..)
            .map(|n| match n {
                1 => stem.clone(),
                n => format!("{stem}{n}"),
            })
            .find(|id| !taken(id))
            .expect("a free id")
    }

    /// Every data problem of [§5](docs/aschema.md#5-validation) one file can have, with
    /// asset references to files not among `assets` (the input's, from [`asset_files`]) when
    /// given, and ids and refs checked against `others`, the project's other files' instances.
    pub fn check_with(
        &self,
        schema: &Schema,
        assets: Option<&[String]>,
        others: &[Target],
    ) -> Vec<DataProblem> {
        let ids: HashSet<(&str, &str)> = self
            .instances
            .iter()
            .map(|i| (i.ty.as_str(), i.id.as_str()))
            .chain(others.iter().map(|t| (t.ty.as_str(), t.id.as_str())))
            .collect();
        let known = Known { ids, assets };
        let mut out = Vec::new();
        for (index, inst) in self.instances.iter().enumerate() {
            let mut add = |field: Option<String>, message: String, warning: bool| {
                out.push(DataProblem {
                    instance: index,
                    field,
                    message,
                    warning,
                });
            };
            if inst.id.is_empty() {
                add(None, "no $id".into(), false);
            } else if !aschema::is_identifier(&inst.id) {
                add(
                    None,
                    format!("$id {:?} is not an identifier", inst.id),
                    false,
                );
            }
            if self.instances[..index]
                .iter()
                .any(|o| o.ty == inst.ty && o.id == inst.id)
            {
                add(
                    None,
                    format!("{}/{} is used twice", inst.ty, inst.id),
                    false,
                );
            }
            if let Some(other) = others
                .iter()
                .find(|o| o.ty == inst.ty && o.id == inst.id && !inst.id.is_empty())
            {
                let message = format!("{}/{} is also in {}", inst.ty, inst.id, other.file);
                add(None, message, false);
            }
            let Some(si) = struct_index(schema, &inst.ty) else {
                add(
                    None,
                    format!("$type {:?} is not a struct of the schema", inst.ty),
                    false,
                );
                continue;
            };
            let mut found = Vec::new();
            check_struct(schema, si, &inst.values, &inst.id, &known, &mut found);
            for (field, message, warning) in found {
                add(field, message, warning);
            }
            // Required refs: a ref field without a default, not optional, left out or "".
            let Body::Struct { fields, .. } = &schema.types[si].body else {
                continue;
            };
            for f in fields {
                if !f.optional
                    && f.default.is_none()
                    && matches!(parse_type(&f.ty), Ok(Ty::Ref(_)))
                    && inst.values.get(&f.name).is_none_or(|v| v == "")
                {
                    let message = format!("{}/{}: the ref is not set", inst.id, f.name);
                    add(Some(f.name.clone()), message, true);
                }
            }
        }
        out
    }
}

/// The struct named `name`, if it is one.
pub fn struct_index(schema: &Schema, name: &str) -> Option<usize> {
    let si = schema.type_index(name)?;
    matches!(schema.types[si].body, Body::Struct { .. }).then_some(si)
}

/// The value field `f` inherits: the enclosing default's value for it (`outer`), over the
/// field's own default, else its type's implicit default (`null` when optional). See
/// docs/features/instances.md#values.
pub fn inherited(schema: &Schema, f: &Field, outer: Option<&Map<String, Value>>) -> Value {
    let ty = parse_type(&f.ty).ok();
    let own = f.default.clone().unwrap_or_else(|| match &ty {
        _ if f.optional => Value::Null,
        Some(ty) => aschema::implicit_value(schema, ty),
        None => Value::Null,
    });
    match outer.and_then(|o| o.get(&f.name)) {
        Some(Value::Object(over)) if own.is_object() => {
            let mut merged = own;
            merge(&mut merged, over);
            merged
        }
        Some(v) => v.clone(),
        None => own,
    }
}

/// Deep-merges `over` into `base`: objects key by key, anything else replaced.
fn merge(base: &mut Value, over: &Map<String, Value>) {
    let Value::Object(base) = base else {
        return;
    };
    for (k, v) in over {
        match (base.get_mut(k), v) {
            (Some(b @ Value::Object(_)), Value::Object(o)) => merge(b, o),
            _ => {
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

/// Whether two values are the same, numbers compared by value (`10` is `10.0`).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, a)| y.get(k).is_some_and(|b| same(a, b)))
        }
        _ => a == b,
    }
}

/// A value as written: canonical for its type, integers of integer types as integers.
fn written(ty: &Ty, value: &Value) -> Value {
    match (ty, value) {
        (ty, Value::Number(n)) if ty.is_integer() => match n.as_f64() {
            Some(f) if n.as_i64().is_none() && n.as_u64().is_none() && f.fract() == 0.0 => {
                (f as i64).into()
            }
            _ => value.clone(),
        },
        (Ty::List(t), Value::Array(items)) => {
            Value::Array(items.iter().map(|v| written(t, v)).collect())
        }
        _ => aschema::canonical(ty, value),
    }
}

/// The values of struct `si` that differ from what they inherit (`outer`: the enclosing
/// default's values), in field order, then keys that are not fields; see
/// docs/spec/adata.md#written-form.
pub fn normalise(
    schema: &Schema,
    si: usize,
    values: &Map<String, Value>,
    outer: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let Body::Struct { fields, .. } = &schema.types[si].body else {
        return values.clone();
    };
    let mut out = Map::new();
    for f in fields {
        let Some(v) = values.get(&f.name) else {
            continue;
        };
        let Ok(ty) = parse_type(&f.ty) else {
            out.insert(f.name.clone(), v.clone());
            continue;
        };
        let from = inherited(schema, f, outer);
        let inner = match &ty {
            Ty::Named(n) => struct_index(schema, n),
            _ => None,
        };
        let v = &sparse_items(schema, &ty, v);
        match (inner, v) {
            (Some(inner), Value::Object(o)) => {
                let kept = normalise(schema, inner, o, from.as_object());
                // A struct value where `null` is inherited is a value even with no keys.
                if !kept.is_empty() || from.is_null() {
                    out.insert(f.name.clone(), Value::Object(kept));
                }
            }
            _ => {
                let v = written(&ty, v);
                if !same(&v, &written(&ty, &from)) {
                    out.insert(f.name.clone(), v);
                }
            }
        }
    }
    for (k, v) in values {
        if !fields.iter().any(|f| f.name == *k) {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// A list of struct items, each written as a struct value against its struct's defaults
/// (an item with no keys stays `{}`); nested lists likewise; anything else as it is.
fn sparse_items(schema: &Schema, ty: &Ty, v: &Value) -> Value {
    let (Ty::List(item), Value::Array(items)) = (ty, v) else {
        return v.clone();
    };
    let inner = match &**item {
        Ty::Named(n) => struct_index(schema, n),
        _ => None,
    };
    Value::Array(
        items
            .iter()
            .map(|v| match (inner, v) {
                (Some(si), Value::Object(o)) => Value::Object(normalise(schema, si, o, None)),
                _ => sparse_items(schema, item, v),
            })
            .collect(),
    )
}

/// Every field of struct `si`: what `values` sets, else what it inherits (`outer`: the
/// enclosing default's values); nested structs and struct list items likewise. Keys that are
/// not fields are left out. See docs/spec/adata.md#built-form.
pub fn resolve(
    schema: &Schema,
    si: usize,
    values: &Map<String, Value>,
    outer: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let Body::Struct { fields, .. } = &schema.types[si].body else {
        return Map::new();
    };
    let mut out = Map::new();
    for f in fields {
        let from = inherited(schema, f, outer);
        let set = values.get(&f.name);
        let Ok(ty) = parse_type(&f.ty) else {
            out.insert(f.name.clone(), set.cloned().unwrap_or(from));
            continue;
        };
        let inner = match &ty {
            Ty::Named(n) => struct_index(schema, n),
            _ => None,
        };
        let value = match (inner, set) {
            (_, Some(Value::Null)) => Value::Null,
            (Some(inner), Some(Value::Object(o))) => {
                Value::Object(resolve(schema, inner, o, from.as_object()))
            }
            (Some(inner), None) if from.is_object() => {
                Value::Object(resolve(schema, inner, &Map::new(), from.as_object()))
            }
            (_, Some(v)) => resolve_items(schema, &ty, v),
            (_, None) => resolve_items(schema, &ty, &from),
        };
        out.insert(f.name.clone(), written(&ty, &value));
    }
    out
}

/// A list's struct items resolved against their struct's defaults; anything else as it is.
fn resolve_items(schema: &Schema, ty: &Ty, v: &Value) -> Value {
    let (Ty::List(item), Value::Array(items)) = (ty, v) else {
        return v.clone();
    };
    let inner = match &**item {
        Ty::Named(n) => struct_index(schema, n),
        _ => None,
    };
    Value::Array(
        items
            .iter()
            .map(|v| match (inner, v) {
                (Some(si), Value::Object(o)) => Value::Object(resolve(schema, si, o, None)),
                _ => resolve_items(schema, item, v),
            })
            .collect(),
    )
}

/// What values may point at: the file's instances, and the input's files when known.
struct Known<'a> {
    ids: HashSet<(&'a str, &'a str)>,
    assets: Option<&'a [String]>,
}

/// Problems of a struct value: `(top-level field, message, warning)`.
type Found = Vec<(Option<String>, String, bool)>;

fn check_struct(
    schema: &Schema,
    si: usize,
    values: &Map<String, Value>,
    path: &str,
    known: &Known,
    out: &mut Found,
) {
    let Body::Struct { fields, .. } = &schema.types[si].body else {
        return;
    };
    let top = |out: &mut Found, key: &str, message: String, warning: bool| {
        // Nested problems are shown on the instance's field they are under; the instance's
        // own unknown keys on the instance.
        let field = path.split('/').nth(1).map(str::to_owned);
        let _ = key;
        out.push((field, message, warning));
    };
    for (key, v) in values {
        let at = format!("{path}/{key}");
        let Some(f) = fields.iter().find(|f| f.name == *key) else {
            top(
                out,
                key,
                format!("{at}: not a field of {} (kept)", schema.types[si].name),
                true,
            );
            continue;
        };
        if v.is_null() && f.optional {
            continue;
        }
        let Ok(ty) = parse_type(&f.ty) else {
            continue;
        };
        check_value(schema, &ty, f, v, &at, known, out);
    }
}

fn check_value(
    schema: &Schema,
    ty: &Ty,
    f: &Field,
    v: &Value,
    at: &str,
    known: &Known,
    out: &mut Found,
) {
    let mut add = |message: String, warning: bool| {
        let field = at.split('/').nth(1).unwrap_or_default().to_owned();
        out.push((Some(field), format!("{at}: {message}"), warning));
    };
    match ty {
        Ty::Named(n) if struct_index(schema, n).is_some() => match v.as_object() {
            Some(o) => {
                let si = struct_index(schema, n).unwrap_or_default();
                check_struct(schema, si, o, at, known, out);
            }
            None => add(format!("{v} is not a {n}"), false),
        },
        Ty::List(item) => match v.as_array() {
            Some(items) => {
                for (i, v) in items.iter().enumerate() {
                    check_value(schema, item, f, v, &format!("{at}/[{i}]"), known, out);
                }
            }
            None => add(format!("{v} is not a {ty}"), false),
        },
        Ty::Asset(_) if schema.encodes(ty, v).is_ok() => {
            if let (Some(path), Some(assets)) = (v.as_str(), known.assets)
                && !path.is_empty()
                && !assets.iter().any(|a| a == path)
            {
                add(format!("no asset {path:?} in the input"), true);
            }
        }
        Ty::Ref(target) => match v.as_str() {
            Some("") => {}
            Some(id) => {
                let target = target.to_string();
                if !known.ids.contains(&(target.as_str(), id)) {
                    add(format!("no {target} {id:?} in the project"), true);
                }
            }
            None => add(format!("{v} is not a {ty}"), false),
        },
        _ => {
            if let Err(e) = schema.encodes(ty, v) {
                add(e, false);
            } else if let Some(n) = v.as_f64() {
                let below = f.min.as_ref().and_then(|m| m.as_f64()).filter(|m| n < *m);
                let above = f.max.as_ref().and_then(|m| m.as_f64()).filter(|m| n > *m);
                if let Some(m) = below {
                    add(format!("{v} is below the minimum {m}"), true);
                }
                if let Some(m) = above {
                    add(format!("{v} is above the maximum {m}"), true);
                }
            }
        }
    }
}

/// Every file `asset<K>` can name under `input`: its files and `*.atlas` folders (not
/// looked into), by input-relative path, sorted; entries starting with `.` are skipped.
pub fn asset_files(input: &Path) -> Vec<String> {
    fn walk(input: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let rel = || {
                path.strip_prefix(input)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default()
            };
            match e.file_type() {
                Ok(t) if t.is_dir() && crate::atlas::is_atlas(&path) => out.push(rel()),
                Ok(t) if t.is_dir() => walk(input, &path, out),
                Ok(t) if t.is_file() && aschema::asset_kind(&rel()).is_some() => out.push(rel()),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(input, input, &mut out);
    out.sort();
    out
}

/// Every `.aschema` under `input`, by input-relative path; entries starting with `.` are
/// skipped and symlinks are not followed, as in the file browser.
pub fn schemas_in(input: &Path) -> Vec<PathBuf> {
    files_in(input, aschema::is_schema)
}

/// The instances of every data file under `input` whose `$schema` is the schema at `schema`,
/// by file path, then file order; see docs/features/instances.md#references.
pub fn project(input: &Path, schema: &Path) -> Vec<Target> {
    let schema = schema
        .canonicalize()
        .unwrap_or_else(|_| schema.to_path_buf());
    let mut out = Vec::new();
    for file in files_in(input, is_data) {
        let Some(data) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| DataFile::parse(&text).ok())
        else {
            continue;
        };
        let dir = file.parent().unwrap_or(Path::new(""));
        let names = dir.join(&data.schema);
        if data.schema.is_empty() || names.canonicalize().unwrap_or(names) != schema {
            continue;
        }
        let rel = file
            .strip_prefix(input)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        out.extend(
            data.instances
                .into_iter()
                .enumerate()
                .map(|(index, i)| Target {
                    ty: i.ty,
                    id: i.id,
                    file: rel.clone(),
                    index,
                }),
        );
    }
    out.sort_by(|a, b| (&a.file, a.index).cmp(&(&b.file, b.index)));
    out
}

/// The instances of the project of `data` (read from `file`) in files other than `file`.
pub fn others(input: &Path, file: &Path, data: &DataFile) -> Vec<Target> {
    let (schema, _) = load_schema(file, data);
    let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    project(input, &schema)
        .into_iter()
        .filter(|t| {
            let path = input.join(&t.file);
            path.canonicalize().unwrap_or(path) != file
        })
        .collect()
}

/// Every file under `input` that `keep` takes, sorted by path; entries starting with `.` are
/// skipped and symlinks are not followed, as in the file browser.
fn files_in(input: &Path, keep: fn(&Path) -> bool) -> Vec<PathBuf> {
    fn walk(dir: &Path, keep: fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let path = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            match e.file_type() {
                Ok(t) if t.is_dir() => walk(&path, keep, out),
                Ok(t) if t.is_file() && keep(&path) => out.push(path),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(input, keep, &mut out);
    out
}

/// `target` relative to folder `from`, with forward slashes (`../defs/game.aschema`).
pub fn relative(from: &Path, target: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = target.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

/// The schema a data file at `file` names, read: its path, then the schema or why not.
pub fn load_schema(file: &Path, data: &DataFile) -> (PathBuf, Result<Schema, String>) {
    let dir = file.parent().unwrap_or(Path::new(""));
    let path = dir.join(&data.schema);
    let schema = if data.schema.is_empty() {
        Err("no $schema".into())
    } else {
        std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| Schema::parse(&text))
    };
    (path, schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r##"{
      "$dialect": "a/1",
      "types": {
        "Vec2": { "kind": "struct", "fields": [
          { "name": "x", "type": "f32" }, { "name": "y", "type": "f32" } ] },
        "Weapon": { "kind": "struct", "fields": [
          { "name": "name", "type": "string" },
          { "name": "damage", "type": "i32", "default": 10, "min": 0, "max": 99 },
          { "name": "offset", "type": "Vec2", "default": { "x": 0.5 } },
          { "name": "tint", "type": "rgb" },
          { "name": "notes", "type": "string", "optional": true }
        ] },
        "Enemy": { "kind": "struct", "fields": [
          { "name": "weapon", "type": "ref<Weapon>" }
        ] }
      }
    }"##;

    fn schema() -> Schema {
        Schema::parse(SCHEMA).unwrap()
    }

    #[test]
    fn writes_sparse_canonical_instances() {
        let text = r##"{
          "$dialect": "a/1", "$schema": "game.aschema",
          "instances": [
            { "x_note": 1, "$id": "sword", "$type": "Weapon", "damage": 10.0,
              "tint": "#FF0000", "offset": { "x": 0.5, "y": 2 }, "name": "Sword" },
            { "$type": "Weapon", "$id": "stick", "offset": { "x": 0.5 }, "notes": null }
          ]
        }"##;
        let data = DataFile::parse(text).unwrap();
        let out = data.to_json(Some(&schema()));
        let back: Value = serde_json::from_str(&out).unwrap();
        let sword = &back["instances"][0];
        let keys: Vec<&str> = sword
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["$type", "$id", "name", "offset", "tint", "x_note"]);
        assert_eq!(sword["offset"], serde_json::json!({ "y": 2 }));
        assert_eq!(sword["tint"], "#ff0000");
        // Defaults and inherited nulls are left out.
        let stick = back["instances"][1].as_object().unwrap();
        assert_eq!(stick.len(), 2);
        assert!(out.ends_with("}\n"));
        // Struct list items: field order, defaults left out, empty ones kept.
        let enemy = r##"{ "$dialect": "a/1", "$schema": "g.aschema", "instances": [
            { "$type": "Squad", "$id": "s", "members": [ { "tint": "#000000", "name": "A" }, {} ] } ] }"##;
        let schema = Schema::parse(r##"{ "$dialect": "a/1", "types": {
            "Weapon": { "kind": "struct", "fields": [
              { "name": "name", "type": "string" }, { "name": "tint", "type": "rgb" } ] },
            "Squad": { "kind": "struct", "fields": [ { "name": "members", "type": "list<Weapon>" } ] }
          } }"##).unwrap();
        let out = DataFile::parse(enemy).unwrap().to_json(Some(&schema));
        let back: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            back["instances"][0]["members"],
            serde_json::json!([{ "name": "A" }, {}])
        );
    }

    #[test]
    fn refs_reach_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path();
        std::fs::create_dir_all(input.join("enemies")).unwrap();
        std::fs::write(input.join("game.aschema"), SCHEMA).unwrap();
        std::fs::write(input.join("other.aschema"), SCHEMA).unwrap();
        let file = |schema: &str, instances: &str| {
            format!(r#"{{ "$dialect": "a/1", "$schema": "{schema}", "instances": [{instances}] }}"#)
        };
        let weapons = file(
            "game.aschema",
            r#"{ "$type": "Weapon", "$id": "sword" }, { "$type": "Weapon", "$id": "axe" }"#,
        );
        std::fs::write(input.join("weapons.adata"), weapons).unwrap();
        // Another schema's data is not of the project.
        let elsewhere = file("other.aschema", r#"{ "$type": "Weapon", "$id": "bow" }"#);
        std::fs::write(input.join("elsewhere.adata"), elsewhere).unwrap();
        let enemies = file(
            "../game.aschema",
            r#"{ "$type": "Enemy", "$id": "goblin", "weapon": "axe" },
               { "$type": "Enemy", "$id": "orc", "weapon": "bow" },
               { "$type": "Weapon", "$id": "sword" }"#,
        );
        let path = input.join("enemies/goblins.adata");
        std::fs::write(&path, &enemies).unwrap();

        let project = project(input, &input.join("game.aschema"));
        let found: Vec<(String, String, usize)> = project
            .iter()
            .map(|t| (t.file.clone(), t.id.clone(), t.index))
            .collect();
        assert_eq!(
            found[0],
            ("enemies/goblins.adata".into(), "goblin".into(), 0)
        );
        assert_eq!(found[3], ("weapons.adata".into(), "sword".into(), 0));
        assert_eq!(found.len(), 5);

        let data = DataFile::parse(&enemies).unwrap();
        let others = others(input, &path, &data);
        assert_eq!(others.len(), 2);
        let messages: Vec<String> = data
            .check_with(&schema(), None, &others)
            .into_iter()
            .map(|p| p.message)
            .collect();
        assert_eq!(
            messages,
            [
                r#"orc/weapon: no Weapon "bow" in the project"#,
                "Weapon/sword is also in weapons.adata",
            ]
        );

        let bytes =
            crate::compiled::encode_data(&data, &schema(), "enemies/goblins.adata", &others);
        let reader = crate::chunks::Reader::parse(&bytes, crate::compiled::ADATA).unwrap();
        let refs = reader.require("REFS").unwrap();
        assert_eq!(refs.len(), 24);
        let u64_at = |at: usize| u64::from_le_bytes(refs[at..at + 8].try_into().unwrap());
        assert_eq!(u64_at(0), aschema::asset_id("weapons.adata"));
        assert_eq!(u64_at(8), crate::hash::fnv1a64(b"axe"));
        assert_eq!(refs[16], 1);
    }

    #[test]
    fn built_data_is_resolved() {
        let schema = schema();
        let values = serde_json::json!({ "x_old": 1, "offset": { "y": 2 }, "notes": null });
        let resolved = resolve(&schema, 1, values.as_object().unwrap(), None);
        assert_eq!(
            Value::Object(resolved.clone()),
            serde_json::json!({
                "name": "", "damage": 10, "offset": { "x": 0.5, "y": 2 },
                "tint": "#000000", "notes": null
            })
        );
        let keys: Vec<&str> = resolved.keys().map(String::as_str).collect();
        assert_eq!(keys, ["name", "damage", "offset", "tint", "notes"]);
    }

    #[test]
    fn values_inherit_from_enclosing_defaults() {
        let schema = schema();
        let Body::Struct { fields, .. } = &schema.types[1].body else {
            panic!()
        };
        let offset = inherited(&schema, &fields[2], None);
        assert_eq!(offset, serde_json::json!({ "x": 0.5 }));
        let Body::Struct { fields: vec2, .. } = &schema.types[0].body else {
            panic!()
        };
        assert_eq!(inherited(&schema, &vec2[0], offset.as_object()), 0.5);
        assert_eq!(inherited(&schema, &vec2[1], offset.as_object()), 0);
        assert_eq!(inherited(&schema, &fields[4], None), Value::Null);
    }

    #[test]
    fn problems_follow_the_schema() {
        let text = r##"{
          "$dialect": "a/1", "$schema": "game.aschema",
          "instances": [
            { "$type": "Weapon", "$id": "a", "damage": 120, "tint": "red", "old": 1,
              "offset": { "z": 1 } },
            { "$type": "Weapon", "$id": "a" },
            { "$type": "Vec3", "$id": "b" },
            { "$type": "Enemy", "$id": "c", "weapon": "nope" },
            { "$type": "Enemy", "$id": "d" }
          ]
        }"##;
        let problems = DataFile::parse(text)
            .unwrap()
            .check_with(&schema(), None, &[]);
        let has = |i: usize, part: &str, warning: bool| {
            problems
                .iter()
                .any(|p| p.instance == i && p.message.contains(part) && p.warning == warning)
        };
        assert!(has(0, "above the maximum", true));
        assert!(has(0, "\"red\" is not a rgb", false));
        assert!(has(0, "a/old: not a field", true));
        assert!(has(0, "a/offset/z: not a field", true));
        assert!(has(1, "used twice", false));
        assert!(has(2, "not a struct", false));
        assert!(has(3, "no Weapon \"nope\"", true));
        assert!(has(4, "ref is not set", true));
        assert!(
            problems
                .iter()
                .any(|p| p.field.as_deref() == Some("offset"))
        );
    }

    #[test]
    fn ids_and_paths() {
        let mut data = DataFile::new("game.aschema".into());
        assert_eq!(data.free_id("FireBall"), "fire_ball");
        data.instances.push(Instance {
            ty: "FireBall".into(),
            id: "fire_ball".into(),
            meta: Map::new(),
            values: Map::new(),
        });
        assert_eq!(data.free_id("FireBall"), "fire_ball2");
        assert_eq!(
            relative(Path::new("in/levels"), Path::new("in/defs/game.aschema")),
            "../defs/game.aschema"
        );
        assert_eq!(
            relative(Path::new("in"), Path::new("in/game.aschema")),
            "game.aschema"
        );
    }
}
