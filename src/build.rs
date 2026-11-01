//! Compiles `input` into `output`, see docs/features/build.md.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config;
use crate::gradient::{Gradients, is_gradient};
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
    pub skipped: usize,
    /// Stale outputs deleted after a successful build, relative to `output`.
    pub removed: Vec<String>,
    /// Non-empty means the build failed and no manifest was written.
    pub failures: Vec<Failure>,
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
            let count = |kind| self.built.iter().filter(|b| b.kind == kind).count();
            let noun = |n, one: &str| match n {
                1 => format!("1 {one}"),
                n => format!("{n} {one}s"),
            };
            let materials = match count(Kind::Material) {
                0 => String::new(),
                n => format!(", {}", noun(n, "material")),
            };
            let removed = match self.removed.len() {
                0 => String::new(),
                k => format!(", {k} removed"),
            };
            format!(
                "Built {}{materials} ({} skipped{removed})",
                noun(count(Kind::Texture), "texture"),
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
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

pub fn run(input: &Path, output: &Path, textures: &config::Textures) -> Result<Outcome, Error> {
    // Checked first so a bad pattern is reported even without an input directory.
    texture::Settings::new(textures).map_err(Error::Pattern)?;
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
    let plan = plan::plan(input, textures)?;
    let mut outcome = Outcome {
        skipped: plan.skipped,
        failures: plan.failures,
        ..Default::default()
    };

    for asset in &plan.assets {
        let dest = output.join(&asset.output);
        let result = match &asset.job {
            Job::Texture {
                source,
                color,
                mipmaps,
                flip_green,
            } => compile_texture(&input.join(source), &dest, *color, *mipmaps, *flip_green)
                .map(|levels| levels_detail(*color, levels)),
            Job::Orm { sources } => compile_orm(input, sources, &dest)
                .map(|levels| levels_detail(ColorSpace::Linear, levels)),
            Job::Material { material, .. } => {
                write(&dest, &material::encode(material)).map(|()| material_detail(material))
            }
        };
        match result {
            Ok(detail) => outcome.built.push(Built {
                source: asset.source(),
                output: asset.output.clone(),
                kind: asset.kind,
                detail,
            }),
            Err(message) => outcome.failures.push(Failure {
                source: asset.source(),
                message,
            }),
        }
    }

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
    }
    Ok(outcome)
}

