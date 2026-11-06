//! Rewriting paths in files when files move, keeping each file's text as it is around them.
//! See docs/features/file-operations.md#paths.

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Value};

/// Input-relative `(from, to)` paths of moved files and folders.
struct Moves(Vec<(String, String)>);

impl Moves {
    /// Where the file or folder at input-relative `rel` is after the moves, if it moved.
    fn get(&self, rel: &str) -> Option<String> {
        self.0.iter().find_map(|(from, to)| {
            if rel == from {
                Some(to.clone())
            } else {
                let rest = rel.strip_prefix(from.as_str())?.strip_prefix('/')?;
                Some(format!("{to}/{rest}"))
            }
        })
    }
}

/// How a file's string values are read; see docs/features/file-operations.md#paths.
#[derive(Clone, Copy, PartialEq)]
enum Format {
    /// An assetc source: schema attachments and its format's own paths relative to its folder,
    /// any other string that names a file input-relative (`asset<K>` values, `preview`).
    Source(Ext),
    /// A `.gltf`: buffer and image URIs, relative to its folder.
    Gltf,
    /// `assetc.toml`: locator presets' `from`, input-relative.
    Config,
}

#[derive(Clone, Copy, PartialEq)]
enum Ext {
    Data,
    Shader,
    CustomMaterial,
    Material,
    Other,
}

impl Format {
    /// The format of `file` and whether it is JSON (else TOML); `None` for files without paths.
    fn of(file: &Path) -> Option<(Self, bool)> {
        let name = file.file_name()?.to_str()?.to_ascii_lowercase();
        if name == crate::atlas::CONFIG {
            return Some((Self::Source(Ext::Other), false));
        }
        Some(match name.rsplit_once('.')?.1 {
            "adata" => (Self::Source(Ext::Data), true),
            "agraph" | "atree" | "aschema" => (Self::Source(Ext::Other), true),
            "ashader" => (Self::Source(Ext::Shader), false),
            "acmat" => (Self::Source(Ext::CustomMaterial), false),
            "amat" => (Self::Source(Ext::Material), false),
            "amod" | "agradient" | "apalette" | "anoise" => (Self::Source(Ext::Other), false),
            "gltf" => (Self::Gltf, true),
            _ => return None,
        })
    }

    /// Whether the value at `keys` is a path relative to the file's folder.
    fn relative(self, keys: &[String]) -> bool {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        match (self, keys.as_slice()) {
            (Self::Source(_), [list, "schema"]) if list.ends_with("structs") => true,
            (Self::Source(Ext::Data), ["$schema"]) => true,
            (
                Self::Source(Ext::Shader),
                ["params", "schema"] | ["stages", "fragment" | "vertex"],
            ) => true,
            (
                Self::Source(Ext::CustomMaterial),
                ["shader"] | ["textures", _] | ["textures", _, "path"],
            ) => true,
            (Self::Source(Ext::Material), ["textures", _]) => true,
            (Self::Gltf, ["buffers" | "images", "uri"]) => true,
            _ => false,
        }
    }

    /// Whether any other string value naming a moved file is rewritten (input-relative).
    fn input_relative(self, keys: &[String]) -> bool {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        match self {
            Self::Source(_) => true,
            Self::Gltf => false,
            Self::Config => matches!(
                keys.as_slice(),
                ["locators", "presets", "from"] | ["targets", _, "locators", "presets", "from"]
            ),
        }
    }
}

