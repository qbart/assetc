//! Compiles `input` into `output`, see docs/features/build.md.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::atex::{Target, Usage};
use crate::config;
use crate::gradient::{Gradients, is_gradient};
use crate::imagefile::{Channels, Image};
use crate::manifest::{self, Kind};
use crate::material;
use crate::noise::Noise;
use crate::plan::{self, Job, flatten};
use crate::texture::{self, ColorSpace};
use crate::tree;

/// One compiled asset.
#[derive(Debug)]
pub struct Built {
    /// Source path(s) relative to `input`.
    pub source: String,
    /// Output path relative to `output`.
    pub output: String,
    pub kind: Kind,
    /// E.g. `srgb, 11 levels` or `material: base_color, orm`.
    pub detail: String,
}

#[derive(Debug)]
pub struct Failure {
    pub source: String,
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source, self.message)
    }
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub built: Vec<Built>,
    /// Kinds of the outputs written along with a built model or font, counted in the summary
    /// but not listed.
    pub parts: Vec<Kind>,
    pub skipped: usize,
    /// Assets an incremental build left as they were; see docs/features/cli.md#assetc-watch.
    pub up_to_date: usize,
    /// Stale outputs deleted after a successful build, relative to `output`.
    pub removed: Vec<String>,
    /// Non-empty means the build failed and no manifest was written.
    pub failures: Vec<Failure>,
    /// Problems that do not fail the build; see docs/features/build.md#cli.
    pub warnings: Vec<Failure>,
}

impl Outcome {
    pub fn summary(&self) -> String {
        if let Some(first) = self.failures.first() {
            let n = self.failures.len();
            if n == 1 {
                format!("Build failed: {first}")
            } else {
                format!("Build failed: {first} ({n} errors)")
            }
        } else {
            // See docs/features/build.md#summary.
            let counts: Vec<String> = Kind::ALL
                .into_iter()
                .filter_map(|kind| {
                    let n = self.built.iter().filter(|b| b.kind == kind).count()
                        + self.parts.iter().filter(|k| **k == kind).count();
                    let (one, many) = match kind {
                        Kind::Texture => ("texture", "textures"),
                        Kind::Material => ("material", "materials"),
                        Kind::Mesh => ("mesh", "meshes"),
                        Kind::Animation => ("animation", "animations"),
                        Kind::Font => ("font", "fonts"),
                        Kind::Schema => ("schema", "schemas"),
                        Kind::Data => ("data file", "data files"),
                        Kind::Graph => ("graph", "graphs"),
                        Kind::Tree => ("tree", "trees"),
                        Kind::Atlas => ("atlas", "atlases"),
                        Kind::Gradient => ("gradient", "gradients"),
                        Kind::Palette => ("palette", "palettes"),
                        Kind::Shader => ("shader", "shaders"),
                        Kind::CustomMaterial => ("custom material", "custom materials"),
                        Kind::Collider => ("collider", "colliders"),
                    };
                    (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }))
                })
                .collect();
            let counts = if counts.is_empty() {
                "nothing".to_owned()
            } else {
                counts.join(", ")
            };
            let up_to_date = match self.up_to_date {
                0 => String::new(),
                u => format!(", {u} up to date"),
            };
            let removed = match self.removed.len() {
                0 => String::new(),
                k => format!(", {k} removed"),
            };
            let warnings = match self.warnings.len() {
                0 => String::new(),
                1 => ", 1 warning".to_owned(),
                w => format!(", {w} warnings"),
            };
            format!(
                "Built {counts} ({} skipped{up_to_date}{removed}{warnings})",
                self.skipped
            )
        }
    }
}

