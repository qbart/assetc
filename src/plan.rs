//! Decides what a build produces from `input`: which files are plain textures,
//! which form materials, and how each texture is compiled.
//! See docs/features/build.md and docs/features/materials.md.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::amesh::{MeshOptions, MeshSettings};
use crate::build::{
    Error, Failure, is_lut, is_noise, is_texture_source, relative, strip_extension,
};
use crate::config;
use crate::gradient::is_gradient;
use crate::hash::fnv1a64;
use crate::manifest::Kind;
use crate::material::{self, Material, NormalY, Slot, TextureIds};
use crate::model;
use crate::texture::{ColorSpace, Settings};
use crate::tree::{self, Node};

#[derive(Debug, Default)]
pub struct Plan {
    /// In build order: textures in input order, then each material's ORM and `.amat`.
    pub assets: Vec<Asset>,
    /// Problems found while planning; any of them fails the build.
    pub failures: Vec<Failure>,
    /// Input files that are not part of any asset.
    pub skipped: usize,
    /// Materials using each texture source (input-relative path).
    pub uses: HashMap<String, Vec<Use>>,
}

#[derive(Debug)]
pub struct Asset {
    pub id: u64,
    pub kind: Kind,
    pub asset_path: String,
    /// Relative to `output`.
    pub output: String,
    pub job: Job,
}

#[derive(Debug)]
pub enum Job {
    Texture {
        /// Relative to `input`.
        source: String,
        color: ColorSpace,
        mipmaps: bool,
        /// DirectX normal map: invert green.
        flip_green: bool,
    },
    /// Sources (relative to `input`) of the R (occlusion), G (roughness), B (metallic) channels.
    Orm { sources: [Option<String>; 3] },
    /// A color LUT (relative to `input`) as a strip texture, or a 3D one when `volume`; see
    /// docs/features/lut.md#build.
    Lut { source: String, volume: bool },
    /// A glTF model (relative to `input`): writes its `.amesh` and every other output of it.
    Model {
        source: String,
        options: MeshOptions,
        /// The model's `.amod`, its locators merged with inherited ones; see
        /// docs/features/locators.md#build.
        sidecar: Box<crate::amod::Amod>,
    },
    /// Another output of a model, written by its [`Job::Model`].
    ModelPart { source: String },
    /// A font (relative to `input`): writes its `.afont` and SDF atlas; see
    /// docs/features/fonts.md#build.
    Font { source: String },
    /// The atlas of a font, written by its [`Job::Font`].
    FontPart { source: String },
    /// An HDR environment map (relative to `input`) as a half-float cubemap; see
    /// docs/features/hdr.md#build.
    Cubemap { source: String },
    /// A schema, data file, graph or tree (relative to `input`), written to the same path;
    /// see docs/features/build.md#inputs.
    Data { source: String, kind: Kind },
    Material {
        /// `<name>.mat.toml`, or `<name>.*` without one.
        source: String,
        material: Material,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// Material asset path, e.g. `brick/brick.amat`.
    pub material: String,
    pub slot: Slot,
}

impl Asset {
    /// The source(s) shown in build output and errors.
    pub fn source(&self) -> String {
        match &self.job {
            Job::Texture { source, .. } => source.clone(),
            Job::Orm { sources } => sources
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            Job::Material { source, .. }
            | Job::Lut { source, .. }
            | Job::Model { source, .. }
            | Job::ModelPart { source }
            | Job::Font { source }
            | Job::FontPart { source }
            | Job::Data { source, .. }
            | Job::Cubemap { source } => source.clone(),
        }
    }
}

impl Plan {
    pub fn texture(&self, source: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|a| matches!(&a.job, Job::Texture { source: s, .. } if s == source))
    }

    pub fn by_asset_path(&self, asset_path: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.asset_path == asset_path)
    }
}

/// Files of one directory sharing a name: `<name>.<ext>`, `<name>.<suffix>.<ext>`, `<name>.mat.toml`.
#[derive(Default)]
struct Group {
    base: Option<String>,
    slots: BTreeMap<Slot, String>,
    source: Option<String>,
}

