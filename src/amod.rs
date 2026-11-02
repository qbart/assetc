//! `.amod` sidecars: per-file settings next to an input file, see docs/spec/amod.md.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::amesh::MeshPatch;
use crate::enrich::{self, Properties, Region, Shape};
use crate::noise::{self, Modifier};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Amod {
    /// `[mesh]`: meshoptimizer settings of a glTF model.
    pub mesh: MeshPatch,
    /// `[properties]`: a glTF model's properties, see docs/features/properties.md.
    pub properties: Properties,
    /// `[[locators]]`: a glTF model's locators, see docs/features/locators.md.
    pub locators: Vec<Locator>,
    /// `[[shapes]]`: a glTF model's shapes, see docs/features/shapes.md.
    pub shapes: Vec<Shape>,
    /// `[[regions]]`: a glTF model's regions, see docs/features/regions.md.
    pub regions: Vec<Region>,
    /// `[[modifiers]]`: an image's modifier stack, as in `.noise` files.
    pub modifiers: Vec<Modifier>,
}

/// A named transform on a model, in its joint's space or the model's.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locator {
    pub name: String,
    /// Names the game can group locators by; see docs/features/locators.md#locator.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The first skin's joint, by node name; `None` is model space.
    #[serde(default)]
    pub joint: Option<String>,
    #[serde(default)]
    pub translation: [f32; 3],
    #[serde(default = "identity")]
    pub rotation: [f32; 4],
    #[serde(default = "unit")]
    pub scale: [f32; 3],
    /// An input-relative glTF model the 3D view shows at the locator; not compiled.
    #[serde(default)]
    pub preview: Option<String>,
    /// `#rrggbb`, the marker's color in the 3D view; not compiled.
    #[serde(default)]
    pub color: Option<String>,
}

fn identity() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn unit() -> [f32; 3] {
    [1.0; 3]
}

impl Locator {
    /// At the model's origin, unrotated, scale 1.
    pub fn new(name: String) -> Self {
        Self {
            name,
            tags: Vec::new(),
            joint: None,
            translation: [0.0; 3],
            rotation: identity(),
            scale: unit(),
            preview: None,
            color: None,
        }
    }
}

/// Tags typed as `text`: split at commas, trimmed, without empty or repeated ones.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.split(',').map(str::trim) {
        if !tag.is_empty() && !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_owned());
        }
    }
    tags
}

/// The RGB of a locator's `#rrggbb` color.
pub fn locator_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `locator`, or `locator_<n>` with the smallest free `n` from 2.
pub fn free_locator_name(locators: &[Locator]) -> String {
    let taken = |name: &str| locators.iter().any(|l| l.name == name);
    std::iter::once("locator".to_owned())
        .chain((2..).map(|n| format!("locator_{n}")))
        .find(|name| !taken(name))
        .expect("a free name")
}

/// The tables read with serde.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Tables {
    mesh: MeshPatch,
    locators: Vec<Locator>,
}

impl Amod {
    pub fn is_empty(&self) -> bool {
        self.mesh.is_empty()
            && self.properties.is_empty()
            && self.locators.is_empty()
            && self.shapes.is_empty()
            && self.regions.is_empty()
            && self.modifiers.is_empty()
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let modifiers = match table.remove("modifiers") {
            None => Vec::new(),
            Some(value) => noise::parse_modifiers(value)?,
        };
        let properties = table
            .remove("properties")
            .map(enrich::parse_properties)
            .transpose()?
            .unwrap_or_default();
        let shapes = table
            .remove("shapes")
            .map(enrich::parse_shapes)
            .transpose()?
            .unwrap_or_default();
        let regions = table
            .remove("regions")
            .map(enrich::parse_regions)
            .transpose()?
            .unwrap_or_default();
        let Tables { mesh, locators } = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        for (i, locator) in locators.iter().enumerate() {
            if locator.name.is_empty() {
                return Err(format!("locator {}: empty name", i + 1));
            }
            if locators[..i].iter().any(|l| l.name == locator.name) {
                return Err(format!("locator {:?}: name used twice", locator.name));
            }
            for (k, tag) in locator.tags.iter().enumerate() {
                if tag.is_empty() || tag.contains(',') || locator.tags[..k].contains(tag) {
                    return Err(format!(
                        "locator {:?}: tag {tag:?} is empty, has a comma or is repeated",
                        locator.name
                    ));
                }
            }
            if let Some(color) = &locator.color
                && locator_color(color).is_none()
            {
                return Err(format!(
                    "locator {:?}: color {color:?} is not #rrggbb",
                    locator.name
                ));
            }
        }
        Ok(Self {
            mesh,
            properties,
            locators,
            shapes,
            regions,
            modifiers,
        })
    }

