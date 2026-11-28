//! `.anoise` files and modifier tables: reading and writing. See docs/spec/anoise.md; the
//! generators themselves are the assetc-noise crate (docs/features/noise-crate.md).

// Generators, parameters and color modifiers; see docs/features/noise-crate.md.
pub use assetc_noise::{
    GeneratorKind, MAX_SIZE, MIN_SIZE, Modifier, ModifierKind, Noise, ParamKind, ParamSpec, Params,
    Value, apply_modifiers,
};

/// Reads `specs` from `table` (removing them); missing keys get defaults.
fn take_params(specs: &[ParamSpec], table: &mut toml::Table) -> Result<Params, String> {
    let mut params = Params::defaults(specs);
    for (spec, value) in specs.iter().zip(&mut params.0) {
        let Some(raw) = table.remove(spec.key) else {
            continue;
        };
        let bad = || format!("{} has an invalid value {raw}", spec.key);
        *value = match spec.kind {
            ParamKind::Int { min, max, .. } => {
                let v = raw.as_integer().ok_or_else(bad)?;
                in_range(spec.key, v as f64, min as f64, max as f64)?;
                Value::Int(v)
            }
            ParamKind::Float { min, max, .. } => {
                let v = number(&raw).ok_or_else(bad)?;
                in_range(spec.key, v, min, max)?;
                Value::Float(v)
            }
            ParamKind::Gray { .. } => {
                let v = number(&raw).ok_or_else(bad)?;
                in_range(spec.key, v, 0.0, 1.0)?;
                Value::Float(v)
            }
            ParamKind::Choice { options, .. } => {
                let v = raw.as_str().ok_or_else(bad)?;
                let i = options
                    .iter()
                    .position(|o| *o == v)
                    .ok_or_else(|| format!("{} must be one of {}", spec.key, options.join(", ")))?;
                Value::Choice(i)
            }
        };
    }
    Ok(params)
}

fn write_params(params: &Params, specs: &[ParamSpec], out: &mut String) {
    for (spec, value) in specs.iter().zip(&params.0) {
        let value = match (value, spec.kind) {
            (Value::Int(v), _) => toml::Value::Integer(*v),
            (Value::Float(v), _) => toml::Value::Float(*v),
            (Value::Choice(i), ParamKind::Choice { options, .. }) => {
                toml::Value::String(options[*i].into())
            }
            (Value::Choice(_), _) => unreachable!("choice without options"),
        };
        out.push_str(&format!("{} = {value}\n", spec.key));
    }
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
        .filter(|f| f.is_finite())
}

fn in_range(key: &str, v: f64, min: f64, max: f64) -> Result<(), String> {
    if (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(format!("{key} must be {min}..={max}, not {v}"))
    }
}

/// Reads an `.anoise` file.
pub fn parse(text: &str) -> Result<Noise, String> {
    let mut table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| e.message().to_owned())?;
    let kind = match table.remove("type") {
        None => GeneratorKind::default(),
        Some(v) => v
            .as_str()
            .and_then(GeneratorKind::from_key)
            .ok_or_else(|| format!("unknown type {v}"))?,
    };
    let size = match table.remove("size") {
        None => 512,
        Some(v) => {
            let v = v.as_integer().ok_or("size must be an integer")?;
            in_range("size", v as f64, MIN_SIZE as f64, MAX_SIZE as f64)?;
            v as u32
        }
    };
    let modifiers = match table.remove("modifiers") {
        None => Vec::new(),
        Some(value) => parse_modifiers(value)?,
    };
    let mut sections = Vec::new();
    for (group, value) in table {
        let toml::Value::Table(group_table) = value else {
            return Err(format!(
                "unknown key {group} (parameters go in [{}])",
                kind.section()
            ));
        };
        for (name, value) in group_table {
            let section = format!("{group}.{name}");
            let generator = GeneratorKind::from_key(&format!("{group}/{name}"))
                .ok_or_else(|| format!("unknown section [{section}]"))?;
            let toml::Value::Table(mut values) = value else {
                return Err(format!("[{section}] must be a table"));
            };
            let params = take_params(generator.specs(), &mut values)
                .map_err(|e| format!("[{section}] {e}"))?;
            if let Some(key) = values.keys().next() {
                return Err(format!("[{section}] has no parameter {key}"));
            }
            sections.push((generator, params));
        }
    }
    let params = match sections.iter().position(|(k, _)| *k == kind) {
        Some(i) => sections.remove(i).1,
        None => Params::defaults(kind.specs()),
    };
    sections.retain(|(k, p)| *p != Params::defaults(k.specs()));
    sections.sort_by_key(|(k, _)| k.index());
    Ok(Noise {
        kind,
        size,
        params,
        kept: sections,
        modifiers,
    })
}

/// The written form, see docs/spec/anoise.md#written-form.
pub fn to_toml(noise: &Noise) -> String {
    let mut out = format!(
        "type = {}\nsize = {}\n\n[{}]\n",
        toml::Value::String(noise.kind.key().into()),
        noise.size,
        noise.kind.section()
    );
    write_params(&noise.params, noise.kind.specs(), &mut out);
    for (kind, params) in &noise.kept {
        if *params != Params::defaults(kind.specs()) {
            out.push_str(&format!("\n[{}]\n", kind.section()));
            write_params(params, kind.specs(), &mut out);
        }
    }
    write_modifiers(&noise.modifiers, &mut out);
    out
}

/// A `modifiers` array of tables, as in `.anoise` files and `.amod` sidecars.
pub fn parse_modifiers(value: toml::Value) -> Result<Vec<Modifier>, String> {
    let toml::Value::Array(items) = value else {
        return Err("modifiers must be an array of tables".into());
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| parse_modifier(item).map_err(|e| format!("modifier {}: {e}", i + 1)))
        .collect()
}

