//! The Assets panel's list, read from the manifest, see docs/features/assets.md.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;

use crate::build::{relative, strip_extension};
use crate::config::Project;
use crate::hash::fnv1a64;
use crate::inspect::Location;
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
                asset_path: strip_extension(&entry.path).to_owned(),
                file: output.join(&entry.path),
            })
            .collect();
        assets.sort_by_cached_key(|a| a.asset_path.to_lowercase());
        Self::Loaded(assets)
    }

    /// Index of the asset linked to a selected file, if any.
    pub fn linked(&self, project: &Project, location: Location, file: &Path) -> Option<usize> {
        let Self::Loaded(assets) = self else {
            return None;
        };
        match location {
            Location::Output => assets.iter().position(|a| a.file == file),
            Location::Input => {
                let is_png = file
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
                let rel = relative(&project.input_dir()?, file).ok()?;
                let id = fnv1a64(strip_extension(&rel).as_bytes());
                is_png
                    .then(|| assets.iter().position(|a| a.id == id))
                    .flatten()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn links_compiled_files_and_sources() {
        let dir = tempfile::tempdir().unwrap();
        let project = built(dir.path());
        let assets = Assets::load(&project);
        let root = dir.path();
        let link = |location, path: &str| assets.linked(&project, location, &root.join(path));

        assert_eq!(link(Location::Output, "build/textures/b.ktx2"), Some(2));
        assert_eq!(link(Location::Input, "assets/textures/b.png"), Some(2));
        assert_eq!(link(Location::Input, "assets/A.png"), Some(0));
        assert_eq!(link(Location::Output, "build/assets.manifest"), None);
        assert_eq!(link(Location::Input, "assets/textures/b.txt"), None);
        assert_eq!(link(Location::Input, "assets/missing.png"), None);
    }
}
