//! `.aschema` files: struct and enum types in the a/1 property format; see docs/aschema.md,
//! docs/spec/aschema.md and docs/features/schemas.md.

use std::path::Path;

use serde_json::{Map, Number, Value};

pub const DIALECT: &str = "a/1";

/// Whether `file` is a schema (`*.aschema`, any case).
pub fn is_schema(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("aschema"))
}

/// A schema file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Schema {
    pub types: Vec<TypeDef>,
    /// Other top-level keys, kept in their order.
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub doc: String,
    pub body: Body,
    /// Other keys of the type, kept.
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Struct {
        fields: Vec<Field>,
        groups: Vec<Group>,
    },
    Enum {
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    pub name: String,
    pub collapsed: bool,
    pub doc: String,
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Field {
    pub name: String,
    /// The type expression, as written.
    pub ty: String,
    /// The field may hold `null`; see docs/aschema.md#optional-fields.
    pub optional: bool,
    pub default: Option<Value>,
    pub min: Option<Number>,
    pub max: Option<Number>,
    pub step: Option<Number>,
    pub doc: String,
    pub group: String,
    pub order: Option<Number>,
    pub label: String,
    pub readonly: bool,
    pub show_if: String,
    pub hide_if: String,
    pub widget: String,
    pub extra: Map<String, Value>,
}

impl Field {
    pub fn new(name: String, ty: &str) -> Self {
        Self {
            name,
            ty: ty.to_owned(),
            ..Self::default()
        }
    }
}

impl TypeDef {
    pub fn new_struct(name: String) -> Self {
        Self {
            name,
            doc: String::new(),
            body: Body::Struct {
                fields: Vec::new(),
                groups: Vec::new(),
            },
            extra: Map::new(),
        }
    }

    pub fn new_enum(name: String) -> Self {
        Self {
            name,
            doc: String::new(),
            body: Body::Enum {
                values: vec!["value".into()],
            },
            extra: Map::new(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self.body {
            Body::Struct { .. } => "struct",
            Body::Enum { .. } => "enum",
        }
    }
}

impl Schema {
    /// One struct `Item` with a `name: string`; see docs/features/schemas.md#creating.
    pub fn new() -> Self {
        let mut item = TypeDef::new_struct("Item".into());
        if let Body::Struct { fields, .. } = &mut item.body {
            fields.push(Field::new("name".into(), "string"));
        }
        Self {
            types: vec![item],
            extra: Map::new(),
        }
    }

    pub fn type_index(&self, name: &str) -> Option<usize> {
        self.types.iter().position(|t| t.name == name)
    }

    /// Reads a schema; the errors that make a file unusable as one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
        let Value::Object(mut top) = value else {
            return Err("not a JSON object".into());
        };
        check_dialect(&mut top)?;
        let Some(Value::Object(types)) = top.shift_remove("types") else {
            return Err("no types object".into());
        };
        let types = types
            .into_iter()
            .map(|(name, def)| parse_type_def(name, def))
            .collect::<Result<_, _>>()?;
        Ok(Self { types, extra: top })
    }

    /// The written form; see docs/spec/aschema.md#written-form.
    pub fn to_json(&self) -> String {
        let mut top = Map::new();
        top.insert("$dialect".into(), DIALECT.into());
        let types: Map<String, Value> = self
            .types
            .iter()
            .map(|t| (t.name.clone(), self.write_type(t)))
            .collect();
        top.insert("types".into(), Value::Object(types));
        for (k, v) in &self.extra {
            top.insert(k.clone(), v.clone());
        }
        let mut text = serde_json::to_string_pretty(&Value::Object(top)).unwrap_or_default();
        text.push('\n');
        text
    }

    fn write_type(&self, t: &TypeDef) -> Value {
        let mut out = Map::new();
        out.insert("kind".into(), t.kind().into());
        if !t.doc.is_empty() {
            out.insert("doc".into(), t.doc.clone().into());
        }
        match &t.body {
            Body::Struct { fields, groups } => {
                if !groups.is_empty() {
                    let groups: Map<String, Value> = groups
                        .iter()
                        .map(|g| {
                            let mut o = Map::new();
                            if g.collapsed {
                                o.insert("collapsed".into(), true.into());
                            }
                            if !g.doc.is_empty() {
                                o.insert("doc".into(), g.doc.clone().into());
                            }
                            o.extend(g.extra.clone());
                            (g.name.clone(), Value::Object(o))
                        })
                        .collect();
                    out.insert("groups".into(), Value::Object(groups));
                }
                let fields = fields.iter().map(|f| self.write_field(f)).collect();
                out.insert("fields".into(), Value::Array(fields));
            }
            Body::Enum { values } => {
                out.insert(
                    "values".into(),
                    Value::Array(values.iter().map(|v| v.clone().into()).collect()),
                );
            }
        }
        out.extend(t.extra.clone());
        Value::Object(out)
    }

    fn write_field(&self, f: &Field) -> Value {
        let mut o = Map::new();
        o.insert("name".into(), f.name.clone().into());
        o.insert("type".into(), f.ty.clone().into());
        if f.optional {
            o.insert("optional".into(), true.into());
        }
        if let Some(default) = &f.default {
            let canonical = parse_type(&f.ty)
                .map(|ty| canonical(&ty, default))
                .unwrap_or_else(|_| default.clone());
            o.insert("default".into(), canonical);
        }
        for (key, value) in [("min", &f.min), ("max", &f.max), ("step", &f.step)] {
            if let Some(n) = value {
                o.insert(key.into(), Value::Number(n.clone()));
            }
        }
        let mut text = |key: &str, value: &str| {
            if !value.is_empty() {
                o.insert(key.into(), value.into());
            }
        };
        text("doc", &f.doc);
        text("group", &f.group);
        if let Some(n) = &f.order {
            o.insert("order".into(), Value::Number(n.clone()));
        }
        let mut text = |key: &str, value: &str| {
            if !value.is_empty() {
                o.insert(key.into(), value.into());
            }
        };
        text("label", &f.label);
        if f.readonly {
            o.insert("readonly".into(), true.into());
        }
        let mut text = |key: &str, value: &str| {
            if !value.is_empty() {
                o.insert(key.into(), value.into());
            }
        };
        text("show_if", &f.show_if);
        text("hide_if", &f.hide_if);
        text("widget", &f.widget);
        o.extend(f.extra.clone());
        Value::Object(o)
    }

    /// Renames type `from` to `to` in every type expression of the file.
    pub fn rename_type(&mut self, from: &str, to: &str) {
        for t in &mut self.types {
            if let Body::Struct { fields, .. } = &mut t.body {
                for f in fields {
                    if let Ok(mut ty) = parse_type(&f.ty)
                        && ty.rename(from, to)
                    {
                        f.ty = ty.to_string();
                    }
                }
            }
        }
    }
}

/// Takes `$dialect` out of a file's top level; errors unless it is `a/1` (§1).
pub fn check_dialect(top: &mut Map<String, Value>) -> Result<(), String> {
    match top.shift_remove("$dialect") {
        Some(Value::String(d)) if d == DIALECT => Ok(()),
        Some(Value::String(d)) => match d.strip_prefix("a/").map(str::parse::<u32>) {
            Some(Ok(major)) if major > 1 => Err(format!("$dialect {d:?} is newer than a/1")),
            _ => Err(format!("unknown $dialect {d:?}")),
        },
        _ => Err("no $dialect".into()),
    }
}

fn parse_type_def(name: String, def: Value) -> Result<TypeDef, String> {
    let fail = |e: &str| format!("type {name:?}: {e}");
    let Value::Object(mut def) = def else {
        return Err(fail("not an object"));
    };
    let doc = match def.shift_remove("doc") {
        Some(Value::String(d)) => d,
        Some(_) => return Err(fail("doc is not a string")),
        None => String::new(),
    };
    let kind = def.shift_remove("kind");
    let body = match kind.as_ref().and_then(Value::as_str) {
        Some("struct") => {
            let fields = match def.shift_remove("fields") {
                Some(Value::Array(items)) => items
                    .into_iter()
                    .map(parse_field)
                    .collect::<Result<_, _>>()
                    .map_err(|e| fail(&e))?,
                _ => return Err(fail("fields is not an array")),
            };
            let groups = match def.shift_remove("groups") {
                Some(Value::Object(groups)) => groups
                    .into_iter()
                    .map(|(name, g)| {
                        let Value::Object(mut g) = g else {
                            return Err(fail(&format!("group {name:?} is not an object")));
                        };
                        let collapsed = g
                            .shift_remove("collapsed")
                            .is_some_and(|v| v.as_bool() == Some(true));
                        let doc = g
                            .shift_remove("doc")
                            .and_then(|v| v.as_str().map(str::to_owned))
                            .unwrap_or_default();
                        Ok(Group {
                            name,
                            collapsed,
                            doc,
                            extra: g,
                        })
                    })
                    .collect::<Result<_, _>>()?,
                Some(_) => return Err(fail("groups is not an object")),
                None => Vec::new(),
            };
            Body::Struct { fields, groups }
        }
        Some("enum") => match def.shift_remove("values") {
            Some(Value::Array(items)) => Body::Enum {
                values: items
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => Ok(s),
                        _ => Err(fail("values holds a non-string")),
                    })
                    .collect::<Result<_, _>>()?,
            },
            _ => return Err(fail("values is not an array")),
        },
        Some(other) => return Err(fail(&format!("unknown kind {other:?}"))),
        None => return Err(fail("no kind")),
    };
    Ok(TypeDef {
        name,
        doc,
        body,
        extra: def,
    })
}

