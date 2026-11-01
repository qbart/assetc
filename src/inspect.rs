//! What the Inspector shows for a file, see docs/features/inspector.md.

use std::fs;
use std::path::Path;

use crate::build::{is_texture_source, relative, strip_extension};
use crate::config::Project;
use crate::hash::fnv1a64;
use crate::material::{self, AlphaMode, HeightMode};
use crate::plan::{self, Job, join, split_dir};
use crate::texture::{self, ColorSpace, Image};
use crate::{ktx2, manifest};

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
    /// Shown instead of a preview, e.g. for unsupported formats.
    pub note: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum Kind {
    SourceTexture,
    MaterialSource,
    CompiledTexture,
    CompiledMaterial,
    Manifest,
    Ignored,
    File,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::SourceTexture => "Source texture",
            Self::MaterialSource => "Material source",
            Self::CompiledTexture => "Compiled texture",
            Self::CompiledMaterial => "Compiled material",
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
        Location::Input if is_texture_source(file) => Kind::SourceTexture,
        Location::Input => Kind::Ignored,
        Location::Output if has_ext(manifest::Kind::Texture.extension()) => Kind::CompiledTexture,
        Location::Output if has_ext(manifest::Kind::Material.extension()) => Kind::CompiledMaterial,
        Location::Output if rel == manifest::FILE_NAME => Kind::Manifest,
        Location::Output => Kind::File,
    };
    out.rows.push(("Kind", kind.name().into()));
    out.rows.push(("Size", format_size(meta.len())));

    let result = match kind {
        Kind::SourceTexture => source_texture(project, file, &rel, &mut out),
        Kind::MaterialSource => material_source(project, file, &rel, &mut out),
        Kind::CompiledTexture => compiled_texture(file, &rel, &mut out),
        Kind::CompiledMaterial => compiled_material(file, &rel, &mut out),
        Kind::Manifest => manifest(file, &mut out),
        Kind::Ignored | Kind::File => Ok(()),
    };
    if let Err(message) = result {
        out.rows.push(("Error", message));
        out.preview = None;
        out.note = None;
    }
    out
}

fn source_texture(
    project: &Project,
    file: &Path,
    rel: &str,
    out: &mut Inspection,
) -> Result<(), String> {
    let image = image::ImageReader::open(file)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let asset_path = strip_extension(rel);
    out.rows
        .push(("Dimensions", format!("{width} × {height} px")));
    asset_rows(asset_path, out);

    let color_name = |color| match color {
        ColorSpace::Srgb => "sRGB",
        ColorSpace::Linear => "linear",
    };
    let mut output = None;
    match project
        .input_dir()
        .map(|dir| plan::plan(&dir, &project.config.textures))
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
                let orm_path = format!("{}.orm", u.material.strip_suffix(".mat")?);
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
    out.rows
        .push(("Dimensions", format!("{} × {} px", info.width, info.height)));
    out.rows.push(("Format", info.format_name()));
    out.rows.push(("Mip levels", info.levels.to_string()));
    asset_rows(strip_extension(rel), out);

    match info.level0 {
        Some(level) => {
            let image = image::RgbaImage::from_raw(level.width, level.height, level.pixels)
                .ok_or("level 0 has the wrong size")?;
            out.preview = Some(to_image(image));
        }
        None => out.note = Some("No preview for this format"),
    }
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
    let asset_path = join(dir, &format!("{name}.mat"));
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

fn compiled_material(file: &Path, rel: &str, out: &mut Inspection) -> Result<(), String> {
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let m = material::decode(&bytes)?;
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
        (
            "Height",
            format!(
                "{height_mode}, scale {:.2}, midlevel {:.2}",
                m.height_scale, m.height_midlevel
            ),
        ),
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
    Ok(())
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

        let input = project.input_dir().unwrap();
        let output = project.output_dir().unwrap();
        build::run(&input, &output, &project.config.textures).unwrap();
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
        assert_eq!(compiled.preview.unwrap().pixels[..4], [1, 2, 3, 255]);

        let m = inspect(
            &project,
            Location::Output,
            &output.join(manifest::FILE_NAME),
        );
        assert_eq!(row(&m, "Kind"), "Manifest");
        assert_eq!(row(&m, "Entries"), "1");
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
        assert_eq!(row(&r, "Material"), "brick/brick.mat (roughness)");
        assert_eq!(row(&r, "Color"), "linear");
        assert_eq!(
            row(&r, "Output"),
            "brick/brick.orm.ktx2 (channel G) (not built)"
        );
        let base = inspect(&project, Location::Input, &file("assets/brick/brick.png"));
        assert_eq!(row(&base, "Material"), "brick/brick.mat (base_color)");
        assert_eq!(row(&base, "Color"), "sRGB");
        let toml = inspect(
            &project,
            Location::Input,
            &file("assets/brick/brick.mat.toml"),
        );
        assert_eq!(row(&toml, "Kind"), "Material source");
        assert_eq!(row(&toml, "Asset path"), "brick/brick.mat");
        assert_eq!(row(&toml, "Output"), "brick/brick.mat (not built)");

        let input = project.input_dir().unwrap();
        let output = project.output_dir().unwrap();
        build::run(&input, &output, &project.config.textures).unwrap();
        let r = inspect(&project, Location::Input, &file("assets/brick/brick.r.png"));
        assert_eq!(row(&r, "Output"), "brick/brick.orm.ktx2 (channel G)");

        let mat = inspect(&project, Location::Output, &file("build/brick/brick.mat"));
        assert_eq!(row(&mat, "Kind"), "Compiled material");
        assert_eq!(row(&mat, "Asset path"), "brick/brick.mat");
        assert_eq!(row(&mat, "Roughness"), "0.50");
        assert_eq!(row(&mat, "Alpha"), "mask, cutoff 0.50");
        assert_eq!(
            row(&mat, "ORM map"),
            format!("{:#018x}", fnv1a64(b"brick/brick.orm"))
        );
        assert_eq!(row(&mat, "Normal map"), "—");

        fs::write(file("assets/brick/brick.mat.toml"), "roughness = 3.0").unwrap();
        let bad = inspect(
            &project,
            Location::Input,
            &file("assets/brick/brick.mat.toml"),
        );
        assert!(row(&bad, "Error").contains("out of range"));
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

        let nested_manifest = dir.path().join("build/sub/assets.manifest");
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
