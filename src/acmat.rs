//! Custom materials: `.acmat` files and `.acmat.bin`. See docs/features/custom-materials.md,
//! docs/spec/acmat.md and docs/spec/acmat.bin.md.

use std::path::Path;

use serde_json::{Map, Value};

use crate::chunks::{Bytes, Writer};
use crate::material::NormalY;
use crate::shader::State;

pub const FORMAT: &str = "ACMAT";
pub const VERSION: u32 = 1;
pub const SUFFIX: &str = ".acmat";
/// The scope of a custom material's params in the struct chunks; see docs/spec/astruct.md.
pub const PARAMS_SCOPE: u32 = 3;

pub fn is_custom_material(file: &Path) -> bool {
    file.extension().is_some_and(|ext| ext == "acmat")
}

/// A `[textures]` entry: a path, relative to the material's folder.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureRef {
    pub path: String,
    pub normal_y: Option<NormalY>,
}

/// A parsed `.acmat`; see docs/spec/acmat.md#schema.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Source {
    /// The `.ashader`, relative to the material's folder; `""` when none is chosen yet.
    pub shader: String,
    /// Overrides of the shader's state.
    pub state: State,
    /// The params struct's values, sparse.
    pub params: Map<String, Value>,
    /// By texture or input name, in the file's order.
    pub textures: Vec<(String, TextureRef)>,
    pub structs: Vec<crate::structs::Attachment>,
    pub values: crate::structs::Values,
}

impl Source {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let structs = crate::structs::take_toml_attachments(&mut table, "structs")?;
        let values = crate::structs::take_toml_values(&mut table)?;
        let shader = match table.remove("shader") {
            Some(toml::Value::String(s)) => s,
            Some(_) => return Err("shader is not a string".into()),
            None => return Err("missing shader".into()),
        };
        let params = match table.remove("params") {
            None => Map::new(),
            Some(v @ toml::Value::Table(_)) => match crate::structs::from_toml(v) {
                Value::Object(o) => o,
                _ => unreachable!(),
            },
            Some(_) => return Err("params is not a table".into()),
        };
        let mut textures = Vec::new();
        match table.remove("textures") {
            None => {}
            Some(toml::Value::Table(t)) => {
                for (name, v) in t {
                    let entry = match v {
                        toml::Value::String(path) => TextureRef {
                            path,
                            normal_y: None,
                        },
                        toml::Value::Table(mut t) => {
                            let path = match t.remove("path") {
                                Some(toml::Value::String(p)) => p,
                                _ => return Err(format!("textures.{name}: path is not a string")),
                            };
                            let normal_y = match t.remove("normal_y") {
                                None => None,
                                Some(v) => Some(v.try_into().map_err(|e: toml::de::Error| {
                                    format!("textures.{name}.normal_y: {}", e.message())
                                })?),
                            };
                            if let Some(other) = t.keys().next() {
                                return Err(format!("textures.{name}: unknown key {other}"));
                            }
                            TextureRef { path, normal_y }
                        }
                        _ => return Err(format!("textures.{name} is not a path or a table")),
                    };
                    textures.push((name, entry));
                }
            }
            Some(_) => return Err("textures is not a table".into()),
        }
        let state: State = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        state.check()?;
        Ok(Self {
            shader,
            state,
            params,
            textures,
            structs,
            values,
        })
    }

    pub fn texture(&self, name: &str) -> Option<&TextureRef> {
        self.textures
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
    }

    /// The file, as docs/spec/acmat.md#written-by-assetc says; `order` is the shader's
    /// texture and input names, which `[textures]` follows.
    pub fn to_toml(&self, order: &[String]) -> String {
        let q = |s: &str| toml::Value::String(s.to_owned()).to_string();
        let mut out = format!("shader = {}\n", q(&self.shader));
        for line in self.state.toml_lines() {
            out += &line;
            out.push('\n');
        }
        if !self.params.is_empty() {
            out += "\n[params]\n";
            for (k, v) in &self.params {
                out += &format!("{} = {}\n", key(k), crate::structs::to_toml(v));
            }
        }
        if !self.textures.is_empty() {
            out += "\n[textures]\n";
            let rank = |n: &str| order.iter().position(|o| o == n).unwrap_or(usize::MAX);
            let mut list: Vec<&(String, TextureRef)> = self.textures.iter().collect();
            list.sort_by_key(|(n, _)| rank(n));
            for (name, t) in list {
                match t.normal_y {
                    None => out += &format!("{} = {}\n", key(name), q(&t.path)),
                    Some(y) => {
                        out += &format!(
                            "{} = {{ path = {}, normal_y = \"{}\" }}\n",
                            key(name),
                            q(&t.path),
                            y.key()
                        )
                    }
                }
            }
        }
        crate::structs::write_toml_attachments(&mut out, "structs", &self.structs);
        crate::structs::write_toml_values(&mut out, "values", &self.values);
        out
    }
}