fn parse_field(value: Value) -> Result<Field, String> {
    let Value::Object(mut o) = value else {
        return Err("a field is not an object".into());
    };
    let mut text = |key: &str| -> Result<String, String> {
        match o.shift_remove(key) {
            Some(Value::String(s)) => Ok(s),
            None => Ok(String::new()),
            Some(_) => Err(format!("field {key} is not a string")),
        }
    };
    let name = text("name")?;
    let ty = text("type")?;
    let doc = text("doc")?;
    let group = text("group")?;
    let label = text("label")?;
    let show_if = text("show_if")?;
    let hide_if = text("hide_if")?;
    let widget = text("widget")?;
    let mut number = |key: &str| -> Result<Option<Number>, String> {
        match o.shift_remove(key) {
            Some(Value::Number(n)) => Ok(Some(n)),
            None => Ok(None),
            Some(_) => Err(format!("field {name:?}: {key} is not a number")),
        }
    };
    let (min, max, step, order) = (
        number("min")?,
        number("max")?,
        number("step")?,
        number("order")?,
    );
    let readonly = o
        .shift_remove("readonly")
        .is_some_and(|v| v.as_bool() == Some(true));
    let optional = match o.shift_remove("optional") {
        Some(Value::Bool(b)) => b,
        None => false,
        Some(_) => return Err(format!("field {name:?}: optional is not a boolean")),
    };
    let default = o.shift_remove("default");
    Ok(Field {
        name,
        ty,
        optional,
        default,
        min,
        max,
        step,
        doc,
        group,
        order,
        label,
        readonly,
        show_if,
        hide_if,
        widget,
        extra: o,
    })
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
pub fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub const PRIMITIVES: [&str; 15] = [
    "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "string", "rgb",
    "rgba", "curve",
];
/// The last primitive, kept apart so [`PRIMITIVES`] lists them in the type menu's order.
pub const GRADIENT: &str = "gradient";
pub const GENERICS: [&str; 2] = ["list", "ref"];

/// Whether `name` is reserved: a primitive or a generic.
pub fn is_reserved(name: &str) -> bool {
    PRIMITIVES.contains(&name) || name == GRADIENT || GENERICS.contains(&name)
}

/// A parsed type expression; see docs/aschema.md#3-type-expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Prim(&'static str),
    Named(String),
    List(Box<Ty>),
    Ref(Box<Ty>),
}

