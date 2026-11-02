//! What the Inspector shows for a file, see docs/features/inspector.md.

use std::fs;
use std::path::Path;

use crate::build::{is_noise, is_texture_source, load_source as decode, relative, strip_extension};
use crate::config::Project;
use crate::gradient::{CellGrid, Gradients, is_gradient};
use crate::hash::fnv1a64;
use crate::lut::Lut;
use crate::material::{self, AlphaMode, HeightMode};
use crate::noise::Noise;
use crate::palette::{Palette, is_palette};
use crate::plan::{self, Job, join, split_dir};
use crate::texture::{self, ColorSpace, Image};
use crate::{aanim, amesh, font, ktx2, manifest, model};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Input,
    Output,
}

#[derive(Debug, Default)]
pub struct Inspection {
    pub name: String,
    /// Project-relative path, or the full path outside the project.
    pub path: String,
    pub missing: bool,
    pub rows: Vec<(&'static str, String)>,
    pub preview: Option<Image>,
    /// Which of R, G, B, A the previewed file actually has.
    pub channels: [bool; 4],
    /// Shown instead of a preview, e.g. for unsupported formats.
    pub note: Option<String>,
    /// Environment map faces in cube layer order (+X, −X, +Y, −Y, +Z, −Z).
    pub cubemap: Option<Vec<Image>>,
    /// A parsed color LUT.
    pub lut: Option<Lut>,
    /// `preview` is a rendered font specimen, not an image file: no tiling, channels or 3D.
    pub specimen: bool,
    /// A glTF model that parsed, previewed in 3D by the canvas.
    pub model: bool,
    /// A compiled texture's mip levels after level 0 (which is `preview`).
    pub mips: Vec<Image>,
    /// A compiled mesh's triangles by meshlet, for the canvas.
    pub compiled_mesh: Option<crate::amesh::Preview>,
    /// The compiled mesh file itself, for the Chunks view.
    pub compiled_bytes: Option<Vec<u8>>,
    /// Its material slots, as found in the output (`None` when missing or unreadable).
    pub compiled_materials: Vec<Option<MaterialPreview>>,
    /// `compiled_bytes` is an `.afont`, and `preview` its atlas (if found).
    pub compiled_font: bool,
    /// The file's data as a table, for files without a preview (manifest, `.amat`).
    pub data: Option<DataTable>,
    /// A model's animations: name and duration in seconds.
    pub animations: Vec<(String, f32)>,
    /// A model's first skin's named joints, for its locators.
    pub joints: Vec<String>,
    /// The other glTF models of the input, input-relative and sorted, for locator previews.
    pub models: Vec<String>,
}

/// A file's data as a table, for the canvas; see docs/features/inspector.md#data-tables.
#[derive(Debug, Clone, Default)]
pub struct DataTable {
    pub header: String,
    pub columns: Vec<&'static str>,
    pub rows: Vec<DataRow>,
    /// Rows can be filtered by text (the manifest view).
    pub filterable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DataRow {
    pub cells: Vec<String>,
    /// Drawn in `TEXT_WEAK`.
    pub weak: bool,
    /// A color swatch (sRGB) before the text of the cell in this column.
    pub swatch: Option<(usize, [u8; 3])>,
    /// What the filter matches, lowercase.
    pub filter: String,
}

/// Face file stems of an environment map folder, in cube layer order.
pub const CUBE_FACES: [&str; 6] = ["px", "nx", "py", "ny", "pz", "nz"];

/// Whether `dir` is an environment map folder (`*.env`, any case).
pub fn is_environment_map(dir: &Path) -> bool {
    dir.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("env"))
}

#[derive(Clone, Copy)]
enum Kind {
    SourceTexture,
    Noise,
    Gradient,
    Palette,
    MaterialSource,
    ColorLut,
    Font,
    Model,
    CompiledTexture,
    CompiledMaterial,
    CompiledMesh,
    CompiledAnimation,
    CompiledFont,
    Manifest,
    Ignored,
    File,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::SourceTexture => "Source texture",
            Self::Noise => "Noise",
            Self::Gradient => "Gradient",
            Self::Palette => "Palette",
            Self::MaterialSource => "Material source",
            Self::ColorLut => "Color LUT",
            Self::Font => "Font",
            Self::Model => "Model",
            Self::CompiledTexture => "Compiled texture",
            Self::CompiledMaterial => "Compiled material",
            Self::CompiledMesh => "Compiled mesh",
            Self::CompiledAnimation => "Compiled animation",
            Self::CompiledFont => "Compiled font",
            Self::Manifest => "Manifest",
            Self::Ignored => "Ignored",
            Self::File => "File",
        }
    }
}

