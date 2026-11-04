//! Every data file: its instances with their fields, named by the pack's schema.

use std::fmt::Write;

use assetc_sdk as sdk;
use bevy::prelude::*;

use crate::{BAR, InScene, Spawn, TEXT, TEXT_WEAK};

pub fn spawn(s: &mut Spawn) {
    let Some(assets) = s.demo.assets.clone() else {
        return;
    };
    let schemas: Vec<sdk::Schema> = assets
        .of_kind(sdk::Kind::Schema)
        .filter_map(|e| assets.load(e.id).ok())
        .collect();
    let (width, height) = (s.window.width(), s.window.height());
    let files: Vec<_> = assets.of_kind(sdk::Kind::Data).cloned().collect();
    let columns = files.len().clamp(1, 3);
    let column_width = (width - 48.0) / columns as f32;
    for (i, entry) in files.iter().enumerate() {
        let Ok(data) = assets.load::<sdk::Data>(entry.id) else {
            continue;
        };
        let mut text = String::new();
        for instance in &data.instances {
            let _ = writeln!(text, "{} {}", instance.type_name, instance.id);
            write_value(&mut text, &instance.value, &schemas, (&data, &assets), 1);
            text.push('\n');
        }
        let x = -width / 2.0 + 24.0 + (i % columns) as f32 * column_width;
        let top = height / 2.0 - BAR - 24.0;
        s.commands.spawn((
            InScene,
            Text2d::new(entry.path.clone()),
            crate::font(15.0),
            TextColor(TEXT),
            bevy::sprite::Anchor::TOP_LEFT,
            Transform::from_xyz(x, top, 1.0),
        ));
        s.commands.spawn((
            InScene,
            Text2d::new(text),
            crate::font(12.0),
            TextColor(TEXT_WEAK),
            TextLayout::new(Justify::Left, LineBreak::WordBoundary),
            bevy::text::TextBounds::new_horizontal(column_width - 24.0),
            bevy::sprite::Anchor::TOP_LEFT,
            Transform::from_xyz(x, top - 28.0, 1.0),
        ));
    }
}

/// A field's name from any schema, else its hash.
fn field_name(schemas: &[sdk::Schema], hash: u64) -> String {
    schemas
        .iter()
        .find_map(|s| s.field_name(hash))
        .map_or_else(|| format!("#{hash:016x}"), str::to_owned)
}

/// `value` as indented lines: structs one field per line, short lists inline.
/// The data file and the assets, for refs and asset IDs.
type Context<'a> = (&'a sdk::Data, &'a sdk::Assets);

fn write_value(
    out: &mut String,
    value: &sdk::data::Value,
    schemas: &[sdk::Schema],
    cx: Context,
    depth: usize,
) {
    use sdk::data::Value;
    let indent = "  ".repeat(depth);
    match value {
        Value::Struct(fields) => {
            for (hash, field) in fields {
                let name = field_name(schemas, *hash);
                match field {
                    Value::Struct(_) => {
                        let _ = writeln!(out, "{indent}{name}:");
                        write_value(out, field, schemas, cx, depth + 1);
                    }
                    other => {
                        let _ = writeln!(out, "{indent}{name}: {}", inline(other, cx));
                    }
                }
            }
        }
        other => {
            let _ = writeln!(out, "{indent}{}", inline(other, cx));
        }
    }
}

/// A value on one line; an asset as its output path, found by ID.
fn inline(value: &sdk::data::Value, cx: Context) -> String {
    let (data, assets) = cx;
    use sdk::data::Value;
    match value {
        Value::Null => "-".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Text(t) => format!("{t:?}"),
        Value::Color([r, g, b, a]) => format!("#{r:02x}{g:02x}{b:02x}{a:02x}"),
        Value::Enum { name, .. } => name.clone(),
        Value::Asset(0) => "none".into(),
        Value::Asset(id) => match assets.entry(*id) {
            Some(entry) => entry.path.clone(),
            None => format!("asset {id:#018x} (not in the pack)"),
        },
        Value::Ref { id, target } => match target.and_then(|t| data.refs.get(t as usize)) {
            Some(_) => format!("-> {id}"),
            None if id.is_empty() => "none".into(),
            None => format!("-> {id} (missing)"),
        },
        Value::List(items) => {
            let items: Vec<String> = items.iter().map(|v| inline(v, cx)).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Struct(fields) => format!("{{{} fields}}", fields.len()),
    }
}
