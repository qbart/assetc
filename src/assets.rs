//! Built assets, read from the manifest, see docs/features/inspector.md#assets-tab.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;

use crate::config::Project;
use crate::manifest::{self, Kind};

#[derive(Debug, PartialEq, Eq)]
pub struct Asset {
    pub id: u64,
    pub kind: Kind,
    /// Output path without extension, e.g. `textures/Grass`.
    pub asset_path: String,
    /// Absolute path of the compiled file.
    pub file: PathBuf,
}

#[derive(Debug, Resource)]
pub enum Assets {
    NoProject,
    NotBuilt,
    Failed(String),
    /// Sorted by asset path, case-insensitively.
    Loaded(Vec<Asset>),
}

impl Assets {
    pub fn load(project: &Project) -> Self {
        let Some(output) = project.output_dir() else {
            return Self::NoProject;
        };
        let path = output.join(manifest::FILE_NAME);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Self::NotBuilt,
            Err(e) => return Self::Failed(format!("{}: {e}", path.display())),
        };
        let entries = match manifest::decode(&bytes) {
            Ok(entries) => entries,
            Err(e) => return Self::Failed(e.to_string()),
        };
        let mut assets: Vec<_> = entries
            .into_iter()
            .map(|entry| Asset {
                id: entry.id,
                kind: entry.kind,
                asset_path: entry.kind.asset_path(&entry.path).to_owned(),
                file: output.join(&entry.path),
            })
            .collect();
        assets.sort_by_cached_key(|a| a.asset_path.to_lowercase());
        Self::Loaded(assets)
    }

    /// Assets stored in `file` (an absolute path in the output directory), by asset path.
    pub fn in_file(&self, file: &Path) -> Vec<&Asset> {
        match self {
            Self::Loaded(assets) => assets.iter().filter(|a| a.file == file).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::fnv1a64;
    use crate::{build, config};

    fn project(dir: &Path) -> Project {
        Project {
            root: Some(dir.to_path_buf()),
            config: Default::default(),
        }
    }

    fn png(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::new(1, 1).save(path).unwrap();
    }

    fn built(dir: &Path) -> Project {
        let project = project(dir);
        png(&dir.join("assets/textures/b.png"));
        png(&dir.join("assets/A.png"));
        png(&dir.join("assets/c.png"));
        build::run(
            &project.input_dir().unwrap(),
            &project.output_dir().unwrap(),
            &config::Textures::default(),
        )
        .unwrap();
        project
    }

    #[test]
    fn lists_manifest_sorted_by_asset_path() {
        let dir = tempfile::tempdir().unwrap();
        let project = built(dir.path());
        let Assets::Loaded(assets) = Assets::load(&project) else {
            panic!("not loaded");
        };
        let paths: Vec<_> = assets.iter().map(|a| a.asset_path.as_str()).collect();
        assert_eq!(paths, ["A", "c", "textures/b"]);
        assert_eq!(assets[2].id, fnv1a64(b"textures/b"));
        assert_eq!(assets[2].kind, Kind::Texture);
        assert_eq!(assets[2].file, dir.path().join("build/textures/b.ktx2"));
    }

    #[test]
    fn placeholders() {
        assert!(matches!(
            Assets::load(&Project::default()),
            Assets::NoProject
        ));
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path());
        assert!(matches!(Assets::load(&project), Assets::NotBuilt));
        fs::create_dir_all(dir.path().join("build")).unwrap();
        fs::write(dir.path().join("build").join(manifest::FILE_NAME), "junk").unwrap();
        assert!(matches!(Assets::load(&project), Assets::Failed(_)));
    }

    #[test]
    fn assets_in_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = built(dir.path());
        let assets = Assets::load(&project);
        let in_file = |path: &str| -> Vec<String> {
            assets
                .in_file(&dir.path().join(path))
                .iter()
                .map(|a| a.asset_path.clone())
                .collect()
        };
        assert_eq!(in_file("build/textures/b.ktx2"), ["textures/b"]);
        assert!(in_file("build/assets.manifest").is_empty());
        assert!(in_file("assets/textures/b.png").is_empty());
        assert!(Assets::NotBuilt.in_file(Path::new("x.ktx2")).is_empty());
    }
}