pub fn inspect(project: &Project, location: Location, file: &Path) -> Inspection {
    let mut out = Inspection {
        name: file
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        path: project
            .root
            .as_deref()
            .and_then(|root| file.strip_prefix(root).ok())
            .unwrap_or(file)
            .display()
            .to_string(),
        ..Default::default()
    };
    let meta = match fs::metadata(file) {
        Ok(meta) if meta.is_file() => meta,
        Ok(meta) if meta.is_dir() => {
            folder(location, file, &mut out);
            return out;
        }
        _ => {
            out.missing = true;
            return out;
        }
    };

    let has_ext = |ext: &str| {
        file.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };
    let dir = match location {
        Location::Input => project.input_dir(),
        Location::Output => project.output_dir(),
    };
    let rel = dir
        .as_deref()
        .and_then(|dir| relative(dir, file).ok())
        .unwrap_or_else(|| out.name.clone());
    let material_name = split_dir(&rel)
        .1
        .strip_suffix(material::SOURCE_SUFFIX)
        .filter(|name| !name.is_empty());
    let kind = match location {
        Location::Input if material_name.is_some() => Kind::MaterialSource,
        Location::Input if has_ext("cube") => Kind::ColorLut,
        Location::Input if is_noise(file) => Kind::Noise,
        Location::Input if is_gradient(file) => Kind::Gradient,
        Location::Input if is_palette(file) => Kind::Palette,
        Location::Input if font::is_font(file) => Kind::Font,
        Location::Input if model::is_model(file) => Kind::Model,
        Location::Input if is_texture_source(file) => Kind::SourceTexture,
        Location::Input => Kind::Ignored,
        Location::Output if has_ext(manifest::Kind::Texture.extension()) => Kind::CompiledTexture,
        Location::Output if has_ext(manifest::Kind::Material.extension()) => Kind::CompiledMaterial,
        Location::Output if has_ext(manifest::Kind::Mesh.extension()) => Kind::CompiledMesh,
        Location::Output if has_ext(manifest::Kind::Animation.extension()) => {
            Kind::CompiledAnimation
        }
        Location::Output if has_ext(manifest::Kind::Font.extension()) => Kind::CompiledFont,
        Location::Output if rel == manifest::FILE_NAME => Kind::Manifest,
        Location::Output => Kind::File,
    };
    out.rows.push(("Kind", kind.name().into()));
    out.rows.push(("Size", format_size(meta.len())));

    let result = match kind {
        Kind::SourceTexture => source_texture(project, file, &rel, &mut out),
        Kind::Noise => {
            noise_rows(file, &mut out).and_then(|()| source_texture(project, file, &rel, &mut out))
        }
        Kind::Gradient => gradient_rows(file, &mut out)
            .and_then(|()| source_texture(project, file, &rel, &mut out)),
        Kind::Palette => palette_rows(file, &mut out)
            .and_then(|()| source_texture(project, file, &rel, &mut out)),
        Kind::MaterialSource => material_source(project, file, &rel, &mut out),
        Kind::ColorLut => color_lut(project, file, &rel, &mut out),
        Kind::Font => font_rows(project, file, &rel, &mut out),
        Kind::Model => model_rows(project, file, &rel, &mut out),
        Kind::CompiledTexture => compiled_texture(file, &rel, &mut out),
        Kind::CompiledMaterial => compiled_material(project, file, &rel, &mut out),
        Kind::CompiledMesh => fs::read(file)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Ok((amesh::rows(&bytes)?, amesh::preview(&bytes)?, bytes)))
            .map(|(rows, preview, bytes)| {
                out.rows.extend(rows);
                asset_rows(&rel, &mut out);
                out.compiled_materials = compiled_materials(project, &preview.materials);
                out.compiled_mesh = Some(preview);
                out.compiled_bytes = Some(bytes);
            }),
        Kind::CompiledAnimation => fs::read(file)
            .map_err(|e| e.to_string())
            .and_then(|bytes| aanim::rows(&bytes))
            .map(|rows| {
                out.rows.extend(rows);
                asset_rows(&rel, &mut out);
            }),
        Kind::CompiledFont => compiled_font(project, file, &rel, &mut out),
        Kind::Manifest => manifest(file, &mut out),
        Kind::Ignored | Kind::File => Ok(()),
    };
    if let Err(message) = result {
        out.preview = None;
        // A LUT has no image to fall back on, so the canvas shows the error itself.
        out.note = matches!(
            kind,
            Kind::ColorLut | Kind::CompiledMesh | Kind::CompiledMaterial | Kind::Manifest
        )
        .then(|| message.clone());
        out.rows.push(("Error", message));
    }
    out
}

fn source_texture(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let (image, color) = decode(file)?;
    let rgb = color.has_color();
    out.channels = [rgb, rgb, rgb, color.has_alpha()];
    let (width, height) = image.dimensions();
    let asset_path = strip_extension(rel);
    out.rows
        .push(("Dimensions", format!("{width} × {height} px")));
    if !is_noise(file)
        && !is_gradient(file)
        && !is_palette(file)
        && let Ok(Some(amod)) = crate::amod::read(file)
        && !amod.modifiers.is_empty()
    {
        out.rows
            .push(("Modifiers", crate::noise::modifier_summary(&amod.modifiers)));
    }
    asset_rows(asset_path, out);

    let color_name = |color| match color {
        ColorSpace::Srgb => "sRGB",
        ColorSpace::Linear => "linear",
    };
    let mut output = None;
    match project
        .input_dir()
        .map(|dir| plan::plan(&dir, &project.config))
    {
        Some(Ok(plan)) => {
            let uses = plan.uses.get(rel).map(Vec::as_slice).unwrap_or_default();
            if !uses.is_empty() {
                let materials: Vec<_> = uses
                    .iter()
                    .map(|u| format!("{} ({})", u.material, u.slot.name()))
                    .collect();
                out.rows.push(("Material", materials.join(", ")));
            }
            let orm_use = uses.iter().find_map(|u| {
                let channel = u.slot.orm_channel()?;
                let orm_path = format!("{}.orm", u.material.strip_suffix(".amat")?);
                Some((plan.by_asset_path(&orm_path)?, channel))
            });
            if let Some(asset) = plan.texture(rel)
                && let Job::Texture { color, mipmaps, .. } = asset.job
            {
                let levels = if mipmaps {
                    texture::level_count(width, height)
                } else {
                    1
                };
                out.rows.push(("Color", color_name(color).into()));
                out.rows.push(("Mip levels", levels.to_string()));
                output = Some((asset.output.clone(), ""));
            } else if let Some((orm, channel)) = orm_use {
                out.rows
                    .push(("Color", color_name(ColorSpace::Linear).into()));
                let levels = texture::level_count(width, height);
                out.rows.push(("Mip levels", levels.to_string()));
                let channel = [" (channel R)", " (channel G)", " (channel B)"][channel];
                output = Some((orm.output.clone(), channel));
            }
        }
        Some(Err(e)) => {
            out.rows.push(("Color", e.to_string()));
            out.rows.push(("Mip levels", e.to_string()));
        }
        None => {}
    }

    let (output_rel, note) =
        output.unwrap_or_else(|| (manifest::Kind::Texture.output_path(asset_path), ""));
    output_row(project, &output_rel, note, out);

    out.preview = Some(to_image(image));
    Ok(())
}

fn compiled_texture(file: &Path, rel: &str, out: &mut Inspection) -> Result<(), String> {
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let info = ktx2::read(&bytes)?;
    out.rows.push((
        "Dimensions",
        if info.depth > 0 {
            format!("{} × {} × {} px", info.width, info.height, info.depth)
        } else {
            format!("{} × {} px", info.width, info.height)
        },
    ));
    out.rows.push(("Format", info.format_name()));
    out.rows.push(("Mip levels", info.levels.to_string()));
    asset_rows(strip_extension(rel), out);

    match info.level0 {
        Some(level) => {
            let image = image::RgbaImage::from_raw(level.width, level.height, level.pixels)
                .ok_or("level 0 has the wrong size")?;
            out.preview = Some(to_image(image));
            out.mips = info.mips;
            out.channels = [true; 4];
        }
        None => out.note = Some("No preview for this format".into()),
    }
    Ok(())
}

