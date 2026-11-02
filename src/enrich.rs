//! Model properties, shapes and regions as kept in `.amod` sidecars; see
//! docs/spec/amod.md#properties, docs/features/shapes.md and docs/features/regions.md.

use std::collections::{BTreeMap, BTreeSet};

use crate::amod::locator_color;

/// A property's value; see docs/features/properties.md#property.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl PropValue {
    pub const TYPES: [&'static str; 4] = ["Bool", "Int", "Float", "Text"];

    /// Index in [`Self::TYPES`].
    pub fn type_index(&self) -> usize {
        match self {
            Self::Bool(_) => 0,
            Self::Int(_) => 1,
            Self::Float(_) => 2,
            Self::Text(_) => 3,
        }
    }

    /// This value as type `index` of [`Self::TYPES`], converted where it can be.
    pub fn converted(&self, index: usize) -> Self {
        let number = match self {
            Self::Bool(b) => Some(f64::from(u8::from(*b))),
            Self::Int(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            Self::Text(t) => t.trim().parse::<f64>().ok(),
        };
        match index {
            0 => Self::Bool(match self {
                Self::Text(t) => t.trim() == "true" || number.is_some_and(|n| n != 0.0),
                _ => number.is_some_and(|n| n != 0.0),
            }),
            1 => Self::Int(number.map_or(0, |n| n.round() as i64)),
            2 => Self::Float(number.unwrap_or(0.0)),
            _ => Self::Text(match self {
                Self::Text(t) => t.clone(),
                other => other.to_toml(),
            }),
        }
    }

    fn from_toml(key: &str, value: &toml::Value) -> Result<Self, String> {
        Ok(match value {
            toml::Value::Boolean(b) => Self::Bool(*b),
            toml::Value::Integer(i) => Self::Int(*i),
            toml::Value::Float(f) => Self::Float(*f),
            toml::Value::String(s) => Self::Text(s.clone()),
            _ => {
                return Err(format!(
                    "property {key:?} is not a bool, integer, float or string"
                ));
            }
        })
    }

    pub fn to_toml(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => toml::Value::Float(*f).to_string(),
            Self::Text(t) => toml::Value::from(t.as_str()).to_string(),
        }
    }
}

/// Key / value pairs, sorted by key.
pub type Properties = BTreeMap<String, PropValue>;

pub fn parse_properties(value: toml::Value) -> Result<Properties, String> {
    let toml::Value::Table(table) = value else {
        return Err("properties is not a table".into());
    };
    table
        .iter()
        .map(|(key, value)| {
            if key.is_empty() {
                return Err("a property has an empty key".into());
            }
            Ok((key.clone(), PropValue::from_toml(key, value)?))
        })
        .collect()
}

/// A TOML key: bare when it can be, quoted otherwise.
fn key(key: &str) -> String {
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

pub fn write_properties(table: &str, properties: &Properties, out: &mut String) {
    if properties.is_empty() {
        return;
    }
    *out += &format!("\n[{table}]\n");
    for (k, v) in properties {
        *out += &format!("{} = {}\n", key(k), v.to_toml());
    }
}

/// `<base>`, or `<base>_<n>` with the smallest `n` from 2 that `taken` does not hold.
pub fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    std::iter::once(base.to_owned())
        .chain((2..).map(|n| format!("{base}_{n}")))
        .find(|name| !taken(name))
        .expect("a free name")
}

/// What a shape is, with its size; see docs/features/shapes.md#shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeType {
    Box { size: [f32; 3] },
    Sphere { radius: f32 },
    Capsule { radius: f32, height: f32 },
    Hull,
}