/// Errors that stop the build before any source is compiled, or while writing the manifest.
#[derive(Debug)]
pub enum Error {
    MissingInput(PathBuf),
    /// `output` is `input` or one of its ancestors.
    OutputContainsInput(PathBuf),
    Pattern(globset::Error),
    /// Invalid `[meshes]` settings.
    Meshes(String),
    /// Invalid `[environments]` settings.
    Environments(String),
    Io(PathBuf, io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput(dir) => {
                write!(f, "input directory {} does not exist", dir.display())
            }
            Self::OutputContainsInput(dir) => write!(
                f,
                "output directory {} must not contain the input directory",
                dir.display()
            ),
            Self::Pattern(e) => write!(f, "{} [textures]: {e}", config::FILE_NAME),
            Self::Meshes(e) => write!(f, "{} [meshes]: {e}", config::FILE_NAME),
            Self::Environments(e) => write!(f, "{} [environments]: {e}", config::FILE_NAME),
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

pub fn run(input: &Path, output: &Path, config: &config::Config) -> Result<Outcome, Error> {
    run_cached(input, output, config, &mut Cache::default())
}

/// What the builds so far read for each asset they compiled, by output path; see
/// docs/features/cli.md#assetc-watch.
#[derive(Default)]
pub struct Cache(HashMap<String, u64>);

/// Builds like [`run`], compiling only the assets whose job or read files changed since
/// `cache` saw them built.
pub fn run_cached(
    input: &Path,
    output: &Path,
    config: &config::Config,
    cache: &mut Cache,
) -> Result<Outcome, Error> {
    // Checked first so bad settings are reported even without an input directory.
    texture::Settings::new(&config.textures).map_err(Error::Pattern)?;
    crate::amesh::MeshSettings::new(&config.meshes).map_err(Error::Meshes)?;
    config
        .environments
        .validate()
        .map_err(Error::Environments)?;
    if !input.is_dir() {
        return Err(Error::MissingInput(input.to_path_buf()));
    }
    // A missing output cannot contain the existing input.
    if output.exists() {
        let canonical = |dir: &Path| fs::canonicalize(dir).map_err(|e| Error::Io(dir.into(), e));
        if canonical(input)?.starts_with(canonical(output)?) {
            return Err(Error::OutputContainsInput(output.to_path_buf()));
        }
    }
    let plan = plan::plan(input, config)?;
    let mut outcome = Outcome {
        skipped: plan.skipped,
        failures: plan.failures,
        ..Default::default()
    };

    // Every output of a source, parts included, must exist for it to be up to date.
    let mut outputs: HashMap<String, Vec<&str>> = HashMap::new();
    // A collider is a job of its own; see docs/features/colliders.md#build.
    for asset in plan
        .assets
        .iter()
        .filter(|a| !matches!(a.job, Job::Collider { .. }))
    {
        outputs
            .entry(asset.source())
            .or_default()
            .push(&asset.output);
    }
    let mut schemas = None;
    let mut parts = Vec::new();
    // See docs/features/textures.md#format.
    let format = Target::of(&config.textures);
    for asset in &plan.assets {
        let dest = output.join(&asset.output);
        let stamp = match &asset.job {
            Job::ModelPart { .. } | Job::FontPart { .. } | Job::AtlasPart { .. } => None,
            job => Some(
                stamp(input, job, &mut schemas)
                    ^ crate::hash::fnv1a64(format!("{format:?}").as_bytes()),
            ),
        };
        let complete = match &asset.job {
            Job::Collider { .. } => dest.is_file(),
            _ => outputs[&asset.source()]
                .iter()
                .all(|out| output.join(out).is_file()),
        };
        if let Some(stamp) = stamp
            && cache.0.get(&asset.output) == Some(&stamp)
            && complete
        {
            outcome.up_to_date += 1;
            continue;
        }
        let mut warnings = Vec::new();
        let warnings = &mut warnings;
        let result = match &asset.job {
            Job::Texture {
                source,
                color,
                usage,
                mipmaps,
                flip_green,
                custom,
            } => compile_texture(
                (input, source),
                &dest,
                (format, *color, *usage),
                *mipmaps,
                *flip_green,
                custom.as_ref(),
            )
            .map(|(levels, custom)| {
                let detail = levels_detail(format.for_usage(*usage), *color, levels);
                match custom {
                    0 => detail,
                    // See docs/features/mipmaps.md#build.
                    n => format!("{detail}, {n} custom"),
                }
            }),
            Job::Orm { sources } => compile_orm(input, sources, &dest, format)
                .map(|levels| levels_detail(format, ColorSpace::Linear, levels)),
            Job::Model {
                source,
                options,
                sidecar,
            } => compile_model(input, source, output, (options, sidecar, format), warnings),
            Job::Collider { source, settings } => compile_collider(input, source, &dest, settings),
            // Written with its model, font or atlas; counted once that is built.
            Job::ModelPart { source } | Job::FontPart { source } | Job::AtlasPart { source } => {
                parts.push((source, asset.kind));
                continue;
            }
            Job::Font { source } => compile_font(input, source, output, format),
            Job::Data { source, kind } => compile_data(input, source, &dest, *kind, warnings),
            Job::Cubemap { source, map } => {
                compile_cubemap(&input.join(source), &dest, format, *map)
            }
            Job::Atlas {
                source,
                color,
                mipmaps,
            } => compile_atlas(input, source, output, (format, *color, *mipmaps), warnings),
            Job::Lut { source, volume } => {
                compile_lut(&input.join(source), &dest, *volume).map(|size| {
                    let layout = if *volume { "volume" } else { "strip" };
                    format!("{size}³ LUT {layout}, linear, 1 level")
                })
            }
            Job::Material {
                source,
                material,
                structs,
                values,
            } => compile_material(input, source, material, (structs, values), warnings)
                .and_then(|bytes| write(&dest, &bytes))
                .map(|()| material_detail(material)),
            Job::Shader { source } => crate::shader::load(input, source).and_then(|loaded| {
                write(&dest, &loaded.encode())?;
                Ok(loaded.detail())
            }),
            Job::Pack {
                sources,
                fill,
                mipmaps,
            } => compile_pack(input, sources, *fill, &dest, (format, *mipmaps))
                .map(|levels| levels_detail(format, ColorSpace::Linear, levels)),
            Job::CustomMaterial {
                source,
                shader,
                textures,
                material,
            } => compile_custom_material(input, source, shader, textures, material, warnings)
                .and_then(|bytes| write(&dest, &bytes))
                .map(|()| {
                    let name = crate::plan::split_dir(strip_extension(shader)).1;
                    let n = textures.iter().filter(|id| **id != 0).count();
                    format!(
                        "custom material: {name}, {n} texture{}",
                        if n == 1 { "" } else { "s" }
                    )
                }),
        };
        if result.is_ok() {
            outcome
                .warnings
                .extend(warnings.drain(..).map(|message| Failure {
                    source: asset.source(),
                    message,
                }));
        }
        match result {
            Ok(detail) => {
                if let Some(stamp) = stamp {
                    cache.0.insert(asset.output.clone(), stamp);
                }
                outcome.built.push(Built {
                    source: asset.source(),
                    output: asset.output.clone(),
                    kind: asset.kind,
                    detail,
                });
            }
            Err(message) => {
                cache.0.remove(&asset.output);
                outcome.failures.push(Failure {
                    source: asset.source(),
                    message,
                });
            }
        }
    }
    cache
        .0
        .retain(|out, _| plan.assets.iter().any(|a| a.output == *out));

    outcome.parts = parts
        .into_iter()
        .filter(|(source, _)| outcome.built.iter().any(|b| b.source == **source))
        .map(|(_, kind)| kind)
        .collect();

    if outcome.failures.is_empty() {
        let entries: Vec<_> = plan
            .assets
            .iter()
            .map(|a| manifest::Entry {
                id: a.id,
                kind: a.kind,
                path: a.output.clone(),
            })
            .collect();
        let path = output.join(manifest::FILE_NAME);
        fs::create_dir_all(output).map_err(|e| Error::Io(output.to_path_buf(), e))?;
        fs::write(&path, manifest::encode(&entries)).map_err(|e| Error::Io(path, e))?;

        let produced = plan
            .assets
            .iter()
            .map(|a| a.output.to_lowercase())
            .collect();
        outcome.removed = remove_stale(output, &produced)?;
        for name in manifest::LEGACY_FILE_NAMES {
            let legacy = output.join(name);
            if legacy.is_file() {
                fs::remove_file(&legacy).map_err(|e| Error::Io(legacy, e))?;
                outcome.removed.push(name.into());
            }
        }
    }
    Ok(outcome)
}

/// `bc7, srgb, 10 levels`; see docs/features/textures.md#format.
fn levels_detail(format: Target, color: ColorSpace, levels: usize) -> String {
    let noun = if levels == 1 { "level" } else { "levels" };
    format!(
        "{}, {color}, {levels} {noun}",
        format.token(crate::atex::Kind::Rgba)
    )
}

fn material_detail(m: &material::Material) -> String {
    let t = &m.textures;
    let slots: Vec<_> = [
        ("base_color", t.base_color),
        ("normal", t.normal),
        ("orm", t.orm),
        ("emissive", t.emissive),
        ("height", t.height),
    ]
    .into_iter()
    .filter(|(_, id)| *id != 0)
    .map(|(name, _)| name)
    .collect();
    if slots.is_empty() {
        "material: no textures".into()
    } else {
        format!("material: {}", slots.join(", "))
    }
}

/// A hash of `job` and of the size and modification time of every file it reads; see
/// docs/features/cli.md#assetc-watch. `schemas` holds the input's schemas and data files once
/// listed.
fn stamp(input: &Path, job: &Job, schemas: &mut Option<Vec<PathBuf>>) -> u64 {
    // Attached structs depend on every schema and data file; see
    // docs/features/structs.md#build. A source that mentions them is taken to have some.
    let mentions =
        |file: PathBuf| fs::read_to_string(file).is_ok_and(|text| text.contains("structs"));
    let structs = match job {
        Job::Model { sidecar, .. } => !sidecar.structs.is_empty(),
        Job::Material { structs, .. } => !structs.is_empty(),
        Job::Atlas { source, .. } => mentions(input.join(source).join(crate::atlas::CONFIG)),
        Job::Data { source, kind } => *kind != Kind::Schema && mentions(input.join(source)),
        // Params are a struct of a schema; see docs/features/custom-materials.md#build.
        Job::Shader { .. } | Job::CustomMaterial { .. } => true,
        _ => false,
    };
    let mut reads: Vec<PathBuf> = match job {
        // With its custom mip chain's files, and the modifiers of every image; see
        // docs/features/mipmaps.md#build.
        Job::Texture { source, custom, .. } => {
            let mut reads = vec![input.join(source)];
            if let Some(chain) = custom {
                reads.push(crate::mipmap::path_for(&input.join(source)));
                reads.extend(chain.files.values().map(|file| input.join(&file.path)));
            }
            let sidecars: Vec<PathBuf> = reads
                .iter()
                .filter(|file| is_image(file))
                .map(|file| crate::amod::path_for(file))
                .collect();
            reads.extend(sidecars);
            reads
        }
        Job::Lut { source, .. } | Job::Cubemap { source, .. } | Job::Font { source } => {
            vec![input.join(source)]
        }
        Job::Orm { sources } => sources.iter().flatten().map(|s| input.join(s)).collect(),
        Job::Model { source, .. } if source.to_ascii_lowercase().ends_with(".gltf") => {
            files_under(input.join(source).parent().unwrap_or(input))
        }
        // The model's files and its sidecar; see docs/features/colliders.md#build.
        Job::Collider { source, .. } => {
            let file = input.join(source);
            let mut reads = if source.to_ascii_lowercase().ends_with(".gltf") {
                files_under(file.parent().unwrap_or(input))
            } else {
                vec![file.clone()]
            };
            reads.push(crate::acollider::path_for(&file));
            reads
        }
        // With its texture paints' files; see docs/features/texture-paint.md#build.
        Job::Model {
            source, sidecar, ..
        } => std::iter::once(input.join(source))
            .chain(
                sidecar
                    .paints
                    .iter()
                    .map(|p| crate::texture_paint::file_for(&input.join(source), &p.name)),
            )
            .collect(),
        Job::Atlas { source, .. } => files_under(&input.join(source)),
        Job::Data { source, .. } => vec![input.join(source)],
        Job::Pack { sources, .. } => sources
            .iter()
            .flatten()
            .map(|(s, _)| input.join(s))
            .collect(),
        Job::Shader { source } => shader_reads(input, source),
        Job::CustomMaterial { source, shader, .. } => {
            let mut reads = shader_reads(input, shader);
            reads.push(input.join(source));
            reads
        }
        Job::Material { source, .. } if source.ends_with(material::SOURCE_SUFFIX) => {
            vec![input.join(source)]
        }
        Job::Material { .. }
        | Job::ModelPart { .. }
        | Job::FontPart { .. }
        | Job::AtlasPart { .. } => Vec::new(),
    };
    if structs
        || matches!(
            job,
            Job::Data {
                kind: Kind::Data,
                ..
            }
        )
    {
        reads.extend(
            schemas
                .get_or_insert_with(|| {
                    files_under(input)
                        .into_iter()
                        .filter(|f| crate::aschema::is_schema(f) || crate::adata::is_data(f))
                        .collect()
                })
                .iter()
                .cloned(),
        );
    }
    reads.sort();
    let mut text = format!("{job:?}");
    for file in reads {
        let meta = fs::metadata(&file).ok();
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        let len = meta.map_or(u64::MAX, |m| m.len());
        text += &format!("\n{} {len} {modified}", file.display());
    }
    crate::hash::fnv1a64(text.as_bytes())
}

/// Every file under `dir`, as the build lists them (dotfiles skipped, symlinks not followed).
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    fn walk(nodes: &[tree::Node], out: &mut Vec<PathBuf>) {
        for node in nodes {
            match &node.children {
                Some(children) => walk(children, out),
                None => out.push(node.path.clone()),
            }
        }
    }
    let mut out = Vec::new();
    walk(&tree::scan(dir).unwrap_or_default(), &mut out);
    out
}

/// Deletes outputs not in `produced` (lowercased `output`-relative paths) and
/// the directories this leaves empty. Returns the deleted paths.
///
/// Only files matching an output pattern (`*.<ext>` of an asset kind, or an extension outputs
/// had before `.bin`, any case) are candidates, so other files in `output` are never touched.
fn remove_stale(output: &Path, produced: &HashSet<String>) -> Result<Vec<String>, Error> {
    let nodes = tree::scan(output).map_err(|e| Error::Io(output.to_path_buf(), e))?;
    let mut files = Vec::new();
    flatten(&nodes, &mut files);

    let mut removed = Vec::new();
    for file in files {
        let is_output = Kind::of(file).is_some()
            || file.extension().is_some_and(|ext| {
                manifest::LEGACY_EXTENSIONS
                    .iter()
                    .any(|old| ext.eq_ignore_ascii_case(old))
            });
        let Ok(rel) = relative(output, file) else {
            continue;
        };
        if !is_output || produced.contains(&rel.to_lowercase()) {
            continue;
        }
        fs::remove_file(file).map_err(|e| Error::Io(file.to_path_buf(), e))?;
        for dir in file.ancestors().skip(1).take_while(|dir| *dir != output) {
            let mut entries = fs::read_dir(dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
            if entries.next().is_some() {
                break;
            }
            fs::remove_dir(dir).map_err(|e| Error::Io(dir.to_path_buf(), e))?;
        }
        removed.push(rel);
    }
    Ok(removed)
}

/// Texture source extensions, matched case-insensitively: images, and generated noise.
const TEXTURE_EXTENSIONS: [&str; 7] = [
    "png",
    "jpg",
    "jpeg",
    "tga",
    "anoise",
    "agradient",
    "apalette",
];

pub fn is_texture_source(file: &Path) -> bool {
    file.extension().is_some_and(|ext| {
        TEXTURE_EXTENSIONS
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

/// `input`-relative, `/`-separated path.
pub fn relative(input: &Path, file: &Path) -> Result<String, String> {
    let rel = file.strip_prefix(input).map_err(|e| e.to_string())?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    parts
        .map(|p| p.join("/"))
        .ok_or_else(|| "path is not valid UTF-8".into())
}

pub fn strip_extension(rel: &str) -> &str {
    let name_start = rel.rfind('/').map_or(0, |i| i + 1);
    match rel[name_start..].rfind('.') {
        Some(dot) if dot > 0 => &rel[..name_start + dot],
        _ => rel,
    }
}

fn decode(source: &Path) -> Result<Image, String> {
    load_source(source).map(|(image, _)| image)
}

/// Decodes a texture source, or generates it for an `.anoise` file; also returns the
/// source's channels (noise is grayscale).
pub fn load_source(source: &Path) -> Result<(Image, Channels), String> {
    let opaque = |alpha: bool| if alpha { Channels::Rgba } else { Channels::Rgb };
    if is_noise(source) {
        let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
        let noise = Noise::parse(&text)?;
        let color = if noise.has_color() {
            Channels::Rgb
        } else {
            Channels::Gray
        };
        return Ok((noise.generate(), color));
    }
    if crate::palette::is_palette(source) {
        let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
        let palette = crate::palette::Palette::parse(&text)?;
        return Ok((palette.render()?, opaque(palette.has_alpha())));
    }
    if is_gradient(source) {
        let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
        let gradients = Gradients::parse(&text)?;
        return Ok((gradients.render()?, opaque(gradients.has_alpha())));
    }
    let (mut image, color) = crate::imagefile::read(source)?;
    let color = apply_sidecar(source, &mut image, color)?;
    Ok((image, color))
}

/// Runs the image's `.amod` modifiers on it, returning its channels after them; see
/// docs/features/textures.md#modifiers.
fn apply_sidecar(source: &Path, image: &mut Image, color: Channels) -> Result<Channels, String> {
    let Some(amod) = crate::amod::read(source)? else {
        return Ok(color);
    };
    let modifiers = &amod.modifiers;
    if !modifiers.iter().any(|m| m.enabled) {
        return Ok(color);
    }
    let (width, height) = (image.width as usize, image.height as usize);
    let mut pixels: Vec<[f64; 3]> = image
        .texels()
        .iter()
        .map(|p| [0, 1, 2].map(|c| f64::from(p[c]) / 255.0))
        .collect();
    crate::noise::apply_modifiers(modifiers, &mut pixels, width, height);
    for (p, rgb) in image.texels_mut().iter_mut().zip(pixels) {
        for c in 0..3 {
            p[c] = (rgb[c].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    // Colorize turns a grayscale image into a colored one.
    let colorized = modifiers
        .iter()
        .any(|m| m.enabled && m.kind == crate::noise::ModifierKind::Colorize);
    Ok(match (color.has_color(), colorized, color.has_alpha()) {
        (false, true, true) => Channels::Rgba,
        (false, true, false) => Channels::Rgb,
        _ => color,
    })
}

/// Writes every output of a glTF model; see docs/features/models.md#build.
fn compile_model(
    input: &Path,
    source: &str,
    output: &Path,
    (options, sidecar, format): (&crate::amesh::MeshOptions, &crate::amod::Amod, Target),
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let p = strip_extension(source);
    let model = crate::model::compile(&input.join(source), p, options, sidecar)?;
    // Attached structs; see docs/features/structs.md#build.
    let scopes = [crate::structs::Scope {
        scope: crate::structs::ASSET,
        attachments: &sidecar.structs,
        owners: vec![crate::structs::Owner {
            item: 0,
            label: "model".into(),
            values: &sidecar.values,
        }],
    }];
    let structs = crate::structs::compile(dir_of(&input.join(source)), input, &scopes, warnings)?;
    let mesh = crate::chunks::with_chunks(model.mesh, crate::amesh::FORMAT, structs)?;
    write(&output.join(format!("{p}.amesh.bin")), &mesh)?;
    if let Some(animation) = &model.animation {
        write(&output.join(format!("{p}.aanim.bin")), animation)?;
    }
    for (name, m) in &model.materials {
        write(
            &output.join(format!("{p}/{name}.amat.bin")),
            &material::encode(m),
        )?;
    }
    for (name, image, color) in model.images {
        // A glTF's linear images are its normal maps; see docs/features/models.md#build.
        let usage = match color {
            ColorSpace::Srgb => Usage::Color,
            ColorSpace::Linear => Usage::Normal,
        };
        let dest = output.join(format!("{p}/{name}.atex.bin"));
        write_texture(image, &dest, (format, color, usage), true)?;
    }
    for (name, image) in model.orm {
        let dest = output.join(format!("{p}/{name}.orm.atex.bin"));
        write_texture(image, &dest, (format, ColorSpace::Linear, Usage::Orm), true)?;
    }
    // A lookup texture: always `rgba`; see docs/features/textures.md#format.
    for (name, image) in model.color_textures {
        let dest = output.join(format!("{p}/{name}.colors.atex.bin"));
        let target = (Target::RGBA, ColorSpace::Srgb, Usage::Color);
        write_texture(image, &dest, target, false)?;
    }
    // Texture paints; see docs/features/texture-paint.md#build.
    for (name, levels, export) in model.paint_textures {
        let dest = output.join(format!("{p}/{name}.paint.atex.bin"));
        let bytes = match export {
            crate::texture_paint::Export::Rgba(color) => {
                crate::atex::encode_rgba8(format, color, Usage::of(color), &levels)?
            }
            crate::texture_paint::Export::Unorm(channels) => {
                crate::atex::encode_unorm(format, channels, Usage::Data, &levels)?
            }
        };
        write(&dest, &bytes)?;
    }
    Ok(model.detail)
}

/// Makes a model's `P.acollider.bin`; see docs/features/colliders.md#build.
fn compile_collider(
    input: &Path,
    source: &str,
    dest: &Path,
    settings: &crate::decompose::Settings,
) -> Result<String, String> {
    let collider = crate::acollider::make(&input.join(source), settings)?;
    let mesh = crate::hash::fnv1a64(format!("{}.amesh.bin", strip_extension(source)).as_bytes());
    write(dest, &crate::acollider::encode(&collider, mesh))?;
    let n = collider.hulls.len();
    Ok(format!(
        "{n} hull{}, {} points",
        if n == 1 { "" } else { "s" },
        collider.points()
    ))
}

/// Packs an atlas folder into `P.atex.bin` and `P.aatlas.bin`; see docs/features/atlases.md#build.
fn compile_atlas(
    input: &Path,
    source: &str,
    output: &Path,
    (format, color, mipmaps): (Target, ColorSpace, bool),
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    let p = strip_extension(source);
    let (config, packed) = crate::atlas::build(&input.join(source))?;
    let summary = crate::atlas::packing_summary(&config, &packed);
    let (width, height) = packed.image.dimensions();
    let frames = packed.frames.len();
    let meta = crate::atlas::encode(&config, &packed, crate::hash::fnv1a64(p.as_bytes()));
    // Attached structs, kept in its atlas.toml; see docs/features/structs.md#build.
    let scopes = [crate::structs::Scope {
        scope: crate::structs::ASSET,
        attachments: &config.structs,
        owners: vec![crate::structs::Owner {
            item: 0,
            label: "atlas".into(),
            values: &config.values,
        }],
    }];
    let structs = crate::structs::compile(&input.join(source), input, &scopes, warnings)?;
    let meta = crate::chunks::with_chunks(meta, crate::atlas::FORMAT, structs)?;
    write(&output.join(format!("{p}.aatlas.bin")), &meta)?;
    write_texture(
        packed.image,
        &output.join(format!("{p}.atex.bin")),
        (format, color, Usage::Atlas),
        mipmaps,
    )?;
    Ok(format!("{frames} frames, {width} × {height} px, {summary}"))
}

/// Writes one cubemap of an HDR environment map; see docs/features/hdr.md.
fn compile_cubemap(
    source: &Path,
    dest: &Path,
    format: Target,
    map: plan::Environment,
) -> Result<String, String> {
    use crate::hdr;
    use plan::Environment;
    let hdr = hdr::read(source)?;
    let (usage, levels, sh) = match map {
        Environment::Skybox => (
            Usage::Environment,
            hdr.cubemap(hdr::face_size(hdr.height)),
            None,
        ),
        Environment::Specular { size, samples } => (
            Usage::EnvironmentSpecular,
            hdr::specular(&hdr::lighting_source(&hdr), size as usize, samples),
            None,
        ),
        Environment::Irradiance { size } => {
            let (level, sh) = hdr::irradiance(&hdr::lighting_source(&hdr), size as usize);
            (Usage::EnvironmentIrradiance, vec![level], Some(sh))
        }
    };
    write(
        dest,
        &crate::atex::encode_cubemap(format, usage, &levels, sh.as_ref())?,
    )?;
    let token = format.token(crate::atex::Kind::Hdr);
    let (size, count) = (levels[0].size, levels.len());
    let noun = if count == 1 { "level" } else { "levels" };
    Ok(format!("cubemap {size}², {token}, {count} {noun}"))
}

fn first_problem(problems: &[String]) -> Result<(), String> {
    match problems {
        [] => Ok(()),
        [one] => Err(one.clone()),
        [one, rest @ ..] => Err(format!("{one} (and {} more)", rest.len())),
    }
}

/// A material's record, then its attached structs as an `ASTRUCT` file; see
/// docs/spec/amat.bin.md#struct-data.
fn compile_material(
    input: &Path,
    source: &str,
    m: &material::Material,
    (structs, values): (&[crate::structs::Attachment], &crate::structs::Values),
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, String> {
    let mut bytes = material::encode(m);
    let scopes = [crate::structs::Scope {
        scope: crate::structs::ASSET,
        attachments: structs,
        owners: vec![crate::structs::Owner {
            item: 0,
            label: "material".into(),
            values,
        }],
    }];
    let chunks = crate::structs::compile(dir_of(&input.join(source)), input, &scopes, warnings)?;
    if !chunks.is_empty() {
        let mut writer = crate::chunks::Writer::new(material::ASTRUCT, 1);
        crate::compiled::put_structs(&mut writer, chunks);
        bytes.extend(writer.finish());
    }
    Ok(bytes)
}

/// The folder of `file`.
fn dir_of(file: &Path) -> &Path {
    file.parent().unwrap_or(Path::new(""))
}

/// Compiles a schema, data file (resolved), graph or tree to its binary; see
/// docs/features/build.md#inputs.
fn compile_data(
    input: &Path,
    source: &str,
    dest: &Path,
    kind: Kind,
    warnings: &mut Vec<String>,
) -> Result<String, String> {
    use crate::compiled;
    let rel = &source.replace('\\', "/");
    let source = &input.join(source);
    let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
    let (written, detail) = match kind {
        Kind::Schema => {
            let schema = crate::aschema::Schema::parse(&text)?;
            let problems: Vec<String> = schema
                .check()
                .into_iter()
                .map(|p| format!("{}: {}", p.path, p.message))
                .collect();
            first_problem(&problems)?;
            let detail = format!("{} types", schema.types.len());
            (compiled::encode_schema(&schema), detail)
        }
        Kind::Data => {
            let data = crate::adata::DataFile::parse(&text)?;
            let (path, schema) = crate::adata::load_schema(source, &data);
            let schema = schema.map_err(|e| format!("schema {}: {e}", path.display()))?;
            // Asset references are checked against the input; see
            // docs/aschema.md#asset-references.
            let assets = crate::adata::asset_files(input);
            // Ids and refs reach the project's other files; see
            // docs/features/instances.md#references.
            let others = crate::adata::others(input, source, &data);
            let (found, errors): (Vec<_>, Vec<_>) = data
                .check_with(&schema, Some(&assets), &others)
                .into_iter()
                .partition(|p| p.warning);
            warnings.extend(found.into_iter().map(|p| p.message));
            let errors: Vec<String> = errors.into_iter().map(|p| p.message).collect();
            first_problem(&errors)?;
            let n = data.instances.len();
            (
                compiled::encode_data(&data, &schema, rel, &others),
                format!("{n} instances"),
            )
        }
        Kind::Gradient => {
            let file = crate::gradient::Gradients::parse(&text)?;
            let texture = crate::hash::fnv1a64(strip_extension(rel).as_bytes());
            let structs = crate::structs::compile(dir_of(source), input, &file.scopes(), warnings)?;
            let n = file.gradients.len();
            (
                compiled::encode_gradients(&file, texture, structs),
                format!("{n} gradients"),
            )
        }
        Kind::Palette => {
            let file = crate::palette::Palette::parse(&text)?;
            let texture = crate::hash::fnv1a64(strip_extension(rel).as_bytes());
            let structs = crate::structs::compile(dir_of(source), input, &file.scopes(), warnings)?;
            let n = file.colors.len();
            (
                compiled::encode_palette(&file, texture, structs),
                format!("{n} colors"),
            )
        }
        Kind::Graph => {
            let graph = crate::agraph::Graph::parse(&text)?;
            first_problem(&graph.check())?;
            let detail = format!("{} nodes, {} edges", graph.nodes.len(), graph.edges.len());
            let structs =
                crate::structs::compile(dir_of(source), input, &graph.scopes(None), warnings)?;
            (compiled::encode_graph(&graph, structs), detail)
        }
        _ => {
            let tree = crate::atree::parse(&text)?;
            first_problem(&tree.check())?;
            let detail = format!(
                "{} nodes, depth {}",
                tree.nodes.len(),
                crate::atree::depth(&tree)
            );
            let mut places = vec![0u32; tree.nodes.len()];
            for (place, (node, _, _)) in compiled::breadth_first(&tree).into_iter().enumerate() {
                places[node] = place as u32;
            }
            let scopes = tree.scopes(Some(&places));
            let structs = crate::structs::compile(dir_of(source), input, &scopes, warnings)?;
            (compiled::encode_tree(&tree, structs), detail)
        }
    };
    write(dest, &written)?;
    Ok(detail)
}

/// Writes a font's `.afont.bin` and SDF atlas; see docs/features/fonts.md#build.
fn compile_font(
    input: &Path,
    source: &str,
    output: &Path,
    format: Target,
) -> Result<String, String> {
    let p = strip_extension(source);
    let data = fs::read(input.join(source)).map_err(|e| e.to_string())?;
    let font = crate::font::compile(&data, crate::hash::fnv1a64(p.as_bytes()))?;
    write(&output.join(format!("{p}.afont.bin")), &font.afont)?;
    let (width, height) = (font.atlas.width, font.atlas.height);
    write_texture(
        font.atlas,
        &output.join(format!("{p}.atex.bin")),
        (format, ColorSpace::Linear, Usage::Font),
        false,
    )?;
    Ok(format!(
        "{} glyphs, {} kerning pairs, {width} × {height} atlas",
        font.glyphs, font.kerning
    ))
}

/// An image file (`png`, `jpg`, `jpeg`, `tga`), as opposed to generated texture sources.
pub fn is_image(file: &Path) -> bool {
    is_texture_source(file)
        && !is_noise(file)
        && !is_gradient(file)
        && !crate::palette::is_palette(file)
}

pub fn is_lut(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cube"))
}

/// Writes a LUT as a strip, or a 3D texture when `volume`; returns its size. See
/// docs/features/lut.md#build.
fn compile_lut(source: &Path, dest: &Path, volume: bool) -> Result<usize, String> {
    let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
    let lut = crate::lut::Lut::parse(&text)?;
    let bytes = if volume {
        let n = lut.size as u32;
        crate::atex::encode_rgba8_volume(ColorSpace::Linear, Usage::Lut, [n, n, n], &lut.volume())?
    } else {
        crate::atex::encode_rgba8(
            Target::RGBA,
            ColorSpace::Linear,
            Usage::Lut,
            &[lut.strip()?],
        )?
    };
    write(dest, &bytes)?;
    Ok(lut.size)
}

pub fn is_noise(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("anoise"))
}

/// Compiles `source` (relative to `input`), with the levels of its `custom` chain when it has
/// one (see docs/features/mipmaps.md); returns the number of levels written and how many of
/// them came from files.
fn compile_texture(
    (input, source): (&Path, &str),
    dest: &Path,
    color: (Target, ColorSpace, Usage),
    mipmaps: bool,
    flip_green: bool,
    custom: Option<&crate::mipmap::Chain>,
) -> Result<(usize, usize), String> {
    let load = |rel: &str| -> Result<Image, String> {
        let mut image = decode(&input.join(rel))?;
        if flip_green {
            for pixel in image.texels_mut().iter_mut() {
                pixel[1] = 255 - pixel[1];
            }
        }
        Ok(image)
    };
    let image = load(source)?;
    let Some(chain) = custom else {
        return write_texture(image, dest, color, mipmaps).map(|levels| (levels, 0));
    };
    let (format, space, usage) = color;
    let (levels, custom) = crate::mipmap::build(image, space, chain, load)?;
    write(
        dest,
        &crate::atex::encode_rgba8(format, space, usage, &levels)?,
    )?;
    Ok((levels.len(), custom))
}

/// Packs the red channels of occlusion, roughness, metallic into R, G, B;
/// missing ones are 255. Returns the number of levels written.
fn compile_orm(
    input: &Path,
    sources: &[Option<String>; 3],
    dest: &Path,
    format: Target,
) -> Result<usize, String> {
    let mut size = None;
    let mut channels: Vec<(usize, Image)> = Vec::new();
    for (channel, source) in sources.iter().enumerate() {
        let Some(source) = source else {
            continue;
        };
        let image = decode(&input.join(source)).map_err(|e| format!("{source}: {e}"))?;
        match size {
            None => size = Some((source, image.dimensions())),
            Some((first, dims)) if dims != image.dimensions() => {
                let (w, h) = image.dimensions();
                return Err(format!(
                    "ORM inputs differ in size: {first} is {} × {}, {source} is {w} × {h}",
                    dims.0, dims.1
                ));
            }
            Some(_) => {}
        }
        channels.push((channel, image));
    }
    let Some((_, (width, height))) = size else {
        return Err("no ORM inputs".into());
    };
    let mut packed = Image::filled(width, height, [255; 4]);
    for (channel, image) in &channels {
        for (out, src) in packed.texels_mut().iter_mut().zip(image.texels()) {
            out[*channel] = src[0];
        }
    }
    write_texture(packed, dest, (format, ColorSpace::Linear, Usage::Orm), true)
}

/// A custom material's packed texture: channel `c` from its input's channel, else `fill`; see
/// docs/features/custom-materials.md#packing.
fn compile_pack(
    input: &Path,
    sources: &[Option<(String, u8)>; 4],
    fill: [u8; 4],
    dest: &Path,
    (format, mipmaps): (Target, bool),
) -> Result<usize, String> {
    let mut size: Option<(&str, (u32, u32))> = None;
    let mut images: HashMap<&str, Image> = HashMap::new();
    for (source, _) in sources.iter().flatten() {
        if images.contains_key(source.as_str()) {
            continue;
        }
        let image = decode(&input.join(source)).map_err(|e| format!("{source}: {e}"))?;
        match size {
            None => size = Some((source, image.dimensions())),
            Some((first, dims)) if dims != image.dimensions() => {
                let (w, h) = image.dimensions();
                return Err(format!(
                    "pack inputs differ in size: {first} is {} × {}, {source} is {w} × {h}",
                    dims.0, dims.1
                ));
            }
            Some(_) => {}
        }
        images.insert(source, image);
    }
    let Some((_, (width, height))) = size else {
        return Err("no pack inputs".into());
    };
    let mut packed = Image::filled(width, height, fill);
    for (channel, source) in sources.iter().enumerate() {
        let Some((source, from)) = source else {
            continue;
        };
        for (out, src) in packed
            .texels_mut()
            .iter_mut()
            .zip(images[source.as_str()].texels())
        {
            out[channel] = src[*from as usize];
        }
    }
    write_texture(
        packed,
        dest,
        (format, ColorSpace::Linear, Usage::Data),
        mipmaps,
    )
}

/// The files a shader's build reads: the `.ashader` and its stages.
fn shader_reads(input: &Path, rel: &str) -> Vec<PathBuf> {
    let mut reads = vec![input.join(rel)];
    if let Ok(shader) = fs::read_to_string(input.join(rel))
        .map_err(|e| e.to_string())
        .and_then(|t| crate::shader::Shader::parse(&t))
    {
        let dir = crate::plan::split_dir(rel).0;
        for path in [&shader.stages.vertex, &shader.stages.fragment]
            .into_iter()
            .flatten()
        {
            if let Some(file) = crate::plan::resolve(dir, path) {
                reads.push(input.join(file));
            }
        }
    }
    reads
}

/// `.acmat.bin` of a custom material; see docs/features/custom-materials.md#build.
fn compile_custom_material(
    input: &Path,
    source: &str,
    shader_rel: &str,
    textures: &[u64],
    material: &crate::acmat::Source,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, String> {
    use crate::structs::{Attachment, Owner, Scope, Values};
    let loaded =
        crate::shader::load(input, shader_rel).map_err(|e| format!("shader {shader_rel}: {e}"))?;
    let dir = crate::plan::split_dir(source).0;
    // The params are checked and compiled like an attached struct of scope 3.
    let params: Vec<Attachment> = loaded
        .params
        .iter()
        .map(|(schema_rel, ty, ..)| {
            let rel = crate::adata::relative(Path::new(dir), Path::new(schema_rel));
            Attachment::new(rel, ty.clone())
        })
        .collect();
    let mut params_values = Values::new();
    if let Some(a) = params.first() {
        params_values.insert(a.ty.clone(), material.params.clone().into());
    } else if !material.params.is_empty() {
        warnings.push(format!("[params] is set but {shader_rel} has no params"));
    }
    let scopes = [
        Scope {
            scope: crate::structs::ASSET,
            attachments: &material.structs,
            owners: vec![Owner {
                item: 0,
                label: "material".into(),
                values: &material.values,
            }],
        },
        Scope {
            scope: crate::acmat::PARAMS_SCOPE,
            attachments: &params,
            owners: vec![Owner {
                item: 0,
                label: "params".into(),
                values: &params_values,
            }],
        },
    ];
    let schemas = crate::structs::load(&input.join(dir), Some(input), &scopes);
    let chunks = crate::structs::finish(input, &scopes, &schemas, warnings)?;
    let block = match &loaded.params {
        Some((_, _, schema, si)) => {
            let values = crate::adata::resolve(schema, *si, &material.params, None);
            crate::shader::pack(&loaded.block, &values)
        }
        None => Vec::new(),
    };
    let shader_id = crate::hash::fnv1a64(
        format!(
            "{}.{}",
            strip_extension(shader_rel),
            Kind::Shader.extension()
        )
        .as_bytes(),
    );
    let state = loaded.shader.state.with(material.state);
    Ok(crate::acmat::encode(
        shader_id, &state, &block, textures, chunks,
    ))
}

fn write_texture(
    image: Image,
    dest: &Path,
    (format, color, usage): (Target, ColorSpace, Usage),
    mipmaps: bool,
) -> Result<usize, String> {
    let levels = if mipmaps {
        texture::mip_chain(image, color)
    } else {
        vec![image]
    };
    write(
        dest,
        &crate::atex::encode_rgba8(format, color, usage, &levels)?,
    )?;
    Ok(levels.len())
}

fn write(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atex::Format;

    #[test]
    fn hdr_maps_become_half_float_cubemaps() {
        let dir = tempfile::tempdir().unwrap();
        let (source, dest) = (dir.path().join("sky.exr"), dir.path().join("sky.atex.bin"));
        crate::hdr::tests::write_exr(&source, 64, 32, |_, y| [4.0, y as f32 / 32.0, 0.5]);
        let detail =
            compile_cubemap(&source, &dest, Target::default(), plan::Environment::Skybox).unwrap();
        assert_eq!(detail, "cubemap 16², bc6h, 5 levels");
        let info = crate::atex::read(&fs::read(&dest).unwrap()).unwrap();
        assert_eq!(info.format_name(), "BC6H_UFLOAT_BLOCK");

        let detail =
            compile_cubemap(&source, &dest, Target::RGBA, plan::Environment::Skybox).unwrap();
        assert_eq!(detail, "cubemap 16², rgba16f, 5 levels");
        let info = crate::atex::read(&fs::read(&dest).unwrap()).unwrap();
        assert_eq!(
            (info.width, info.height, info.faces, info.levels),
            (16, 16, 6, 5)
        );
        assert_eq!(info.format_name(), "R16G16B16A16_SFLOAT");
        // Level 0 is six 16 × 16 faces of 8-byte texels; red stays 4.0.
        let (_, level0) = level0(&dest);
        assert_eq!(level0.len(), 6 * 16 * 16 * 8);
        let red = imgio::half::to_f32(u16::from_le_bytes([level0[0], level0[1]]));
        assert_eq!(red, 4.0);
    }

    /// The bytes of level `i` of a `.atex.bin` file.
    fn level_data(path: &Path, i: usize) -> Vec<u8> {
        let bytes = fs::read(path).unwrap();
        let reader = crate::chunks::Reader::parse(&bytes, crate::atex::FORMAT).unwrap();
        let levels = reader.chunk("LEVELS").unwrap();
        let at = |o| crate::chunks::u32_at(levels, i, 32, o).unwrap() as usize;
        reader.chunk("DATA").unwrap()[at(0)..at(0) + at(8)].to_vec()
    }

    #[test]
    fn hdr_maps_get_lighting_maps() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("env")).unwrap();
        crate::hdr::tests::write_exr(&input.join("env/sky.exr"), 64, 32, |x, y| {
            [1.0 + x as f32 / 8.0, 2.0, if y < 8 { 50.0 } else { 0.5 }]
        });
        let config = config::Config::default();
        let outcome = run(&input, &output, &config).unwrap();
        let lines: Vec<String> = outcome
            .built
            .iter()
            .map(|b| format!("{} -> {} ({})", b.source, b.output, b.detail))
            .collect();
        assert_eq!(
            lines,
            [
                "env/sky.exr -> env/sky.atex.bin (cubemap 16², bc6h, 5 levels)",
                "env/sky.exr -> env/sky.specular.atex.bin (cubemap 16², bc6h, 3 levels)",
                "env/sky.exr -> env/sky.irradiance.atex.bin (cubemap 32², bc6h, 1 level)",
            ]
        );
        let entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        for path in ["env/sky", "env/sky.specular", "env/sky.irradiance"] {
            let entry = entries
                .iter()
                .find(|e| e.id == crate::hash::fnv1a64(path.as_bytes()))
                .unwrap();
            assert_eq!(entry.path, format!("{path}.atex.bin"));
        }
        let usage = |name: &str| {
            crate::atex::read(&fs::read(output.join(name)).unwrap())
                .unwrap()
                .usage
        };
        assert_eq!(usage("env/sky.specular.atex.bin"), "environment_specular");
        assert_eq!(
            usage("env/sky.irradiance.atex.bin"),
            "environment_irradiance"
        );
        let irradiance = fs::read(output.join("env/sky.irradiance.atex.bin")).unwrap();
        let reader = crate::chunks::Reader::parse(&irradiance, crate::atex::FORMAT).unwrap();
        assert_eq!(reader.chunk("SH9").unwrap().len(), 27 * 4);

        // Two builds of the same input are byte-identical.
        let again = dir.path().join("again");
        run(&input, &again, &config).unwrap();
        for name in ["sky", "sky.specular", "sky.irradiance"] {
            let file = format!("env/{name}.atex.bin");
            assert_eq!(
                fs::read(output.join(&file)).unwrap(),
                fs::read(again.join(&file)).unwrap(),
                "{file}"
            );
        }

        // Specular level 0 is the skybox's level of its size.
        let mut config = config::Config::default();
        config.textures.format = crate::atex::Format::Rgba;
        config.environments.specular_size = 8;
        let mut cache = Cache::default();
        run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(
            level_data(&output.join("env/sky.specular.atex.bin"), 0),
            level_data(&output.join("env/sky.atex.bin"), 1)
        );
        // Only the lighting maps depend on [environments].
        config.environments.samples = 16;
        config.environments.irradiance_size = 4;
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        let built: Vec<&str> = outcome.built.iter().map(|b| b.output.as_str()).collect();
        assert_eq!(
            built,
            ["env/sky.specular.atex.bin", "env/sky.irradiance.atex.bin"]
        );
        assert_eq!(outcome.up_to_date, 1);

        // Without lighting, only the skybox is left.
        config.environments.lighting = false;
        let outcome = run(&input, &output, &config).unwrap();
        assert_eq!(outcome.built.len(), 1);
        let mut removed = outcome.removed.clone();
        removed.sort();
        assert_eq!(
            removed,
            ["env/sky.irradiance.atex.bin", "env/sky.specular.atex.bin"]
        );

        config.environments.specular_size = 4;
        let e = run(&input, &output, &config).unwrap_err();
        assert_eq!(
            e.to_string(),
            "assetc.toml [environments]: specular_size must be a power of two, 8–4096, not 4"
        );
    }

    #[test]
    fn textures_are_bc_by_default_and_follow_the_format() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("grass.png"), 8, 8);
        let mut cache = Cache::default();
        let config = config::Config::default();
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built[0].detail, "bc7, srgb, 4 levels");
        assert_eq!(level0(&output.join("grass.atex.bin")).0, "BC7_SRGB_BLOCK");

        // Another format rebuilds every texture.
        let astc = config::Config {
            textures: config::Textures {
                format: Format::Astc,
                ..Default::default()
            },
            ..Default::default()
        };
        let outcome = run_cached(&input, &output, &astc, &mut cache).unwrap();
        assert_eq!(outcome.up_to_date, 0);
        assert_eq!(outcome.built[0].detail, "astc 4x4, srgb, 4 levels");
        assert_eq!(
            level0(&output.join("grass.atex.bin")).0,
            "ASTC_4x4_SRGB_BLOCK"
        );
        let again = run_cached(&input, &output, &astc, &mut cache).unwrap();
        assert_eq!(again.up_to_date, 1);

        // So does another quality.
        let mut thorough = astc.clone();
        thorough.textures.quality = crate::atex::Quality::High;
        let outcome = run_cached(&input, &output, &thorough, &mut cache).unwrap();
        assert_eq!(outcome.up_to_date, 0);
    }

    #[test]
    fn colliders_build_from_their_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        let fox = input.join("Fox.glb");
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Fox.glb"),
            &fox,
        )
        .unwrap();
        let config = config::Config::default();
        let mut cache = Cache::default();
        // No sidecar, no collider.
        run_cached(&input, &output, &config, &mut cache).unwrap();
        assert!(!output.join("Fox.acollider.bin").exists());

        let sidecar = crate::acollider::path_for(&fox);
        fs::write(&sidecar, "max_hulls = 4\n").unwrap();
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let built: Vec<_> = outcome.built.iter().map(|b| b.output.as_str()).collect();
        assert_eq!(built, ["Fox.acollider.bin"]);
        assert!(
            outcome.built[0].detail.starts_with("4 hulls, "),
            "{}",
            outcome.built[0].detail
        );
        let bytes = fs::read(output.join("Fox.acollider.bin")).unwrap();
        let collider = assetc::Collider::decode(&bytes).unwrap();
        assert_eq!(collider.hulls.len(), 4);
        assert_eq!(collider.mesh, crate::hash::fnv1a64(b"Fox.amesh.bin"));
        let manifest = fs::read(output.join(crate::manifest::FILE_NAME)).unwrap();
        let manifest = assetc::Manifest::decode(&manifest).unwrap();
        assert!(
            manifest
                .entries
                .iter()
                .any(|e| e.path == "Fox.acollider.bin" && e.kind == assetc::Kind::Collider)
        );

        // A new setting rebuilds the collider only.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&sidecar, "max_hulls = 2\n").unwrap();
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        let built: Vec<_> = outcome.built.iter().map(|b| b.output.as_str()).collect();
        assert_eq!(built, ["Fox.acollider.bin"]);

        // A bad sidecar fails the model.
        fs::write(&sidecar, "max_hulls = 0\n").unwrap();
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(
            outcome.failures[0].message,
            "Fox.glb.acollider: `max_hulls` must be 1 to 256, not 0"
        );

        // Without its sidecar, the collider is a stale output.
        fs::remove_file(&sidecar).unwrap();
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert!(!output.join("Fox.acollider.bin").exists());
    }

    #[test]
    fn summaries_count_every_kind() {
        let built = |kind| Built {
            source: String::new(),
            output: String::new(),
            kind,
            detail: String::new(),
        };
        let mut outcome = Outcome {
            skipped: 1,
            ..Default::default()
        };
        assert_eq!(outcome.summary(), "Built nothing (1 skipped)");
        outcome.built = [Kind::Font, Kind::Mesh, Kind::Texture, Kind::Texture]
            .into_iter()
            .chain([Kind::Animation, Kind::Material, Kind::Mesh])
            .map(built)
            .collect();
        assert_eq!(
            outcome.summary(),
            "Built 2 textures, 1 material, 2 meshes, 1 animation, 1 font (1 skipped)"
        );
    }

    /// `textures` written as `rgba`, so tests can read the texels back.
    fn with_textures(textures: &config::Textures) -> config::Config {
        config::Config {
            textures: config::Textures {
                format: Format::Rgba,
                ..textures.clone()
            },
            ..Default::default()
        }
    }

    /// The default settings, textures written as `rgba`.
    fn rgba() -> config::Config {
        with_textures(&config::Textures::default())
    }
    use crate::hash::fnv1a64;

    fn png(path: &Path, w: u32, h: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::imagefile::tests::png(
            path,
            &Image::from_fn(w, h, |x, y| [x as u8, y as u8, 7, 255]),
        );
    }

    #[test]
    fn asset_path_strips_final_extension_only() {
        assert_eq!(strip_extension("textures/Grass.png"), "textures/Grass");
        assert_eq!(strip_extension("a.b/c.d.png"), "a.b/c.d");
        assert_eq!(strip_extension("dir.x/noext"), "dir.x/noext");
        assert_eq!(strip_extension(".hidden"), ".hidden");
    }

    #[test]
    fn builds_textures_and_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("textures/Grass.png"), 4, 2);
        png(&input.join("logo.PNG"), 1, 1);
        fs::write(input.join("notes.txt"), "x").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built.len(), 2);
        assert_eq!(outcome.skipped, 1);
        assert_eq!(outcome.summary(), "Built 2 textures (1 skipped)");