fn noise_rows(file: &Path, out: &mut Inspection) -> Result<(), String> {
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let n = Noise::parse(&text)?;
    out.rows.push(("Type", n.kind.name().to_owned()));
    let specs = n.kind.specs();
    for (i, spec) in specs.iter().enumerate() {
        out.rows.push((spec.label, n.params.display(specs, i)));
    }
    out.rows
        .push(("Modifiers", crate::noise::modifier_summary(&n.modifiers)));
    Ok(())
}

fn gradient_rows(file: &Path, out: &mut Inspection) -> Result<(), String> {
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let gradients = Gradients::parse(&text)?;
    out.rows
        .push(("Gradients", gradients.gradients.len().to_string()));
    grid_rows(&gradients, out);
    Ok(())
}

fn palette_rows(file: &Path, out: &mut Inspection) -> Result<(), String> {
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let palette = Palette::parse(&text)?;
    out.rows.push(("Colors", palette.colors.len().to_string()));
    grid_rows(&palette, out);
    Ok(())
}

/// Cell, Grid and Layout of a `.gradient` or `.palette` file.
fn grid_rows(grid: &impl CellGrid, out: &mut Inspection) {
    let [_, _, cell_width, cell_height] = grid.grid();
    out.rows
        .push(("Cell", format!("{cell_width} × {cell_height} px")));
    out.rows
        .push(("Grid", format!("{} × {}", grid.columns(), grid.rows())));
    out.rows.push(("Layout", grid.layout().key().into()));
}

fn font_rows(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let data = fs::read(file).map_err(|e| e.to_string())?;
    out.rows.extend(font::rows(&data)?);
    // Compiled to an `.afont` and its atlas, see docs/features/fonts.md#build.
    let afont = format!("{}.afont", strip_extension(rel));
    out.rows.push(("Asset path", afont.clone()));
    output_row(project, &afont, "", out);
    out.preview = Some(font::specimen(&data)?);
    out.channels = [true; 4];
    out.specimen = true;
    Ok(())
}

/// See docs/features/fonts.md#compiled-fonts.
fn compiled_font(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let (mut rows, atlas) = font::afont_rows(&bytes)?;
    let entry = project.output_dir().and_then(|output| {
        let entries = manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).ok()?).ok()?;
        let entry = entries.into_iter().find(|e| e.id == atlas)?;
        let level = ktx2::read(&fs::read(output.join(&entry.path)).ok()?)
            .ok()?
            .level0?;
        Some((entry.path, level))
    });
    let at = rows
        .iter()
        .position(|(k, _)| *k == "Distance range")
        .map_or(rows.len(), |i| i + 1);
    match entry {
        Some((path, level)) => {
            let row = format!("{} × {} px ({path})", level.width, level.height);
            rows.insert(at, ("Atlas", row));
            out.preview = Some(to_image(
                image::RgbaImage::from_raw(level.width, level.height, level.pixels)
                    .ok_or("atlas level 0 has the wrong size")?,
            ));
            out.channels = [true; 4];
            out.specimen = true;
        }
        None => {
            rows.insert(at, ("Atlas", "not found".into()));
            out.note = Some("Atlas not found".into());
        }
    }
    out.rows.extend(rows);
    asset_rows(rel, out);
    out.compiled_font = true;
    out.compiled_bytes = Some(bytes);
    Ok(())
}

/// Every glTF model under `input`, input-relative with `/`, sorted.
fn input_models(input: &Path) -> Vec<String> {
    fn walk(dir: &Path, input: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, input, out);
            } else if model::is_model(&path)
                && let Ok(rel) = crate::build::relative(input, &path)
            {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(input, input, &mut out);
    out.sort();
    out
}

fn model_rows(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let data = fs::read(file).map_err(|e| e.to_string())?;
    out.rows.extend(model::rows(&data)?);
    out.animations = model::animations(&data)?;
    out.joints = model::joint_names(&data)?;
    out.model = true;
    if let Some(input) = project.input_dir() {
        out.models = input_models(&input);
        out.models.retain(|m| m != rel);
    }
    // Compiled to an `.amesh` and more, see docs/features/models.md#build.
    match mesh_options(project, file, rel) {
        Ok(options) => {
            out.rows.push((
                "Meshlet limits",
                format!(
                    "{} vertices, {} triangles",
                    options.meshlet_max_vertices, options.meshlet_max_triangles
                ),
            ));
            let ratios: Vec<String> = options
                .lod_ratios
                .iter()
                .map(|r| format!("{}%", (r * 100.0).round()))
                .collect();
            out.rows.push((
                "Levels of detail",
                if ratios.is_empty() {
                    "none".into()
                } else {
                    ratios.join(", ")
                },
            ));
        }
        Err(e) => out.rows.push(("Mesh settings", e)),
    }
    // See docs/features/locators.md#inspector-info; a bad sidecar shows as Mesh settings.
    if let Ok(amod) = crate::amod::read(file) {
        let count = amod.map_or(0, |a| a.locators.len());
        out.rows.push((
            "Locators",
            if count > 0 {
                count.to_string()
            } else {
                "none".into()
            },
        ));
    }
    let mesh = format!("{}.amesh", strip_extension(rel));
    out.rows.push(("Asset path", mesh.clone()));
    output_row(project, &mesh, "", out);
    Ok(())
}

/// A compiled material with its textures, for the compiled mesh preview.
#[derive(Debug, Clone)]
pub struct MaterialPreview {
    pub material: material::Material,
    pub base_color: Option<Image>,
    pub orm: Option<Image>,
    pub normal: Option<Image>,
    pub emissive: Option<Image>,
}