pub fn parse_type(text: &str) -> Result<Ty, String> {
    if text.is_empty() {
        return Err("no type".into());
    }
    if text.contains(char::is_whitespace) {
        return Err(format!("type {text:?} has whitespace"));
    }
    if let Some(inner) = text.strip_suffix('>') {
        let (outer, inner) = inner
            .split_once('<')
            .ok_or_else(|| format!("type {text:?} does not parse"))?;
        let inner = Box::new(parse_type(inner)?);
        return match outer {
            "list" => Ok(Ty::List(inner)),
            "optional" => Err("optional<…> is not a type: make the field optional".into()),
            "ref" => Ok(Ty::Ref(inner)),
            _ => Err(format!("unknown generic {outer:?}")),
        };
    }
    if let Some(p) = PRIMITIVES
        .iter()
        .chain(std::iter::once(&GRADIENT))
        .find(|p| **p == text)
    {
        return Ok(Ty::Prim(p));
    }
    if is_identifier(text) && !GENERICS.contains(&text) {
        Ok(Ty::Named(text.to_owned()))
    } else {
        Err(format!("type {text:?} does not parse"))
    }
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Prim(p) => write!(f, "{p}"),
            Ty::Named(n) => write!(f, "{n}"),
            Ty::List(t) => write!(f, "list<{t}>"),
            Ty::Ref(t) => write!(f, "ref<{t}>"),
        }
    }
}

impl Ty {
    /// Renames named type `from`; returns whether any was renamed.
    fn rename(&mut self, from: &str, to: &str) -> bool {
        match self {
            Ty::Named(n) if n == from => {
                *n = to.to_owned();
                true
            }
            Ty::List(t) | Ty::Ref(t) => t.rename(from, to),
            _ => false,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Ty::Prim(p) if p.starts_with('i') || p.starts_with('u'))
    }

    pub fn is_number(&self) -> bool {
        self.is_integer() || matches!(self, Ty::Prim("f32" | "f64"))
    }
}

/// Integer ranges by type name.
pub fn int_range(p: &str) -> (i128, i128) {
    match p {
        "i8" => (i8::MIN.into(), i8::MAX.into()),
        "i16" => (i16::MIN.into(), i16::MAX.into()),
        "i32" => (i32::MIN.into(), i32::MAX.into()),
        "i64" => (i64::MIN.into(), i64::MAX.into()),
        "u8" => (0, u8::MAX.into()),
        "u16" => (0, u16::MAX.into()),
        "u32" => (0, u32::MAX.into()),
        _ => (0, u64::MAX.into()),
    }
}

