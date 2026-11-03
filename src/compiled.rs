//! Compiled schemas, data, graphs and trees: binary chunked files; see docs/spec/atype.md,
//! docs/spec/arec.md, docs/spec/agph.md and docs/spec/atre.md.

use serde_json::Value;

use crate::agraph::Graph;
use crate::amesh::{Strings, Table, put_properties};
use crate::aschema::{Body, Schema, Ty, parse_type};
use crate::chunks::{self, Bytes, Writer};
use crate::hash::fnv1a64;

pub const ATYPE: &str = "ATYPE";
pub const AREC: &str = "AREC";
pub const AGRAPH: &str = "AGRAPH";
pub const ATREE: &str = "ATREE";
pub const VERSION: u32 = 1;

/// Adds text to `strings`, as `offset len` in `out`.
fn text(out: &mut Bytes, strings: &mut Strings, text: &str) {
    let (at, len) = strings.add(text);
    out.u32(at).u32(len);
}

fn f64s(out: &mut Bytes, v: f64) {
    out.u64(v.to_bits());
}

/// `#rrggbb[aa]` as `R G B A` bytes, all zero without one.
fn color_bytes(color: Option<&str>) -> [u8; 4] {
    color
        .and_then(crate::gradient::parse_color)
        .unwrap_or([0; 4])
}

/// A schema as an `.atype`; see docs/spec/atype.md.
pub fn encode_schema(schema: &Schema) -> Vec<u8> {
    let (mut types, mut fields, mut values) =
        (Bytes::default(), Bytes::default(), Bytes::default());
    let mut strings = Strings::default();
    let (mut field_count, mut value_count) = (0u32, 0u32);
    for t in &schema.types {
        types.u64(fnv1a64(t.name.as_bytes()));
        text(&mut types, &mut strings, &t.name);
        match &t.body {
            Body::Struct { fields: list, .. } => {
                types.u32(0).u32(field_count).u32(list.len() as u32).u32(0);
                for f in list {
                    field_count += 1;
                    fields.u64(fnv1a64(f.name.as_bytes()));
                    for value in [&f.name, &f.ty, &f.label, &f.group, &f.doc] {
                        text(&mut fields, &mut strings, value);
                    }
                    let default = match (&f.default, parse_type(&f.ty)) {
                        (Some(d), Ok(ty)) => crate::aschema::canonical(&ty, d).to_string(),
                        (Some(d), Err(_)) => d.to_string(),
                        (None, _) => String::new(),
                    };
                    for value in [&default, &f.show_if, &f.hide_if, &f.widget] {
                        text(&mut fields, &mut strings, value);
                    }
                    let number =
                        |n: &Option<serde_json::Number>| n.as_ref().and_then(|n| n.as_f64());
                    let mut flags = u32::from(f.optional) | u32::from(f.readonly) << 1;
                    for (bit, value) in [
                        (2, number(&f.min)),
                        (3, number(&f.max)),
                        (4, number(&f.step)),
                    ] {
                        f64s(&mut fields, value.unwrap_or(0.0));
                        flags |= u32::from(value.is_some()) << bit;
                    }
                    flags |= u32::from(f.default.is_some()) << 5;
                    let order = number(&f.order);
                    flags |= u32::from(order.is_some()) << 6;
                    fields.u32(flags).f32s(&[order.unwrap_or(0.0) as f32]);
                }
            }
            Body::Enum { values: list } => {
                types.u32(1).u32(value_count).u32(list.len() as u32).u32(0);
                for v in list {
                    value_count += 1;
                    values.u64(fnv1a64(v.as_bytes()));
                    text(&mut values, &mut strings, v);
                }
            }
        }
    }
    let mut desc = Bytes::default();
    desc.u32(schema.types.len() as u32)
        .u32(field_count)
        .u32(value_count)
        .u32(0);
    let mut writer = Writer::new(ATYPE, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("TYPES", types.take())
        .chunk("FIELDS", fields.take())
        .chunk("VALUES", values.take())
        .chunk("STRINGS", strings.0);
    writer.finish()
}

/// The `VALUES` being built for an `.arec`.
struct Rec<'a> {
    schema: &'a Schema,
    values: Vec<[u8; 24]>,
    strings: Strings,
}