/// The `.amat` (and its textures) of each asset ID, through the output's manifest; see
/// docs/features/models.md#compiled-mesh-preview.
fn compiled_materials(project: &Project, ids: &[u64]) -> Vec<Option<MaterialPreview>> {
    let Some(output) = project.output_dir() else {
        return vec![None; ids.len()];
    };
    let entries = fs::read(output.join(manifest::FILE_NAME))
        .ok()
        .and_then(|bytes| manifest::decode(&bytes).ok())
        .unwrap_or_default();
    let file = |id: u64| {
        let entry = entries.iter().find(|e| e.id == id && id != 0)?;
        fs::read(output.join(&entry.path)).ok()
    };
    let texture = |id: u64| {
        let level = ktx2::read(&file(id)?).ok()?.level0?;
        Some(level)
    };
    ids.iter()
        .map(|&id| {
            let material = material::decode(&file(id)?).ok()?;
            let t = material.textures;
            Some(MaterialPreview {
                base_color: texture(t.base_color),
                orm: texture(t.orm),
                normal: texture(t.normal),
                emissive: texture(t.emissive),
                material,
            })
        })
        .collect()
}

/// The meshoptimizer settings a model builds with: `assetc.toml` `[meshes]` and its `.amod`.
pub fn mesh_options(
    project: &Project,
    file: &Path,
    rel: &str,
) -> Result<crate::amesh::MeshOptions, String> {
    let settings = crate::amesh::MeshSettings::new(&project.config.meshes)
        .map_err(|e| format!("assetc.toml [meshes]: {e}"))?;
    let amod = crate::amod::read(file)?;
    settings.options_with(rel, amod.as_ref()).map_err(|e| {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        format!("{name}.amod: {e}")
    })
}

fn color_lut(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let lut = Lut::parse(&text)?;
    let n = lut.size;
    let list = |v: [f32; 3]| v.map(|c| c.to_string()).join(", ");
    out.rows
        .push(("Title", lut.title.clone().unwrap_or_else(|| "—".into())));
    out.rows.push(("LUT size", format!("{n} × {n} × {n}")));
    out.rows.push((
        "Domain",
        format!("{} – {}", list(lut.domain_min), list(lut.domain_max)),
    ));
    // Compiled to a strip texture, see docs/features/lut.md#build.
    let asset_path = strip_extension(rel);
    asset_rows(asset_path, out);
    output_row(
        project,
        &manifest::Kind::Texture.output_path(asset_path),
        "",
        out,
    );
    out.lut = Some(lut);
    Ok(())
}

fn folder(location: Location, dir: &Path, out: &mut Inspection) {
    if !is_environment_map(dir) {
        out.rows.push(("Kind", "Folder".into()));
        let items = fs::read_dir(dir).map(|entries| {
            entries
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .count()
        });
        match items {
            Ok(n) => out.rows.push(("Items", n.to_string())),
            Err(e) => out.rows.push(("Error", e.to_string())),
        }
        return;
    }
    out.rows.push(("Kind", "Environment map".into()));
    if let Err(message) = environment_map(location, dir, out) {
        out.rows.push(("Error", message.clone()));
        out.note = Some(message);
    }
}

/// Finds and decodes the six faces; see docs/features/canvas.md#environment-maps.
fn environment_map(location: Location, dir: &Path, out: &mut Inspection) -> Result<(), String> {
    let face_file = |stem: &str| {
        let entries = fs::read_dir(dir).ok()?;
        entries.flatten().map(|e| e.path()).find(|path| {
            path.is_file()
                && path.file_stem().is_some_and(|s| s == stem)
                && match location {
                    Location::Input => is_texture_source(path),
                    Location::Output => path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("ktx2")),
                }
        })
    };
    let files = CUBE_FACES.map(face_file);
    let missing: Vec<_> = CUBE_FACES
        .iter()
        .zip(&files)
        .filter(|(_, f)| f.is_none())
        .map(|(stem, _)| *stem)
        .collect();
    let found = 6 - missing.len();
    out.rows.push((
        "Faces",
        if missing.is_empty() {
            "6 of 6".into()
        } else {
            format!("{found} of 6 (missing {})", missing.join(", "))
        },
    ));
    if !missing.is_empty() {
        return Err(format!("Missing faces: {}", missing.join(", ")));
    }

    let mut faces = Vec::with_capacity(6);
    for (stem, file) in CUBE_FACES.iter().zip(files.into_iter().flatten()) {
        let face = match location {
            Location::Input => decode(&file).map(|(image, _)| to_image(image)),
            Location::Output => fs::read(&file)
                .map_err(|e| e.to_string())
                .and_then(|bytes| ktx2::read(&bytes))
                .and_then(|info| info.level0.ok_or_else(|| "unsupported KTX2 format".into())),
        }
        .map_err(|e| format!("Face {stem}: {e}"))?;
        faces.push(face);
    }
    let (w, h) = (faces[0].width, faces[0].height);
    out.rows.push(("Face size", format!("{w} × {h} px")));
    if faces.iter().any(|f| (f.width, f.height) != (w, h)) {
        return Err("Faces differ in size".into());
    }
    if w != h || !w.is_power_of_two() {
        return Err(format!(
            "Faces must be square with a power-of-two size, not {w} × {h}"
        ));
    }
    out.cubemap = Some(faces);
    Ok(())
}

/// `Output` row: the output-relative path, `note`, and `(not built)` when missing.
fn output_row(project: &Project, output_rel: &str, note: &str, out: &mut Inspection) {
    let built = project
        .output_dir()
        .is_some_and(|dir| dir.join(output_rel).is_file());
    let missing = if built { "" } else { " (not built)" };
    out.rows
        .push(("Output", format!("{output_rel}{note}{missing}")));
}

fn material_source(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let (dir, name) = split_dir(rel);
    let name = name.strip_suffix(material::SOURCE_SUFFIX).unwrap_or(name);
    let asset_path = join(dir, &format!("{name}.amat"));
    asset_rows(&asset_path, out);
    output_row(
        project,
        &manifest::Kind::Material.output_path(&asset_path),
        "",
        out,
    );
    let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
    material::Source::parse(&text)?;
    Ok(())
}