pub fn is_color(text: &str, alpha: bool) -> bool {
    let Some(hex) = text.strip_prefix('#') else {
        return false;
    };
    (hex.len() == 6 || (alpha && hex.len() == 8)) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Curve points `[x, y, in, out]` of a curve value (shortened points get 0 tangents); errors
/// for fewer than two points, points out of 0..1 or unsorted.
pub fn curve_points(value: &Value) -> Result<Vec<[f32; 4]>, String> {
    let items = value.as_array().ok_or("a curve is an array of points")?;
    let mut points = Vec::with_capacity(items.len());
    for item in items {
        let numbers: Vec<f64> = item
            .as_array()
            .filter(|a| a.len() == 2 || a.len() == 4)
            .and_then(|a| a.iter().map(Value::as_f64).collect())
            .ok_or("a curve point is [x, y] or [x, y, in, out]")?;
        let at = |i: usize| numbers.get(i).copied().unwrap_or(0.0) as f32;
        let point = [at(0), at(1), at(2), at(3)];
        if !(0.0..=1.0).contains(&point[0]) {
            return Err(format!("curve point x {} is outside 0..1", point[0]));
        }
        if points.last().is_some_and(|p: &[f32; 4]| p[0] > point[0]) {
            return Err("curve points are not sorted by x".into());
        }
        points.push(point);
    }
    if points.len() < 2 {
        return Err("a curve needs at least 2 points".into());
    }
    Ok(points)
}

/// The segment around `x` (its points and width, and `x` within it), if `x` is inside the
/// points and the segment has a width.
fn curve_segment(points: &[[f32; 4]], x: f32) -> Option<([f32; 4], [f32; 4], f32, f32)> {
    let (first, last) = (points.first()?, points.last()?);
    if x <= first[0] || x >= last[0] {
        return None;
    }
    let i = points
        .partition_point(|p| p[0] <= x)
        .clamp(1, points.len() - 1);
    let (a, b) = (points[i - 1], points[i]);
    let width = b[0] - a[0];
    (width > f32::EPSILON).then(|| (a, b, width, (x - a[0]) / width))
}

/// The curve's value at `x`: matma's cubic Hermite between neighbouring points, each
/// segment's tangents its slopes times its width; flat outside the points. See
/// docs/features/curves.md#evaluating.
pub fn evaluate_curve(points: &[[f32; 4]], x: f32) -> f32 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0.0;
    };
    if x <= first[0] {
        return first[1];
    }
    if x >= last[0] {
        return last[1];
    }
    match curve_segment(points, x) {
        Some((a, b, width, t)) => matma::hermite(a[1], a[3] * width, b[1], b[2] * width, t),
        // A zero-width segment: a step to the later point.
        None => points[points.partition_point(|p| p[0] <= x).min(points.len() - 1)][1],
    }
}

/// The curve's slope (dy/dx) at `x`: matma's Hermite derivative over the segment's width;
/// 0 outside the points.
pub fn curve_slope(points: &[[f32; 4]], x: f32) -> f32 {
    curve_segment(points, x).map_or(0.0, |(a, b, width, t)| {
        matma::hermite_derivative(a[1], a[3] * width, b[1], b[2] * width, t) / width
    })
}

/// The linear curve, the implicit default.
pub fn linear_curve() -> Value {
    serde_json::json!([[0, 0, 1, 1], [1, 1, 1, 1]])
}

/// Black to white, the implicit gradient default.
pub fn default_gradient() -> Value {
    serde_json::json!([[0, "#000000ff"], [1, "#ffffffff"]])
}

/// Gradient stops `(t, "#rrggbbaa")`; errors for stops out of 0..1, unsorted or fewer than two.
pub fn gradient_stops(value: &Value) -> Result<Vec<(f64, [u8; 4])>, String> {
    let items = value.as_array().ok_or("a gradient is an array of stops")?;
    let mut stops: Vec<(f64, [u8; 4])> = Vec::new();
    for item in items {
        let (t, color) = item
            .as_array()
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].as_f64()?, a[1].as_str()?)))
            .ok_or("a gradient stop is [t, \"#rrggbbaa\"]")?;
        if !(0.0..=1.0).contains(&t) {
            return Err(format!("gradient stop t {t} is outside 0..1"));
        }
        if !is_color(color, true) {
            return Err(format!(
                "gradient colour {color:?} is not #rrggbb or #rrggbbaa"
            ));
        }
        if stops.last().is_some_and(|s| s.0 > t) {
            return Err("gradient stops are not sorted by t".into());
        }
        let color = crate::gradient::parse_color(color).unwrap_or([0, 0, 0, 255]);
        stops.push((t, color));
    }
    if stops.len() < 2 {
        return Err("a gradient needs at least 2 stops".into());
    }
    Ok(stops)
}

/// A value in the written form: lower-case colours, full curve points.
pub fn canonical(ty: &Ty, value: &Value) -> Value {
    match (ty, value) {
        (Ty::Prim("rgb"), Value::String(s)) => s.to_lowercase().into(),
        (Ty::Prim("rgba"), Value::String(s)) if is_color(s, true) => {
            let s = s.to_lowercase();
            if s.len() == 7 { format!("{s}ff") } else { s }.into()
        }
        (Ty::Prim("curve"), v) => match curve_points(v) {
            Ok(points) => Value::Array(
                points
                    .iter()
                    .map(|p| Value::Array(p.iter().map(|&n| json_f32(n)).collect()))
                    .collect(),
            ),
            Err(_) => v.clone(),
        },
        (Ty::Prim("gradient"), Value::Array(items)) => Value::Array(
            items
                .iter()
                .map(|stop| match stop.as_array().map(Vec::as_slice) {
                    Some([t, Value::String(c)]) if is_color(c, true) => {
                        let c = c.to_lowercase();
                        let c = if c.len() == 7 { format!("{c}ff") } else { c };
                        Value::Array(vec![t.clone(), c.into()])
                    }
                    _ => stop.clone(),
                })
                .collect(),
        ),
        (Ty::List(t), Value::Array(items)) => {
            Value::Array(items.iter().map(|v| canonical(t, v)).collect())
        }
        _ => value.clone(),
    }
}