impl Rec<'_> {
    fn put(&mut self, slot: usize, name: u64, kind: u32, count: u32, payload: [u8; 8]) {
        let v = &mut self.values[slot];
        v[0..8].copy_from_slice(&name.to_le_bytes());
        v[8..12].copy_from_slice(&kind.to_le_bytes());
        v[12..16].copy_from_slice(&count.to_le_bytes());
        v[16..24].copy_from_slice(&payload);
    }

    fn text(&mut self, text: &str) -> ([u8; 8], u32) {
        let (at, len) = self.strings.add(text);
        let mut payload = [0; 8];
        payload[0..4].copy_from_slice(&at.to_le_bytes());
        (payload, len)
    }

    /// Reserves `n` slots one after another; returns the first.
    fn reserve(&mut self, n: usize) -> usize {
        let first = self.values.len();
        self.values.resize(first + n, [0; 24]);
        first
    }

    fn children(&mut self, slot: usize, name: u64, kind: u32, items: &[(u64, Option<Ty>, Value)]) {
        let first = self.reserve(items.len());
        let mut payload = [0; 8];
        payload[0..4].copy_from_slice(&(first as u32).to_le_bytes());
        self.put(slot, name, kind, items.len() as u32, payload);
        for (i, (child, ty, v)) in items.iter().enumerate() {
            self.value(first + i, *child, ty.as_ref(), v);
        }
    }

    /// Writes `v` (of type `ty` when known) into `slot`; see docs/spec/arec.md#value-24-bytes.
    fn value(&mut self, slot: usize, name: u64, ty: Option<&Ty>, v: &Value) {
        let float = |f: f64| f.to_bits().to_le_bytes();
        match (ty, v) {
            (_, Value::Null) => self.put(slot, name, 0, 0, [0; 8]),
            (_, Value::Bool(b)) => self.put(slot, name, 1, 0, u64::from(*b).to_le_bytes()),
            (Some(Ty::Prim(c @ ("rgb" | "rgba"))), Value::String(s)) => {
                let mut rgba = color_bytes(Some(s));
                if *c == "rgb" {
                    rgba[3] = 255;
                }
                let mut payload = [0; 8];
                payload[0..4].copy_from_slice(&rgba);
                self.put(slot, name, 5, 0, payload);
            }
            (Some(Ty::Prim("curve")), Value::Array(points)) => {
                let items: Vec<(u64, Option<Ty>, Value)> = points
                    .iter()
                    .map(|p| (0, Some(Ty::List(Box::new(Ty::Prim("f32")))), p.clone()))
                    .collect();
                self.children(slot, name, 7, &items);
            }
            (Some(Ty::Prim("gradient")), Value::Array(stops)) => {
                let items: Vec<(u64, Option<Ty>, Value)> = stops
                    .iter()
                    .map(|s| (0, Some(Ty::Named("$stop".into())), s.clone()))
                    .collect();
                self.children(slot, name, 7, &items);
            }
            // A gradient stop: its position, then its color.
            (Some(Ty::Named(n)), Value::Array(stop)) if n == "$stop" => {
                let items: Vec<(u64, Option<Ty>, Value)> = stop
                    .iter()
                    .zip([Ty::Prim("f32"), Ty::Prim("rgba")])
                    .map(|(v, ty)| (0, Some(ty), v.clone()))
                    .collect();
                self.children(slot, name, 7, &items);
            }
            (Some(Ty::List(item)), Value::Array(items)) => {
                let items: Vec<(u64, Option<Ty>, Value)> = items
                    .iter()
                    .map(|v| (0, Some((**item).clone()), v.clone()))
                    .collect();
                self.children(slot, name, 7, &items);
            }
            (Some(Ty::Ref(_)), Value::String(s)) => {
                let (payload, len) = self.text(s);
                self.put(slot, name, 8, len, payload);
            }
            (Some(ty @ Ty::Prim(_)), Value::Number(n)) if ty.is_integer() => {
                let i = n
                    .as_i64()
                    .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64);
                self.put(slot, name, 2, 0, i.to_le_bytes());
            }
            (Some(Ty::Named(n)), v) if self.schema.type_index(n).is_some() => {
                let index = self.schema.type_index(n).unwrap_or_default();
                match (&self.schema.types[index].body, v) {
                    (Body::Enum { values }, Value::String(s)) => {
                        let position = values.iter().position(|e| e == s).unwrap_or(0) as u32;
                        let (mut payload, len) = self.text(s);
                        payload[4..8].copy_from_slice(&position.to_le_bytes());
                        self.put(slot, name, 9, len, payload);
                    }
                    (Body::Struct { fields, .. }, Value::Object(o)) => {
                        let items: Vec<(u64, Option<Ty>, Value)> = fields
                            .iter()
                            .map(|f| {
                                let v = o.get(&f.name).cloned().unwrap_or(Value::Null);
                                (fnv1a64(f.name.as_bytes()), parse_type(&f.ty).ok(), v)
                            })
                            .collect();
                        self.children(slot, name, 6, &items);
                    }
                    _ => self.value(slot, name, None, v),
                }
            }
            (_, Value::Number(n)) => match (ty, n.as_i64()) {
                (None, Some(i)) => self.put(slot, name, 2, 0, i.to_le_bytes()),
                _ => self.put(slot, name, 3, 0, float(n.as_f64().unwrap_or(0.0))),
            },
            (_, Value::String(s)) => {
                let (payload, len) = self.text(s);
                self.put(slot, name, 4, len, payload);
            }
            (_, Value::Array(items)) => {
                let items: Vec<(u64, Option<Ty>, Value)> =
                    items.iter().map(|v| (0, None, v.clone())).collect();
                self.children(slot, name, 7, &items);
            }
            (_, Value::Object(o)) => {
                let items: Vec<(u64, Option<Ty>, Value)> = o
                    .iter()
                    .map(|(k, v)| (fnv1a64(k.as_bytes()), None, v.clone()))
                    .collect();
                self.children(slot, name, 6, &items);
            }
        }
    }
}