fn levels_detail(color: ColorSpace, levels: usize) -> String {
    let noun = if levels == 1 { "level" } else { "levels" };
    format!("{color}, {levels} {noun}")
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

/// Deletes outputs not in `produced` (lowercased `output`-relative paths) and
/// the directories this leaves empty. Returns the deleted paths.
///
/// Only files matching an output pattern (`*.<ext>` of an asset kind, any
/// case) are candidates, so other files in `output` are never touched.
fn remove_stale(output: &Path, produced: &HashSet<String>) -> Result<Vec<String>, Error> {
    let nodes = tree::scan(output).map_err(|e| Error::Io(output.to_path_buf(), e))?;
    let mut files = Vec::new();
    flatten(&nodes, &mut files);

    let mut removed = Vec::new();
    for file in files {
        let is_output = file.extension().is_some_and(|ext| {
            Kind::ALL
                .iter()
                .any(|kind| ext.eq_ignore_ascii_case(kind.extension()))
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
const TEXTURE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "noise", "gradient"];

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

fn decode(source: &Path) -> Result<image::RgbaImage, String> {
    load_source(source).map(|(image, _)| image)
}

/// Decodes a texture source, or generates it for a `.noise` file; also returns the
/// source's color type (noise is grayscale).
pub fn load_source(source: &Path) -> Result<(image::RgbaImage, image::ColorType), String> {
    if is_noise(source) {
        let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
        let noise = Noise::parse(&text)?;
        let color = if noise.has_color() {
            image::ColorType::Rgb8
        } else {
            image::ColorType::L8
        };
        let generated = noise.generate();
        let image = image::RgbaImage::from_raw(generated.width, generated.height, generated.pixels)
            .expect("noise image size");
        return Ok((image, color));
    }
    if is_gradient(source) {
        let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
        let gradients = Gradients::parse(&text)?;
        let color = if gradients.has_alpha() {
            image::ColorType::Rgba8
        } else {
            image::ColorType::Rgb8
        };
        let rendered = gradients.render()?;
        let image = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.pixels)
            .expect("gradient image size");
        return Ok((image, color));
    }
    let decoded = image::ImageReader::open(source)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let color = decoded.color();
    Ok((decoded.to_rgba8(), color))
}

pub fn is_noise(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("noise"))
}

/// Returns the number of levels written.
fn compile_texture(
    source: &Path,
    dest: &Path,
    color: ColorSpace,
    mipmaps: bool,
    flip_green: bool,
) -> Result<usize, String> {
    let mut image = decode(source)?;
    if flip_green {
        for pixel in image.pixels_mut() {
            pixel[1] = 255 - pixel[1];
        }
    }
    write_texture(image, dest, color, mipmaps)
}

/// Packs the red channels of occlusion, roughness, metallic into R, G, B;
/// missing ones are 255. Returns the number of levels written.
fn compile_orm(input: &Path, sources: &[Option<String>; 3], dest: &Path) -> Result<usize, String> {
    let mut size = None;
    let mut channels: Vec<(usize, image::RgbaImage)> = Vec::new();
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
    let mut packed = image::RgbaImage::from_pixel(width, height, image::Rgba([255; 4]));
    for (channel, image) in &channels {
        for (out, src) in packed.pixels_mut().zip(image.pixels()) {
            out[*channel] = src[0];
        }
    }
    write_texture(packed, dest, ColorSpace::Linear, true)
}

fn write_texture(
    image: image::RgbaImage,
    dest: &Path,
    color: ColorSpace,
    mipmaps: bool,
) -> Result<usize, String> {
    let base = texture::Image {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    };
    let levels = if mipmaps {
        texture::mip_chain(base, color)
    } else {
        vec![base]
    };
    write(dest, &crate::ktx2::encode_rgba8(color, &levels))?;
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
    use crate::hash::fnv1a64;

    fn png(path: &Path, w: u32, h: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([x as u8, y as u8, 7, 255]))
            .save(path)
            .unwrap();
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

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built.len(), 2);
        assert_eq!(outcome.skipped, 1);
        assert_eq!(outcome.summary(), "Built 2 textures (1 skipped)");

        let ktx = fs::read(output.join("textures/Grass.ktx2")).unwrap();
        let reader = ::ktx2::Reader::new(&ktx[..]).unwrap();
        assert_eq!(reader.header().pixel_width, 4);
        assert_eq!(reader.header().format, Some(::ktx2::Format::R8G8B8A8_SRGB));
        assert_eq!(reader.header().level_count, 3);
        assert_eq!(reader.levels().next().unwrap().data[..4], [0, 0, 7, 255]);

        let mut entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "logo.ktx2");
        assert_eq!(entries[0].id, fnv1a64(b"logo"));
        assert_eq!(entries[1].path, "textures/Grass.ktx2");
        assert_eq!(entries[1].id, fnv1a64(b"textures/Grass"));
    }

    #[test]
    fn failure_skips_manifest_but_keeps_good_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("ok.png"), 1, 1);
        fs::write(input.join("broken.png"), "not a png").unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "broken.png");
        assert!(outcome.summary().starts_with("Build failed: broken.png:"));
        assert!(output.join("ok.ktx2").is_file());
        assert!(!output.join(manifest::FILE_NAME).exists());
    }

    #[test]
    fn builds_jpeg_with_opaque_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        image::RgbImage::from_pixel(4, 4, image::Rgb([200, 100, 50]))
            .save(input.join("brick.n.JPEG"))
            .unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built[0].output, "brick.n.ktx2");
        let ktx = fs::read(output.join("brick.n.ktx2")).unwrap();
        let reader = ::ktx2::Reader::new(&ktx[..]).unwrap();
        let texel = &reader.levels().next().unwrap().data[..4];
        // Lossy, so only roughly the source color; alpha is exact.
        assert!(
            texel[0].abs_diff(200) < 8 && texel[2].abs_diff(50) < 8,
            "{texel:?}"
        );
        assert_eq!(texel[3], 255);
    }

    #[test]
    fn png_and_jpeg_with_same_asset_path_collide() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        image::RgbImage::new(1, 1)
            .save(input.join("a.jpg"))
            .unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert_eq!(outcome.failures.len(), 2);
        assert!(outcome.failures[0].message.contains("collides"));
    }

    #[test]
    fn builds_noise_as_linear_texture() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(input.join("rock")).unwrap();
        fs::write(input.join("clouds.noise"), "size = 16\n").unwrap();
        fs::write(
            input.join("rock/rock.h.noise"),
            "size = 8\ntype = \"noise/voronoi\"\n",
        )
        .unwrap();
        fs::write(input.join("bad.noise"), "size = 0\n").unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].source, "bad.noise");
        assert!(outcome.failures[0].message.contains("size must be"));
        let (format, pixels) = level0(&output.join("clouds.ktx2"));
        assert_eq!(format, Some(::ktx2::Format::R8G8B8A8_UNORM));
        assert_eq!(pixels.len(), 16 * 16 * 4);
        assert_eq!(pixels[..4], [pixels[0], pixels[0], pixels[0], 255]);
        // In a material slot: compiled under its slot.
        assert!(output.join("rock/rock.h.ktx2").is_file());
        assert!(output.join("rock/rock.mat").is_file());
    }

    #[test]
    fn colliding_asset_paths_fail() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        png(&input.join("a.PNG"), 1, 1);

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
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
        };

        let mut outcome = run(&input, &output, &textures).unwrap();
        outcome.built.sort_by(|a, b| a.source.cmp(&b.source));
        let built: Vec<_> = outcome
            .built
            .iter()
            .map(|b| (b.source.as_str(), b.detail.as_str()))
            .collect();
        assert_eq!(
            built,
            [
                ("rock_n.png", "linear, 3 levels"),
                ("ui/button.png", "srgb, 1 level")
            ]
        );
        let ktx = fs::read(output.join("rock_n.ktx2")).unwrap();
        let header = ::ktx2::Reader::new(&ktx[..]).unwrap().header();
        assert_eq!(header.format, Some(::ktx2::Format::R8G8B8A8_UNORM));
    }

    fn gray(path: &Path, w: u32, h: u32, value: u8) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::GrayImage::from_pixel(w, h, image::Luma([value]))
            .save(path)
            .unwrap();
    }

    fn level0(path: &Path) -> (Option<::ktx2::Format>, Vec<u8>) {
        let ktx = fs::read(path).unwrap();
        let reader = ::ktx2::Reader::new(&ktx[..]).unwrap();
        let data = reader.levels().next().unwrap().data.to_vec();
        (reader.header().format, data)
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
        };

        let outcome = run(&input, &output, &textures).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let lines: Vec<_> = outcome
            .built
            .iter()
            .map(|b| format!("{} -> {} ({})", b.source, b.output, b.detail))
            .collect();
        assert_eq!(
            lines,
            [
                "brick/a.b.png -> brick/a.b.ktx2 (srgb, 2 levels)",
                "brick/brick.n.png -> brick/brick.n.ktx2 (linear, 3 levels)",
                "brick/brick.png -> brick/brick.ktx2 (srgb, 3 levels)",
                "brick/grass.png -> brick/grass.ktx2 (srgb, 2 levels)",
                "brick/brick.ao.png, brick/brick.r.png -> brick/brick.orm.ktx2 (linear, 3 levels)",
                "brick/brick.* -> brick/brick.mat (material: base_color, normal, orm)",
            ]
        );
        assert_eq!(
            outcome.summary(),
            "Built 5 textures, 1 material (0 skipped)"
        );
        assert!(!output.join("brick/brick.r.ktx2").exists());
        assert!(!output.join("brick/brick.ao.ktx2").exists());

        let (format, orm) = level0(&output.join("brick/brick.orm.ktx2"));
        assert_eq!(format, Some(::ktx2::Format::R8G8B8A8_UNORM));
        assert_eq!(orm[..4], [50, 100, 255, 255]);
        assert_eq!(
            level0(&output.join("brick/brick.ktx2")).0,
            Some(::ktx2::Format::R8G8B8A8_SRGB)
        );

        let m = material::decode(&fs::read(output.join("brick/brick.mat")).unwrap()).unwrap();
        assert_eq!(m.textures.base_color, fnv1a64(b"brick/brick"));
        assert_eq!(m.textures.normal, fnv1a64(b"brick/brick.n"));
        assert_eq!(m.textures.orm, fnv1a64(b"brick/brick.orm"));
        assert_eq!((m.textures.emissive, m.textures.height), (0, 0));
        assert_eq!(m.metallic, 0.0);

        let entries =
            manifest::decode(&fs::read(output.join(manifest::FILE_NAME)).unwrap()).unwrap();
        let material = entries.iter().find(|e| e.kind == Kind::Material).unwrap();
        assert_eq!(material.id, fnv1a64(b"brick/brick.mat"));
        assert_eq!(material.path, "brick/brick.mat");
        assert_eq!(entries.len(), 6);
    }

    #[test]
    fn scalar_only_material() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        fs::write(
            input.join("gold.mat.toml"),
            "metallic = 1.0\nroughness = 0.3\n",
        )
        .unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(outcome.built[0].source, "gold.mat.toml");
        assert_eq!(outcome.built[0].detail, "material: no textures");
        assert_eq!(
            outcome.summary(),
            "Built 0 textures, 1 material (0 skipped)"
        );
        let m = material::decode(&fs::read(output.join("gold.mat")).unwrap()).unwrap();
        assert_eq!((m.metallic, m.roughness), (1.0, 0.3));
        assert_eq!(m.textures, material::TextureIds::default());
    }

    #[test]
    fn toml_overrides_slots_and_flips_dx_normals() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 40, 255, 255]))
            .save({
                fs::create_dir_all(input.join("wall")).unwrap();
                input.join("wall/wall.n.png")
            })
            .unwrap();
        gray(&input.join("wall/wall.ao.png"), 1, 1, 10);
        gray(&input.join("shared/ao.png"), 1, 1, 90);
        fs::write(
            input.join("wall/wall.mat.toml"),
            "normal_y = \"dx\"\n[textures]\nocclusion = \"../shared/ao.png\"\n",
        )
        .unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        assert_eq!(
            level0(&output.join("wall/wall.n.ktx2")).1,
            [128, 215, 255, 255]
        );
        assert_eq!(level0(&output.join("wall/wall.orm.ktx2")).1[0], 90);
        // The replaced convention file is a plain texture again.
        assert!(output.join("wall/wall.ao.ktx2").is_file());
        assert!(!output.join("shared/ao.ktx2").exists());
    }

    #[test]
    fn material_failures() {
        let cases: [(&[(&str, &str)], &str); 4] = [
            (&[("m.mat.toml", "roughness = 2.0")], "out of range"),
            (
                &[("m.mat.toml", "[textures]\nnormal = \"../../x.png\"")],
                "not a texture source",
            ),
            (
                &[("m.mat.toml", "[textures]\nnormal = \"missing.png\"")],
                "not a texture source",
            ),
            (
                &[
                    ("m.mat.toml", "[textures]\nnormal = \"shared.png\""),
                    ("k.mat.toml", "[textures]\nbase_color = \"shared.png\""),
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
            let outcome = run(&input, &output, &config::Textures::default()).unwrap();
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
        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
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
        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
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
        fs::write(input.join("a.mat.toml"), "").unwrap();
        run(&input, &output, &config::Textures::default()).unwrap();
        fs::remove_file(input.join("a.mat.toml")).unwrap();
        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert_eq!(outcome.removed, ["a.mat"]);
    }

    #[test]
    fn invalid_pattern_stops_build() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("a.png"), 1, 1);
        let textures = config::Textures {
            linear: vec!["[".into()],
            no_mipmaps: vec![],
        };
        assert!(matches!(
            run(&input, &output, &textures),
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
        run(&input, &output, &textures).unwrap();

        fs::remove_dir_all(input.join("old")).unwrap();
        fs::remove_file(input.join("shared/gone.png")).unwrap();
        fs::write(output.join("notes.txt"), "mine").unwrap();
        fs::write(output.join(".hidden.ktx2"), "mine").unwrap();
        fs::create_dir_all(output.join("empty")).unwrap();

        let outcome = run(&input, &output, &textures).unwrap();
        let mut removed = outcome.removed.clone();
        removed.sort();
        assert_eq!(removed, ["old/deep/gone.ktx2", "shared/gone.ktx2"]);
        assert_eq!(outcome.summary(), "Built 2 textures (0 skipped, 2 removed)");

        assert!(!output.join("old").exists());
        assert!(output.join("shared/stays.ktx2").is_file());
        assert!(output.join("keep.ktx2").is_file());
        assert!(output.join("notes.txt").is_file());
        assert!(output.join(".hidden.ktx2").is_file());
        assert!(output.join("empty").is_dir());

        let again = run(&input, &output, &textures).unwrap();
        assert!(again.removed.is_empty());
        assert_eq!(again.summary(), "Built 2 textures (0 skipped)");
    }

    #[test]
    fn stale_check_ignores_case() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        png(&input.join("Grass.png"), 1, 1);
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("grass.KTX2"), "older build").unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert!(outcome.removed.is_empty());
        assert!(output.join("grass.KTX2").is_file());
    }

    #[test]
    fn failed_build_removes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (input, output) = (dir.path().join("assets"), dir.path().join("build"));
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("broken.png"), "not a png").unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("stale.ktx2"), "x").unwrap();

        let outcome = run(&input, &output, &config::Textures::default()).unwrap();
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.removed.is_empty());
        assert!(output.join("stale.ktx2").is_file());
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
                    run(&input, &output, &textures),
                    Err(Error::OutputContainsInput(_))
                ),
                "{}",
                output.display()
            );
        }
        assert!(!dir.path().join("a.ktx2").exists());
        assert!(!input.join("a.ktx2").exists());
    }

    #[test]
    fn missing_input_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = run(
            &dir.path().join("nope"),
            &dir.path().join("build"),
            &config::Textures::default(),
        );
        assert!(matches!(result, Err(Error::MissingInput(_))));
    }
}