        let grass = output.join("textures/Grass.atex.bin");
        let info = crate::atex::read(&fs::read(&grass).unwrap()).unwrap();
        assert_eq!((info.width, info.levels), (4, 3));
        let (format, data) = level0(&grass);
        assert_eq!(format, "R8G8B8A8_SRGB");
        assert_eq!(data[..4], [0, 0, 7, 255]);

        let mut entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "logo.atex.bin");
        assert_eq!(entries[0].id, fnv1a64(b"logo"));
        assert_eq!(entries[1].path, "textures/Grass.atex.bin");
        assert_eq!(entries[1].id, fnv1a64(b"textures/Grass"));
    }

    #[test]
    fn builds_fonts_to_afont_and_atlas() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("ui")).unwrap();
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/samples/monogram.ttf"),
            input.join("ui/monogram.ttf"),
        )
        .unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built.len(), 1);
        assert_eq!(outcome.built[0].output, "ui/monogram.afont.bin");
        assert!(outcome.built[0].detail.contains(" glyphs, "));

        let afont = fs::read(output.join("ui/monogram.afont.bin")).unwrap();
        let (_, atlas) = crate::font::afont_rows(&afont).unwrap();
        assert_eq!(atlas, fnv1a64(b"ui/monogram"));
        let info =
            crate::atex::read(&fs::read(output.join("ui/monogram.atex.bin")).unwrap()).unwrap();
        assert_eq!(info.format_name(), "R8G8B8A8_UNORM");
        assert_eq!(info.levels, 1);

        let mut entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, manifest::Kind::Font);
        assert_eq!(entries[0].id, fnv1a64(b"ui/monogram.afont.bin"));
        assert_eq!(entries[1].path, "ui/monogram.atex.bin");
        assert_eq!(entries[1].id, atlas);
    }

    #[test]
    fn incremental_builds_compile_what_changed() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        png(&input.join("b.png"), 1, 1);
        let config = config::Config::default();
        let mut cache = Cache::default();
        let first = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(first.summary(), "Built 2 textures (0 skipped)");

        let again = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(again.summary(), "Built nothing (0 skipped, 2 up to date)");

        // Changed, new and deleted files.
        png(&input.join("a.png"), 2, 2);
        png(&input.join("c.png"), 1, 1);
        fs::remove_file(input.join("b.png")).unwrap();
        let changed = run_cached(&input, &output, &config, &mut cache).unwrap();
        let built: Vec<&str> = changed.built.iter().map(|b| b.output.as_str()).collect();
        assert_eq!(built, ["a.atex.bin", "c.atex.bin"]);
        assert_eq!(changed.removed, ["b.atex.bin"]);

        // A missing output is written again.
        fs::remove_file(output.join("c.atex.bin")).unwrap();
        let restored = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(
            restored.summary(),
            "Built 1 texture (0 skipped, 1 up to date)"
        );
    }

    #[test]
    fn removes_the_legacy_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        fs::create_dir_all(&output).unwrap();
        for name in manifest::LEGACY_FILE_NAMES {
            fs::write(output.join(name), "old").unwrap();
        }
        fs::write(output.join("old.atre"), "old").unwrap();
        // A texture from before `.atex.bin`.
        fs::write(output.join("a.ktx2"), "old").unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(
            outcome.removed,
            [
                "a.ktx2",
                "old.atre",
                "assets.manifest",
                "assets.aman",
                "assets.aman.bin"
            ]
        );
        for name in manifest::LEGACY_FILE_NAMES {
            assert!(!output.join(name).exists());
        }
        assert!(output.join(manifest::FILE_NAME).is_file());
    }

    #[test]
    fn failure_skips_manifest_but_keeps_good_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("ok.png"), 1, 1);
        fs::write(input.join("broken.png"), "not a png").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "broken.png");
        assert!(outcome.summary().starts_with("Build failed: broken.png:"));
        assert!(output.join("ok.atex.bin").is_file());
        assert!(!output.join(manifest::FILE_NAME).exists());
    }

    #[test]
    fn schema_changes_fail_or_warn_what_attaches_them() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        let schema = |fields: &str| {
            let text = format!(
                r#"{{ "$dialect": "a/1", "types": {{ "Step": {{ "kind": "struct", "fields": [{fields}] }} }} }}"#
            );
            fs::write(input.join("game.aschema"), text).unwrap();
        };
        schema(r#"{ "name": "cost", "type": "i32", "max": 10 }"#);
        let graph = |ty: &str| {
            let text = format!(
                r#"{{ "$dialect": "agraph/1",
                     "node_structs": [{{ "schema": "game.aschema", "type": "{ty}" }}],
                     "nodes": [{{ "id": "a", "x": 0, "y": 0, "values": {{ "Step": {{ "cost": 12 }} }} }}],
                     "edges": [] }}"#
            );
            fs::write(input.join("quest.agraph"), text).unwrap();
        };
        graph("Step");

        // Out of range: built, with a warning.
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].source, "quest.agraph");
        assert_eq!(
            outcome.warnings[0].message,
            "node a: Step/cost: 12 is above the maximum 10"
        );
        assert!(
            outcome.summary().ends_with("skipped, 1 warning)"),
            "{}",
            outcome.summary()
        );

        // The field's type changes: the value no longer encodes, the graph fails.
        schema(r#"{ "name": "cost", "type": "string" }"#);
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "quest.agraph");
        assert!(outcome.warnings.is_empty());

        // The struct is gone.
        graph("Gone");
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(
            outcome.failures[0].message,
            "Gone is not a struct of game.aschema"
        );
    }

    #[test]
    fn custom_materials_fill_pack_and_check_their_textures() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("water")).unwrap();
        fs::write(
            input.join("game.aschema"),
            r#"{ "$dialect": "a/1", "types": { "P": { "kind": "struct", "fields": [
                 { "name": "speed", "type": "f32", "default": 2, "max": 5 },
                 { "name": "sound", "type": "string" } ] } } }"#,
        )
        .unwrap();
        fs::write(
            input.join("water.ashader"),
            "params = { schema = \"game.aschema\", type = \"P\" }\n\n\
             [[textures]]\nname = \"color\"\nusage = \"color\"\nbare = true\n\n\
             [[textures]]\nname = \"mask\"\nusage = \"data\"\npack = { r = \"foam\", b = \"depth.g\" }\n\n\
             [[textures]]\nname = \"detail\"\nusage = \"data\"\nrequired = true\n",
        )
        .unwrap();
        let png = |name: &str, rgba: [u8; 4]| {
            crate::imagefile::tests::png(
                &input.join("water").join(name),
                &Image::filled(4, 4, rgba),
            );
        };
        png("lake.png", [10, 20, 30, 255]);
        png("lake.foam.png", [200, 0, 0, 255]);
        png("lake.depth.png", [0, 77, 0, 255]);
        png("other.png", [1, 2, 3, 255]);
        let material = |text: &str| fs::write(input.join("water/lake.acmat"), text).unwrap();

        // `detail` is required and nothing fills it.
        material("shader = \"../water.ashader\"\n");
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1, "{:?}", outcome.failures);
        assert_eq!(outcome.failures[0].message, "texture detail is required");

        material(
            "shader = \"../water.ashader\"\n\n[params]\nspeed = 9\n\n[textures]\ndetail = \"other.png\"\n",
        );
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(
            outcome.warnings[0].message,
            "params: P/speed: 9 is above the maximum 5"
        );
        let mask =
            crate::atex::read(&fs::read(output.join("water/lake.mask.atex.bin")).unwrap()).unwrap();
        let texel = &mask.level0.unwrap().pixels[..4];
        assert_eq!(
            texel,
            [200, 255, 77, 255],
            "foam's R, fill, depth's G, fill"
        );
        assert!(
            !output.join("water/lake.foam.atex.bin").exists(),
            "inputs make no textures"
        );
        let bytes = fs::read(output.join("water/lake.acmat.bin")).unwrap();
        let reader = crate::chunks::Reader::parse(&bytes, crate::acmat::FORMAT).unwrap();
        assert_eq!(reader.require("PARAMS").unwrap()[..4], 9f32.to_le_bytes());
        let ids: Vec<u64> = reader
            .require("TEXTURES")
            .unwrap()
            .chunks(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let id = |p: &str| crate::hash::fnv1a64(p.as_bytes());
        assert_eq!(
            ids,
            [id("water/lake"), id("water/lake.mask"), id("water/other")]
        );

        // Keys that are not the shader's, and a PBR material of the same name.
        material("shader = \"../water.ashader\"\n\n[textures]\nnormal = \"lake.png\"\n");
        fs::write(input.join("water/lake.amat"), "").unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(
            outcome.failures[0].message,
            "water/lake.amat and water/lake.acmat are both material lake"
        );
        fs::remove_file(input.join("water/lake.amat")).unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(
            outcome.failures[0].message,
            "[textures] normal is not a texture or input of water.ashader"
        );
    }

    #[test]
    fn builds_jpeg_with_opaque_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("brick.n.JPEG"), crate::imagefile::tests::JPEG).unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built[0].output, "brick.n.atex.bin");
        let (_, data) = level0(&output.join("brick.n.atex.bin"));
        let texel = &data[..4];
        // Lossy, so only roughly the source color; alpha is exact.
        assert!(
            texel[0].abs_diff(200) < 8 && texel[2].abs_diff(50) < 8,
            "{texel:?}"
        );
        assert_eq!(texel[3], 255);
    }

    /// A TGA file: uncompressed (type 2) or RLE (type 10) true color, 24-bit BGR rows.
    fn tga(image_type: u8, width: u16, height: u16, top_left: bool, data: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, image_type, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        out.extend([24, if top_left { 0x20 } else { 0 }]);
        out.extend(data);
        out
    }

    #[test]
    fn builds_tga_in_both_origins_and_rle() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        // 1 × 2, bottom-left origin: the bottom row (red) comes first.
        fs::write(
            input.join("bottom.tga"),
            tga(2, 1, 2, false, &[0, 0, 255, 0, 255, 0]),
        )
        .unwrap();
        // Same image, top-left origin: the top row (green) comes first.
        fs::write(
            input.join("top.TGA"),
            tga(2, 1, 2, true, &[0, 255, 0, 0, 0, 255]),
        )
        .unwrap();
        // 2 × 1 RLE: one packet repeating blue twice.
        fs::write(
            input.join("rle.tga"),
            tga(10, 2, 1, true, &[0x81, 255, 0, 0]),
        )
        .unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built.len(), 3);
        for name in ["bottom", "top"] {
            let (image, color) = load_source(&input.join(if name == "top" {
                "top.TGA"
            } else {
                "bottom.tga"
            }))
            .unwrap();
            assert!(color.has_color() && !color.has_alpha());
            assert_eq!(image.pixel(0, 0), [0, 255, 0, 255], "{name}");
            assert_eq!(image.pixel(0, 1), [255, 0, 0, 255], "{name}");
            assert!(output.join(format!("{name}.atex.bin")).is_file());
        }
        let (image, _) = load_source(&input.join("rle.tga")).unwrap();
        assert_eq!(image.pixels[..], [0, 0, 255, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn png_and_jpeg_with_same_asset_path_collide() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        fs::write(input.join("a.jpg"), crate::imagefile::tests::JPEG).unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 2);
        assert!(outcome.failures[0].message.contains("collides"));
    }

    #[test]
    fn builds_luts_as_strips() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("grades")).unwrap();
        let mut cube = String::from("LUT_3D_SIZE 2\n");
        for i in 0..8 {
            cube += &format!("{} {} {}\n", i & 1, (i >> 1) & 1, (i >> 2) & 1);
        }
        fs::write(input.join("grades/warm.lut.cube"), cube).unwrap();
        fs::write(input.join("bad.cube"), "LUT_3D_SIZE 2\n").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "bad.cube");
        let path = output.join("grades/warm.lut.atex.bin");
        let info = crate::atex::read(&fs::read(&path).unwrap()).unwrap();
        assert_eq!((info.width, info.height, info.levels), (4, 2, 1));
        let (format, pixels) = level0(&path);
        assert_eq!(format, "R8G8B8A8_UNORM");
        // Pixel (b * 2 + r, g): (1, 0) is red, (2, 0) blue, (0, 1) green.
        assert_eq!(pixels[4..8], [255, 0, 0, 255]);
        assert_eq!(pixels[8..12], [0, 0, 255, 255]);
        assert_eq!(pixels[16..20], [0, 255, 0, 255]);
    }

    #[test]
    fn builds_volume_luts_when_configured() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        let mut cube = String::from("LUT_3D_SIZE 2\n");
        for i in 0..8 {
            cube += &format!("{} {} {}\n", i & 1, (i >> 1) & 1, (i >> 2) & 1);
        }
        fs::write(input.join("warm.cube"), &cube).unwrap();
        fs::write(input.join("cold.cube"), &cube).unwrap();
        let textures = config::Textures {
            volume_luts: vec!["warm.cube".into()],
            ..Default::default()
        };

        let outcome = run(&input, &output, &with_textures(&textures)).unwrap();
        assert!(outcome.failures.is_empty());
        let size = |name: &str| {
            let info = crate::atex::read(&fs::read(output.join(name)).unwrap()).unwrap();
            (info.width, info.height, info.depth)
        };
        assert_eq!(size("warm.atex.bin"), (2, 2, 2));
        assert_eq!(size("cold.atex.bin"), (4, 2, 1));
        let detail = |name: &str| {
            let built = outcome.built.iter().find(|b| b.source == name).unwrap();
            built.detail.clone()
        };
        assert_eq!(detail("warm.cube"), "2³ LUT volume, linear, 1 level");
        assert_eq!(detail("cold.cube"), "2³ LUT strip, linear, 1 level");
        // Table order: texel 1 is red.
        assert_eq!(
            level0(&output.join("warm.atex.bin")).1[4..8],
            [255, 0, 0, 255]
        );
    }

    #[test]
    fn presets_share_locators() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("chars")).unwrap();
        let fox = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Fox.glb");
        for name in ["hero", "villain"] {
            fs::copy(fox, input.join(format!("chars/{name}.glb"))).unwrap();
        }
        fs::write(
            input.join("chars/hero.glb.amod"),
            "[[locators]]\nname = \"mouth\"\njoint = \"b_Head_05\"\n",
        )
        .unwrap();
        // The villain changes it and adds its own.
        fs::write(
            input.join("chars/villain.glb.amod"),
            "[[locators]]\nname = \"tail\"\n\n[[locators]]\nname = \"mouth\"\njoint = \"b_Neck_04\"\n",
        )
        .unwrap();
        let mut config = config::Config::default();
        config.locators.presets.push(config::LocatorPreset {
            from: "chars/hero.glb".into(),
            patterns: Some(vec!["chars/*.glb".into()]),
        });
        let outcome = run(&input, &output, &config).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let names = |file: &str| -> Vec<String> {
            let mesh = fs::read(output.join(file)).unwrap();
            (0..2)
                .map(|row| crate::amesh::cell(&mesh, "LOCATORS", row, 1))
                .filter(|name| !name.is_empty())
                .collect()
        };
        assert_eq!(names("chars/hero.amesh.bin"), ["mouth"]);
        // Inherited first, replaced in place by the sidecar; then its own.
        assert_eq!(names("chars/villain.amesh.bin"), ["mouth", "tail"]);
        let villain = fs::read(output.join("chars/villain.amesh.bin")).unwrap();
        // `b_Neck_04`, after `_rootJoint` in skin order.
        assert_eq!(crate::amesh::cell(&villain, "LOCATORS", 0, 3), "5");

        config.locators.presets[0].from = "chars/missing.glb".into();
        let outcome = run(&input, &output, &config).unwrap();
        assert_eq!(outcome.failures.len(), 2);
        assert!(
            outcome.failures[0]
                .message
                .starts_with("assetc.toml [[locators.presets]] 1:")
        );
    }

    #[test]
    fn builds_gltf_models() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("models")).unwrap();
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Fox.glb"),
            input.join("models/Fox.glb"),
        )
        .unwrap();
        fs::write(input.join("models/broken.gltf"), "{").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "models/broken.gltf");
        fs::remove_file(input.join("models/broken.gltf")).unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let built: Vec<_> = outcome.built.iter().map(|b| b.output.as_str()).collect();
        assert_eq!(built, ["models/Fox.amesh.bin"]);
        assert!(outcome.built[0].detail.contains("24 joints, 3 clips"));
        // Its animation, material and texture count too; see docs/features/build.md#summary.
        assert_eq!(
            outcome.summary(),
            "Built 1 texture, 1 material, 1 mesh, 1 animation (0 skipped)"
        );

        let mesh = fs::read(output.join("models/Fox.amesh.bin")).unwrap();
        let rows = crate::amesh::rows(&mesh).unwrap();
        assert!(rows.contains(&("Skin", "24 joints".into())));
        assert!(rows.contains(&("Materials", "1".into())));
        let anim = fs::read(output.join("models/Fox.aanim.bin")).unwrap();
        let rows = crate::aanim::rows(&anim).unwrap();
        assert!(rows[0].1.starts_with("Survey ("), "{rows:?}");
        let m =
            material::decode(&fs::read(output.join("models/Fox/fox_material.amat.bin")).unwrap())
                .unwrap();
        assert_eq!(m.textures.base_color, fnv1a64(b"models/Fox/image_0"));
        assert!(output.join("models/Fox/image_0.atex.bin").is_file());

        // Every output is in the manifest with its kind; the mesh lists the material's ID.
        let entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        let kind = |path: &str| entries.iter().find(|e| e.path == path).map(|e| e.kind);
        assert_eq!(kind("models/Fox.amesh.bin"), Some(Kind::Mesh));
        assert_eq!(kind("models/Fox.aanim.bin"), Some(Kind::Animation));
        assert_eq!(
            kind("models/Fox/fox_material.amat.bin"),
            Some(Kind::Material)
        );
        assert_eq!(kind("models/Fox/image_0.atex.bin"), Some(Kind::Texture));
        let reader = crate::chunks::Reader::parse(&mesh, "AMESH").unwrap();
        assert_eq!(
            reader.require("MATERIAL").unwrap(),
            fnv1a64(b"models/Fox/fox_material.amat.bin").to_le_bytes()
        );

        // Meshlet limits come from [meshes], overridden per model.
        let mut config = config::Config::default();
        config.meshes.overrides.push(config::MeshOverride {
            patterns: vec!["models/*.glb".into()],
            patch: crate::amesh::MeshPatch {
                meshlet_max_vertices: Some(32),
                meshlet_max_triangles: Some(32),
                ..Default::default()
            },
        });
        run(&input, &output, &config).unwrap();
        let mesh = fs::read(output.join("models/Fox.amesh.bin")).unwrap();
        let reader = crate::chunks::Reader::parse(&mesh, "AMESH").unwrap();
        let desc = reader.require("DESC").unwrap();
        assert_eq!(desc[24..32], [32, 0, 0, 0, 32, 0, 0, 0]);
        // The model's .amod wins over the project.
        fs::write(
            input.join("models/Fox.glb.amod"),
            "[mesh]\nmeshlet_max_vertices = 16\n",
        )
        .unwrap();
        let outcome = run(&input, &output, &config).unwrap();
        assert!(outcome.failures.is_empty());
        let mesh = fs::read(output.join("models/Fox.amesh.bin")).unwrap();
        let reader = crate::chunks::Reader::parse(&mesh, "AMESH").unwrap();
        assert_eq!(
            reader.require("DESC").unwrap()[24..32],
            [16, 0, 0, 0, 32, 0, 0, 0]
        );
        fs::write(
            input.join("models/Fox.glb.amod"),
            "[mesh]\nmeshlet_max_vertices = 1\n",
        )
        .unwrap();
        let outcome = run(&input, &output, &config).unwrap();
        assert_eq!(
            outcome.failures[0].message,
            "Fox.glb.amod: meshlet_max_vertices 1 is outside 3–256"
        );
        fs::remove_file(input.join("models/Fox.glb.amod")).unwrap();
        config.meshes.defaults.meshlet_max_triangles = 7;
        let error = run(&input, &output, &config).unwrap_err().to_string();
        assert_eq!(
            error,
            "assetc.toml [meshes]: meshlet_max_triangles 7 must be 4–512 and a multiple of 4"
        );
    }

    #[test]
    fn builds_noise_as_linear_texture() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("rock")).unwrap();
        fs::write(input.join("clouds.anoise"), "size = 16\n").unwrap();
        fs::write(
            input.join("rock/rock.h.anoise"),
            "size = 8\ntype = \"noise/voronoi\"\n",
        )
        .unwrap();
        fs::write(input.join("bad.anoise"), "size = 0\n").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "bad.anoise");
        assert!(outcome.failures[0].message.contains("size must be"));
        let (format, pixels) = level0(&output.join("clouds.atex.bin"));
        assert_eq!(format, "R8G8B8A8_UNORM");
        assert_eq!(pixels.len(), 16 * 16 * 4);
        assert_eq!(pixels[..4], [pixels[0], pixels[0], pixels[0], 255]);
        // In a material slot: compiled under its slot.
        assert!(output.join("rock/rock.h.atex.bin").is_file());
        assert!(output.join("rock/rock.amat.bin").is_file());
    }

    #[test]
    fn colliding_asset_paths_fail() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        png(&input.join("a.PNG"), 1, 1);
        if fs::read_dir(&input).unwrap().count() < 2 {
            return; // case-insensitive file system (Windows, macOS)
        }

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 2);
        assert!(outcome.failures[0].message.contains("collides"));
        assert!(outcome.summary().ends_with("(2 errors)"));
        assert!(!output.join(manifest::FILE_NAME).exists());
    }

    #[test]
    fn settings_pick_color_space_and_mipmaps() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("rock_n.png"), 4, 4);
        png(&input.join("ui/button.png"), 4, 4);
        let textures = config::Textures {
            linear: vec!["**/*_n.png".into()],
            no_mipmaps: vec!["ui/**".into()],
            ..Default::default()
        };

        let mut outcome = run(&input, &output, &with_textures(&textures)).unwrap();
        outcome.built.sort_by(|a, b| a.source.cmp(&b.source));
        let built: Vec<_> = outcome
            .built
            .iter()
            .map(|b| (b.source.as_str(), b.detail.as_str()))
            .collect();
        assert_eq!(
            built,
            [
                ("rock_n.png", "rgba8, linear, 3 levels"),
                ("ui/button.png", "rgba8, srgb, 1 level")
            ]
        );
        assert_eq!(level0(&output.join("rock_n.atex.bin")).0, "R8G8B8A8_UNORM");
    }

    /// Every level of a built texture: (width, red of its first texel).
    fn levels_of(path: &Path) -> Vec<(u32, u8)> {
        let info = crate::atex::read(&fs::read(path).unwrap()).unwrap();
        std::iter::once(info.level0.unwrap())
            .chain(info.mips)
            .map(|level| (level.width, level.pixels[0]))
            .collect()
    }

    #[test]
    fn custom_mip_chains() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        gray(&input.join("t/rock.png"), 8, 8, 10);
        gray(&input.join("t/rock.mip1.png"), 4, 4, 200);
        gray(&input.join("shared/far.png"), 1, 1, 50);
        gray(&input.join("ui/icon.png"), 4, 4, 90);
        fs::write(
            input.join("t/rock.png.amipmap"),
            "[files]\n3 = \"../shared/far.png\"\n",
        )
        .unwrap();
        // Its own setting wins over `no_mipmaps`.
        fs::write(input.join("ui/icon.png.amipmap"), "levels = 2\n").unwrap();
        let config = with_textures(&config::Textures {
            linear: vec!["**".into()],
            no_mipmaps: vec!["ui/**".into()],
            ..Default::default()
        });
        let mut cache = Cache::default();

        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let built: Vec<_> = outcome
            .built
            .iter()
            .map(|b| (b.output.as_str(), b.detail.as_str()))
            .collect();
        // The level files are part of the texture, not textures of their own.
        assert_eq!(
            built,
            [
                ("t/rock.atex.bin", "rgba8, linear, 4 levels, 2 custom"),
                ("ui/icon.atex.bin", "rgba8, linear, 2 levels")
            ]
        );
        assert_eq!(outcome.skipped, 2);
        // Level 1 by name, level 2 generated from it, level 3 chosen.
        assert_eq!(
            levels_of(&output.join("t/rock.atex.bin")),
            [(8, 10), (4, 200), (2, 200), (1, 50)]
        );
        assert_eq!(
            levels_of(&output.join("ui/icon.atex.bin")),
            [(4, 90), (2, 90)]
        );
        let again = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(again.summary(), "Built nothing (2 skipped, 2 up to date)");

        // A changed level file, a level file's modifiers and the `.amipmap` rebuild it.
        png(&input.join("t/rock.mip1.png"), 4, 4);
        let changed = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(
            changed.summary(),
            "Built 1 texture (2 skipped, 1 up to date)"
        );
        assert_eq!(levels_of(&output.join("t/rock.atex.bin"))[1], (4, 0));
        fs::write(
            input.join("t/rock.mip1.png.amod"),
            "[[modifiers]]\ntype = \"invert\"\n",
        )
        .unwrap();
        let changed = run_cached(&input, &output, &config, &mut cache).unwrap();
        assert_eq!(
            changed.summary(),
            "Built 1 texture (3 skipped, 1 up to date)"
        );
        assert_eq!(levels_of(&output.join("t/rock.atex.bin"))[1], (4, 255));
        fs::write(input.join("t/rock.png.amipmap"), "levels = 2\n").unwrap();
        let changed = run_cached(&input, &output, &config, &mut cache).unwrap();
        let built: Vec<_> = changed
            .built
            .iter()
            .map(|b| (b.output.as_str(), b.detail.as_str()))
            .collect();
        // No longer a level, the chosen file is a texture again.
        assert_eq!(
            built,
            [
                ("shared/far.atex.bin", "rgba8, linear, 1 level"),
                ("t/rock.atex.bin", "rgba8, linear, 2 levels, 1 custom")
            ]
        );
        assert_eq!(levels_of(&output.join("t/rock.atex.bin")).len(), 2);

        // A level of the wrong size fails its texture; a bad `.amipmap` is reported for it.
        gray(&input.join("t/rock.mip1.png"), 3, 3, 0);
        fs::write(input.join("ui/icon.png.amipmap"), "levels = 0\n").unwrap();
        let outcome = run_cached(&input, &output, &config, &mut cache).unwrap();
        let mut failures: Vec<_> = outcome
            .failures
            .iter()
            .map(|f| (f.source.as_str(), f.message.as_str()))
            .collect();
        failures.sort();
        assert_eq!(
            failures,
            [
                ("t/rock.png", "level 1 (rock.mip1.png) is 3 × 3, not 4 × 4"),
                ("ui/icon.png.amipmap", "levels must be 1 or more"),
            ]
        );

        // Without the `.amipmap`, the level file is an ordinary texture.
        fs::remove_file(input.join("t/rock.png.amipmap")).unwrap();
        fs::remove_file(input.join("ui/icon.png.amipmap")).unwrap();
        let outcome = run(&input, &output, &config).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert!(output.join("t/rock.mip1.atex.bin").is_file());
        assert_eq!(levels_of(&output.join("t/rock.atex.bin")).len(), 4);
        assert_eq!(levels_of(&output.join("ui/icon.atex.bin")).len(), 1);
    }

    #[test]
    fn packed_textures_have_no_custom_mip_chain() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        gray(&input.join("brick.png"), 4, 4, 10);
        gray(&input.join("brick.r.png"), 4, 4, 10);
        fs::write(input.join("brick.png.amipmap"), "levels = 2\n").unwrap();
        fs::write(input.join("brick.r.png.amipmap"), "").unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        let failures: Vec<_> = outcome
            .failures
            .iter()
            .map(|f| (f.source.as_str(), f.message.as_str()))
            .collect();
        assert_eq!(
            failures,
            [(
                "brick.r.png.amipmap",
                "brick.r.png is only packed into another texture, which generates its \
                 mipmaps"
            )]
        );
        // A material's own texture keeps its chain.
        assert_eq!(levels_of(&output.join("brick.atex.bin")).len(), 2);
    }

    #[test]
    fn image_sidecars_modify_what_is_built() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 2, 2);
        gray(&input.join("g.png"), 2, 2, 64);
        fs::write(
            input.join("a.png.amod"),
            "[[modifiers]]\ntype = \"invert\"\n",
        )
        .unwrap();
        fs::write(
            input.join("g.png.amod"),
            "[[modifiers]]\ntype = \"colorize\"\nhue = 0.0\nsaturation = 1.0\n",
        )
        .unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        // Sidecars are not sources of their own.
        assert!(!output.join("a.png.atex.bin").exists());
        // Inverted (x = 0, y = 0, b = 7), alpha kept.
        let (_, pixels) = level0(&output.join("a.atex.bin"));
        assert_eq!(pixels[..4], [255, 255, 248, 255]);
        // Colorize turns the dark gray image dark red.
        let (image, color) = load_source(&input.join("g.png")).unwrap();
        assert!(color.has_color());
        assert_eq!(image.pixel(0, 0), [128, 0, 0, 255]);

        // A broken sidecar fails its image.
        fs::write(
            input.join("a.png.amod"),
            "[[modifiers]]\ntype = \"sharpen\"\n",
        )
        .unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert!(
            outcome.failures[0]
                .message
                .contains("a.png.amod: modifier 1")
        );
    }

    fn gray(path: &Path, w: u32, h: u32, value: u8) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::imagefile::tests::gray_png(path, w, h, value, false);
    }

    /// A built texture's format name and level 0's data as stored.
    fn level0(path: &Path) -> (String, Vec<u8>) {
        let bytes = fs::read(path).unwrap();
        let reader = crate::chunks::Reader::parse(&bytes, crate::atex::FORMAT).unwrap();
        let levels = reader.chunk("LEVELS").unwrap();
        let at = |o| crate::chunks::u32_at(levels, 0, 32, o).unwrap() as usize;
        let data = reader.chunk("DATA").unwrap()[at(0)..at(0) + at(8)].to_vec();
        (
            crate::atex::read(&bytes).unwrap().format_name().to_owned(),
            data,
        )
    }

    #[test]
    fn convention_material_with_orm() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("brick/brick.png"), 4, 4);
        png(&input.join("brick/brick.n.png"), 4, 4);
        gray(&input.join("brick/brick.r.png"), 4, 4, 100);
        gray(&input.join("brick/brick.ao.png"), 4, 4, 50);
        png(&input.join("brick/grass.png"), 2, 2);
        png(&input.join("brick/a.b.png"), 2, 2);
        let textures = config::Textures {
            // Slot color space wins over the globs.
            linear: vec!["**/brick.png".into()],
            no_mipmaps: vec![],
            ..Default::default()
        };

        let outcome = run(&input, &output, &with_textures(&textures)).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let lines: Vec<_> = outcome
            .built
            .iter()
            .map(|b| format!("{} -> {} ({})", b.source, b.output, b.detail))
            .collect();
        assert_eq!(
            lines,
            [
                "brick/a.b.png -> brick/a.b.atex.bin (rgba8, srgb, 2 levels)",
                "brick/brick.n.png -> brick/brick.n.atex.bin (rgba8, linear, 3 levels)",
                "brick/brick.png -> brick/brick.atex.bin (rgba8, srgb, 3 levels)",
                "brick/grass.png -> brick/grass.atex.bin (rgba8, srgb, 2 levels)",
                "brick/brick.ao.png, brick/brick.r.png -> brick/brick.orm.atex.bin (rgba8, linear, 3 levels)",
                "brick/brick.* -> brick/brick.amat.bin (material: base_color, normal, orm)",
            ]
        );
        assert_eq!(
            outcome.summary(),
            "Built 5 textures, 1 material (0 skipped)"
        );
        assert!(!output.join("brick/brick.r.atex.bin").exists());
        assert!(!output.join("brick/brick.ao.atex.bin").exists());

        let (format, orm) = level0(&output.join("brick/brick.orm.atex.bin"));
        assert_eq!(format, "R8G8B8A8_UNORM");
        assert_eq!(orm[..4], [50, 100, 255, 255]);
        assert_eq!(
            level0(&output.join("brick/brick.atex.bin")).0,
            "R8G8B8A8_SRGB"
        );

        let m = material::decode(&fs::read(output.join("brick/brick.amat.bin")).unwrap()).unwrap();
        assert_eq!(m.textures.base_color, fnv1a64(b"brick/brick"));
        assert_eq!(m.textures.normal, fnv1a64(b"brick/brick.n"));
        assert_eq!(m.textures.orm, fnv1a64(b"brick/brick.orm"));
        assert_eq!((m.textures.emissive, m.textures.height), (0, 0));
        assert_eq!(m.metallic, 0.0);

        let entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        let material = entries.iter().find(|e| e.kind == Kind::Material).unwrap();
        assert_eq!(material.id, fnv1a64(b"brick/brick.amat.bin"));
        assert_eq!(material.path, "brick/brick.amat.bin");
        assert_eq!(entries.len(), 6);
    }

    #[test]
    fn scalar_only_material() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("gold.amat"), "metallic = 1.0\nroughness = 0.3\n").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built[0].source, "gold.amat");
        assert_eq!(outcome.built[0].detail, "material: no textures");
        assert_eq!(outcome.summary(), "Built 1 material (0 skipped)");
        let m = material::decode(&fs::read(output.join("gold.amat.bin")).unwrap()).unwrap();
        assert_eq!((m.metallic, m.roughness), (1.0, 0.3));
        assert_eq!(m.textures, material::TextureIds::default());
    }

    #[test]
    fn toml_overrides_slots_and_flips_dx_normals() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        crate::imagefile::tests::png(
            &input.join("wall/wall.n.png"),
            &Image::filled(1, 1, [128, 40, 255, 255]),
        );
        gray(&input.join("wall/wall.ao.png"), 1, 1, 10);
        gray(&input.join("shared/ao.png"), 1, 1, 90);
        fs::write(
            input.join("wall/wall.amat"),
            "normal_y = \"dx\"\n[textures]\nocclusion = \"../shared/ao.png\"\n",
        )
        .unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(
            level0(&output.join("wall/wall.n.atex.bin")).1,
            [128, 215, 255, 255]
        );
        assert_eq!(level0(&output.join("wall/wall.orm.atex.bin")).1[0], 90);
        // The replaced convention file is a plain texture again.
        assert!(output.join("wall/wall.ao.atex.bin").is_file());
        assert!(!output.join("shared/ao.atex.bin").exists());
    }

    #[test]
    fn material_failures() {
        let cases: [(&[(&str, &str)], &str); 5] = [
            (&[("m.amat", "roughness = 2.0")], "out of range"),
            (
                &[("m.mat.toml", "roughness = 0.5")],
                "rename m.mat.toml to m.amat",
            ),
            (
                &[("m.amat", "[textures]\nnormal = \"../../x.png\"")],
                "not a texture source",
            ),
            (
                &[("m.amat", "[textures]\nnormal = \"missing.png\"")],
                "not a texture source",
            ),
            (
                &[
                    ("m.amat", "[textures]\nnormal = \"shared.png\""),
                    ("k.amat", "[textures]\nbase_color = \"shared.png\""),
                ],
                "differently by another material",
            ),
        ];
        for (files, expected) in cases {
            let dir = tempfile::tempdir().unwrap();
            let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
            png(&input.join("shared.png"), 1, 1);
            for (name, text) in files {
                fs::write(input.join(name), text).unwrap();
            }
            let outcome = run(&input, &output, &rgba()).unwrap();
            assert!(
                outcome
                    .failures
                    .iter()
                    .any(|f| f.message.contains(expected)),
                "{expected}: {:?}",
                outcome.failures
            );
            assert!(!output.join(manifest::FILE_NAME).exists());
        }
    }

    #[test]
    fn orm_inputs_must_match_in_size() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        gray(&input.join("m.r.png"), 4, 4, 1);
        gray(&input.join("m.ao.png"), 2, 2, 1);
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.failures[0].message.contains("differ in size"));
        assert_eq!(outcome.failures[0].source, "m.ao.png, m.r.png");
    }

    #[test]
    fn orm_name_collides_with_source() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        gray(&input.join("m.r.png"), 1, 1, 1);
        png(&input.join("m.orm.png"), 1, 1);
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(
            outcome
                .failures
                .iter()
                .any(|f| f.message.contains("collides"))
        );
    }

    #[test]
    fn stale_materials_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("a.amat"), "").unwrap();
        run(&input, &output, &rgba()).unwrap();
        fs::remove_file(input.join("a.amat")).unwrap();
        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.removed, ["a.amat.bin"]);
    }

    #[test]
    fn invalid_pattern_stops_build() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        let textures = config::Textures {
            linear: vec!["[".into()],
            no_mipmaps: vec![],
            ..Default::default()
        };
        assert!(matches!(
            run(&input, &output, &with_textures(&textures)),
            Err(Error::Pattern(_))
        ));
        assert!(!output.exists());
    }

    #[test]
    fn removes_stale_outputs_only() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        let textures = config::Textures::default();
        png(&input.join("keep.png"), 1, 1);
        png(&input.join("old/deep/gone.png"), 1, 1);
        png(&input.join("shared/gone.png"), 1, 1);
        png(&input.join("shared/stays.png"), 1, 1);
        run(&input, &output, &with_textures(&textures)).unwrap();

        fs::remove_dir_all(input.join("old")).unwrap();
        fs::remove_file(input.join("shared/gone.png")).unwrap();
        fs::write(output.join("notes.txt"), "mine").unwrap();
        fs::write(output.join(".hidden.atex.bin"), "mine").unwrap();
        fs::create_dir_all(output.join("empty")).unwrap();

        let outcome = run(&input, &output, &with_textures(&textures)).unwrap();
        let mut removed = outcome.removed.clone();
        removed.sort();
        assert_eq!(removed, ["old/deep/gone.atex.bin", "shared/gone.atex.bin"]);
        assert_eq!(outcome.summary(), "Built 2 textures (0 skipped, 2 removed)");

        assert!(!output.join("old").exists());
        assert!(output.join("shared/stays.atex.bin").is_file());
        assert!(output.join("keep.atex.bin").is_file());
        assert!(output.join("notes.txt").is_file());
        assert!(output.join(".hidden.atex.bin").is_file());
        assert!(output.join("empty").is_dir());

        let again = run(&input, &output, &with_textures(&textures)).unwrap();
        assert!(again.removed.is_empty());
        assert_eq!(again.summary(), "Built 2 textures (0 skipped)");
    }

    #[test]
    fn stale_check_ignores_case() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("Grass.png"), 1, 1);
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("grass.ATEX.BIN"), "older build").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert!(outcome.removed.is_empty());
        assert!(output.join("grass.ATEX.BIN").is_file());
    }

    #[test]
    fn failed_build_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("broken.png"), "not a png").unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("stale.atex.bin"), "x").unwrap();

        let outcome = run(&input, &output, &rgba()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.removed.is_empty());
        assert!(output.join("stale.atex.bin").is_file());
    }

    #[test]
    fn output_containing_input_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("assets");
        png(&input.join("a.png"), 1, 1);
        let textures = config::Textures::default();
        for output in [dir.path().to_path_buf(), input.clone(), input.join("..")] {
            assert!(
                matches!(
                    run(&input, &output, &with_textures(&textures)),
                    Err(Error::OutputContainsInput(_))
                ),
                "{}",
                output.display()
            );
        }
        assert!(!dir.path().join("a.atex.bin").exists());
        assert!(!input.join("a.atex.bin").exists());
    }

    #[test]
    fn missing_input_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run(&dir.path().join("nope"), &dir.path().join("build"), &rgba());
        assert!(matches!(result, Err(Error::MissingInput(_))));
    }
}