/// A data file as an `.arec`, every field resolved; see docs/spec/arec.md.
pub fn encode_data(data: &crate::adata::DataFile, schema: &Schema) -> Vec<u8> {
    let mut rec = Rec {
        schema,
        values: Vec::new(),
        strings: Strings::default(),
    };
    let (mut types, mut instances) = (Bytes::default(), Bytes::default());
    let mut used: Vec<&str> = Vec::new();
    for inst in &data.instances {
        let Some(si) = crate::adata::struct_index(schema, &inst.ty) else {
            continue;
        };
        let type_index = match used.iter().position(|t| *t == inst.ty) {
            Some(i) => i,
            None => {
                used.push(&inst.ty);
                types.u64(fnv1a64(inst.ty.as_bytes()));
                let (at, len) = rec.strings.add(&inst.ty);
                types.u32(at).u32(len);
                used.len() - 1
            }
        };
        let slot = rec.reserve(1);
        let resolved = crate::adata::resolve(schema, si, &inst.values, None);
        rec.value(
            slot,
            0,
            Some(&Ty::Named(inst.ty.clone())),
            &Value::Object(resolved),
        );
        instances
            .u32(type_index as u32)
            .u32(slot as u32)
            .u64(fnv1a64(inst.id.as_bytes()));
        let (at, len) = rec.strings.add(&inst.id);
        instances.u32(at).u32(len).u64(0);
    }
    let mut desc = Bytes::default();
    desc.u32(data.instances.len() as u32)
        .u32(rec.values.len() as u32)
        .u32(used.len() as u32)
        .u32(0);
    let values: Vec<u8> = rec.values.concat();
    let mut writer = Writer::new(AREC, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("TYPES", types.take())
        .chunk("INSTANCE", instances.take())
        .chunk("VALUES", values)
        .chunk("STRINGS", rec.strings.0);
    writer.finish()
}

/// A node's shared head: id hash, id, label, position and color.
fn node_head(out: &mut Bytes, strings: &mut Strings, n: &crate::agraph::Node) {
    out.u64(fnv1a64(n.id.as_bytes()));
    text(out, strings, &n.id);
    text(out, strings, &n.label);
    out.f32s(&[n.x as f32, n.y as f32]);
    out.0.extend_from_slice(&color_bytes(n.color.as_deref()));
}

/// A graph as an `.agph`; see docs/spec/agph.md.
pub fn encode_graph(graph: &Graph) -> Vec<u8> {
    let (mut nodes, mut edges, mut props) = (Bytes::default(), Bytes::default(), Bytes::default());
    let mut strings = Strings::default();
    let mut property_count = 0u32;
    for n in &graph.nodes {
        node_head(&mut nodes, &mut strings, n);
        nodes.u32(property_count).u32(n.data.len() as u32).u32(0);
        put_properties(&mut props, &mut strings, &n.data);
        property_count += n.data.len() as u32;
    }
    for e in &graph.edges {
        let index = |id: &str| graph.node_index(id).unwrap_or(0) as u32;
        edges.u32(index(&e.from)).u32(index(&e.to)).i32(e.weight);
        text(&mut edges, &mut strings, &e.label);
        edges.u32(property_count).u32(e.data.len() as u32).u32(0);
        put_properties(&mut props, &mut strings, &e.data);
        property_count += e.data.len() as u32;
    }
    let mut desc = Bytes::default();
    desc.u32(graph.nodes.len() as u32)
        .u32(graph.edges.len() as u32)
        .u32(property_count)
        .u32(0);
    let mut writer = Writer::new(AGRAPH, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("NODES", nodes.take())
        .chunk("EDGES", edges.take());
    if property_count > 0 {
        writer.chunk("PROPS", props.take());
    }
    writer.chunk("STRINGS", strings.0);
    writer.finish()
}

/// A tree as an `.atre`, nodes breadth first; see docs/spec/atre.md.
pub fn encode_tree(tree: &Graph) -> Vec<u8> {
    use crate::atree::{children, depth};
    // Breadth first, with each node's depth and parent (by place in the order).
    let mut order: Vec<(usize, i32, u32)> = Vec::new();
    if !tree.nodes.is_empty() {
        order.push((0, -1, 0));
    }
    let mut k = 0;
    while k < order.len() {
        let (node, _, level) = order[k];
        for c in children(tree, node) {
            if !order.iter().any(|(n, _, _)| *n == c) {
                order.push((c, k as i32, level + 1));
            }
        }
        k += 1;
    }
    let (mut nodes, mut props) = (Bytes::default(), Bytes::default());
    let mut strings = Strings::default();
    let mut property_count = 0u32;
    for (place, &(node, parent, level)) in order.iter().enumerate() {
        let n = &tree.nodes[node];
        let first_child = order
            .iter()
            .position(|(_, p, _)| *p == place as i32)
            .unwrap_or(0);
        let child_count = order.iter().filter(|(_, p, _)| *p == place as i32).count();
        node_head(&mut nodes, &mut strings, n);
        nodes
            .i32(parent)
            .u32(first_child as u32)
            .u32(child_count as u32)
            .u32(property_count)
            .u32(n.data.len() as u32)
            .u32(level)
            .u32(0);
        put_properties(&mut props, &mut strings, &n.data);
        property_count += n.data.len() as u32;
    }
    let mut desc = Bytes::default();
    desc.u32(order.len() as u32)
        .u32(property_count)
        .u32(depth(tree) as u32)
        .u32(0);
    let mut writer = Writer::new(ATREE, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("NODES", nodes.take());
    if property_count > 0 {
        writer.chunk("PROPS", props.take());
    }
    writer.chunk("STRINGS", strings.0);
    writer.finish()
}

/// The format of a compiled schema, data, graph or tree file.
pub fn format_of(bytes: &[u8]) -> Option<&'static str> {
    [
        ATYPE,
        AREC,
        AGRAPH,
        ATREE,
        crate::aanim::FORMAT,
        crate::atlas::FORMAT,
    ]
    .into_iter()
    .find(|f| chunks::Reader::parse(bytes, f).is_ok())
}

/// Info rows: the kind's counts from `DESC`; see docs/features/inspector.md#compiled-data.
pub fn rows(bytes: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let format = format_of(bytes).ok_or("not a compiled schema, data, graph or tree")?;
    let reader = chunks::Reader::parse(bytes, format)?;
    let desc = reader.require("DESC")?;
    let field = |at| chunks::u32_at(desc, 0, 16, at).unwrap_or(0).to_string();
    if format == crate::atlas::FORMAT {
        let u = |at| chunks::u32_at(desc, 0, 48, at).unwrap_or(0);
        let packing = match u(12) {
            0 => format!("grid {} × {}", u(16), u(8).div_ceil(u(16).max(1))),
            _ => "pack".into(),
        };
        return Ok(vec![
            ("Frames", u(8).to_string()),
            ("Size", format!("{} × {} px", u(0), u(4))),
            ("Packing", packing),
        ]);
    }
    let names: [&'static str; 3] = match format {
        ATYPE => ["Types", "Fields", "Values"],
        AREC => ["Instances", "Values", "Types"],
        AGRAPH => ["Nodes", "Edges", "Properties"],
        _ => ["Nodes", "Properties", "Depth"],
    };
    Ok(names
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, field(i * 4)))
        .collect())
}