/// An `f32` as JSON in its shortest decimal form (`0.2`, not `0.20000000298`).
pub fn json_f32(n: f32) -> Value {
    json_number(n.to_string().parse::<f64>().unwrap_or(0.0))
}

/// A number as JSON: an integer when it is one.
pub fn json_number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        Value::from(n as i64)
    } else {
        Number::from_f64(n).map_or(Value::Null, Value::Number)
    }
}

/// A problem of a schema; see docs/features/schemas.md#problems.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// The type it is in.
    pub type_index: usize,
    /// The field it is about, by index.
    pub field: Option<usize>,
    /// `Weapon` or `Weapon/damage`.
    pub path: String,
    pub message: String,
}

impl Schema {
    /// Whether `value` encodes as `ty`; why not otherwise.
    pub fn encodes(&self, ty: &Ty, value: &Value) -> Result<(), String> {
        let bad = || format!("{value} is not a {ty}");
        match ty {
            Ty::Prim("bool") => value.as_bool().map(|_| ()).ok_or_else(bad),
            Ty::Prim("f32" | "f64") => value.as_f64().map(|_| ()).ok_or_else(bad),
            Ty::Prim("string") => value.as_str().map(|_| ()).ok_or_else(bad),
            Ty::Prim(c @ ("rgb" | "rgba")) => value
                .as_str()
                .filter(|s| is_color(s, *c == "rgba"))
                .map(|_| ())
                .ok_or_else(bad),
            Ty::Prim("curve") => curve_points(value).map(|_| ()),
            Ty::Prim("gradient") => gradient_stops(value).map(|_| ()),
            Ty::Prim(p) => {
                let n = value.as_number().ok_or_else(bad)?;
                let n: i128 = match (n.as_i64(), n.as_u64()) {
                    (Some(i), _) => i.into(),
                    (None, Some(u)) => u.into(),
                    _ => return Err(format!("{value} is not an integer")),
                };
                let (lo, hi) = int_range(p);
                if (lo..=hi).contains(&n) {
                    Ok(())
                } else {
                    Err(format!("{value} is outside {p}"))
                }
            }
            Ty::List(t) => {
                let items = value.as_array().ok_or_else(bad)?;
                items.iter().try_for_each(|v| self.encodes(t, v))
            }
            Ty::Ref(_) => value.as_str().map(|_| ()).ok_or_else(bad),
            Ty::Named(name) => match self.type_index(name).map(|i| &self.types[i].body) {
                Some(Body::Enum { values }) => value
                    .as_str()
                    .filter(|s| values.iter().any(|v| v == s))
                    .map(|_| ())
                    .ok_or_else(|| format!("{value} is not a value of {name}")),
                Some(Body::Struct { fields, .. }) => {
                    let object = value.as_object().ok_or_else(bad)?;
                    for (key, v) in object {
                        let field = fields
                            .iter()
                            .find(|f| f.name == *key)
                            .ok_or_else(|| format!("{name} has no field {key:?}"))?;
                        if let Ok(t) = parse_type(&field.ty)
                            && !(field.optional && v.is_null())
                        {
                            self.encodes(&t, v)?;
                        }
                    }
                    Ok(())
                }
                None => Err(format!("unknown type {name:?}")),
            },
        }
    }

    /// Checks a type expression's names and nesting.
    fn check_type(&self, ty: &Ty) -> Result<(), String> {
        match ty {
            Ty::Prim(_) => Ok(()),
            Ty::Named(n) => self
                .type_index(n)
                .map(|_| ())
                .ok_or_else(|| format!("unknown type {n:?}")),
            Ty::List(t) => self.check_type(t),
            Ty::Ref(t) => match &**t {
                Ty::Named(n)
                    if matches!(
                        self.type_index(n).map(|i| &self.types[i].body),
                        Some(Body::Struct { .. })
                    ) =>
                {
                    Ok(())
                }
                Ty::Named(n) if self.type_index(n).is_none() => Err(format!("unknown type {n:?}")),
                other => Err(format!("ref<{other}>: only structs can be referenced")),
            },
        }
    }