impl ShapeType {
    pub const NAMES: [&'static str; 4] = ["Box", "Sphere", "Capsule", "Convex hull"];
    const KEYS: [&'static str; 4] = ["box", "sphere", "capsule", "hull"];

    /// Index in [`Self::NAMES`], and the `type` field of the compiled record.
    pub fn index(self) -> usize {
        match self {
            Self::Box { .. } => 0,
            Self::Sphere { .. } => 1,
            Self::Capsule { .. } => 2,
            Self::Hull => 3,
        }
    }

    /// Type `index` keeping the sizes that apply; others are `extent / 10`.
    pub fn with_index(self, index: usize, extent: f32) -> Self {
        let tenth = extent / 10.0;
        let radius = match self {
            Self::Sphere { radius } | Self::Capsule { radius, .. } => radius,
            _ => tenth / 2.0,
        };
        match index {
            0 => Self::Box {
                size: match self {
                    Self::Box { size } => size,
                    _ => [tenth; 3],
                },
            },
            1 => Self::Sphere { radius },
            2 => Self::Capsule {
                radius,
                height: match self {
                    Self::Capsule { height, .. } => height,
                    _ => tenth,
                },
            },
            _ => Self::Hull,
        }
    }
}

/// A simple volume on a model; see docs/features/shapes.md.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub name: String,
    pub kind: String,
    pub shape: ShapeType,
    pub joint: Option<String>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub color: Option<String>,
    pub properties: Properties,
}

impl Shape {
    /// A box of `extent / 10` in model space at the origin.
    pub fn new(name: String, extent: f32) -> Self {
        Self {
            name,
            kind: String::new(),
            shape: ShapeType::Box {
                size: [extent / 10.0; 3],
            },
            joint: None,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            color: None,
            properties: Properties::new(),
        }
    }
}

fn floats<const N: usize>(name: &str, value: &toml::Value) -> Result<[f32; N], String> {
    let items = value
        .as_array()
        .filter(|a| a.len() == N)
        .ok_or_else(|| format!("{name} is not an array of {N} numbers"))?;
    let mut out = [0.0; N];
    for (o, item) in out.iter_mut().zip(items) {
        *o = item
            .as_float()
            .or_else(|| item.as_integer().map(|i| i as f64))
            .ok_or_else(|| format!("{name} is not an array of {N} numbers"))? as f32;
    }
    Ok(out)
}

fn float(name: &str, value: &toml::Value) -> Result<f32, String> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
        .map(|f| f as f32)
        .ok_or_else(|| format!("{name} is not a number"))
}

fn text(name: &str, value: &toml::Value) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{name} is not a string"))
}

fn color(value: &toml::Value) -> Result<String, String> {
    let color = text("color", value)?;
    locator_color(&color)
        .map(|_| color.clone())
        .ok_or_else(|| format!("color {color:?} is not #rrggbb"))
}

fn tags(value: &toml::Value) -> Result<Vec<String>, String> {
    let items = value.as_array().ok_or("tags is not an array of strings")?;
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let tag = text("a tag", item)?;
        if tag.is_empty() || tag.contains(',') || out.contains(&tag) {
            return Err(format!("tag {tag:?} is empty, has a comma or is repeated"));
        }
        out.push(tag);
    }
    Ok(out)
}

/// The tables of an array of tables named `what`, each with a unique non-empty `name`.
fn named_tables(value: toml::Value, what: &str) -> Result<Vec<(String, toml::Table)>, String> {
    let toml::Value::Array(items) = value else {
        return Err(format!("{what}s is not an array of tables"));
    };
    let mut out: Vec<(String, toml::Table)> = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        let toml::Value::Table(mut table) = item else {
            return Err(format!("{what} {} is not a table", i + 1));
        };
        let name = match table.remove("name") {
            Some(value) => text("name", &value).map_err(|e| format!("{what} {}: {e}", i + 1))?,
            None => return Err(format!("{what} {}: no name", i + 1)),
        };
        if name.is_empty() {
            return Err(format!("{what} {}: empty name", i + 1));
        }
        if out.iter().any(|(n, _)| *n == name) {
            return Err(format!("{what} {name:?}: name used twice"));
        }
        out.push((name, table));
    }
    Ok(out)
}