/// What a record column holds; see docs/features/inspector.md#compiled-data.
#[derive(Clone, Copy)]
enum Col {
    U32(usize),
    I32(usize),
    F32(usize),
    F64(usize),
    Hex(usize),
    Text(usize),
    Color(usize),
    TypeKind(usize),
    Flags(usize),
    ValueKind,
    ValuePayload,
    Byte(usize),
    ChannelPath(usize),
    Interpolation(usize),
    PropType,
    PropValue,
}

/// A chunk's record size and columns, per format.
fn layout(format: &str, chunk: &str) -> Option<(usize, Vec<(&'static str, Col)>)> {
    use Col::*;
    let node = vec![
        ("Id", Text(8)),
        ("Hash", Hex(0)),
        ("Label", Text(16)),
        ("X", F32(24)),
        ("Y", F32(28)),
        ("Color", Color(32)),
    ];
    Some(match (format, chunk) {
        (ATYPE, "DESC") => (
            16,
            vec![
                ("type_count", U32(0)),
                ("field_count", U32(4)),
                ("value_count", U32(8)),
            ],
        ),
        (AREC, "DESC") => (
            16,
            vec![
                ("instance_count", U32(0)),
                ("value_count", U32(4)),
                ("type_count", U32(8)),
            ],
        ),
        (AGRAPH, "DESC") => (
            16,
            vec![
                ("node_count", U32(0)),
                ("edge_count", U32(4)),
                ("property_count", U32(8)),
            ],
        ),
        (ATREE, "DESC") => (
            16,
            vec![
                ("node_count", U32(0)),
                ("property_count", U32(4)),
                ("depth", U32(8)),
            ],
        ),
        (ATYPE, "TYPES") => (
            32,
            vec![
                ("Name", Text(8)),
                ("Hash", Hex(0)),
                ("Kind", TypeKind(16)),
                ("First", U32(20)),
                ("Count", U32(24)),
            ],
        ),
        (ATYPE, "FIELDS") => (
            112,
            vec![
                ("Name", Text(8)),
                ("Type", Text(16)),
                ("Label", Text(24)),
                ("Group", Text(32)),
                ("Default", Text(48)),
                ("Show if", Text(56)),
                ("Hide if", Text(64)),
                ("Widget", Text(72)),
                ("Min", F64(80)),
                ("Max", F64(88)),
                ("Step", F64(96)),
                ("Flags", Flags(104)),
                ("Order", F32(108)),
                ("Doc", Text(40)),
            ],
        ),
        (ATYPE, "VALUES") => (16, vec![("Name", Text(8)), ("Hash", Hex(0))]),
        (AREC, "TYPES") => (16, vec![("Name", Text(8)), ("Hash", Hex(0))]),
        (AREC, "INSTANCE") => (
            32,
            vec![
                ("Id", Text(16)),
                ("Hash", Hex(8)),
                ("Type", U32(0)),
                ("Value", U32(4)),
            ],
        ),
        (AREC, "VALUES") => (
            24,
            vec![
                ("Name hash", Hex(0)),
                ("Kind", ValueKind),
                ("Count", U32(12)),
                ("Value", ValuePayload),
            ],
        ),
        (AGRAPH, "NODES") => {
            let mut columns = node;
            columns.extend([("First property", U32(36)), ("Properties", U32(40))]);
            (48, columns)
        }
        (AGRAPH, "EDGES") => (
            32,
            vec![
                ("From", U32(0)),
                ("To", U32(4)),
                ("Weight", I32(8)),
                ("Label", Text(12)),
                ("First property", U32(20)),
                ("Properties", U32(24)),
            ],
        ),
        (ATREE, "NODES") => {
            let mut columns = node;
            columns.extend([
                ("Parent", I32(36)),
                ("First child", U32(40)),
                ("Children", U32(44)),
                ("First property", U32(48)),
                ("Properties", U32(52)),
                ("Depth", U32(56)),
            ]);
            (64, columns)
        }
        (AGRAPH | ATREE, "PROPS") => (
            32,
            vec![("Key", Text(8)), ("Type", PropType), ("Value", PropValue)],
        ),
        // Atlases; see docs/spec/aatlas.md.
        ("AATLAS", "DESC") => (
            48,
            vec![
                ("width", U32(0)),
                ("height", U32(4)),
                ("frame_count", U32(8)),
                ("packing", U32(12)),
                ("columns", U32(16)),
                ("cell_width", U32(20)),
                ("cell_height", U32(24)),
                ("padding", U32(28)),
                ("texture", Hex(32)),
            ],
        ),
        ("AATLAS", "FRAMES") => (
            64,
            vec![
                ("Name", Text(8)),
                ("Hash", Hex(0)),
                ("X", U32(16)),
                ("Y", U32(20)),
                ("Width", U32(24)),
                ("Height", U32(28)),
                ("Source W", U32(32)),
                ("Source H", U32(36)),
                ("Offset X", U32(40)),
                ("Offset Y", U32(44)),
                ("U0", F32(48)),
                ("V0", F32(52)),
                ("U1", F32(56)),
                ("V1", F32(60)),
            ],
        ),
        // Animations; see docs/spec/aanim.md.
        ("AANIM", "DESC") => (
            32,
            vec![
                ("clip_count", U32(0)),
                ("channel_count", U32(4)),
                ("key_count", U32(8)),
                ("value_count", U32(12)),
                ("names_size", U32(16)),
                ("joint_count", U32(20)),
                ("mesh", Hex(24)),
            ],
        ),
        ("AANIM", "CLIPS") => (
            24,
            vec![
                ("Name", Text(0)),
                ("Duration", F32(8)),
                ("First channel", U32(12)),
                ("Channels", U32(16)),
            ],
        ),
        ("AANIM", "CHANNELS") => (
            24,
            vec![
                ("Joint", U32(0)),
                ("Path", ChannelPath(4)),
                ("Interpolation", Interpolation(5)),
                ("Components", Byte(6)),
                ("First key", U32(8)),
                ("Keys", U32(12)),
                ("First value", U32(16)),
            ],
        ),
        ("AANIM", "TIMES" | "VALUES") => (4, vec![("Value", F32(0))]),
        ("AANIM", "EVENTS") => (
            32,
            vec![
                ("Name", Text(16)),
                ("Clip", U32(0)),
                ("Time", F32(4)),
                ("Hash", Hex(8)),
                ("First property", U32(24)),
                ("Properties", U32(28)),
            ],
        ),
        ("AANIM", "EVPROPS") => (
            32,
            vec![("Key", Text(8)), ("Type", PropType), ("Value", PropValue)],
        ),
        _ => return None,
    })
}

/// The chunk list's descriptions.
pub fn describe(name: &str) -> &'static str {
    match name {
        "DESC" => "Descriptor",
        "TYPES" => "Types",
        "FIELDS" => "Fields",
        "VALUES" => "Values",
        "INSTANCE" => "Instances",
        "NODES" => "Nodes",
        "EDGES" => "Edges",
        "PROPS" => "Properties",
        "STRINGS" => "Strings",
        "CLIPS" => "Clips",
        "CHANNELS" => "Channels",
        "TIMES" => "Key times",
        "NAMES" => "Clip names",
        "EVENTS" => "Events",
        "EVPROPS" => "Event properties",
        "EVTEXT" => "Event text",
        "FRAMES" => "Frames",
        _ => "Unknown chunk",
    }
}