/// The assets of one glTF model; see docs/features/models.md#build.
fn model_assets(
    plan: &mut Plan,
    source: &str,
    outputs: &model::Outputs,
    options: MeshOptions,
    sidecar: crate::amod::Amod,
) {
    let p = strip_extension(source);
    let colors = sidecar.colors.clone();
    let mut push = |kind: Kind, asset_path: String, job: Job| {
        plan.assets.push(Asset {
            id: fnv1a64(asset_path.as_bytes()),
            kind,
            output: kind.output_path(&asset_path),
            asset_path,
            job,
        });
    };
    let part = || Job::ModelPart {
        source: source.to_owned(),
    };
    push(
        Kind::Mesh,
        format!("{p}.amesh"),
        Job::Model {
            source: source.to_owned(),
            options,
            sidecar: Box::new(sidecar),
        },
    );
    if outputs.animation {
        push(Kind::Animation, format!("{p}.aanim"), part());
    }
    for (_, name) in &outputs.materials {
        push(Kind::Material, format!("{p}/{name}.amat"), part());
    }
    for (_, name, _) in &outputs.images {
        push(Kind::Texture, format!("{p}/{name}"), part());
    }
    for name in &outputs.orm {
        push(Kind::Texture, format!("{p}/{name}.orm"), part());
    }
    // Color sets exported as Texture + UV buffer; see docs/features/vertex-colors.md#export.
    // Bases (sets others blend with) are not written.
    let bases = crate::vertex_colors::bases(&colors);
    for (set, base) in colors.iter().zip(bases) {
        if crate::vertex_colors::textured(set) && base.is_none() {
            push(Kind::Texture, format!("{p}/{}.colors", set.name), part());
        }
    }
}

/// The asset kind of a schema, data file, graph or tree.
fn data_kind(file: &Path) -> Option<Kind> {
    if crate::aschema::is_schema(file) {
        Some(Kind::Schema)
    } else if crate::adata::is_data(file) {
        Some(Kind::Data)
    } else if crate::agraph::is_graph(file) {
        Some(Kind::Graph)
    } else if crate::atree::is_tree(file) {
        Some(Kind::Tree)
    } else {
        None
    }
}

/// How a texture source is compiled when a material slot uses it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Claim {
    color: ColorSpace,
    flip_green: bool,
}

