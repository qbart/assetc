use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;

use crate::config::Project;

/// A directory entry; `children` is `None` for anything that is not a directory.
#[derive(Debug, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub children: Option<Vec<Node>>,
}

/// The scanned state of one browsed directory.
#[derive(Debug)]
pub enum Listing {
    NoProject,
    Missing(PathBuf),
    Failed(String),
    Loaded(Vec<Node>),
}

impl Listing {
    fn scan(dir: Option<PathBuf>) -> Self {
        let Some(dir) = dir else {
            return Self::NoProject;
        };
        if !dir.exists() {
            return Self::Missing(dir);
        }
        match scan(&dir) {
            Ok(nodes) => Self::Loaded(nodes),
            Err(e) => Self::Failed(format!("{}: {e}", dir.display())),
        }
    }
}

/// Scanned input and output directories of the project.
#[derive(Debug, Resource)]
pub struct FileTrees {
    pub input: Listing,
    pub output: Listing,
}

impl FileTrees {
    pub fn scan(project: &Project) -> Self {
        Self {
            input: Listing::scan(project.input_dir()),
            output: Listing::scan(project.output_dir()),
        }
    }
}

/// Recursively lists `dir`: folders before files, each sorted case-insensitively,
/// dotfiles hidden, symlinks not followed.
pub fn scan(dir: &Path) -> io::Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let children = if entry.file_type()?.is_dir() {
            Some(scan(&path)?)
        } else {
            None
        };
        nodes.push(Node {
            name,
            path,
            children,
        });
    }
    nodes.sort_by_cached_key(|n| (n.children.is_none(), n.name.to_lowercase()));
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(nodes: &[Node]) -> Vec<&str> {
        nodes.iter().map(|n| n.name.as_str()).collect()
    }

    #[test]
    fn folders_first_then_case_insensitive_names() {
        let dir = tempfile::tempdir().unwrap();
        for f in ["b.png", "A.png", "c.txt"] {
            fs::write(dir.path().join(f), "").unwrap();
        }
        for d in ["zeta", "Alpha"] {
            fs::create_dir(dir.path().join(d)).unwrap();
        }
        let nodes = scan(dir.path()).unwrap();
        assert_eq!(names(&nodes), ["Alpha", "zeta", "A.png", "b.png", "c.txt"]);
    }

    #[test]
    fn nests_and_hides_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("meshes/props")).unwrap();
        fs::write(dir.path().join("meshes/props/crate.glb"), "").unwrap();
        fs::write(dir.path().join(".hidden"), "").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();

        let nodes = scan(dir.path()).unwrap();
        assert_eq!(names(&nodes), ["meshes"]);
        let props = &nodes[0].children.as_ref().unwrap()[0];
        assert_eq!(props.name, "props");
        let leaf = &props.children.as_ref().unwrap()[0];
        assert_eq!(leaf.name, "crate.glb");
        assert!(leaf.children.is_none());
    }

    #[test]
    fn listing_reports_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(matches!(Listing::scan(Some(missing)), Listing::Missing(_)));
        assert!(matches!(Listing::scan(None), Listing::NoProject));
    }
}