    /// The written form, see docs/spec/amod.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        if !self.mesh.is_empty() {
            out += "[mesh]\n";
            out += &self.mesh.to_toml();
        }
        enrich::write_properties("properties", &self.properties, &mut out);
        for locator in &self.locators {
            let floats = |values: &[f32]| {
                let items: Vec<String> = values.iter().map(|v| format!("{v:?}")).collect();
                format!("[{}]", items.join(", "))
            };
            out += "\n[[locators]]\n";
            out += &format!("name = {}\n", toml::Value::from(locator.name.as_str()));
            if !locator.tags.is_empty() {
                let tags: Vec<String> = locator
                    .tags
                    .iter()
                    .map(|t| toml::Value::from(t.as_str()).to_string())
                    .collect();
                out += &format!("tags = [{}]\n", tags.join(", "));
            }
            if let Some(joint) = &locator.joint {
                out += &format!("joint = {}\n", toml::Value::from(joint.as_str()));
            }
            out += &format!("translation = {}\n", floats(&locator.translation));
            out += &format!("rotation = {}\n", floats(&locator.rotation));
            out += &format!("scale = {}\n", floats(&locator.scale));
            if let Some(preview) = &locator.preview {
                out += &format!("preview = {}\n", toml::Value::from(preview.as_str()));
            }
            if let Some(color) = &locator.color {
                out += &format!("color = {}\n", toml::Value::from(color.as_str()));
            }
        }
        enrich::write_shapes(&self.shapes, &mut out);
        enrich::write_regions(&self.regions, &mut out);
        noise::write_modifiers(&self.modifiers, &mut out);
        // Each table after a blank line, but not the first.
        out.trim_start_matches('\n').to_owned()
    }
}

/// `<file name>.amod` next to `file`.
pub fn path_for(file: &Path) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(".amod");
    file.with_file_name(name)
}

/// Whether `path` is a sidecar (`*.amod`).
pub fn is_sidecar(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "amod")
}