/// Appends `[[modifiers]]` tables, each after a blank line.
pub fn write_modifiers(modifiers: &[Modifier], out: &mut String) {
    write_modifier_tables(modifiers, "modifiers", out);
}

/// Appends `[[<table>]]` tables (`modifiers`, `gradients.modifiers`), each after a blank line.
pub fn write_modifier_tables(modifiers: &[Modifier], table: &str, out: &mut String) {
    for m in modifiers {
        out.push_str(&format!(
            "\n[[{table}]]\ntype = {}\nenabled = {}\n",
            toml::Value::String(m.kind.key().into()),
            m.enabled
        ));
        write_params(&m.params, m.kind.specs(), out);
    }
}

/// `Levels, Blur (off)`, or `none`.
pub fn modifier_summary(modifiers: &[Modifier]) -> String {
    if modifiers.is_empty() {
        return "none".into();
    }
    modifiers
        .iter()
        .map(|m| {
            let off = if m.enabled { "" } else { " (off)" };
            format!("{}{off}", m.kind.name())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_modifier(item: toml::Value) -> Result<Modifier, String> {
    let toml::Value::Table(mut table) = item else {
        return Err("not a table".into());
    };
    let kind = table
        .remove("type")
        .and_then(|v| v.as_str().and_then(ModifierKind::from_key))
        .ok_or("missing or unknown type")?;
    let enabled = match table.remove("enabled") {
        None => true,
        Some(v) => v.as_bool().ok_or("enabled must be true or false")?,
    };
    let params = take_params(kind.specs(), &mut table)?;
    if let Some(key) = table.keys().next() {
        return Err(format!("{} has no parameter {key}", kind.key()));
    }
    Ok(Modifier {
        kind,
        enabled,
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_form_round_trips() {
        let mut noise = Noise::new(GeneratorKind::Fbm);
        noise.modifiers.push(Modifier::new(ModifierKind::Levels));
        let mut off = Modifier::new(ModifierKind::Colorize);
        off.enabled = false;
        noise.modifiers.push(off);
        let text = to_toml(&noise);
        assert!(text.starts_with(
            "type = \"noise/fbm\"\nsize = 512\n\n[noise.fbm]\nseed = 0\nbasis = \"perlin\"\nfrequency_x = 4\nfrequency_y = 4\ntranslate_x = 0.0\ntranslate_y = 0.0\noctaves = 5\nlacunarity = 2\ngain = 0.5\nwarp = 0.0\n\n[[modifiers]]\ntype = \"levels\"\nenabled = true\nblack = 0.0\n"
        ), "{text}");
        assert!(
            text.contains("hue = 180.0\nsaturation = 0.5\nlightness = 0.0\n"),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap(), noise);
        assert_eq!(parse("").unwrap(), Noise::default());
    }

    #[test]
    fn switching_type_keeps_parameters() {
        let mut noise = Noise::new(GeneratorKind::Fbm);
        noise.params.0[0] = Value::Int(7);
        noise.set_kind(GeneratorKind::Checkerboard);
        noise.set_kind(GeneratorKind::Voronoi);
        // Checkerboard's untouched defaults are not kept.
        assert_eq!(noise.kept.len(), 1);
        let text = to_toml(&noise);
        assert!(text.starts_with("type = \"noise/voronoi\"\nsize = 512\n\n[noise.voronoi]\n"));
        assert!(text.contains("\n[noise.fbm]\nseed = 7\n"), "{text}");
        assert!(!text.contains("[pattern.checkerboard]"), "{text}");
        let mut read = parse(&text).unwrap();
        assert_eq!(read, noise);
        read.set_kind(GeneratorKind::Fbm);
        assert_eq!(read.params.0[0], Value::Int(7));
        assert!(read.kept.is_empty());
        // Only the current section affects the image.
        let mut plain = Noise::new(GeneratorKind::Voronoi);
        plain.size = 16;
        noise.size = 16;
        assert_eq!(noise.generate(), plain.generate());
    }

    #[test]
    fn parameters_belong_to_their_type() {
        assert!(
            parse("type = \"pattern/checkerboard\"\n[pattern.checkerboard]\nseed = 1").is_err()
        );
        assert!(parse("[noise.perlin]\nbasis = \"value\"").is_err());
        assert!(parse("[noise.uniform]\nwarp = 1").is_err());
        // Parameters must be in a section.
        let error = parse("type = \"noise/perlin\"\nseed = 3").unwrap_err();
        assert!(error.contains("[noise.perlin]"), "{error}");
        let ok = parse("type = \"noise/fbm\"\n[noise.fbm]\noctaves = 3\ngain = 1").unwrap();
        assert_eq!(ok.params.display(GeneratorKind::Fbm.specs(), 6), "3");
        // Other sections are validated too.
        assert!(parse("[noise.voronoi]\njitter = 3").is_err());
        for bad in [
            "size = 0",
            "size = 5000",
            "type = \"noise/blue\"",
            "type = \"checkerboard\"",
            "[noise.blue]",
            "[pattern]\nperlin = 1",
            "noise = 1",
            "[noise.perlin]\nfrequency_x = 0",
            "[noise.perlin]\ntranslate_y = 1.5",
            "[noise.perlin]\nwarp = 5",
            "[noise.value]\ninterpolation = \"cosine\"",
            "[noise.fbm]\nbasis = \"cubic\"",
            "[noise.fbm]\nlacunarity = 2.5",
            "[[modifiers]]\ntype = \"levels\"\ngamma = 0",
            "[[modifiers]]\ntype = \"blur\"\nsteps = 2",
            "[[modifiers]]\ntype = \"sharpen\"",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
