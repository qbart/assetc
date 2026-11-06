//! File operations in the Input tree, see docs/features/file-operations.md.

use std::fs;
use std::path::{Path, PathBuf};

use crate::gradient::Gradients;
use crate::noise::{GeneratorKind, Noise};
use crate::palette::Palette;

/// Creates `New Folder` (or `New Folder 2`, …) in `dir`.
pub fn create_folder(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "New Folder", "");
    fs::create_dir(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `<name>.anoise` (or `<name> 2.anoise`, …) in `dir` with the generator's defaults.
pub fn create_noise(input: &Path, dir: &Path, kind: GeneratorKind) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, kind.file_stem(), ".anoise");
    fs::write(&path, Noise::new(kind).to_toml()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `gradient.agradient` (or `gradient 2.agradient`, …) in `dir`; see
/// docs/features/gradients.md#creating.
pub fn create_gradient(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "gradient", ".agradient");
    fs::write(&path, Gradients::new().to_toml()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `schema.aschema` (or `schema 2.aschema`, …) in `dir`; see
/// docs/features/schemas.md#creating.
pub fn create_schema(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "schema", ".aschema");
    fs::write(&path, crate::aschema::Schema::new().to_json()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `graph.agraph` (or `graph 2.agraph`, …) in `dir`; see
/// docs/features/graphs.md#creating.
pub fn create_graph(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "graph", ".agraph");
    fs::write(&path, crate::agraph::Graph::new().to_json()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `tree.atree` (or `tree 2.atree`, …) in `dir`; see docs/features/trees.md#creating.
pub fn create_tree(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "tree", ".atree");
    fs::write(&path, crate::atree::to_json(&crate::atree::new())).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `data.adata` (or `data 2.adata`, …) in `dir`, naming the first schema of `dir`,
/// else of `input`; see docs/features/instances.md#creating.
pub fn create_data(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    use crate::adata::{DataFile, relative, schemas_in};
    check_inside(input, dir, true)?;
    let all = schemas_in(input);
    let schema = all
        .iter()
        .find(|p| p.parent() == Some(dir))
        .or(all.first())
        .map(|p| relative(dir, p))
        .unwrap_or_default();
    let path = unique(dir, "data", ".adata");
    fs::write(&path, DataFile::new(schema).to_json(None)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `material.amat` (or `material 2.amat`, …) in `dir`; see
/// docs/features/materials.md#creating.
pub fn create_material(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "material", crate::material::SOURCE_SUFFIX);
    let text = "base_color = [0.8, 0.8, 0.8, 1.0]\nroughness = 0.5\n";
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `material.acmat` (or `material 2.acmat`, …) in `dir`, with no shader chosen yet;
/// see docs/features/custom-materials.md#custom-materials.
pub fn create_custom_material(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "material", crate::acmat::SUFFIX);
    fs::write(&path, "shader = \"\"\n").map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `shader.ashader` (or `shader 2.ashader`, …) in `dir`; see
/// docs/features/custom-materials.md#shader-files.
pub fn create_shader(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "shader", ".ashader");
    let mut texture = crate::shader::Texture::new("color", crate::shader::Usage::Color);
    texture.bare = true;
    let shader = crate::shader::Shader {
        textures: vec![texture],
        ..Default::default()
    };
    fs::write(&path, shader.to_toml()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `palette.apalette` (or `palette 2.apalette`, …) in `dir`; see
/// docs/features/palettes.md#creating.
pub fn create_palette(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "palette", ".apalette");
    fs::write(&path, Palette::new().to_toml()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Renames `path` to `name` in the same folder, with its sidecar, and fixes the paths that point
/// to or from it (`assetc.toml` in `root` too); returns the new path.
pub fn rename(
    input: &Path,
    root: Option<&Path>,
    path: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    check_inside(input, path, false)?;
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(format!("{name:?} is not a valid name"));
    }
    if path.file_name().is_some_and(|n| n == name) {
        return Ok(path.to_path_buf());
    }
    let target = path.with_file_name(name);
    let mut moves = vec![(path.to_path_buf(), target.clone())];
    // The sidecar follows its file.
    let sidecar = crate::amod::path_for(path);
    if sidecar.is_file() {
        moves.push((sidecar, crate::amod::path_for(&target)));
    }
    relocate_all(input, root, &moves)?;
    Ok(target)
}

/// Deletes `path` (a folder with everything inside it) permanently.
pub fn delete(input: &Path, path: &Path) -> Result<(), String> {
    check_inside(input, path, false)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())?;
        // The sidecar goes with its file.
        let sidecar = crate::amod::path_for(path);
        if sidecar.is_file() {
            fs::remove_file(sidecar).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// What a move or copy did.
#[derive(Debug, PartialEq)]
pub struct Report {
    /// Where the entry went (the first one, for a copy).
    pub path: PathBuf,
    /// The other entries that went with it.
    pub more: usize,
    /// The files whose paths were fixed.
    pub fixed: usize,
}

/// Moves `path` into folder `dest` with its companions, and fixes the paths that point to or
/// from what moved (`assetc.toml` in `root` too). See docs/features/file-operations.md#move.
pub fn move_into(
    input: &Path,
    root: Option<&Path>,
    path: &Path,
    dest: &Path,
) -> Result<Report, String> {
    check_inside(input, path, false)?;
    check_inside(input, dest, true)?;
    if !dest.is_dir() {
        return Err(format!("{} is not a folder", dest.display()));
    }
    // A sidecar is moved by its file, when it has one.
    let main = sidecar_file(path).unwrap_or_else(|| path.to_path_buf());
    let from = main.parent().unwrap_or(input);
    if same_path(from, dest) {
        return Err(format!(
            "{} is already in {}",
            name_of(&main),
            name_of(dest)
        ));
    }
    if fs::canonicalize(dest).is_ok_and(|d| fs::canonicalize(&main).is_ok_and(|m| d.starts_with(m)))
    {
        return Err("a folder cannot move into itself".into());
    }
    let moves: Vec<(PathBuf, PathBuf)> = companions(Some(input), &main)
        .into_iter()
        .map(|p| {
            let to = dest.join(p.strip_prefix(from).unwrap_or(&p));
            (p, to)
        })
        .collect();
    let fixed = relocate_all(input, root, &moves)?;
    Ok(Report {
        path: dest.join(path.strip_prefix(from).unwrap_or(path)),
        more: moves.len() - 1,
        fixed,
    })
}

/// The entries that go with `path` when it moves (its companions, or its file's for a
/// sidecar), not counting `path`.
pub fn moving_with(input: &Path, path: &Path) -> usize {
    let main = sidecar_file(path).unwrap_or_else(|| path.to_path_buf());
    companions(Some(input), &main).len() - 1
}

/// Whether moving `path` into `dest` would do something: not its own folder, not itself or a
/// folder inside it.
pub fn can_move(path: &Path, dest: &Path) -> bool {
    let main = sidecar_file(path).unwrap_or_else(|| path.to_path_buf());
    let from = main.parent().unwrap_or(Path::new(""));
    !same_path(from, dest) && !dest.starts_with(&main)
}

/// Copies files and folders from outside the input directory into `dest` with their
/// companions, as they are; returns the copies of `paths`. See
/// docs/features/file-operations.md#dropping-files.
pub fn copy_into(input: &Path, paths: &[PathBuf], dest: &Path) -> Result<Report, String> {
    check_inside(input, dest, true)?;
    let mut copies: Vec<(PathBuf, PathBuf)> = Vec::new();
    for path in paths {
        let from = path.parent().unwrap_or(path);
        for p in companions(None, path) {
            let to = dest.join(p.strip_prefix(from).unwrap_or(&p));
            if !copies.iter().any(|(f, _)| *f == p) {
                copies.push((p, to));
            }
        }
    }
    for (_, to) in &copies {
        if to.exists() {
            return Err(format!(
                "{} already exists in {}",
                name_of(to),
                name_of(dest)
            ));
        }
    }
    for (from, to) in &copies {
        copy_entry(from, to).map_err(|e| format!("{}: {e}", name_of(from)))?;
    }
    let first = paths.first().ok_or("nothing to copy")?;
    Ok(Report {
        path: dest.join(first.file_name().unwrap_or_default()),
        more: copies.len() - 1,
        fixed: 0,
    })
}

fn copy_entry(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if from.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// The file whose sidecar `path` is, when that file exists.
fn sidecar_file(path: &Path) -> Option<PathBuf> {
    let main = path.with_extension("");
    (crate::amod::is_sidecar(path) && main.is_file()).then_some(main)
}

fn is_gltf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gltf"))
}

/// Whether `a` and `b` are the same entry (also when the file system ignores case).
fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "input".into())
}

/// `path` and what goes with it, `path` first: each file's `.amod` sidecar, a material's files
/// by name and a `.gltf`'s own files; see docs/features/file-operations.md#companions. A
/// `.gltf`'s files that another model under `input` uses stay; without `input`, none do.
pub fn companions(input: Option<&Path>, path: &Path) -> Vec<PathBuf> {
    let mut out = vec![path.to_path_buf()];
    if path.is_file() {
        let dir = path.parent().unwrap_or(Path::new(""));
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if let Some(material) = name.strip_suffix(crate::material::SOURCE_SUFFIX) {
            out.extend(by_name(dir, |stem, entry| {
                crate::build::is_texture_source(entry)
                    && (stem == material
                        || stem
                            .strip_prefix(material)
                            .and_then(|s| s.strip_prefix('.'))
                            .is_some_and(|suffix| {
                                crate::material::Slot::from_suffix(suffix).is_some()
                            }))
            }));
        } else if let Some(material) = name.strip_suffix(crate::acmat::SUFFIX) {
            out.extend(custom_textures(path, dir, material));
        } else if is_gltf(path) {
            out.extend(gltf_files(input, path));
        }
        let sidecars: Vec<PathBuf> = out
            .iter()
            .map(|p| crate::amod::path_for(p))
            .filter(|s| s.is_file())
            .collect();
        out.extend(sidecars);
    }
    out.dedup();
    out
}

/// The entries of `dir` whose name without its extension passes `keep`, sorted.
fn by_name(dir: &Path, keep: impl Fn(&str, &Path) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            let stem = crate::build::strip_extension(&name);
            !name.starts_with('.') && keep(stem, p)
        })
        .collect();
    out.sort();
    out
}

/// The files a custom material's shader takes by name; none when the shader cannot be read.
fn custom_textures(file: &Path, dir: &Path, material: &str) -> Vec<PathBuf> {
    let Some(shader) = fs::read_to_string(file)
        .ok()
        .and_then(|t| crate::acmat::Source::parse(&t).ok())
        .filter(|s| !s.shader.is_empty())
        .and_then(|s| fs::read_to_string(dir.join(&s.shader)).ok())
        .and_then(|t| crate::shader::Shader::parse(&t).ok())
    else {
        return Vec::new();
    };
    let fillable = shader.fillable();
    by_name(dir, |stem, entry| {
        fillable.iter().any(|(fill, usage)| {
            let bare = shader.textures.iter().any(|t| t.bare && t.name == *fill);
            usage.accepts(entry)
                && (stem == format!("{material}.{fill}") || (bare && stem == material))
        })
    })
}

/// A `.gltf`'s buffer and image files in its folder or below that no other `.gltf` under `input`
/// uses.
fn gltf_files(input: Option<&Path>, file: &Path) -> Vec<PathBuf> {
    let dir = file.parent().unwrap_or(Path::new(""));
    let own = gltf_uris(file);
    let others: Vec<PathBuf> = input
        .map(crate::build::files_under)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| is_gltf(f) && !same_path(f, file))
        .flat_map(|f| gltf_uris(&f))
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect();
    own.into_iter()
        .filter(|p| {
            p.starts_with(dir) && !p.components().any(|c| c == std::path::Component::ParentDir)
        })
        .filter(|p| p.is_file())
        .filter(|p| fs::canonicalize(p).is_ok_and(|c| !others.contains(&c)))
        .collect()
}

/// The files a `.gltf`'s buffers and images name (not data URIs).
fn gltf_uris(file: &Path) -> Vec<PathBuf> {
    let dir = file.parent().unwrap_or(Path::new(""));
    let Some(json) = fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return Vec::new();
    };
    ["buffers", "images"]
        .iter()
        .filter_map(|k| json.get(k)?.as_array())
        .flatten()
        .filter_map(|v| v.get("uri")?.as_str())
        .filter(|uri| !uri.starts_with("data:"))
        .map(|uri| dir.join(crate::relocate::uri_path(uri)))
        .collect()
}

/// Moves each `(from, to)` entry and fixes the paths in the project's files (including
/// `assetc.toml` in `root`); undoes the moves when one fails. Returns how many files were fixed.
fn relocate_all(
    input: &Path,
    root: Option<&Path>,
    moves: &[(PathBuf, PathBuf)],
) -> Result<usize, String> {
    for (from, to) in moves {
        // `exists` is case-insensitive on some file systems; allow pure case changes.
        if to.exists() && !same_path(from, to) {
            let dest = to.parent().unwrap_or(input);
            return Err(format!(
                "{} already exists in {}",
                name_of(to),
                name_of(dest)
            ));
        }
        if !from.exists() {
            return Err(format!("{} does not exist", name_of(from)));
        }
    }
    let fixes = crate::relocate::fixes(input, root, moves)?;
    let mut done: Vec<(&Path, &Path)> = Vec::new();
    for (from, to) in moves {
        let moved = to
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::rename(from, to));
        if let Err(e) = moved {
            for (from, to) in done.into_iter().rev() {
                let _ = fs::rename(to, from);
            }
            return Err(e.to_string());
        }
        done.push((from, to));
    }
    for (file, text) in &fixes {
        fs::write(file, text).map_err(|e| format!("{}: {e}", name_of(file)))?;
    }
    Ok(fixes.len())
}

/// `path` must resolve inside `input`; the `input` directory itself only if `allow_root`.
fn check_inside(input: &Path, path: &Path, allow_root: bool) -> Result<(), String> {
    let root = fs::canonicalize(input).map_err(|e| e.to_string())?;
    // Resolve the parent so a symlinked entry itself is judged by where it is, not its target.
    let resolved = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !allow_root || path != input => fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .join(name),
        _ => fs::canonicalize(path).map_err(|e| e.to_string())?,
    };
    let inside = resolved.starts_with(&root) && (allow_root || resolved != root);
    if inside {
        Ok(())
    } else {
        Err("outside the input directory".into())
    }
}