pub fn parse_shapes(value: toml::Value) -> Result<Vec<Shape>, String> {
    named_tables(value, "shape")?
        .into_iter()
        .map(|(name, mut table)| {
            let fail = |e: String| format!("shape {name:?}: {e}");
            let ty = match table.remove("type") {
                Some(v) => text("type", &v).map_err(fail)?,
                None => return Err(fail("no type".into())),
            };
            let index = ShapeType::KEYS
                .iter()
                .position(|k| *k == ty)
                .ok_or_else(|| fail(format!("type {ty:?} is not box, sphere, capsule or hull")))?;
            let positive = |name: &str, v: f32| {
                if v > 0.0 {
                    Ok(v)
                } else {
                    Err(format!("{name} must be more than 0"))
                }
            };
            let radius = table
                .remove("radius")
                .map(|v| float("radius", &v).and_then(|r| positive("radius", r)))
                .transpose()
                .map_err(fail)?;
            let height = table
                .remove("height")
                .map(|v| float("height", &v).and_then(|h| positive("height", h)))
                .transpose()
                .map_err(fail)?;
            let size = table
                .remove("size")
                .map(|v| {
                    let size = floats::<3>("size", &v)?;
                    for s in size {
                        positive("size", s)?;
                    }
                    Ok(size)
                })
                .transpose()
                .map_err(fail)?;
            let translation = table
                .remove("translation")
                .map(|v| floats::<3>("translation", &v))
                .transpose()
                .map_err(fail)?;
            let rotation = table
                .remove("rotation")
                .map(|v| floats::<4>("rotation", &v))
                .transpose()
                .map_err(fail)?;
            if rotation.is_some_and(|r| r == [0.0; 4]) {
                return Err(fail("zero rotation".into()));
            }
            let misplaced = match index {
                0 => radius.map(|_| "radius").or(height.map(|_| "height")),
                1 => size.map(|_| "size").or(height.map(|_| "height")),
                2 => size.map(|_| "size"),
                _ => size
                    .map(|_| "size")
                    .or(radius.map(|_| "radius"))
                    .or(height.map(|_| "height"))
                    .or(translation.map(|_| "translation"))
                    .or(rotation.map(|_| "rotation")),
            };
            if let Some(key) = misplaced {
                return Err(fail(format!("{key} does not apply to a {ty}")));
            }
            let shape = match index {
                0 => ShapeType::Box {
                    size: size.unwrap_or([1.0; 3]),
                },
                1 => ShapeType::Sphere {
                    radius: radius.unwrap_or(0.5),
                },
                2 => ShapeType::Capsule {
                    radius: radius.unwrap_or(0.5),
                    height: height.unwrap_or(1.0),
                },
                _ => ShapeType::Hull,
            };
            let kind = table
                .remove("kind")
                .map(|v| text("kind", &v))
                .transpose()
                .map_err(fail)?;
            let joint = table
                .remove("joint")
                .map(|v| text("joint", &v))
                .transpose()
                .map_err(fail)?;
            let color = table
                .remove("color")
                .map(|v| color(&v))
                .transpose()
                .map_err(fail)?;
            let properties = table
                .remove("properties")
                .map(parse_properties)
                .transpose()
                .map_err(fail)?;
            if let Some(key) = table.keys().next() {
                return Err(fail(format!("unknown key {key:?}")));
            }
            Ok(Shape {
                name: name.clone(),
                kind: kind.unwrap_or_default(),
                shape,
                joint,
                translation: translation.unwrap_or([0.0; 3]),
                rotation: rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
                color,
                properties: properties.unwrap_or_default(),
            })
        })
        .collect()
}

fn write_floats(values: &[f32]) -> String {
    let items: Vec<String> = values.iter().map(|v| format!("{v:?}")).collect();
    format!("[{}]", items.join(", "))
}

fn write_strings(values: &[String]) -> String {
    let items: Vec<String> = values
        .iter()
        .map(|v| toml::Value::from(v.as_str()).to_string())
        .collect();
    format!("[{}]", items.join(", "))
}