fn compiled_material(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let m = material::decode(&bytes)?;
    let entries = project
        .output_dir()
        .and_then(|output| fs::read(output.join(manifest::FILE_NAME)).ok())
        .and_then(|bytes| manifest::decode(&bytes).ok());
    out.data = Some(material_table(&bytes, entries.as_deref()));
    asset_rows(manifest::Kind::Material.asset_path(rel), out);
    let list = |values: &[f32]| {
        values
            .iter()
            .map(|v| format!("{v:.2}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let height_mode = match m.height_mode {
        HeightMode::Parallax => "parallax",
        HeightMode::Displacement => "displacement",
    };
    let alpha = match m.alpha_mode {
        AlphaMode::Opaque => "opaque".to_owned(),
        AlphaMode::Mask => format!("mask, cutoff {:.2}", m.alpha_cutoff),
        AlphaMode::Blend => "blend".to_owned(),
    };
    let rows = [
        ("Base color", list(&m.base_color)),
        ("Metallic", format!("{:.2}", m.metallic)),
        ("Roughness", format!("{:.2}", m.roughness)),
        ("Occlusion", format!("{:.2}", m.occlusion_strength)),
        ("Normal scale", format!("{:.2}", m.normal_scale)),
        (
            "Emissive",
            format!("{} × {:.2}", list(&m.emissive), m.emissive_strength),
        ),
        ("Height mode", height_mode.into()),
        ("Height scale", format!("{:.2}", m.height_scale)),
        ("Height midlevel", format!("{:.2}", m.height_midlevel)),
        ("Alpha", alpha),
        (
            "Double-sided",
            if m.double_sided { "yes" } else { "no" }.into(),
        ),
    ];
    out.rows.extend(rows);
    let t = m.textures;
    for (label, id) in [
        ("Base color map", t.base_color),
        ("Normal map", t.normal),
        ("ORM map", t.orm),
        ("Emissive map", t.emissive),
        ("Height map", t.height),
    ] {
        let value = if id == 0 {
            "—".to_owned()
        } else {
            format!("{id:#018x}")
        };
        out.rows.push((label, value));
    }
    Ok(())
}

fn manifest(file: &Path, out: &mut Inspection) -> Result<(), String> {
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let entries = manifest::decode(&bytes).map_err(|e| e.to_string())?;
    out.rows.push(("Version", manifest::VERSION.to_string()));
    out.rows.push(("Entries", entries.len().to_string()));
    let output = file.parent().unwrap_or(file);
    out.data = Some(DataTable {
        header: format!(
            "{}   v{}   {} entries   {}",
            String::from_utf8_lossy(&manifest::MAGIC),
            manifest::VERSION,
            entries.len(),
            format_size(bytes.len() as u64)
        ),
        columns: vec!["#", "Asset ID", "Kind", "Asset path", "File"],
        rows: entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let asset_path = e.kind.asset_path(&e.path).to_owned();
                let missing = !output.join(&e.path).is_file();
                DataRow {
                    filter: format!("{asset_path}\n{}", e.path).to_lowercase(),
                    cells: vec![
                        i.to_string(),
                        format!("{:#018x}", e.id),
                        e.kind.name().into(),
                        asset_path,
                        if missing {
                            format!("{} (missing)", e.path)
                        } else {
                            e.path.clone()
                        },
                    ],
                    weak: missing,
                    swatch: None,
                }
            })
            .collect(),
        filterable: true,
    });
    Ok(())
}