/// The new text of every project file whose paths change when each `(from, to)` entry (files
/// or folders under `input`) moves, at the place it will be: the files under `input` that
/// hold paths, and `assetc.toml` in `root`.
pub fn fixes(
    input: &Path,
    root: Option<&Path>,
    moves: &[(PathBuf, PathBuf)],
) -> Result<Vec<(PathBuf, String)>, String> {
    let rel = |p: &Path| crate::build::relative(input, p);
    let moves = Moves(
        moves
            .iter()
            .map(|(from, to)| Ok((rel(from)?, rel(to)?)))
            .collect::<Result<_, String>>()?,
    );
    let mut files: Vec<(PathBuf, Format, bool)> = crate::build::files_under(input)
        .into_iter()
        .filter_map(|f| Format::of(&f).map(|(format, json)| (f, format, json)))
        .collect();
    if let Some(root) = root {
        files.push((root.join(crate::config::FILE_NAME), Format::Config, false));
    }
    let mut out = Vec::new();
    for (file, format, json) in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let (old, new) = match rel(&file) {
            Ok(old) => {
                let new = moves.get(&old).unwrap_or_else(|| old.clone());
                (old, new)
            }
            // assetc.toml: nothing relative to it is rewritten.
            Err(_) => (String::new(), String::new()),
        };
        let (old_dir, new_dir) = (parent(&old), parent(&new));
        let mut fix = |keys: &[String], value: &str| -> Option<String> {
            if value.is_empty() {
                return None;
            }
            if format.relative(keys) {
                if value.starts_with("data:") {
                    return None;
                }
                let gltf = format == Format::Gltf;
                let path = if gltf {
                    uri_path(value)
                } else {
                    value.to_owned()
                };
                let target = normalize(old_dir, &path)?;
                let joined = target.join("/");
                let moved = moves.get(&joined);
                if moved.is_none() && old_dir == new_dir {
                    return None;
                }
                let target = moved.map_or(target, |m| m.split('/').map(str::to_owned).collect());
                let new = relative(new_dir, &target);
                let new = if gltf { new.replace(' ', "%20") } else { new };
                (new != value).then_some(new)
            } else if format.input_relative(keys) {
                moves.get(value)
            } else {
                None
            }
        };
        let fixed = if json {
            json_strings(&text, &mut fix)
        } else {
            toml(&text, &mut fix)
        };
        if let Some(fixed) = fixed {
            let place = if format == Format::Config {
                file
            } else {
                input.join(&new)
            };
            out.push((place, fixed));
        }
    }
    Ok(out)
}

/// The path a glTF URI names; as the model reader, only `%20` is decoded.
pub fn uri_path(uri: &str) -> String {
    uri.replace("%20", " ")
}

fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// `path` from input-relative folder `dir`, as input-relative parts; leading `..` parts stay
/// when it leaves the input directory. `None` for an absolute path.
fn normalize(dir: &str, path: &str) -> Option<Vec<String>> {
    if path.starts_with('/') || path.contains(':') {
        return None;
    }
    let mut parts: Vec<String> = dir
        .split('/')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| p != "..") => {
                parts.pop();
            }
            part => parts.push(part.to_owned()),
        }
    }
    Some(parts)
}

/// Input-relative `target` parts as seen from input-relative folder `dir`.
fn relative(dir: &str, target: &[String]) -> String {
    let from: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let common = from
        .iter()
        .zip(target)
        .take_while(|(a, b)| **a == b.as_str())
        .count();
    let mut parts: Vec<&str> = vec![".."; from.len() - common];
    parts.extend(target[common..].iter().map(String::as_str));
    parts.join("/")
}

/// Rewrites a string value: given its keys from the top (array items add none) and its text,
/// the replacement, if any.
pub type Fix<'a> = dyn FnMut(&[String], &str) -> Option<String> + 'a;