fn key(k: &str) -> String {
    let bare = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        k.to_owned()
    } else {
        toml::Value::from(k).to_string()
    }
}

/// `.acmat.bin`: the shader, state, GPU block, texture IDs and struct chunks; see
/// docs/spec/acmat.bin.md.
pub fn encode(
    shader: u64,
    state: &State,
    block: &[u8],
    textures: &[u64],
    structs: Vec<(&'static str, Vec<u8>)>,
) -> Vec<u8> {
    let mut desc = Bytes::default();
    desc.u64(shader)
        .u32(state.bits())
        .f32s(&[state.alpha_cutoff()])
        .u32(block.len() as u32)
        .u32(textures.len() as u32)
        .u64(0);
    let mut ids = Bytes::default();
    for id in textures {
        ids.u64(*id);
    }
    let mut w = Writer::new(FORMAT, VERSION);
    w.chunk("DESC", desc.take())
        .chunk("PARAMS", block.to_vec())
        .chunk("TEXTURES", ids.take());
    let structs = if structs.is_empty() {
        ["STRUCTS", "SVALUES", "SREFS", "SSTRINGS"]
            .map(|n| (n, Vec::new()))
            .to_vec()
    } else {
        structs
    };
    crate::compiled::put_structs(&mut w, structs);
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_round_trips_in_shader_order() {
        let text = r##"shader = "../shaders/water.ashader"
blend = "blend"

[params]
depth_color = "#0b3d4f"
foam = { amount = 0.4 }

[textures]
color = "lake.png"
ripples = { path = "../shared/ripples.png", normal_y = "dx" }

[[structs]]
schema = "game.aschema"
type = "Surface"
"##;
        let source = Source::parse(text).unwrap();
        assert_eq!(source.state.blend, Some(crate::shader::Blend::Blend));
        assert_eq!(
            source.texture("ripples").unwrap().normal_y,
            Some(NormalY::Dx)
        );
        let order = ["color".to_owned(), "ripples".to_owned()];
        assert_eq!(source.to_toml(&order), text);
        assert!(
            Source::parse("blend = \"add\"\n")
                .unwrap_err()
                .contains("shader")
        );
        assert!(Source::parse("shader = \"\"\nbase_color = 1\n").is_err());
    }

    #[test]
    fn encode_layout() {
        let bytes = encode(7, &State::default(), &[1; 16], &[3, 0], Vec::new());
        let r = crate::chunks::Reader::parse(&bytes, FORMAT).unwrap();
        let desc = r.require("DESC").unwrap();
        assert_eq!(desc.len(), 32);
        assert_eq!(u64::from_le_bytes(desc[..8].try_into().unwrap()), 7);
        assert_eq!(r.require("PARAMS").unwrap(), &[1; 16]);
        assert_eq!(r.require("TEXTURES").unwrap().len(), 16);
        assert!(r.chunk("STRUCTS").is_some());
    }
}