/// `<dir>/<stem><ext>`, or `<stem> 2<ext>`, `<stem> 3<ext>`, … if taken.
fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    (1..)
        .map(|n| match n {
            1 => dir.join(format!("{stem}{ext}")),
            n => dir.join(format!("{stem} {n}{ext}")),
        })
        .find(|path| !path.exists())
        .expect("a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("assets");
        fs::create_dir_all(input.join("sub")).unwrap();
        (dir, input)
    }

    #[test]
    fn creates_unique_folders_and_noise() {
        let (_dir, input) = input();
        assert_eq!(
            create_folder(&input, &input).unwrap(),
            input.join("New Folder")
        );
        assert_eq!(
            create_folder(&input, &input).unwrap(),
            input.join("New Folder 2")
        );
        let noise = create_noise(&input, &input.join("sub"), GeneratorKind::PerlinWorley).unwrap();
        assert_eq!(noise, input.join("sub/perlin-worley.anoise"));
        let again = create_noise(&input, &input.join("sub"), GeneratorKind::PerlinWorley).unwrap();
        assert_eq!(again, input.join("sub/perlin-worley 2.anoise"));
        let parsed = Noise::parse(&fs::read_to_string(&noise).unwrap()).unwrap();
        assert_eq!(parsed, Noise::new(GeneratorKind::PerlinWorley));
        let gradient = create_gradient(&input, &input).unwrap();
        assert_eq!(gradient, input.join("gradient.agradient"));
        let parsed = Gradients::parse(&fs::read_to_string(&gradient).unwrap()).unwrap();
        assert_eq!(parsed, Gradients::new());
        let palette = create_palette(&input, &input).unwrap();
        assert_eq!(palette, input.join("palette.apalette"));
        let parsed = Palette::parse(&fs::read_to_string(&palette).unwrap()).unwrap();
        assert_eq!(parsed, Palette::new());
        let schema = create_schema(&input, &input).unwrap();
        assert_eq!(schema, input.join("schema.aschema"));
        let parsed = crate::aschema::Schema::parse(&fs::read_to_string(&schema).unwrap()).unwrap();
        assert_eq!(parsed, crate::aschema::Schema::new());
        let graph = create_graph(&input, &input).unwrap();
        assert_eq!(graph, input.join("graph.agraph"));
        let parsed = crate::agraph::Graph::parse(&fs::read_to_string(&graph).unwrap()).unwrap();
        assert_eq!(parsed, crate::agraph::Graph::new());
        let tree = create_tree(&input, &input).unwrap();
        assert_eq!(tree, input.join("tree.atree"));
        let parsed = crate::atree::parse(&fs::read_to_string(&tree).unwrap()).unwrap();
        assert_eq!(parsed, crate::atree::new());
    }

    #[test]
    fn data_files_name_the_nearest_schema() {
        use crate::adata::DataFile;
        let (_dir, input) = input();
        let read = |path: &Path| DataFile::parse(&fs::read_to_string(path).unwrap()).unwrap();
        let none = create_data(&input, &input).unwrap();
        assert_eq!(none, input.join("data.adata"));
        assert_eq!(read(&none).schema, "");
        create_schema(&input, &input).unwrap();
        let data = create_data(&input, &input.join("sub")).unwrap();
        assert_eq!(read(&data).schema, "../schema.aschema");
        create_schema(&input, &input.join("sub")).unwrap();
        let data = create_data(&input, &input.join("sub")).unwrap();
        assert_eq!(data, input.join("sub/data 2.adata"));
        assert_eq!(read(&data).schema, "schema.aschema");
    }

    #[test]
    fn renames_with_validation() {
        let (_dir, input) = input();
        let file = input.join("a.png");
        fs::write(&file, "x").unwrap();
        fs::write(input.join("b.png"), "x").unwrap();
        for bad in ["", ".", "..", "x/y", "x\\\\y", "b.png"] {
            assert!(rename(&input, None, &file, bad).is_err(), "{bad:?}");
        }
        assert_eq!(rename(&input, None, &file, "a.png").unwrap(), file);
        let renamed = rename(&input, None, &file, "c.png").unwrap();
        assert_eq!(renamed, input.join("c.png"));
        assert!(renamed.is_file() && !file.exists());
        let folder = rename(&input, None, &input.join("sub"), "textures").unwrap();
        assert!(folder.is_dir());

        // Sidecars follow their file, and go with it.
        let model = input.join("Fox.glb");
        fs::write(&model, "x").unwrap();
        fs::write(input.join("Fox.glb.amod"), "").unwrap();
        let hero = rename(&input, None, &model, "Hero.glb").unwrap();
        assert!(input.join("Hero.glb.amod").is_file());
        assert!(!input.join("Fox.glb.amod").exists());
        delete(&input, &hero).unwrap();
        assert!(!input.join("Hero.glb.amod").exists());
    }

    #[test]
    fn moves_with_companions_and_fixes_paths() {
        let (dir, input) = input();
        let root = dir.path();
        let put = |rel: &str, text: &str| {
            let path = input.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        let read = |rel: &str| fs::read_to_string(input.join(rel)).unwrap();
        put("game.aschema", "{\n  \"$dialect\": \"a/1\"\n}\n");
        put(
            "data/hero.adata",
            "{\n  \"$dialect\": \"a/1\",\n  \"$schema\": \"../game.aschema\",\n  \"instances\": [\n    {\n      \"$id\": \"a\",\n      \"icon\": \"ui/icon.png\"\n    }\n  ]\n}\n",
        );
        put("ui/icon.png", "x");
        let amat = "# keep me\n[textures]\nnormal = \"../shared/n.png\" # here\n\n[[structs]]\nschema = \"../game.aschema\"\ntype = \"T\"\n\n[values.T]\nicon = \"ui/icon.png\"\n";
        put("mats/lake.amat", amat);
        for f in ["lake.png", "lake.n.png", "lake.x.png", "lake.png.amod"] {
            put(&format!("mats/{f}"), "");
        }
        put("shared/n.png", "x");
        put(
            "models/a.gltf",
            "{\"buffers\": [{\"uri\": \"a.bin\"}], \"images\": [{\"uri\": \"tex/a%20b.png\"}, {\"uri\": \"data:x\"}]}",
        );
        put("models/b.gltf", "{\"buffers\": [{\"uri\": \"a.bin\"}]}");
        put("models/a.bin", "");
        put("models/tex/a b.png", "");
        put(
            "models/a.gltf.amod",
            "[[structs]]\nschema = \"../game.aschema\"\ntype = \"T\"\n\n[[locators]]\nname = \"hand\"\npreview = \"models/b.gltf\"\n",
        );
        fs::write(
            root.join("assetc.toml"),
            "[[locators.presets]]\nfrom = \"models/a.gltf\" # the hero\nmatch = [\"models/*\"]\n",
        )
        .unwrap();
        fs::create_dir_all(input.join("deep/er")).unwrap();
        let deep = input.join("deep/er");

        // A material with its files by name and their sidecars; its own paths still point
        // where they did.
        let report = move_into(&input, Some(root), &input.join("mats/lake.amat"), &deep).unwrap();
        assert_eq!(report.path, deep.join("lake.amat"));
        assert_eq!(report.more, 3);
        for f in ["lake.amat", "lake.png", "lake.n.png", "lake.png.amod"] {
            assert!(deep.join(f).is_file(), "{f}");
        }
        assert!(input.join("mats/lake.x.png").is_file());
        assert_eq!(
            read("deep/er/lake.amat"),
            amat.replace("../shared", "../../shared")
                .replace("../game", "../../game")
        );
        assert!(!can_move(&deep.join("lake.amat"), &deep));
        assert!(!can_move(&input.join("deep"), &deep));
        assert!(can_move(&deep.join("lake.png.amod"), &input));

        // Input-relative asset values follow the file.
        let report = move_into(&input, Some(root), &input.join("ui/icon.png"), &input).unwrap();
        assert_eq!((report.more, report.fixed), (0, 2));
        assert!(read("data/hero.adata").contains("\"icon\": \"icon.png\""));
        assert!(read("deep/er/lake.amat").contains("icon = \"icon.png\""));

        // A glTF takes its own files, not one another model uses; its sidecar and the presets
        // follow it.
        let report = move_into(&input, Some(root), &input.join("models/a.gltf"), &deep).unwrap();
        assert_eq!(report.more, 2);
        assert!(deep.join("tex/a b.png").is_file() && deep.join("a.gltf.amod").is_file());
        assert!(input.join("models/a.bin").is_file());
        assert_eq!(
            read("deep/er/a.gltf"),
            "{\"buffers\": [{\"uri\": \"../../models/a.bin\"}], \"images\": [{\"uri\": \"tex/a%20b.png\"}, {\"uri\": \"data:x\"}]}"
        );
        assert!(read("deep/er/a.gltf.amod").contains("schema = \"../../game.aschema\""));
        assert!(read("deep/er/a.gltf.amod").contains("preview = \"models/b.gltf\""));
        assert_eq!(
            fs::read_to_string(root.join("assetc.toml")).unwrap(),
            "[[locators.presets]]\nfrom = \"deep/er/a.gltf\" # the hero\nmatch = [\"models/*\"]\n"
        );

        // A schema: every file attaching it or naming it follows.
        move_into(
            &input,
            Some(root),
            &input.join("game.aschema"),
            &input.join("data"),
        )
        .unwrap();
        assert!(read("data/hero.adata").contains("\"$schema\": \"game.aschema\""));
        assert!(read("deep/er/lake.amat").contains("schema = \"../../data/game.aschema\""));

        // Renaming a folder fixes the paths into it.
        rename(&input, Some(root), &input.join("shared"), "common").unwrap();
        assert!(read("deep/er/lake.amat").contains("normal = \"../../common/n.png\" # here"));

        // Refused: a folder into itself, its own folder, a name already there (nothing moves).
        assert!(move_into(&input, Some(root), &input.join("deep"), &deep).is_err());
        assert!(move_into(&input, Some(root), &deep.join("lake.png"), &deep).is_err());
        put("lake.png", "");
        let e = move_into(&input, Some(root), &deep.join("lake.amat"), &input).unwrap_err();
        assert_eq!(e, "lake.png already exists in assets");
        assert!(deep.join("lake.amat").is_file() && deep.join("lake.png").is_file());
    }

    #[test]
    fn custom_materials_take_their_shaders_files_by_name() {
        let (_dir, input) = input();
        let shader = create_shader(&input, &input).unwrap();
        fs::write(
            &shader,
            "[[textures]]\nname = \"color\"\nusage = \"color\"\nbare = true\n\n[[textures]]\nname = \"ripples\"\nusage = \"normal\"\n",
        )
        .unwrap();
        fs::write(
            input.join("sub/lake.acmat"),
            "shader = \"../shader.ashader\"\n",
        )
        .unwrap();
        for f in [
            "lake.png",
            "lake.ripples.png",
            "lake.foam.png",
            "lake.ripples.anoise",
        ] {
            fs::write(input.join("sub").join(f), "").unwrap();
        }
        let found = companions(Some(&input), &input.join("sub/lake.acmat"));
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["lake.acmat", "lake.png", "lake.ripples.png"]);
    }

    #[test]
    fn copies_files_in_with_their_companions() {
        let (dir, input) = input();
        let outside = dir.path().join("downloads");
        fs::create_dir_all(outside.join("pack")).unwrap();
        fs::write(outside.join("Fox.glb"), "x").unwrap();
        fs::write(outside.join("Fox.glb.amod"), "").unwrap();
        fs::write(outside.join("pack/a.png"), "x").unwrap();
        let paths = [outside.join("Fox.glb"), outside.join("pack")];
        let report = copy_into(&input, &paths, &input.join("sub")).unwrap();
        assert_eq!((report.path, report.more), (input.join("sub/Fox.glb"), 2));
        assert!(input.join("sub/Fox.glb.amod").is_file() && input.join("sub/pack/a.png").is_file());
        assert!(outside.join("Fox.glb").is_file());
        let e = copy_into(&input, &paths[..1], &input.join("sub")).unwrap_err();
        assert_eq!(e, "Fox.glb already exists in sub");
    }

    #[test]
    fn deletes_files_and_folders() {
        let (_dir, input) = input();
        fs::write(input.join("sub/a.png"), "x").unwrap();
        delete(&input, &input.join("sub/a.png")).unwrap();
        assert!(!input.join("sub/a.png").exists());
        delete(&input, &input.join("sub")).unwrap();
        assert!(!input.join("sub").exists());
    }

    #[test]
    fn refuses_outside_and_root() {
        let (dir, input) = input();
        let outside = dir.path().join("outside.png");
        fs::write(&outside, "x").unwrap();
        assert!(delete(&input, &outside).is_err());
        assert!(delete(&input, &input).is_err());
        assert!(delete(&input, &input.join("sub/../../outside.png")).is_err());
        assert!(rename(&input, None, &input, "other").is_err());
        assert!(create_folder(&input, dir.path()).is_err());
        assert!(outside.exists() && input.exists());
    }
}