/// Rewrites the string values of a JSON text through `fix`, changing nothing else in the text;
/// `None` when nothing changed or the text is not JSON.
pub fn json_strings(text: &str, fix: &mut Fix) -> Option<String> {
    enum Frame {
        Object { key: Option<String>, at_key: bool },
        Array,
    }
    let bytes = text.as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let (mut copied, mut changed, mut i) = (0, false, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'{' => stack.push(Frame::Object {
                key: None,
                at_key: true,
            }),
            b'[' => stack.push(Frame::Array),
            b'}' | b']' => {
                stack.pop()?;
            }
            b',' => {
                if let Some(Frame::Object { at_key, .. }) = stack.last_mut() {
                    *at_key = true;
                }
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                let end = (i + 1).min(bytes.len());
                let value: String = serde_json::from_str(&text[start..end]).ok()?;
                match stack.last_mut() {
                    Some(Frame::Object { key, at_key }) if *at_key => {
                        *key = Some(value);
                        *at_key = false;
                    }
                    _ => {
                        let keys: Vec<String> = stack
                            .iter()
                            .filter_map(|f| match f {
                                Frame::Object { key, .. } => key.clone(),
                                Frame::Array => None,
                            })
                            .collect();
                        if let Some(new) = fix(&keys, &value) {
                            out.push_str(&text[copied..start]);
                            out.push_str(&serde_json::to_string(&new).ok()?);
                            copied = end;
                            changed = true;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    if !changed {
        return None;
    }
    out.push_str(&text[copied..]);
    Some(out)
}

/// As [`json_strings`], for a TOML text.
pub fn toml(text: &str, fix: &mut Fix) -> Option<String> {
    fn walk(item: &mut Item, keys: &mut Vec<String>, fix: &mut Fix, changed: &mut bool) {
        match item {
            Item::Value(v) => value(v, keys, fix, changed),
            Item::Table(t) => {
                for (k, v) in t.iter_mut() {
                    keys.push(k.get().to_owned());
                    walk(v, keys, fix, changed);
                    keys.pop();
                }
            }
            Item::ArrayOfTables(a) => {
                for t in a.iter_mut() {
                    for (k, v) in t.iter_mut() {
                        keys.push(k.get().to_owned());
                        walk(v, keys, fix, changed);
                        keys.pop();
                    }
                }
            }
            Item::None => {}
        }
    }
    fn value(v: &mut Value, keys: &mut Vec<String>, fix: &mut Fix, changed: &mut bool) {
        match v {
            Value::String(s) => {
                if let Some(new) = fix(keys, s.value()) {
                    let decor = s.decor().clone();
                    *v = Value::from(new);
                    *v.decor_mut() = decor;
                    *changed = true;
                }
            }
            Value::Array(a) => {
                for v in a.iter_mut() {
                    value(v, keys, fix, changed);
                }
            }
            Value::InlineTable(t) => {
                for (k, v) in t.iter_mut() {
                    keys.push(k.get().to_owned());
                    value(v, keys, fix, changed);
                    keys.pop();
                }
            }
            _ => {}
        }
    }
    let mut doc: DocumentMut = text.parse().ok()?;
    let mut changed = false;
    walk(doc.as_item_mut(), &mut Vec::new(), fix, &mut changed);
    changed.then(|| doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_are_rewritten_in_place() {
        let text = "{\n  \"$schema\": \"a.aschema\",\n  \"images\": [ { \"uri\": \"t\\u0020x.png\" }, {\"name\": \"uri\"} ],\n  \"n\": 1\n}\n";
        let mut seen = Vec::new();
        let out = json_strings(text, &mut |keys, v| {
            seen.push((keys.join("."), v.to_owned()));
            (v == "t x.png").then(|| "../t x.png".to_owned())
        })
        .unwrap();
        assert_eq!(
            out,
            "{\n  \"$schema\": \"a.aschema\",\n  \"images\": [ { \"uri\": \"../t x.png\" }, {\"name\": \"uri\"} ],\n  \"n\": 1\n}\n"
        );
        assert_eq!(
            seen,
            [
                ("$schema".into(), "a.aschema".into()),
                ("images.uri".into(), "t x.png".into()),
                ("images.name".into(), "uri".into()),
            ]
        );
        assert_eq!(json_strings(text, &mut |_, _| None), None);
    }

    #[test]
    fn toml_strings_are_rewritten_keeping_the_rest() {
        let text = "# shader\nshader = \"../s.ashader\" # here\n\n[textures]\ncolor = { path = \"a.png\", normal_y = \"gl\" }\n\n[[structs]]\nschema = \"g.aschema\"\ntype = \"T\"\n";
        let mut seen = Vec::new();
        let out = toml(text, &mut |keys, v| {
            seen.push(keys.join("."));
            v.ends_with(".png").then(|| format!("sub/{v}"))
        })
        .unwrap();
        assert_eq!(out, text.replace("\"a.png\"", "\"sub/a.png\""));
        assert_eq!(
            seen,
            [
                "shader",
                "textures.color.path",
                "textures.color.normal_y",
                "structs.schema",
                "structs.type"
            ]
        );
    }

    #[test]
    fn sample_toml_files_round_trip_unchanged() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = crate::build::files_under(&root.join("samples"));
        files.push(root.join("assetc.toml"));
        let mut checked = 0;
        for file in files {
            let Some((_, false)) = Format::of(&file) else {
                continue;
            };
            let text = std::fs::read_to_string(&file).unwrap();
            let doc: DocumentMut = text.parse().unwrap();
            assert_eq!(doc.to_string(), text, "{}", file.display());
            checked += 1;
        }
        assert!(checked > 10);
    }
}