/// Chunks of text, shown as rows of bytes.
fn is_text(name: &str) -> bool {
    matches!(name, "STRINGS" | "NAMES" | "EVTEXT")
}

/// Text chunks as rows of this many bytes.
const STRING_ROW: usize = 48;

pub fn table(bytes: &[u8], name: &str) -> Option<Table> {
    let format = format_of(bytes)?;
    let reader = chunks::Reader::parse(bytes, format).ok()?;
    let payload = reader.chunk(name)?;
    if is_text(name) {
        return Some(Table {
            columns: vec!["Offset".into(), "Text".into()],
            rows: payload.len().div_ceil(STRING_ROW),
            numbered: false,
        });
    }
    let (stride, columns) = layout(format, name)?;
    Some(Table {
        columns: columns.iter().map(|(n, _)| (*n).to_owned()).collect(),
        rows: payload.len() / stride,
        numbered: name != "DESC",
    })
}

pub fn cell(bytes: &[u8], name: &str, row: usize, column: usize) -> String {
    let Some(format) = format_of(bytes) else {
        return String::new();
    };
    let Ok(reader) = chunks::Reader::parse(bytes, format) else {
        return String::new();
    };
    let text_chunk = match name {
        "CLIPS" => "NAMES",
        "EVENTS" | "EVPROPS" => "EVTEXT",
        _ => "STRINGS",
    };
    let (Some(p), strings) = (
        reader.chunk(name),
        reader.chunk(text_chunk).unwrap_or_default(),
    ) else {
        return String::new();
    };
    if is_text(name) {
        let at = row * STRING_ROW;
        return match column {
            0 => at.to_string(),
            _ => p
                .get(at..(at + STRING_ROW).min(p.len()))
                .map(|t| String::from_utf8_lossy(t).into_owned())
                .unwrap_or_default(),
        };
    }
    let Some((stride, columns)) = layout(format, name) else {
        return String::new();
    };
    let numbered = name != "DESC";
    let column = match (numbered, column) {
        (true, 0) => return row.to_string(),
        (true, c) => c - 1,
        (false, c) => c,
    };
    let Some(&(_, col)) = columns.get(column) else {
        return String::new();
    };
    let base = row * stride;
    let bytes_at = |at: usize, n: usize| p.get(base + at..base + at + n);
    let u32_at =
        |at: usize| bytes_at(at, 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
    let u64_at =
        |at: usize| bytes_at(at, 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()));
    let text_at = |at: usize, len: usize| {
        strings
            .get(at..at + len)
            .map(|t| String::from_utf8_lossy(t).into_owned())
            .unwrap_or_default()
    };
    match col {
        Col::U32(at) => u32_at(at).to_string(),
        Col::I32(at) => (u32_at(at) as i32).to_string(),
        Col::F32(at) => format!("{}", f32::from_bits(u32_at(at))),
        Col::F64(at) => format!("{}", f64::from_bits(u64_at(at))),
        Col::Hex(at) => format!("{:#018x}", u64_at(at)),
        Col::Text(at) => text_at(u32_at(at) as usize, u32_at(at + 4) as usize),
        Col::Color(at) => match bytes_at(at, 4) {
            Some([0, 0, 0, 0]) | None => "—".into(),
            Some(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3]),
        },
        Col::TypeKind(at) => match u32_at(at) {
            0 => "struct".into(),
            1 => "enum".into(),
            n => n.to_string(),
        },
        Col::Flags(at) => {
            let flags = u32_at(at);
            let names = [
                "optional", "readonly", "min", "max", "step", "default", "order",
            ];
            let set: Vec<&str> = names
                .iter()
                .enumerate()
                .filter(|(i, _)| flags & (1 << i) != 0)
                .map(|(_, n)| *n)
                .collect();
            set.join(", ")
        }
        Col::ValueKind => match u32_at(8) {
            0 => "null",
            1 => "bool",
            2 => "int",
            3 => "float",
            4 => "text",
            5 => "color",
            6 => "struct",
            7 => "list",
            8 => "ref",
            9 => "enum",
            _ => "?",
        }
        .into(),
        Col::ValuePayload => {
            let count = u32_at(12);
            match u32_at(8) {
                0 => "null".into(),
                1 => (u64_at(16) != 0).to_string(),
                2 => (u64_at(16) as i64).to_string(),
                3 => format!("{}", f64::from_bits(u64_at(16))),
                4 | 8 => format!("{:?}", text_at(u32_at(16) as usize, count as usize)),
                5 => bytes_at(16, 4).map_or_else(String::new, |c| {
                    format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
                }),
                6 | 7 => format!("→ {} ({count})", u32_at(16)),
                9 => format!(
                    "{} (#{})",
                    text_at(u32_at(16) as usize, count as usize),
                    u32_at(20)
                ),
                _ => String::new(),
            }
        }
        Col::Byte(at) => bytes_at(at, 1).map_or(0, |b| b[0]).to_string(),
        Col::ChannelPath(at) => match bytes_at(at, 1).map(|b| b[0]) {
            Some(0) => "translation".into(),
            Some(1) => "rotation".into(),
            Some(2) => "scale".into(),
            other => other.unwrap_or_default().to_string(),
        },
        Col::Interpolation(at) => match bytes_at(at, 1).map(|b| b[0]) {
            Some(0) => "step".into(),
            Some(1) => "linear".into(),
            other => other.unwrap_or_default().to_string(),
        },
        Col::PropType => match u32_at(16) {
            0 => "Bool",
            1 => "Int",
            2 => "Float",
            3 => "Text",
            _ => "?",
        }
        .into(),
        Col::PropValue => match u32_at(16) {
            0 => (u64_at(24) != 0).to_string(),
            1 => (u64_at(24) as i64).to_string(),
            2 => format!("{}", f64::from_bits(u64_at(24))),
            _ => format!("{:?}", text_at(u32_at(24) as usize, u32_at(28) as usize)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_and_data_read_back_as_tables() {
        let schema = Schema::parse(
            r##"{ "$dialect": "a/1", "types": {
              "Rarity": { "kind": "enum", "values": ["common", "rare"] },
              "Weapon": { "kind": "struct", "fields": [
                { "name": "name", "type": "string" },
                { "name": "damage", "type": "i32", "default": 10, "min": 0 },
                { "name": "rarity", "type": "Rarity" },
                { "name": "tint", "type": "rgb", "default": "#ff8000" },
                { "name": "tags", "type": "list<string>" } ] } } }"##,
        )
        .unwrap();
        let types = encode_schema(&schema);
        assert_eq!(format_of(&types), Some(ATYPE));
        assert_eq!(rows(&types).unwrap()[1], ("Fields", "5".into()));
        assert_eq!(cell(&types, "TYPES", 1, 1), "Weapon");
        assert_eq!(cell(&types, "FIELDS", 1, 5), "10");
        assert_eq!(cell(&types, "FIELDS", 1, 12), "min, default");

        let data = crate::adata::DataFile::parse(
            r##"{ "$dialect": "a/1", "$schema": "s.aschema", "instances": [
              { "$type": "Weapon", "$id": "sword", "name": "Sword", "rarity": "rare", "tags": ["a"] } ] }"##,
        )
        .unwrap();
        let rec = encode_data(&data, &schema);
        assert_eq!(format_of(&rec), Some(AREC));
        assert_eq!(cell(&rec, "INSTANCE", 0, 1), "sword");
        // The struct, then its five fields, then the tag list's item.
        let kinds: Vec<String> = (0..7).map(|r| cell(&rec, "VALUES", r, 2)).collect();
        assert_eq!(
            kinds,
            ["struct", "text", "int", "enum", "color", "list", "text"]
        );
        assert_eq!(cell(&rec, "VALUES", 2, 4), "10");
        assert_eq!(cell(&rec, "VALUES", 3, 4), "rare (#1)");
        assert_eq!(cell(&rec, "VALUES", 4, 4), "#ff8000ff");
        assert_eq!(cell(&rec, "VALUES", 5, 4), "→ 6 (1)");
    }

    #[test]
    fn trees_are_breadth_first() {
        let tree = crate::atree::parse(
            r##"{ "$dialect": "atree/1", "root": { "id": "r", "x": 0, "y": 0, "children": [
              { "id": "a", "x": 0, "y": 1, "children": [ { "id": "a1", "x": 0, "y": 2 } ] },
              { "id": "b", "x": 1, "y": 1, "color": "#ef4444", "data": { "cost": 2 } } ] } }"##,
        )
        .unwrap();
        let bytes = encode_tree(&tree);
        let ids: Vec<String> = (0..4).map(|r| cell(&bytes, "NODES", r, 1)).collect();
        assert_eq!(ids, ["r", "a", "b", "a1"]);
        // Root: children 1 and 2; a: child 3.
        assert_eq!(cell(&bytes, "NODES", 0, 8), "1");
        assert_eq!(cell(&bytes, "NODES", 0, 9), "2");
        assert_eq!(cell(&bytes, "NODES", 1, 8), "3");
        assert_eq!(cell(&bytes, "NODES", 3, 7), "1");
        assert_eq!(cell(&bytes, "NODES", 2, 6), "#ef4444ff");
        assert_eq!(cell(&bytes, "PROPS", 0, 3), "2");
        let graph = crate::agraph::Graph::new();
        assert_eq!(
            rows(&encode_graph(&graph)).unwrap()[0],
            ("Nodes", "1".into())
        );
    }
}
