//! `.amod` sidecars: per-file settings next to an input file, see docs/spec/amod.md.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::amesh::MeshPatch;

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Amod {
    /// `[mesh]`: meshoptimizer settings of a glTF model.
    pub mesh: MeshPatch,
}

impl Amod {
    pub fn is_empty(&self) -> bool {
        self.mesh.is_empty()
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.message().to_owned())
    }

    /// The written form, see docs/spec/amod.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        if !self.mesh.is_empty() {
            out += "[mesh]\n";
            out += &self.mesh.to_toml();
        }
        out
    }
}

/// `<file name>.amod` next to `file`.
pub fn path_for(file: &Path) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(".amod");
    file.with_file_name(name)
}

/// Whether `path` is a sidecar (`*.amod`).
pub fn is_sidecar(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "amod")
}

/// The sidecar of `file`, if it has one; errors name the sidecar.
pub fn read(file: &Path) -> Result<Option<Amod>, String> {
    let path = path_for(file);
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match fs::read_to_string(&path) {
        Ok(text) => Amod::parse(&text)
            .map(Some)
            .map_err(|e| format!("{name}: {e}")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// Writes the sidecar of `file`, or deletes it when `amod` holds nothing.
pub fn write(file: &Path, amod: &Amod) -> io::Result<()> {
    let path = path_for(file);
    if amod.is_empty() {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        fs::write(path, amod.to_toml())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_form_round_trips() {
        let amod = Amod {
            mesh: MeshPatch {
                meshlet_max_vertices: Some(128),
                meshlet_max_triangles: Some(256),
                lod_ratios: Some(vec![0.5, 0.1]),
                ..Default::default()
            },
        };
        let text = amod.to_toml();
        assert_eq!(
            text,
            "[mesh]\nmeshlet_max_vertices = 128\nmeshlet_max_triangles = 256\nlod_ratios = [0.5, 0.1]\n"
        );
        assert_eq!(Amod::parse(&text).unwrap(), amod);
        assert_eq!(Amod::parse("").unwrap(), Amod::default());
        assert!(Amod::parse("[mesh]\nmax = 1\n").is_err());
        assert!(Amod::parse("[sound]\n").is_err());
    }

    #[test]
    fn sidecars_sit_next_to_their_file() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("Fox.glb");
        assert_eq!(path_for(&model), dir.path().join("Fox.glb.amod"));
        assert_eq!(read(&model).unwrap(), None);
        let mut amod = Amod::default();
        amod.mesh.meshlet_max_vertices = Some(32);
        write(&model, &amod).unwrap();
        assert_eq!(read(&model).unwrap(), Some(amod));
        write(&model, &Amod::default()).unwrap();
        assert!(!path_for(&model).exists());
        fs::write(path_for(&model), "nope").unwrap();
        assert!(read(&model).unwrap_err().starts_with("Fox.glb.amod: "));
    }
}