pub fn plan(input: &Path, config: &config::Config) -> Result<Plan, Error> {
    let settings = Settings::new(&config.textures).map_err(Error::Pattern)?;
    let meshes = MeshSettings::new(&config.meshes).map_err(Error::Meshes)?;
    let nodes = tree::scan(input).map_err(|e| Error::Io(input.to_path_buf(), e))?;
    let mut files = Vec::new();
    flatten(&nodes, &mut files);

    let mut plan = Plan::default();
    let mut sources = Vec::new();
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for file in files {
        let rel = match relative(input, file) {
            Ok(rel) => rel,
            Err(message) => {
                plan.failures.push(Failure {
                    source: file.display().to_string(),
                    message,
                });
                continue;
            }
        };
        let (dir, name) = split_dir(&rel);
        if let Some(material) = name.strip_suffix(material::SOURCE_SUFFIX)
            && !material.is_empty()
        {
            let key = (dir.to_owned(), material.to_owned());
            groups.entry(key).or_default().source = Some(rel);
        } else if is_texture_source(file) {
            let stem = strip_extension(name);
            match stem.rsplit_once('.') {
                Some((material, suffix))
                    if !material.is_empty() && Slot::from_suffix(suffix).is_some() =>
                {
                    let slot = Slot::from_suffix(suffix).unwrap();
                    let group = groups.entry((dir.into(), material.into())).or_default();
                    group.slots.insert(slot, rel.clone());
                }
                _ => groups.entry((dir.into(), stem.into())).or_default().base = Some(rel.clone()),
            }
            sources.push(rel);
        } else if model::is_model(file) {
            let options = crate::amod::read(file).and_then(|amod| {
                let options = meshes
                    .options_with(&rel, amod.as_ref())
                    .map_err(|e| format!("{}.amod: {e}", split_dir(&rel).1))?;
                Ok((options, amod.unwrap_or_default()))
            });
            match options.and_then(|(options, sidecar)| {
                let data = fs::read(file).map_err(|e| e.to_string())?;
                // See docs/features/locators.md#inherited-locators.
                let inherited = crate::presets::inherited(&config.locators, input, &rel, &data)?;
                let sidecar = crate::amod::Amod {
                    locators: crate::presets::merged(&inherited, &sidecar.locators),
                    ..sidecar
                };
                Ok((model::outputs(&data)?, options, sidecar))
            }) {
                Ok((outputs, options, sidecar)) => {
                    model_assets(&mut plan, &rel, &outputs, options, sidecar)
                }
                Err(message) => plan.failures.push(Failure {
                    source: rel,
                    message,
                }),
            }
        } else if crate::hdr::is_hdr(file) {
            let asset_path = strip_extension(&rel).to_owned();
            plan.assets.push(Asset {
                id: fnv1a64(asset_path.as_bytes()),
                kind: Kind::Texture,
                output: Kind::Texture.output_path(&asset_path),
                asset_path,
                job: Job::Cubemap {
                    source: rel.clone(),
                },
            });
        } else if let Some(kind) = data_kind(file) {
            // Written to the same path; see docs/features/build.md#inputs.
            plan.assets.push(Asset {
                id: fnv1a64(rel.as_bytes()),
                kind,
                output: kind.output_path(&rel),
                asset_path: rel.clone(),
                job: Job::Data {
                    source: rel.clone(),
                    kind,
                },
            });
        } else if crate::font::is_font(file) {
            let p = strip_extension(&rel).to_owned();
            for (kind, asset_path, job) in [
                (
                    Kind::Font,
                    format!("{p}.afont"),
                    Job::Font {
                        source: rel.clone(),
                    },
                ),
                (
                    Kind::Texture,
                    p.clone(),
                    Job::FontPart {
                        source: rel.clone(),
                    },
                ),
            ] {
                plan.assets.push(Asset {
                    id: fnv1a64(asset_path.as_bytes()),
                    kind,
                    output: kind.output_path(&asset_path),
                    asset_path,
                    job,
                });
            }
        } else if is_lut(file) {
            let asset_path = strip_extension(&rel).to_owned();
            plan.assets.push(Asset {
                id: fnv1a64(asset_path.as_bytes()),
                kind: Kind::Texture,
                output: Kind::Texture.output_path(&asset_path),
                asset_path,
                job: Job::Lut {
                    volume: settings.volume_lut(&rel),
                    source: rel,
                },
            });
        } else {
            plan.skipped += 1;
        }
    }

    let known: HashSet<&str> = sources.iter().map(String::as_str).collect();
    let mut claims: HashMap<String, Claim> = HashMap::new();
    let mut orm_inputs = HashSet::new();
    let mut materials = Vec::new();
    for ((dir, name), group) in &groups {
        if group.source.is_none() && group.slots.is_empty() {
            continue;
        }
        let asset_path = join(dir, &format!("{name}.amat"));
        let label = group
            .source
            .clone()
            .unwrap_or_else(|| join(dir, &format!("{name}.*")));
        let fail = |plan: &mut Plan, message: String| {
            plan.failures.push(Failure {
                source: label.clone(),
                message,
            })
        };

        let source = match &group.source {
            Some(rel) => {
                let text = match fs::read_to_string(input.join(rel)) {
                    Ok(text) => text,
                    Err(e) => {
                        fail(&mut plan, e.to_string());
                        continue;
                    }
                };
                match material::Source::parse(&text) {
                    Ok(source) => source,
                    Err(message) => {
                        fail(&mut plan, message);
                        continue;
                    }
                }
            }
            None => material::Source::default(),
        };

        let mut slots = group.slots.clone();
        if let Some(base) = &group.base {
            slots.insert(Slot::BaseColor, base.clone());
        }
        let mut ok = true;
        for slot in Slot::ALL {
            let Some(path) = source.textures.get(slot) else {
                continue;
            };
            match resolve(dir, path).filter(|rel| known.contains(rel.as_str())) {
                Some(rel) => {
                    slots.insert(slot, rel);
                }
                None => {
                    let message = format!(
                        "[textures] {} = {path:?} is not a texture source in input",
                        slot.name()
                    );
                    fail(&mut plan, message);
                    ok = false;
                }
            }
        }

        for (&slot, rel) in &slots {
            plan.uses.entry(rel.clone()).or_default().push(Use {
                material: asset_path.clone(),
                slot,
            });
            if slot.orm_channel().is_some() {
                orm_inputs.insert(rel.clone());
                continue;
            }
            let claim = Claim {
                color: slot.color_space(),
                flip_green: slot == Slot::Normal && source.normal_y == NormalY::Dx,
            };
            match claims.get(rel) {
                Some(existing) if *existing != claim => {
                    let message = format!(
                        "{rel} is used as {} here but differently by another material",
                        slot.name()
                    );
                    fail(&mut plan, message);
                    ok = false;
                }
                _ => {
                    claims.insert(rel.clone(), claim);
                }
            }
        }
        if ok {
            materials.push((dir, name, asset_path, label, source, slots));
        }
    }

    for rel in &sources {
        let (color, flip_green) = match claims.get(rel) {
            Some(claim) => (claim.color, claim.flip_green),
            None if orm_inputs.contains(rel) => continue,
            // Noise is data: linear unless a material slot says otherwise.
            None if is_noise(Path::new(rel)) => (ColorSpace::Linear, false),
            None => (settings.color_space(rel), false),
        };
        let asset_path = strip_extension(rel).to_owned();
        plan.assets.push(Asset {
            id: fnv1a64(asset_path.as_bytes()),
            kind: Kind::Texture,
            output: Kind::Texture.output_path(&asset_path),
            asset_path,
            job: Job::Texture {
                source: rel.clone(),
                color,
                // Smaller levels would blend neighbouring gradient or palette cells.
                mipmaps: settings.mipmaps(rel)
                    && !is_gradient(Path::new(rel))
                    && !crate::palette::is_palette(Path::new(rel)),
                flip_green,
            },
        });
    }

    for (dir, name, asset_path, label, source, slots) in materials {
        let texture_id = |slot| {
            slots
                .get(&slot)
                .map_or(0, |rel: &String| fnv1a64(strip_extension(rel).as_bytes()))
        };
        let orm_sources: [Option<String>; 3] = [Slot::Occlusion, Slot::Roughness, Slot::Metallic]
            .map(|slot| slots.get(&slot).cloned());
        let mut orm = 0;
        if orm_sources.iter().any(Option::is_some) {
            let orm_path = join(dir, &format!("{name}.orm"));
            orm = fnv1a64(orm_path.as_bytes());
            plan.assets.push(Asset {
                id: orm,
                kind: Kind::Texture,
                output: Kind::Texture.output_path(&orm_path),
                asset_path: orm_path,
                job: Job::Orm {
                    sources: orm_sources,
                },
            });
        }
        let ids = TextureIds {
            base_color: texture_id(Slot::BaseColor),
            normal: texture_id(Slot::Normal),
            orm,
            emissive: texture_id(Slot::Emissive),
            height: texture_id(Slot::Height),
        };
        let maps: Vec<Slot> = slots.keys().copied().collect();
        plan.assets.push(Asset {
            id: fnv1a64(asset_path.as_bytes()),
            kind: Kind::Material,
            output: Kind::Material.output_path(&asset_path),
            asset_path,
            job: Job::Material {
                source: label,
                material: source.material(&maps, ids),
            },
        });
    }

    reject_bad_ids(&mut plan);
    Ok(plan)
}