    /// Whether struct `start` contains itself through plain (not optional) struct-typed
    /// fields.
    fn embeds_itself(&self, start: usize) -> bool {
        let mut stack = vec![start];
        let mut seen = vec![false; self.types.len()];
        while let Some(i) = stack.pop() {
            let Body::Struct { fields, .. } = &self.types[i].body else {
                continue;
            };
            for f in fields.iter().filter(|f| !f.optional) {
                if let Ok(Ty::Named(n)) = parse_type(&f.ty)
                    && let Some(j) = self.type_index(&n)
                {
                    if j == start {
                        return true;
                    }
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        false
    }

    /// Every schema error of docs/aschema.md#5-validation.
    pub fn check(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        for (ti, t) in self.types.iter().enumerate() {
            let mut add = |field: Option<usize>, path: String, message: String| {
                out.push(Problem {
                    type_index: ti,
                    field,
                    path,
                    message,
                });
            };
            if !is_identifier(&t.name) {
                add(None, t.name.clone(), "the name is not an identifier".into());
            } else if is_reserved(&t.name) {
                add(
                    None,
                    t.name.clone(),
                    "the name is a primitive or generic".into(),
                );
            }
            if self.types[..ti].iter().any(|o| o.name == t.name) {
                add(None, t.name.clone(), "the name is used twice".into());
            }
            match &t.body {
                Body::Enum { values } => {
                    if values.is_empty() {
                        add(None, t.name.clone(), "an enum needs a value".into());
                    }
                    for (i, v) in values.iter().enumerate() {
                        if values[..i].contains(v) {
                            add(None, t.name.clone(), format!("value {v:?} is used twice"));
                        }
                    }
                }
                Body::Struct { fields, .. } => {
                    if self.embeds_itself(ti) {
                        add(
                            None,
                            t.name.clone(),
                            "embeds itself; use ref, list or an optional field".into(),
                        );
                    }
                    for (fi, f) in fields.iter().enumerate() {
                        let path = format!("{}/{}", t.name, f.name);
                        let mut field = |message: String| add(Some(fi), path.clone(), message);
                        if !is_identifier(&f.name) {
                            field("the name is not an identifier".into());
                        }
                        if fields[..fi].iter().any(|o| o.name == f.name) {
                            field("the name is used twice".into());
                        }
                        let ty = parse_type(&f.ty).and_then(|ty| {
                            self.check_type(&ty)?;
                            Ok(ty)
                        });
                        let ty = match ty {
                            Ok(ty) => Some(ty),
                            Err(e) => {
                                field(e);
                                None
                            }
                        };
                        if let (Some(ty), Some(default)) = (&ty, &f.default)
                            && !(f.optional && default.is_null())
                            && let Err(e) = self.encodes(ty, default)
                        {
                            field(format!("default: {e}"));
                        }
                        if ty.as_ref().is_some_and(|t| !t.is_number())
                            && (f.min.is_some() || f.max.is_some() || f.step.is_some())
                        {
                            field("min, max and step are only for numbers".into());
                        }
                        for (key, condition) in [("show_if", &f.show_if), ("hide_if", &f.hide_if)] {
                            if condition.is_empty() {
                                continue;
                            }
                            if let Err(e) = check_condition(condition, fields) {
                                field(format!("{key}: {e}"));
                            }
                        }
                        let target = ty.as_ref().and_then(|ty| match ty {
                            Ty::Named(n) => self.type_index(n).map(|i| self.types[i].kind()),
                            _ => None,
                        });
                        match (f.widget.as_str(), target) {
                            ("", _) | ("toggle", Some("enum")) | ("inline", Some("struct")) => {}
                            ("toggle", _) => field("widget toggle is only for enum fields".into()),
                            ("inline", _) => {
                                field("widget inline is only for struct fields".into())
                            }
                            (other, _) => field(format!("unknown widget {other:?}")),
                        }
                    }
                }
            }
        }
        out
    }
}

/// Checks a `show_if` / `hide_if` condition against the struct's fields.
fn check_condition(text: &str, fields: &[Field]) -> Result<(), String> {
    let text = text.trim();
    let split = ["==", "!=", "<=", ">=", "<", ">"]
        .into_iter()
        .find_map(|op| text.split_once(op).map(|(l, r)| (op, l.trim(), r.trim())));
    let (op, left, right) = match split {
        Some(split) => split,
        None => ("", text.strip_prefix('!').unwrap_or(text).trim(), ""),
    };
    if !is_identifier(left) {
        return Err(format!("{left:?} is not a field name"));
    }
    if !fields.iter().any(|f| f.name == left) {
        return Err(format!("no field {left:?}"));
    }
    if matches!(op, "<" | "<=" | ">" | ">=") && !right.parse::<f64>().is_ok_and(f64::is_finite) {
        return Err(format!("{op} needs a number, not {right:?}"));
    }
    Ok(())
}

/// Whether condition `text` holds, given its sibling fields' resolved values (`None` for
/// no such sibling); see docs/aschema.md#24-conditions-show_if--hide_if.
pub fn condition_holds(text: &str, sibling: impl Fn(&str) -> Option<Value>) -> bool {
    let text = text.trim();
    let split = ["==", "!=", "<=", ">=", "<", ">"]
        .into_iter()
        .find_map(|op| text.split_once(op).map(|(l, r)| (op, l.trim(), r.trim())));
    let Some((op, left, right)) = split else {
        let (name, want) = match text.strip_prefix('!') {
            Some(name) => (name.trim(), false),
            None => (text, true),
        };
        return sibling(name).is_some_and(|v| v == Value::Bool(want));
    };
    let Some(value) = sibling(left) else {
        return false;
    };
    let number = right.parse::<f64>().ok().filter(|n| n.is_finite());
    let expected = match right {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => match number {
            Some(n) => json_number(n),
            None => Value::String(
                right
                    .strip_prefix('"')
                    .and_then(|r| r.strip_suffix('"'))
                    .unwrap_or(right)
                    .to_owned(),
            ),
        },
    };
    let equal = match (value.as_f64(), expected.as_f64()) {
        (Some(a), Some(b)) => a == b,
        _ => value == expected,
    };
    match op {
        "==" => equal,
        "!=" => !equal,
        _ => match (value.as_f64(), number) {
            (Some(a), Some(b)) => match op {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            },
            _ => false,
        },
    }
}

/// The value a field of type `ty` holds without a default, to start an explicit one from;
/// see docs/features/schemas.md#defaults.
pub fn implicit_value(schema: &Schema, ty: &Ty) -> Value {
    match ty {
        Ty::Prim("bool") => false.into(),
        Ty::Prim("string") => "".into(),
        Ty::Prim("rgb") => "#000000".into(),
        Ty::Prim("rgba") => "#000000ff".into(),
        Ty::Prim("curve") => linear_curve(),
        Ty::Prim("gradient") => default_gradient(),
        Ty::Prim(_) => 0.into(),
        Ty::List(_) => Value::Array(Vec::new()),
        Ty::Ref(_) => "".into(),
        Ty::Named(n) => match schema.type_index(n).map(|i| &schema.types[i].body) {
            Some(Body::Enum { values }) => values.first().cloned().unwrap_or_default().into(),
            _ => Value::Object(Map::new()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r##"{
  "$dialect": "a/1",
  "types": {
    "Rarity": { "kind": "enum", "values": ["common", "rare", "epic"], "color_hint": 3 },
    "Vec2": {
      "kind": "struct",
      "fields": [
        { "name": "x", "type": "f32", "default": 0 },
        { "name": "y", "type": "f32", "default": 0 }
      ]
    },
    "Weapon": {
      "kind": "struct",
      "doc": "A thing to hit with",
      "groups": { "Advanced": { "collapsed": true, "doc": "Rarely tuned" } },
      "fields": [
        { "name": "name", "type": "string", "doc": "Shown in the inventory" },
        { "name": "damage", "type": "i32", "default": 10, "min": 0, "max": 999, "group": "Combat" },
        { "name": "rarity", "type": "Rarity", "default": "common", "widget": "toggle" },
        { "name": "crit", "type": "f32", "default": 0.05, "step": 0.01 },
        { "name": "mult", "type": "f32", "show_if": "crit > 0" },
        { "name": "tint", "type": "rgb", "default": "#C8C8C8" },
        { "name": "falloff", "type": "curve", "default": [[0, 0.2], [1, 0.4, -1.5, -1.5]] },
        { "name": "trail", "type": "gradient" },
        { "name": "offset", "type": "Vec2", "default": { "x": 0.5 }, "widget": "inline" },
        { "name": "drops", "type": "list<ref<Weapon>>", "editor_note": "keep me" },
        { "name": "notes", "type": "string", "optional": true, "readonly": true },
        { "name": "next", "type": "Weapon", "optional": true, "default": null }
      ]
    }
  },
  "x_tool": { "v": 1 }
}"##;

    #[test]
    fn examples_read_check_and_round_trip() {
        let schema = Schema::parse(EXAMPLE).unwrap();
        assert_eq!(schema.types.len(), 3);
        assert_eq!(schema.check(), []);
        let text = schema.to_json();
        assert!(text.starts_with("{\n  \"$dialect\": \"a/1\",\n  \"types\": {\n"));
        assert!(text.ends_with("}\n"));
        // Colours lower-cased, curve points in full, unknown keys kept in place.
        assert!(text.contains("\"default\": \"#c8c8c8\""));
        assert!(text.contains("-1.5,\n"));
        assert!(text.contains("0.2,\n"));
        assert!(text.contains("\"editor_note\": \"keep me\""));
        assert!(text.contains("\"color_hint\": 3"));
        assert!(text.contains("\"x_tool\""));
        // Integers stay integers.
        assert!(text.contains("\"default\": 10,"));
        let again = Schema::parse(&text).unwrap();
        assert_eq!(again.to_json(), text);
        let Body::Struct { groups, fields } = &again.types[2].body else {
            panic!()
        };
        assert!(groups[0].collapsed);
        assert!(fields[10].readonly);
        assert_eq!(
            Schema::parse(&Schema::new().to_json()).unwrap(),
            Schema::new()
        );
    }

    #[test]
    fn unreadable_files_say_why() {
        assert!(Schema::parse("{").unwrap_err().contains("invalid JSON"));
        assert!(Schema::parse("{}").unwrap_err().contains("no $dialect"));
        let newer = r#"{"$dialect": "a/2", "types": {}}"#;
        assert!(Schema::parse(newer).unwrap_err().contains("newer"));
        let kind = r#"{"$dialect": "a/1", "types": {"A": {"kind": "union"}}}"#;
        assert!(Schema::parse(kind).unwrap_err().contains("unknown kind"));
    }

    #[test]
    fn problems_name_their_field() {
        let text = r##"{
          "$dialect": "a/1",
          "types": {
            "E": { "kind": "enum", "values": ["a", "a"] },
            "S": {
              "kind": "struct",
              "fields": [
                { "name": "self_", "type": "S" },
                { "name": "r", "type": "ref<E>" },
                { "name": "o", "type": "optional<i32>" },
                { "name": "small", "type": "u8", "default": 300 },
                { "name": "frac", "type": "i32", "default": 12.5 },
                { "name": "s", "type": "string", "min": 1 },
                { "name": "c", "type": "bool", "show_if": "missing" },
                { "name": "n", "type": "i32", "hide_if": "c > yes" },
                { "name": "w", "type": "i32", "widget": "toggle" },
                { "name": "u", "type": "Nope" },
                { "name": "bad name", "type": "list<i32" },
                { "name": "curve", "type": "curve", "default": [[0.5, 0], [0.2, 1]] },
                { "name": "i32", "type": "list<ref<S>>" }
              ]
            },
            "i32": { "kind": "enum", "values": ["x"] }
          }
        }"##;
        let schema = Schema::parse(text).unwrap();
        let problems = schema.check();
        let has = |path: &str, part: &str| {
            problems
                .iter()
                .any(|p| p.path == path && p.message.contains(part))
        };
        assert!(has("E", "used twice"));
        assert!(has("S", "embeds itself"));
        assert!(has("S/r", "only structs"));
        assert!(has("S/o", "make the field optional"));
        assert!(has("S/small", "outside u8"));
        assert!(has("S/frac", "not an integer"));
        assert!(has("S/s", "only for numbers"));
        assert!(has("S/c", "no field"));
        assert!(has("S/n", "needs a number"));
        assert!(has("S/w", "toggle"));
        assert!(has("S/u", "unknown type"));
        assert!(has("S/bad name", "not an identifier"));
        assert!(has("S/curve", "not sorted"));
        assert!(has("i32", "primitive"));
        // A field may be named like a primitive, and nested generics are fine.
        assert!(!problems.iter().any(|p| p.path == "S/i32"));
    }

    #[test]
    fn renames_follow_into_type_expressions() {
        let mut schema = Schema::parse(EXAMPLE).unwrap();
        schema.types[2].name = "Blade".into();
        schema.rename_type("Weapon", "Blade");
        let Body::Struct { fields, .. } = &schema.types[2].body else {
            panic!()
        };
        assert_eq!(fields[9].ty, "list<ref<Blade>>");
        assert_eq!(schema.check(), []);
    }

    #[test]
    fn curves_pass_through_their_points() {
        let points = curve_points(&linear_curve()).unwrap();
        assert!((evaluate_curve(&points, 0.25) - 0.25).abs() < 1e-5);
        let points = [[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0]];
        // Flat tangents: an ease in and out, symmetric about the middle.
        assert!((evaluate_curve(&points, 0.5) - 0.5).abs() < 1e-5);
        assert!(evaluate_curve(&points, 0.25) < 0.25);
        assert_eq!(evaluate_curve(&points, -1.0), 0.0);
        assert_eq!(evaluate_curve(&points, 2.0), 1.0);
    }

    #[test]
    fn conditions_hold() {
        let values = serde_json::json!({ "boss": true, "crit": 0.5, "kind": "ice" });
        let sibling = |name: &str| values.get(name).cloned();
        assert!(condition_holds("boss", sibling));
        assert!(!condition_holds("!boss", sibling));
        assert!(condition_holds("crit > 0", sibling));
        assert!(!condition_holds("crit >= 1", sibling));
        assert!(condition_holds("kind == ice", sibling));
        assert!(condition_holds("kind == \"ice\"", sibling));
        assert!(condition_holds("kind != fire", sibling));
        assert!(!condition_holds("missing", sibling));
        assert!(!condition_holds("kind > 1", sibling));
    }

    #[test]
    fn curve_slopes_and_steps() {
        let points = curve_points(&linear_curve()).unwrap();
        assert!((curve_slope(&points, 0.3) - 1.0).abs() < 1e-5);
        assert_eq!(curve_slope(&points, 1.5), 0.0);
        // y = x² from flat to slope 2.
        let points = [[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 2.0, 2.0]];
        assert!((evaluate_curve(&points, 0.5) - 0.25).abs() < 1e-5);
        assert!((curve_slope(&points, 0.5) - 1.0).abs() < 1e-5);
        // Two points at one x: a step.
        let step = serde_json::json!([[0, 0], [0.5, 0], [0.5, 1], [1, 1]]);
        let points = curve_points(&step).unwrap();
        assert_eq!(evaluate_curve(&points, 0.25), 0.0);
        assert_eq!(evaluate_curve(&points, 0.75), 1.0);
        assert!(curve_points(&serde_json::json!([[0, 0]])).is_err());
    }
}