/// The `.amat` record field by field; see docs/features/inspector.md#material-view.
fn material_table(bytes: &[u8], entries: Option<&[manifest::Entry]>) -> DataTable {
    let f = |at: usize| {
        bytes
            .get(at..at + 4)
            .map_or(0.0, |b| f32::from_le_bytes(b.try_into().unwrap()))
    };
    let u = |at: usize| {
        bytes
            .get(at..at + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let floats = |at: usize, n: usize| {
        (0..n)
            .map(|k| format!("{:.3}", f(at + k * 4)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let swatch = |at: usize| {
        [0, 1, 2].map(|k| {
            (crate::texture::linear_to_srgb(f(at + k * 4).clamp(0.0, 1.0)) * 255.0).round() as u8
        })
    };
    let flags = u(68);
    let flag_text = format!(
        "{flags:#04x} ({}, {}, {})",
        ["opaque", "mask", "blend", "reserved"][(flags & 3) as usize],
        if flags & 4 != 0 {
            "double-sided"
        } else {
            "single-sided"
        },
        if flags & 8 != 0 {
            "displacement"
        } else {
            "parallax"
        },
    );
    let texture = |at: usize| {
        let id = bytes
            .get(at..at + 8)
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()));
        if id == 0 {
            return "—".to_owned();
        }
        let file = entries
            .and_then(|entries| entries.iter().find(|e| e.id == id))
            .map_or("(not in manifest)", |e| e.path.as_str());
        format!("{id:#018x}  {file}")
    };
    let row = |at: usize, name: &str, value: String, color: bool| DataRow {
        cells: vec![at.to_string(), name.into(), value],
        swatch: color.then(|| (2, swatch(at))),
        ..Default::default()
    };
    let mut rows = vec![
        row(
            0,
            "magic",
            String::from_utf8_lossy(&bytes[..4.min(bytes.len())]).into(),
            false,
        ),
        row(4, "version", u(4).to_string(), false),
        row(8, "base_color", floats(8, 4), true),
        row(24, "emissive", floats(24, 3), true),
    ];
    for (at, name) in [
        (36, "emissive_strength"),
        (40, "metallic"),
        (44, "roughness"),
        (48, "occlusion_strength"),
        (52, "normal_scale"),
        (56, "height_scale"),
        (60, "height_midlevel"),
        (64, "alpha_cutoff"),
    ] {
        rows.push(row(at, name, format!("{:.3}", f(at)), false));
    }
    rows.push(row(68, "flags", flag_text, false));
    for (at, name) in [
        (72, "base_color_texture"),
        (80, "normal_texture"),
        (88, "orm_texture"),
        (96, "emissive_texture"),
        (104, "height_texture"),
    ] {
        rows.push(row(at, name, texture(at), false));
    }
    DataTable {
        header: format!(
            "{}   v{}   {}",
            String::from_utf8_lossy(&bytes[..4.min(bytes.len())]),
            u(4),
            format_size(bytes.len() as u64)
        ),
        columns: vec!["Offset", "Field", "Value"],
        rows,
        filterable: false,
    }
}

fn asset_rows(asset_path: &str, out: &mut Inspection) {
    out.rows.push(("Asset path", asset_path.into()));
    out.rows.push((
        "Asset ID",
        format!("{:#018x}", fnv1a64(asset_path.as_bytes())),
    ));
}

fn to_image(image: image::RgbaImage) -> Image {
    Image {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    }
}

/// Downscales `image` (aspect kept) so neither side exceeds `max_side`, e.g. the GPU limit.
pub fn fit_within(image: Image, max_side: u32) -> Image {
    let (w, h) = (image.width, image.height);
    if w.max(h) <= max_side {
        return image;
    }
    let scale = max_side as f64 / w.max(h) as f64;
    let (nw, nh) = (
        ((w as f64 * scale).round() as u32).clamp(1, max_side),
        ((h as f64 * scale).round() as u32).clamp(1, max_side),
    );
    let source = image::RgbaImage::from_raw(w, h, image.pixels).expect("RGBA8 image");
    to_image(image::imageops::resize(
        &source,
        nw,
        nh,
        image::imageops::FilterType::Triangle,
    ))
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KiB", "MiB", "GiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build, config};

    fn project(dir: &Path, textures: config::Textures) -> Project {
        Project {
            root: Some(dir.to_path_buf()),
            config: config::Config {
                textures,
                ..Default::default()
            },
        }
    }

    fn png(path: &Path, w: u32, h: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(w, h, image::Rgba([1, 2, 3, 255]))
            .save(path)
            .unwrap();
    }

    fn row<'a>(inspection: &'a Inspection, label: &str) -> &'a str {
        inspection
            .rows
            .iter()
            .find(|(l, _)| *l == label)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("no {label} row in {:?}", inspection.rows))
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(12_595), "12.3 KiB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MiB");
        assert_eq!(format_size(3 << 40), "3072.0 GiB");
    }

    #[test]
    fn source_texture_before_and_after_build() {
        let dir = tempfile::tempdir().unwrap();
        let textures = config::Textures {
            linear: vec!["**/*_n.png".into()],
            no_mipmaps: vec![],
            ..Default::default()
        };
        let project = project(dir.path(), textures);
        let file = dir.path().join("assets/textures/rock_n.png");
        png(&file, 8, 4);

        let i = inspect(&project, Location::Input, &file);
        assert_eq!(i.name, "rock_n.png");
        assert_eq!(i.path, "assets/textures/rock_n.png");
        assert_eq!(row(&i, "Kind"), "Source texture");
        assert_eq!(row(&i, "Dimensions"), "8 × 4 px");
        assert_eq!(row(&i, "Asset path"), "textures/rock_n");
        assert_eq!(
            row(&i, "Asset ID"),
            format!("{:#018x}", fnv1a64(b"textures/rock_n"))
        );
        assert_eq!(row(&i, "Color"), "linear");
        assert_eq!(row(&i, "Mip levels"), "4");
        assert_eq!(row(&i, "Output"), "textures/rock_n.ktx2 (not built)");
        assert_eq!(
            i.preview.as_ref().map(|p| (p.width, p.height)),
            Some((8, 4))
        );
        assert_eq!(i.channels, [true; 4]);

        let input = project.input_dir().unwrap();
        let output = project.output_dir().unwrap();
        build::run(&input, &output, &project.config).unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert_eq!(row(&i, "Output"), "textures/rock_n.ktx2");

        let compiled = inspect(
            &project,
            Location::Output,
            &output.join("textures/rock_n.ktx2"),
        );
        assert_eq!(row(&compiled, "Kind"), "Compiled texture");
        assert_eq!(row(&compiled, "Format"), "R8G8B8A8_UNORM");
        assert_eq!(row(&compiled, "Mip levels"), "4");
        assert_eq!(row(&compiled, "Asset ID"), row(&i, "Asset ID"));
        let sizes: Vec<_> = compiled.mips.iter().map(|m| (m.width, m.height)).collect();
        assert_eq!(sizes.len(), 3);
        assert_eq!(compiled.preview.unwrap().pixels[..4], [1, 2, 3, 255]);

        let m = inspect(
            &project,
            Location::Output,
            &output.join(manifest::FILE_NAME),
        );
        assert_eq!(row(&m, "Kind"), "Manifest");
        assert_eq!(row(&m, "Entries"), "1");
        let data = m.data.unwrap();
        assert!(
            data.header.starts_with("AMAN   v1   1 entries"),
            "{}",
            data.header
        );
        assert_eq!(data.rows.len(), 1);
        assert_eq!(data.rows[0].cells[2], "Texture");
        assert!(!data.rows[0].weak);
    }

    #[test]
    fn channels_follow_the_source_color_type() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        fs::create_dir_all(dir.path().join("assets")).unwrap();
        let file = |name: &str| dir.path().join("assets").join(name);
        image::RgbImage::new(2, 2).save(file("rgb.jpg")).unwrap();
        image::GrayImage::new(2, 2).save(file("gray.png")).unwrap();
        image::GrayAlphaImage::new(2, 2)
            .save(file("gray_alpha.png"))
            .unwrap();
        let channels = |name| inspect(&project, Location::Input, &file(name)).channels;
        assert_eq!(channels("rgb.jpg"), [true, true, true, false]);
        assert_eq!(channels("gray.png"), [false; 4]);
        assert_eq!(channels("gray_alpha.png"), [false, false, false, true]);
    }

    #[test]
    fn noise_sources() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        let file = dir.path().join("assets/clouds.noise");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(
            &file,
            "type = \"noise/value\"\nsize = 16\n\n[noise.value]\nseed = 3\n\n[[modifiers]]\ntype = \"blur\"\nenabled = false\n",
        )
        .unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert_eq!(row(&i, "Kind"), "Noise");
        assert_eq!(row(&i, "Type"), "Value");
        assert_eq!(row(&i, "Seed"), "3");
        assert_eq!(row(&i, "Frequency X"), "8");
        assert_eq!(row(&i, "Interpolation"), "quintic");
        assert_eq!(row(&i, "Modifiers"), "Blur (off)");
        assert_eq!(row(&i, "Dimensions"), "16 × 16 px");
        assert_eq!(row(&i, "Asset path"), "clouds");
        assert_eq!(row(&i, "Color"), "linear");
        assert_eq!(i.channels, [false, false, false, false]);
        assert!(i.preview.is_some());

        fs::write(&file, "type = \"noise/fbm\"\n[noise.fbm]\noctaves = 99\n").unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert!(row(&i, "Error").contains("octaves"));
        assert!(i.preview.is_none());
    }

    #[test]
    fn gradient_sources() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        let file = dir.path().join("assets/sky.gradient");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, Gradients::new().to_toml()).unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert_eq!(row(&i, "Kind"), "Gradient");
        assert_eq!(row(&i, "Gradients"), "1");
        assert_eq!(row(&i, "Cell"), "16 × 64 px");
        assert_eq!(row(&i, "Grid"), "16 × 4");
        assert_eq!(row(&i, "Layout"), "rows");
        assert_eq!(row(&i, "Dimensions"), "256 × 256 px");
        assert_eq!(row(&i, "Asset path"), "sky");
        assert_eq!(row(&i, "Color"), "sRGB");
        // Never mipmapped: levels would blend cells.
        assert_eq!(row(&i, "Mip levels"), "1");
        assert_eq!(i.channels, [true, true, true, false]);

        fs::write(&file, "cell_width = 999\n").unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert!(row(&i, "Error").contains("cell_width"));
        assert!(i.preview.is_none());
    }

    #[test]
    fn palette_sources() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        let file = dir.path().join("assets/ui/theme.palette");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, Palette::new().to_toml()).unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert_eq!(row(&i, "Kind"), "Palette");
        assert_eq!(row(&i, "Colors"), "2");
        assert_eq!(row(&i, "Cell"), "16 × 16 px");
        assert_eq!(row(&i, "Grid"), "8 × 8");
        assert_eq!(row(&i, "Layout"), "rows");
        assert_eq!(row(&i, "Dimensions"), "128 × 128 px");
        assert_eq!(row(&i, "Asset path"), "ui/theme");
        assert_eq!(row(&i, "Color"), "sRGB");
        // Never mipmapped: levels would blend cells.
        assert_eq!(row(&i, "Mip levels"), "1");
        assert_eq!(i.channels, [true, true, true, false]);

        fs::write(
            &file,
            "[[colors]]
name = \"x\"\n",
        )
        .unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert_eq!(row(&i, "Error"), "color 1: missing color");
        assert!(i.preview.is_none());
    }

    #[test]
    fn color_luts() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        let file = dir.path().join("assets/grade.lut.CUBE");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mut text = String::from("TITLE \"Warm\"\nLUT_3D_SIZE 2\n");
        for i in 0..8 {
            text += &format!("{} {} {}\n", i & 1, (i >> 1) & 1, (i >> 2) & 1);
        }
        fs::write(&file, text).unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert_eq!(row(&i, "Kind"), "Color LUT");
        assert_eq!(row(&i, "Title"), "Warm");
        assert_eq!(row(&i, "LUT size"), "2 × 2 × 2");
        assert_eq!(row(&i, "Domain"), "0, 0, 0 – 1, 1, 1");
        assert_eq!(row(&i, "Asset path"), "grade.lut");
        assert_eq!(row(&i, "Output"), "grade.lut.ktx2 (not built)");
        assert!(i.lut.is_some());

        fs::write(&file, "LUT_3D_SIZE 2\n").unwrap();
        let i = inspect(&project, Location::Input, &file);
        assert!(row(&i, "Error").contains("expected 8 entries"));
        assert_eq!(i.note.as_deref(), Some(row(&i, "Error")));
    }

    #[test]
    fn folders_and_environment_maps() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        let env = dir.path().join("assets/sky.ENV");
        for face in CUBE_FACES {
            png(&env.join(format!("{face}.png")), 4, 4);
        }
        fs::write(env.join(".hidden"), "").unwrap();

        let plain = inspect(&project, Location::Input, &dir.path().join("assets"));
        assert_eq!(
            plain.rows,
            [("Kind", "Folder".into()), ("Items", "1".into())]
        );
        assert!(plain.cubemap.is_none());

        let i = inspect(&project, Location::Input, &env);
        assert_eq!(row(&i, "Kind"), "Environment map");
        assert_eq!(row(&i, "Faces"), "6 of 6");
        assert_eq!(row(&i, "Face size"), "4 × 4 px");
        assert_eq!(i.cubemap.as_ref().map(Vec::len), Some(6));

        fs::remove_file(env.join("nz.png")).unwrap();
        fs::remove_file(env.join("pz.png")).unwrap();
        let i = inspect(&project, Location::Input, &env);
        assert_eq!(row(&i, "Faces"), "4 of 6 (missing pz, nz)");
        assert_eq!(i.note.as_deref(), Some("Missing faces: pz, nz"));
        assert!(i.cubemap.is_none());

        png(&env.join("pz.png"), 4, 4);
        png(&env.join("nz.png"), 3, 3);
        let i = inspect(&project, Location::Input, &env);
        assert_eq!(row(&i, "Error"), "Faces differ in size");

        // Output side: compiled faces.
        let output = dir.path().join("build");
        png(&env.join("nz.png"), 4, 4);
        build::run(&project.input_dir().unwrap(), &output, &Default::default()).unwrap();
        let i = inspect(&project, Location::Output, &output.join("sky.ENV"));
        assert_eq!(row(&i, "Faces"), "6 of 6");
        assert!(i.cubemap.is_some());
    }

    #[test]
    fn previews_are_full_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        let file = dir.path().join("assets/big.png");
        png(&file, 1024, 256);
        let i = inspect(&project, Location::Input, &file);
        let preview = i.preview.unwrap();
        assert_eq!((preview.width, preview.height), (1024, 256));

        let fitted = fit_within(preview, 512);
        assert_eq!((fitted.width, fitted.height), (512, 128));
        assert_eq!(fitted.pixels.len(), 512 * 128 * 4);
        let small = Image {
            width: 3,
            height: 1,
            pixels: vec![0; 12],
        };
        assert_eq!(fit_within(small.clone(), 512), small);
    }

    #[test]
    fn material_sources_and_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        png(&dir.path().join("assets/brick/brick.png"), 4, 4);
        png(&dir.path().join("assets/brick/brick.r.png"), 4, 4);
        fs::write(
            dir.path().join("assets/brick/brick.mat.toml"),
            "roughness = 0.5\nalpha_mode = \"mask\"\n",
        )
        .unwrap();
        let file = |p: &str| dir.path().join(p);

        let r = inspect(&project, Location::Input, &file("assets/brick/brick.r.png"));
        assert_eq!(row(&r, "Material"), "brick/brick.amat (roughness)");
        assert_eq!(row(&r, "Color"), "linear");
        assert_eq!(
            row(&r, "Output"),
            "brick/brick.orm.ktx2 (channel G) (not built)"
        );
        let base = inspect(&project, Location::Input, &file("assets/brick/brick.png"));
        assert_eq!(row(&base, "Material"), "brick/brick.amat (base_color)");
        assert_eq!(row(&base, "Color"), "sRGB");
        let toml = inspect(
            &project,
            Location::Input,
            &file("assets/brick/brick.mat.toml"),
        );
        assert_eq!(row(&toml, "Kind"), "Material source");
        assert_eq!(row(&toml, "Asset path"), "brick/brick.amat");
        assert_eq!(row(&toml, "Output"), "brick/brick.amat (not built)");

        let input = project.input_dir().unwrap();
        let output = project.output_dir().unwrap();
        build::run(&input, &output, &project.config).unwrap();
        let r = inspect(&project, Location::Input, &file("assets/brick/brick.r.png"));
        assert_eq!(row(&r, "Output"), "brick/brick.orm.ktx2 (channel G)");

        let mat = inspect(&project, Location::Output, &file("build/brick/brick.amat"));
        assert_eq!(row(&mat, "Kind"), "Compiled material");
        assert_eq!(row(&mat, "Asset path"), "brick/brick.amat");
        assert_eq!(row(&mat, "Roughness"), "0.50");
        assert_eq!(row(&mat, "Alpha"), "mask, cutoff 0.50");
        assert_eq!(
            row(&mat, "ORM map"),
            format!("{:#018x}", fnv1a64(b"brick/brick.orm"))
        );
        assert_eq!(row(&mat, "Normal map"), "—");
        let data = mat.data.unwrap();
        let field = |name: &str| {
            data.rows
                .iter()
                .find(|r| r.cells[1] == name)
                .unwrap()
                .clone()
        };
        assert_eq!(field("roughness").cells[2], "0.500");
        assert_eq!(
            field("flags").cells[2],
            "0x01 (mask, single-sided, parallax)"
        );
        assert_eq!(
            field("orm_texture").cells[2],
            format!(
                "{:#018x}  brick/brick.orm.ktx2",
                fnv1a64(b"brick/brick.orm")
            )
        );
        assert_eq!(field("normal_texture").cells[2], "—");
        assert!(field("base_color").swatch.is_some());

        fs::write(file("assets/brick/brick.mat.toml"), "roughness = 3.0").unwrap();
        let bad = inspect(
            &project,
            Location::Input,
            &file("assets/brick/brick.mat.toml"),
        );
        assert!(row(&bad, "Error").contains("out of range"));
    }

    #[test]
    fn fonts() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        fs::create_dir_all(dir.path().join("assets")).unwrap();
        let font = dir.path().join("assets/monogram.ttf");
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/samples/monogram.ttf"),
            &font,
        )
        .unwrap();
        let i = inspect(&project, Location::Input, &font);
        assert_eq!(row(&i, "Kind"), "Font");
        assert_eq!(row(&i, "Outlines"), "TrueType");
        assert!(i.specimen);
        assert!(i.preview.is_some());

        let broken = dir.path().join("assets/broken.otf");
        fs::write(&broken, "nope").unwrap();
        let i = inspect(&project, Location::Input, &broken);
        assert_eq!(row(&i, "Kind"), "Font");
        assert!(row(&i, "Error").starts_with("not a valid font"));
        assert!(i.preview.is_none());

        // Fonts are only inspected as sources.
        let i = inspect(&project, Location::Output, &font);
        assert_eq!(row(&i, "Kind"), "File");
    }

    #[test]
    fn models() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        fs::create_dir_all(dir.path().join("assets")).unwrap();
        let fox = dir.path().join("assets/Fox.glb");
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/samples/Fox.glb"),
            &fox,
        )
        .unwrap();
        let i = inspect(&project, Location::Input, &fox);
        assert_eq!(row(&i, "Kind"), "Model");
        assert_eq!(row(&i, "Animations"), "3");
        assert_eq!(row(&i, "Meshlet limits"), "64 vertices, 124 triangles");
        assert_eq!(row(&i, "Asset path"), "Fox.amesh");
        assert_eq!(i.animations.len(), 3);
        assert!(i.model);

        let broken = dir.path().join("assets/broken.gltf");
        fs::write(&broken, "{").unwrap();
        let i = inspect(&project, Location::Input, &broken);
        assert!(row(&i, "Error").starts_with("not a valid glTF file"));
        assert!(!i.model);
    }

    #[test]
    fn other_files_missing_files_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), Default::default());
        fs::create_dir_all(dir.path().join("assets")).unwrap();
        fs::create_dir_all(dir.path().join("build")).unwrap();

        let notes = dir.path().join("assets/notes.txt");
        fs::write(&notes, "hello").unwrap();
        let i = inspect(&project, Location::Input, &notes);
        assert_eq!(i.rows, [("Kind", "Ignored".into()), ("Size", "5 B".into())]);

        let broken = dir.path().join("assets/broken.png");
        fs::write(&broken, "not a png").unwrap();
        let i = inspect(&project, Location::Input, &broken);
        assert_eq!(row(&i, "Kind"), "Source texture");
        assert!(i.preview.is_none());
        assert!(!row(&i, "Error").is_empty());

        let fake = dir.path().join("build/fake.ktx2");
        fs::write(&fake, "nope").unwrap();
        let i = inspect(&project, Location::Output, &fake);
        assert_eq!(row(&i, "Error"), "not a KTX2 file");

        let nested_manifest = dir.path().join("build/sub/assets.aman");
        fs::create_dir_all(nested_manifest.parent().unwrap()).unwrap();
        fs::write(&nested_manifest, "x").unwrap();
        let i = inspect(&project, Location::Output, &nested_manifest);
        assert_eq!(row(&i, "Kind"), "File");

        let gone = inspect(
            &project,
            Location::Input,
            &dir.path().join("assets/gone.png"),
        );
        assert!(gone.missing);
        assert_eq!(gone.name, "gone.png");
    }
}