/// The sidecar of `file`, if it has one; errors name the sidecar.
pub fn read(file: &Path) -> Result<Option<Amod>, String> {
    let path = path_for(file);
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match fs::read_to_string(&path) {
        Ok(text) => Amod::parse(&text)
            .map(Some)
            .map_err(|e| format!("{name}: {e}")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// Writes the sidecar of `file`, or deletes it when `amod` holds nothing.
pub fn write(file: &Path, amod: &Amod) -> io::Result<()> {
    let path = path_for(file);
    if amod.is_empty() {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        fs::write(path, amod.to_toml())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_form_round_trips() {
        let amod = Amod {
            mesh: MeshPatch {
                meshlet_max_vertices: Some(128),
                meshlet_max_triangles: Some(256),
                lod_ratios: Some(vec![0.5, 0.1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let text = amod.to_toml();
        assert_eq!(
            text,
            "[mesh]\nmeshlet_max_vertices = 128\nmeshlet_max_triangles = 256\nlod_ratios = [0.5, 0.1]\n"
        );
        assert_eq!(Amod::parse(&text).unwrap(), amod);
        assert_eq!(Amod::parse("").unwrap(), Amod::default());
        assert!(Amod::parse("[mesh]\nmax = 1\n").is_err());
        assert!(Amod::parse("[sound]\n").is_err());

        // Modifiers, after the mesh table, in the .noise form.
        let mut with_modifiers = amod.clone();
        with_modifiers
            .modifiers
            .push(Modifier::new(noise::ModifierKind::Invert));
        let text = with_modifiers.to_toml();
        assert!(text.ends_with("\n\n[[modifiers]]\ntype = \"invert\"\nenabled = true\n"));
        assert_eq!(Amod::parse(&text).unwrap(), with_modifiers);
        let only = Amod {
            modifiers: with_modifiers.modifiers.clone(),
            ..Default::default()
        };
        assert!(only.to_toml().starts_with("[[modifiers]]"));
        assert!(Amod::parse("[[modifiers]]\ntype = \"sharpen\"\n").is_err());
    }

    #[test]
    fn locators_round_trip() {
        let mut hand = Locator::new("hand \"r\"".into());
        hand.joint = Some("RightHand".into());
        hand.translation = [0.02, 0.08, 0.0];
        hand.rotation = [0.0, 0.0, 0.70710677, 0.70710677];
        let amod = Amod {
            mesh: MeshPatch {
                meshlet_max_vertices: Some(128),
                ..Default::default()
            },
            locators: vec![
                Locator {
                    preview: Some("weapons/sword.glb".into()),
                    ..hand
                },
                Locator::new("muzzle".into()),
            ],
            ..Default::default()
        };
        let text = amod.to_toml();
        assert!(text.contains(
            "\n\n[[locators]]\nname = 'hand \"r\"'\njoint = \"RightHand\"\ntranslation = [0.02, 0.08, 0.0]\nrotation = [0.0, 0.0, 0.70710677, 0.70710677]\nscale = [1.0, 1.0, 1.0]\npreview = \"weapons/sword.glb\"\n"
        ), "{text}");
        assert!(text.ends_with("\n\n[[locators]]\nname = \"muzzle\"\ntranslation = [0.0, 0.0, 0.0]\nrotation = [0.0, 0.0, 0.0, 1.0]\nscale = [1.0, 1.0, 1.0]\n"));
        assert_eq!(Amod::parse(&text).unwrap(), amod);

        // Defaults, and what is rejected.
        let only = Amod::parse("[[locators]]\nname = \"a\"\n").unwrap();
        assert_eq!(only.locators, [Locator::new("a".into())]);
        assert!(only.to_toml().starts_with("[[locators]]"));
        assert!(Amod::parse("[[locators]]\njoint = \"a\"\n").is_err());
        assert!(Amod::parse("[[locators]]\nname = \"\"\n").is_err());
        assert!(Amod::parse("[[locators]]\nname = \"a\"\n[[locators]]\nname = \"a\"\n").is_err());
        assert!(Amod::parse("[[locators]]\nname = \"a\"\nscale = [1.0]\n").is_err());
        assert!(Amod::parse("[[locators]]\nname = \"a\"\nkind = \"x\"\n").is_err());
        let colored = Amod::parse("[[locators]]\nname = \"a\"\ncolor = \"#22c55e\"\n").unwrap();
        assert_eq!(
            locator_color(colored.locators[0].color.as_deref().unwrap()),
            Some([0x22, 0xc5, 0x5e])
        );
        assert!(colored.to_toml().ends_with("color = \"#22c55e\"\n"));
        assert!(Amod::parse("[[locators]]\nname = \"a\"\ncolor = \"red\"\n").is_err());

        // Tags: written after the name, typed with commas.
        let tagged = Amod {
            locators: vec![Locator {
                tags: vec!["weapon".into(), "left hand".into()],
                ..Locator::new("a".into())
            }],
            ..Default::default()
        };
        let text = tagged.to_toml();
        assert!(
            text.starts_with("[[locators]]\nname = \"a\"\ntags = [\"weapon\", \"left hand\"]\n"),
            "{text}"
        );
        assert_eq!(Amod::parse(&text).unwrap(), tagged);
        assert_eq!(
            parse_tags(" weapon, ,left hand,weapon,"),
            ["weapon", "left hand"]
        );
        assert!(Amod::parse("[[locators]]\nname = \"a\"\ntags = [\"\"]\n").is_err());
        assert!(Amod::parse("[[locators]]\nname = \"a\"\ntags = [\"a,b\"]\n").is_err());
        assert!(Amod::parse("[[locators]]\nname = \"a\"\ntags = [\"x\", \"x\"]\n").is_err());
    }

    #[test]
    fn enrichments_round_trip() {
        use crate::enrich::{PropValue, ShapeType};
        let text = r##"[properties]
"display name" = "Hero"
lives = 3
mass = 80.5
playable = true

[[shapes]]
name = "head"
kind = "hitbox"
type = "capsule"
joint = "Head"
translation = [0.0, 0.1, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
radius = 0.1
height = 0.12
color = "#ef4444"

[shapes.properties]
damage = 2.0

[[shapes]]
name = "body"
type = "hull"

[[regions]]
name = "armor"
tags = ["metal"]
color = "#3b82f6"
materials = ["Steel"]
submeshes = [[0, 2], [1, 0]]
triangles = [[0, 0, 120, 40], [0, 1, 0, 12]]
"##;
        let amod = Amod::parse(text).unwrap();
        assert_eq!(amod.properties["lives"], PropValue::Int(3));
        assert_eq!(
            amod.properties["display name"],
            PropValue::Text("Hero".into())
        );
        assert_eq!(
            amod.shapes[0].shape,
            ShapeType::Capsule {
                radius: 0.1,
                height: 0.12
            }
        );
        assert_eq!(amod.shapes[0].properties["damage"], PropValue::Float(2.0));
        assert_eq!(amod.shapes[1].shape, ShapeType::Hull);
        assert_eq!(amod.regions[0].triangles.len(), 52);
        assert!(amod.regions[0].submeshes.contains(&(1, 0)));
        assert_eq!(amod.to_toml(), text);
        assert_eq!(Amod::parse(&amod.to_toml()).unwrap(), amod);

        for bad in [
            "[properties]\na = [1]\n",
            "[[shapes]]\nname = \"a\"\n",
            "[[shapes]]\nname = \"a\"\ntype = \"cone\"\n",
            "[[shapes]]\nname = \"a\"\ntype = \"sphere\"\nsize = [1.0, 1.0, 1.0]\n",
            "[[shapes]]\nname = \"a\"\ntype = \"hull\"\ntranslation = [0.0, 0.0, 0.0]\n",
            "[[shapes]]\nname = \"a\"\ntype = \"box\"\nsize = [1.0, 0.0, 1.0]\n",
            "[[shapes]]\nname = \"a\"\ntype = \"box\"\nmass = 1\n",
            "[[regions]]\nname = \"a\"\n",
            "[[regions]]\nname = \"a\"\ncolor = \"#000000\"\ntriangles = [[0, 0, 1, 0]]\n",
            "[[regions]]\nname = \"a\"\ncolor = \"#000000\"\nsubmeshes = [[0]]\n",
            "[[regions]]\nname = \"a\"\ncolor = \"#000000\"\nsubmeshes = [[0, 1], [0, 1]]\n",
        ] {
            assert!(Amod::parse(bad).is_err(), "{bad}");
        }
        let many: String = (0..33)
            .map(|i| format!("[[regions]]\nname = \"r{i}\"\ncolor = \"#000000\"\n"))
            .collect();
        assert!(Amod::parse(&many).unwrap_err().contains("at most 32"));
    }

    #[test]
    fn free_names_count_from_two() {
        assert_eq!(free_locator_name(&[]), "locator");
        let taken = [
            Locator::new("locator".into()),
            Locator::new("locator_2".into()),
        ];
        assert_eq!(free_locator_name(&taken), "locator_3");
    }

    #[test]
    fn sidecars_sit_next_to_their_file() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("Fox.glb");
        assert_eq!(path_for(&model), dir.path().join("Fox.glb.amod"));
        assert_eq!(read(&model).unwrap(), None);
        let mut amod = Amod::default();
        amod.mesh.meshlet_max_vertices = Some(32);
        write(&model, &amod).unwrap();
        assert_eq!(read(&model).unwrap(), Some(amod));
        write(&model, &Amod::default()).unwrap();
        assert!(!path_for(&model).exists());
        fs::write(path_for(&model), "nope").unwrap();
        assert!(read(&model).unwrap_err().starts_with("Fox.glb.amod: "));
    }
}