pub fn write_shapes(shapes: &[Shape], out: &mut String) {
    for shape in shapes {
        let string = |s: &str| toml::Value::from(s).to_string();
        *out += "\n[[shapes]]\n";
        *out += &format!("name = {}\n", string(&shape.name));
        if !shape.kind.is_empty() {
            *out += &format!("kind = {}\n", string(&shape.kind));
        }
        *out += &format!("type = \"{}\"\n", ShapeType::KEYS[shape.shape.index()]);
        if let Some(joint) = &shape.joint {
            *out += &format!("joint = {}\n", string(joint));
        }
        if shape.shape != ShapeType::Hull {
            *out += &format!("translation = {}\n", write_floats(&shape.translation));
            *out += &format!("rotation = {}\n", write_floats(&shape.rotation));
        }
        match shape.shape {
            ShapeType::Box { size } => *out += &format!("size = {}\n", write_floats(&size)),
            ShapeType::Sphere { radius } => *out += &format!("radius = {radius:?}\n"),
            ShapeType::Capsule { radius, height } => {
                *out += &format!("radius = {radius:?}\nheight = {height:?}\n");
            }
            ShapeType::Hull => {}
        }
        if let Some(color) = &shape.color {
            *out += &format!("color = {}\n", string(color));
        }
        write_properties("shapes.properties", &shape.properties, out);
    }
}

/// A painted triangle: glTF mesh, primitive and triangle index.
pub type TriangleId = (u32, u32, u32);

/// A named set of triangles; see docs/features/regions.md.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub name: String,
    pub tags: Vec<String>,
    pub color: String,
    pub materials: Vec<String>,
    /// Whole glTF primitives: mesh and primitive index.
    pub submeshes: BTreeSet<(u32, u32)>,
    pub triangles: BTreeSet<TriangleId>,
}

/// Most regions a model has: one bit each in a `u32`.
pub const MAX_REGIONS: usize = 32;

pub fn parse_regions(value: toml::Value) -> Result<Vec<Region>, String> {
    let tables = named_tables(value, "region")?;
    if tables.len() > MAX_REGIONS {
        return Err(format!("{} regions, at most {MAX_REGIONS}", tables.len()));
    }
    tables
        .into_iter()
        .map(|(name, mut table)| {
            let fail = |e: String| format!("region {name:?}: {e}");
            let color = match table.remove("color") {
                Some(v) => color(&v).map_err(fail)?,
                None => return Err(fail("no color".into())),
            };
            let tags = table
                .remove("tags")
                .map(|v| tags(&v))
                .transpose()
                .map_err(fail)?
                .unwrap_or_default();
            let mut materials: Vec<String> = Vec::new();
            if let Some(v) = table.remove("materials") {
                let items = v
                    .as_array()
                    .ok_or_else(|| fail("materials is not an array of strings".into()))?;
                for item in items {
                    let material = text("a material", item).map_err(fail)?;
                    if materials.contains(&material) {
                        return Err(fail(format!("material {material:?} is listed twice")));
                    }
                    materials.push(material);
                }
            }
            let mut submeshes = BTreeSet::new();
            if let Some(v) = table.remove("submeshes") {
                let items = v
                    .as_array()
                    .ok_or_else(|| fail("submeshes is not an array of pairs".into()))?;
                for item in items {
                    let pair: Option<Vec<u32>> =
                        item.as_array().filter(|r| r.len() == 2).and_then(|r| {
                            r.iter()
                                .map(|n| n.as_integer().and_then(|n| u32::try_from(n).ok()))
                                .collect()
                        });
                    let Some([mesh, primitive]) = pair.and_then(|n| <[u32; 2]>::try_from(n).ok())
                    else {
                        return Err(fail("a submesh is not [mesh, primitive]".into()));
                    };
                    if !submeshes.insert((mesh, primitive)) {
                        return Err(fail(format!(
                            "submesh [{mesh}, {primitive}] is listed twice"
                        )));
                    }
                }
            }
            let mut triangles = BTreeSet::new();
            if let Some(v) = table.remove("triangles") {
                let runs = v
                    .as_array()
                    .ok_or_else(|| fail("triangles is not an array of runs".into()))?;
                for run in runs {
                    let numbers: Option<Vec<u32>> =
                        run.as_array().filter(|r| r.len() == 4).and_then(|r| {
                            r.iter()
                                .map(|n| n.as_integer().and_then(|n| u32::try_from(n).ok()))
                                .collect()
                        });
                    let Some([mesh, primitive, first, count]) =
                        numbers.and_then(|n| <[u32; 4]>::try_from(n).ok())
                    else {
                        return Err(fail(
                            "a triangle run is not [mesh, primitive, first, count]".into(),
                        ));
                    };
                    if count == 0 {
                        return Err(fail("a triangle run has a count of 0".into()));
                    }
                    let end = first
                        .checked_add(count)
                        .ok_or_else(|| fail("a triangle run is too long".into()))?;
                    triangles.extend((first..end).map(|t| (mesh, primitive, t)));
                }
            }
            if let Some(key) = table.keys().next() {
                return Err(fail(format!("unknown key {key:?}")));
            }
            Ok(Region {
                name: name.clone(),
                tags,
                color,
                materials,
                submeshes,
                triangles,
            })
        })
        .collect()
}