/// Drops assets whose IDs collide or are the reserved `0`, reporting each.
fn reject_bad_ids(plan: &mut Plan) {
    let mut by_id: HashMap<u64, Vec<String>> = HashMap::new();
    for asset in &plan.assets {
        by_id.entry(asset.id).or_default().push(asset.source());
    }
    let assets = std::mem::take(&mut plan.assets);
    for asset in assets {
        let sources = &by_id[&asset.id];
        let source = asset.source();
        if sources.len() > 1 {
            let others: Vec<_> = sources.iter().filter(|s| **s != source).collect();
            plan.failures.push(Failure {
                message: format!("asset ID {:#018x} collides with {others:?}", asset.id),
                source,
            });
        } else if asset.id == 0 {
            plan.failures.push(Failure {
                message: format!(
                    "asset path {:?} hashes to the reserved ID 0",
                    asset.asset_path
                ),
                source,
            });
        } else {
            plan.assets.push(asset);
        }
    }
}

pub fn flatten<'a>(nodes: &'a [Node], files: &mut Vec<&'a Path>) {
    for node in nodes {
        match &node.children {
            Some(children) => flatten(children, files),
            None => files.push(&node.path),
        }
    }
}

/// `("a/b", "c.png")` for `a/b/c.png`, `("", "c.png")` at the root.
pub fn split_dir(rel: &str) -> (&str, &str) {
    rel.rsplit_once('/').unwrap_or(("", rel))
}

pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// Resolves a `/`-separated `path` relative to `dir` (both input-relative);
/// `None` if it is absolute or leaves the input directory.
fn resolve(dir: &str, path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_stays_inside_input() {
        assert_eq!(
            resolve("brick", "brick.n.jpg").as_deref(),
            Some("brick/brick.n.jpg")
        );
        assert_eq!(resolve("a/b", "../c/./d.png").as_deref(), Some("a/c/d.png"));
        assert_eq!(resolve("", "x.png").as_deref(), Some("x.png"));
        assert_eq!(resolve("a", "../../x.png"), None);
        assert_eq!(resolve("a", "/etc/x.png"), None);
    }

    #[test]
    fn split_dir_and_join() {
        assert_eq!(split_dir("a/b/c.png"), ("a/b", "c.png"));
        assert_eq!(split_dir("c.png"), ("", "c.png"));
        assert_eq!(join("", "x"), "x");
        assert_eq!(join("a", "x"), "a/x");
    }
}
