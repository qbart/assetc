//! File operations in the Input tree, see docs/features/file-operations.md.

use std::fs;
use std::path::{Path, PathBuf};

use crate::noise::{Noise, NoiseType};

/// Creates `New Folder` (or `New Folder 2`, …) in `dir`.
pub fn create_folder(input: &Path, dir: &Path) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, "New Folder", "");
    fs::create_dir(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Creates `<type>.noise` (or `<type> 2.noise`, …) in `dir` with default parameters.
pub fn create_noise(input: &Path, dir: &Path, kind: NoiseType) -> Result<PathBuf, String> {
    check_inside(input, dir, true)?;
    let path = unique(dir, &kind.name().to_lowercase(), ".noise");
    let noise = Noise {
        kind,
        ..Default::default()
    };
    fs::write(&path, noise.to_toml()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Renames `path` to `name` in the same folder; returns the new path.
pub fn rename(input: &Path, path: &Path, name: &str) -> Result<PathBuf, String> {
    check_inside(input, path, false)?;
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(format!("{name:?} is not a valid name"));
    }
    if path.file_name().is_some_and(|n| n == name) {
        return Ok(path.to_path_buf());
    }
    let target = path.with_file_name(name);
    // `exists` is case-insensitive on some file systems; allow pure case changes.
    let case_change = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name));
    if target.exists() && !case_change {
        return Err(format!("{name} already exists"));
    }
    fs::rename(path, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Deletes `path` (a folder with everything inside it) permanently.
pub fn delete(input: &Path, path: &Path) -> Result<(), String> {
    check_inside(input, path, false)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|e| e.to_string())
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
        let noise = create_noise(&input, &input.join("sub"), NoiseType::Worley).unwrap();
        assert_eq!(noise, input.join("sub/worley.noise"));
        let again = create_noise(&input, &input.join("sub"), NoiseType::Worley).unwrap();
        assert_eq!(again, input.join("sub/worley 2.noise"));
        let parsed = Noise::parse(&fs::read_to_string(&noise).unwrap()).unwrap();
        assert_eq!(parsed.kind, NoiseType::Worley);
    }

    #[test]
    fn renames_with_validation() {
        let (_dir, input) = input();
        let file = input.join("a.png");
        fs::write(&file, "x").unwrap();
        fs::write(input.join("b.png"), "x").unwrap();
        for bad in ["", ".", "..", "x/y", "x\\\\y", "b.png"] {
            assert!(rename(&input, &file, bad).is_err(), "{bad:?}");
        }
        assert_eq!(rename(&input, &file, "a.png").unwrap(), file);
        let renamed = rename(&input, &file, "c.png").unwrap();
        assert_eq!(renamed, input.join("c.png"));
        assert!(renamed.is_file() && !file.exists());
        let folder = rename(&input, &input.join("sub"), "textures").unwrap();
        assert!(folder.is_dir());
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
        assert!(rename(&input, &input, "other").is_err());
        assert!(create_folder(&input, dir.path()).is_err());
        assert!(outside.exists() && input.exists());
    }
}