/// `triangles` as runs `[mesh, primitive, first, count]`.
pub fn triangle_runs(triangles: &BTreeSet<TriangleId>) -> Vec<[u32; 4]> {
    let mut runs: Vec<[u32; 4]> = Vec::new();
    for &(mesh, primitive, t) in triangles {
        match runs.last_mut() {
            Some(run) if run[0] == mesh && run[1] == primitive && run[2] + run[3] == t => {
                run[3] += 1;
            }
            _ => runs.push([mesh, primitive, t, 1]),
        }
    }
    runs
}

pub fn write_regions(regions: &[Region], out: &mut String) {
    for region in regions {
        *out += "\n[[regions]]\n";
        *out += &format!("name = {}\n", toml::Value::from(region.name.as_str()));
        if !region.tags.is_empty() {
            *out += &format!("tags = {}\n", write_strings(&region.tags));
        }
        *out += &format!("color = {}\n", toml::Value::from(region.color.as_str()));
        if !region.materials.is_empty() {
            *out += &format!("materials = {}\n", write_strings(&region.materials));
        }
        if !region.submeshes.is_empty() {
            let pairs: Vec<String> = region
                .submeshes
                .iter()
                .map(|(m, p)| format!("[{m}, {p}]"))
                .collect();
            *out += &format!("submeshes = [{}]\n", pairs.join(", "));
        }
        let runs = triangle_runs(&region.triangles);
        if !runs.is_empty() {
            let lines: Vec<String> = runs
                .chunks(4)
                .map(|line| {
                    line.iter()
                        .map(|r| format!("[{}, {}, {}, {}]", r[0], r[1], r[2], r[3]))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect();
            *out += &if lines.len() == 1 {
                format!("triangles = [{}]\n", lines[0])
            } else {
                format!("triangles = [\n    {},\n]\n", lines.join(",\n    "))
            };
        }
    }
}

/// A curve on a model; see docs/features/paths.md.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub name: String,
    pub kind: String,
    pub joint: Option<String>,
    pub points: Vec<[f32; 3]>,
    pub closed: bool,
    pub tension: f32,
    pub alpha: f32,
    pub color: Option<String>,
    pub properties: Properties,
}

impl Path {
    /// Two points in model space: the origin and `extent / 10` along X.
    pub fn new(name: String, extent: f32) -> Self {
        Self {
            name,
            kind: String::new(),
            joint: None,
            points: vec![[0.0; 3], [extent / 10.0, 0.0, 0.0]],
            closed: false,
            tension: 0.0,
            alpha: 0.5,
            color: None,
            properties: Properties::new(),
        }
    }
}

pub fn parse_paths(value: toml::Value) -> Result<Vec<Path>, String> {
    named_tables(value, "path")?
        .into_iter()
        .map(|(name, mut table)| {
            let fail = |e: String| format!("path {name:?}: {e}");
            let unit = |key: &str, v: &toml::Value| {
                let f = float(key, v)?;
                if (0.0..=1.0).contains(&f) {
                    Ok(f)
                } else {
                    Err(format!("{key} is not between 0 and 1"))
                }
            };
            let text_key = |table: &mut toml::Table, key: &str| {
                table.remove(key).map(|v| text(key, &v)).transpose()
            };
            let kind = text_key(&mut table, "kind").map_err(fail)?;
            let joint = text_key(&mut table, "joint").map_err(fail)?;
            let points = match table.remove("points") {
                Some(v) => v
                    .as_array()
                    .ok_or_else(|| fail("points is not an array of points".into()))?
                    .iter()
                    .map(|p| floats::<3>("a point", p))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(fail)?,
                None => Vec::new(),
            };
            let closed = match table.remove("closed") {
                Some(v) => v
                    .as_bool()
                    .ok_or_else(|| fail("closed is not true or false".into()))?,
                None => false,
            };
            let tension = table
                .remove("tension")
                .map(|v| unit("tension", &v))
                .transpose()
                .map_err(fail)?;
            let alpha = table
                .remove("alpha")
                .map(|v| unit("alpha", &v))
                .transpose()
                .map_err(fail)?;
            let color = table
                .remove("color")
                .map(|v| color(&v))
                .transpose()
                .map_err(fail)?;
            let properties = table
                .remove("properties")
                .map(parse_properties)
                .transpose()
                .map_err(fail)?;
            if let Some(key) = table.keys().next() {
                return Err(fail(format!("unknown key {key:?}")));
            }
            Ok(Path {
                name: name.clone(),
                kind: kind.unwrap_or_default(),
                joint,
                points,
                closed,
                tension: tension.unwrap_or(0.0),
                alpha: alpha.unwrap_or(0.5),
                color,
                properties: properties.unwrap_or_default(),
            })
        })
        .collect()
}

pub fn write_paths(paths: &[Path], out: &mut String) {
    for path in paths {
        let string = |s: &str| toml::Value::from(s).to_string();
        *out += "\n[[paths]]\n";
        *out += &format!("name = {}\n", string(&path.name));
        if !path.kind.is_empty() {
            *out += &format!("kind = {}\n", string(&path.kind));
        }
        if let Some(joint) = &path.joint {
            *out += &format!("joint = {}\n", string(joint));
        }
        if path.points.is_empty() {
            *out += "points = []\n";
        } else {
            *out += "points = [\n";
            for point in &path.points {
                *out += &format!("    {},\n", write_floats(point));
            }
            *out += "]\n";
        }
        *out += &format!("closed = {}\n", path.closed);
        *out += &format!("tension = {:?}\n", path.tension);
        *out += &format!("alpha = {:?}\n", path.alpha);
        if let Some(color) = &path.color {
            *out += &format!("color = {}\n", string(color));
        }
        write_properties("paths.properties", &path.properties, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_convert_between_types() {
        let v = PropValue::Text(" 2.6 ".into());
        assert_eq!(v.converted(1), PropValue::Int(3));
        assert_eq!(v.converted(2), PropValue::Float(2.6));
        assert_eq!(v.converted(0), PropValue::Bool(true));
        assert_eq!(
            PropValue::Text("x".into()).converted(2),
            PropValue::Float(0.0)
        );
        assert_eq!(
            PropValue::Bool(true).converted(3),
            PropValue::Text("true".into())
        );
        assert_eq!(
            PropValue::Float(1.5).converted(3),
            PropValue::Text("1.5".into())
        );
        assert_eq!(PropValue::Int(0).converted(0), PropValue::Bool(false));
    }

    #[test]
    fn runs_merge_neighbours() {
        let triangles: BTreeSet<TriangleId> = [
            (0, 0, 1),
            (0, 0, 2),
            (0, 0, 3),
            (0, 0, 7),
            (0, 1, 8),
            (1, 0, 0),
        ]
        .into();
        assert_eq!(
            triangle_runs(&triangles),
            [[0, 0, 1, 3], [0, 0, 7, 1], [0, 1, 8, 1], [1, 0, 0, 1]]
        );
    }

    #[test]
    fn free_names_skip_taken() {
        assert_eq!(free_name("shape", |n| n == "shape"), "shape_2");
    }
}
