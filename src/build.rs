//! Compiles `input` into `output`, see docs/features/build.md.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config;
use crate::hash::fnv1a64;
use crate::manifest::{self, Kind};
use crate::texture::{self, ColorSpace, Settings};
use crate::tree::{self, Node};

/// One compiled source.
#[derive(Debug)]
pub struct Built {
    /// Source path relative to `input`.
    pub source: String,
    /// Output path relative to `output`.
    pub output: String,
    pub color: ColorSpace,
    pub levels: usize,
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
            let n = self.built.len();
            let noun = if n == 1 { "texture" } else { "textures" };
            let removed = match self.removed.len() {
                0 => String::new(),
                k => format!(", {k} removed"),
            };
            format!("Built {n} {noun} ({} skipped{removed})", self.skipped)
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

struct Texture {
    id: u64,
    source: PathBuf,
    source_rel: String,
    output_rel: String,
}

pub fn run(input: &Path, output: &Path, textures: &config::Textures) -> Result<Outcome, Error> {
    let settings = Settings::new(textures).map_err(Error::Pattern)?;
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
    let nodes = tree::scan(input).map_err(|e| Error::Io(input.to_path_buf(), e))?;
    let mut files = Vec::new();
    flatten(&nodes, &mut files);

    let mut outcome = Outcome::default();
    let mut textures = Vec::new();
    for file in files {
        let source_rel = match relative(input, file) {
            Ok(rel) => rel,
            Err(message) => {
                outcome.failures.push(Failure {
                    source: file.display().to_string(),
                    message,
                });
                continue;
            }
        };
        let is_png = file
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
        if !is_png {
            outcome.skipped += 1;
            continue;
        }
        let asset_path = strip_extension(&source_rel);
        textures.push(Texture {
            id: fnv1a64(asset_path.as_bytes()),
            source: file.to_path_buf(),
            output_rel: format!("{asset_path}.ktx2"),
            source_rel,
        });
    }

    let mut by_id: HashMap<u64, Vec<&str>> = HashMap::new();
    for t in &textures {
        by_id.entry(t.id).or_default().push(&t.source_rel);
    }
    for t in &textures {
        let sources = &by_id[&t.id];
        if sources.len() > 1 {
            let others: Vec<_> = sources.iter().filter(|s| **s != t.source_rel).collect();
            outcome.failures.push(Failure {
                source: t.source_rel.clone(),
                message: format!("asset ID {:#018x} collides with {others:?}", t.id),
            });
            continue;
        }
        let color = settings.color_space(&t.source_rel);
        let mipmaps = settings.mipmaps(&t.source_rel);
        match compile_texture(&t.source, &output.join(&t.output_rel), color, mipmaps) {
            Ok(levels) => outcome.built.push(Built {
                source: t.source_rel.clone(),
                output: t.output_rel.clone(),
                color,
                levels,
            }),
            Err(message) => outcome.failures.push(Failure {
                source: t.source_rel.clone(),
                message,
            }),
        }
    }

    if outcome.failures.is_empty() {
        let entries: Vec<_> = textures
            .iter()
            .map(|t| manifest::Entry {
                id: t.id,
                kind: Kind::Texture,
                path: t.output_rel.clone(),
            })
            .collect();
        let path = output.join(manifest::FILE_NAME);
        fs::create_dir_all(output).map_err(|e| Error::Io(output.to_path_buf(), e))?;
        fs::write(&path, manifest::encode(&entries)).map_err(|e| Error::Io(path, e))?;

        let produced = textures
            .iter()
            .map(|t| t.output_rel.to_lowercase())
            .collect();
        outcome.removed = remove_stale(output, &produced)?;
    }
    Ok(outcome)
}

/// Extensions of files assetc writes; only these are ever deleted.
const OUTPUT_EXTENSIONS: [&str; 1] = ["ktx2"];

/// Deletes outputs not in `produced` (lowercased `output`-relative paths) and
/// the directories this leaves empty. Returns the deleted paths.
fn remove_stale(output: &Path, produced: &HashSet<String>) -> Result<Vec<String>, Error> {
    let nodes = tree::scan(output).map_err(|e| Error::Io(output.to_path_buf(), e))?;
    let mut files = Vec::new();
    flatten(&nodes, &mut files);

    let mut removed = Vec::new();
    for file in files {
        let is_output = file.extension().is_some_and(|ext| {
            OUTPUT_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
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

fn flatten<'a>(nodes: &'a [Node], files: &mut Vec<&'a Path>) {
    for node in nodes {
        match &node.children {
            Some(children) => flatten(children, files),
            None => files.push(&node.path),
        }
    }
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

/// Returns the number of levels written.
fn compile_texture(
    source: &Path,
    dest: &Path,
    color: ColorSpace,
    mipmaps: bool,
) -> Result<usize, String> {
    let image = image::ImageReader::open(source)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();
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
    let bytes = crate::ktx2::encode_rgba8(color, &levels);
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(levels.len())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .map(|b| (b.source.as_str(), b.color, b.levels))
            .collect();
        assert_eq!(
            built,
            [
                ("rock_n.png", ColorSpace::Linear, 3),
                ("ui/button.png", ColorSpace::Srgb, 1)
            ]
        );
        let ktx = fs::read(output.join("rock_n.ktx2")).unwrap();
        let header = ::ktx2::Reader::new(&ktx[..]).unwrap().header();
        assert_eq!(header.format, Some(::ktx2::Format::R8G8B8A8_UNORM));
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
